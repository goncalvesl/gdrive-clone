//! Shared application commands for the desktop, CLI and background service.
use anyhow::{Context, Result, bail, ensure};
use drive_api::{Drive, Remote, Status, oauth};
use fs2::FileExt;
use local_fs::{LocalWatcher, Root};
use serde::{Deserialize, Serialize};
use state_store::{Store, mirror::Intent};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{File, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};
use sync_core::mirror::{Choice, Entry, Kind, Link, Operation, reconcile};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    pub profile: String,
    pub local: PathBuf,
    pub root_identity: String,
    pub remote_id: String,
    #[serde(default)]
    pub remote_name: Option<String>,
    pub credentials: PathBuf,
    #[serde(default)]
    pub account_profile: Option<String>,
    pub interval_seconds: u64,
    #[serde(default)]
    pub propagate_deletions: bool,
}
impl Config {
    pub fn account_profile(&self) -> &str {
        self.account_profile.as_deref().unwrap_or(&self.profile)
    }
}
/// Connection identity is independent of the synchronized roots. Contains no tokens.
#[derive(Clone, Serialize, Deserialize)]
pub struct Account {
    pub profile: String,
    pub credentials: PathBuf,
}
#[derive(Clone)]
pub struct Paths {
    pub config: PathBuf,
    pub data: PathBuf,
}
impl Paths {
    pub fn watch_local(&self, config: &Config) -> Result<LocalWatcher> {
        LocalWatcher::open(&config.local)
    }
    pub fn discover() -> Result<Self> {
        let home = std::env::var_os("HOME").context("HOME ausente")?;
        let home = PathBuf::from(home);
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"))
            .join("gdrive-clone");
        let data = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share"))
            .join("gdrive-clone");
        ensure!(
            config.is_absolute() && data.is_absolute(),
            "Diretórios XDG devem ser absolutos"
        );
        Ok(Self { config, data })
    }
    pub fn load(&self) -> Result<Config> {
        serde_json::from_slice(
            &std::fs::read(self.config.join("config.json"))
                .context("Configure a pasta local, pasta remota e OAuth primeiro")?,
        )
        .context("Configuração inválida")
    }
    pub fn configured(&self) -> bool {
        self.config.join("config.json").is_file()
    }
    fn write_config(&self, name: &str, value: &impl Serialize) -> Result<()> {
        private_dir(&self.config)?;
        private_dir(&self.data)?;
        let temp = self.config.join(format!("config-{}.tmp", Uuid::new_v4()));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)?;
        file.write_all(&serde_json::to_vec_pretty(value)?)?;
        file.sync_all()?;
        std::fs::rename(temp, self.config.join(name))?;
        File::open(&self.config)?.sync_all()?;
        Ok(())
    }
    pub fn save(&self, config: &Config) -> Result<()> {
        self.write_config("config.json", config)
    }
    pub fn account(&self) -> Result<Account> {
        match std::fs::read(self.config.join("account.json")) {
            Ok(bytes) => serde_json::from_slice(&bytes).context("Conexão salva inválida"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let config = self.load()?;
                Ok(Account {
                    profile: config.account_profile().into(),
                    credentials: config.credentials,
                })
            }
            Err(e) => Err(e.into()),
        }
    }
    /// Prepares login without creating a sync root or changing an existing journal.
    pub fn prepare_account(&self, credentials: &Path) -> Result<Account> {
        oauth::Credentials::load(credentials)?;
        let credentials = credentials.canonicalize()?;
        let existing = self.account().ok();
        let profile = existing
            .filter(|a| a.credentials == credentials)
            .map(|a| a.profile)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let account = Account {
            profile,
            credentials,
        };
        self.write_config("account.json", &account)?;
        Ok(account)
    }
    /// The distributor supplies this Desktop client; users only authorize in a browser.
    /// Existing developer configurations remain compatible with earlier previews.
    pub fn application_credentials(&self) -> Result<PathBuf> {
        if let Some(path) = std::env::var_os("GDRIVE_OAUTH_CLIENT_FILE") {
            let path = PathBuf::from(path);
            oauth::Credentials::load(&path)?;
            return Ok(path.canonicalize()?);
        }
        let executable = std::env::current_exe()?;
        if let Some(prefix) = executable.parent().and_then(Path::parent) {
            let bundled = prefix.join("share/gdrive-clone/oauth-client.json");
            if bundled.is_file() {
                oauth::Credentials::load(&bundled)?;
                return Ok(bundled.canonicalize()?);
            }
        }
        let managed = self.config.join("oauth-client.json");
        if managed.is_file() {
            oauth::Credentials::load(&managed)?;
            return Ok(managed.canonicalize()?);
        }
        if let Ok(account) = self.account() {
            oauth::Credentials::load(&account.credentials)?;
            return Ok(account.credentials);
        }
        bail!(
            "Esta compilação ainda não inclui a configuração de login do aplicativo. O responsável pela compilação deve fornecer o cliente OAuth Desktop do projeto."
        )
    }
    pub fn connect(&self, credentials: &Path) -> Result<()> {
        let account = self.prepare_account(credentials)?;
        oauth::login(
            &account.profile,
            &oauth::Credentials::load(&account.credentials)?,
        )
    }
    pub fn account_info(&self) -> Result<drive_api::AccountInfo> {
        let account = self.account()?;
        Drive::new(
            account.profile,
            oauth::Credentials::load(&account.credentials)?,
            "root".into(),
        )?
        .account_info()
    }
    pub fn folders(&self, parent: &str) -> Result<Vec<(String, String)>> {
        let account = self.account()?;
        Drive::new(
            account.profile,
            oauth::Credentials::load(&account.credentials)?,
            "root".into(),
        )?
        .folders(parent)
    }
    pub fn configure(
        &self,
        local: &Path,
        remote_id: &str,
        credentials: &Path,
        propagate_deletions: bool,
    ) -> Result<Config> {
        self.configure_named(local, remote_id, None, credentials, propagate_deletions)
    }
    pub fn configure_named(
        &self,
        local: &Path,
        remote_id: &str,
        remote_name: Option<&str>,
        credentials: &Path,
        propagate_deletions: bool,
    ) -> Result<Config> {
        oauth::Credentials::load(credentials)?;
        ensure!(
            !remote_id.is_empty()
                && remote_id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "Informe o ID da pasta remota, não a URL inteira"
        );
        ensure!(local.is_absolute(), "Informe um caminho local absoluto");
        std::fs::create_dir_all(local)?;
        let local = local.canonicalize()?;
        private_dir(&self.data)?;
        private_dir(&self.config)?;
        let data = self.data.canonicalize()?;
        let conf = self.config.canonicalize()?;
        ensure!(
            !data.starts_with(&local)
                && !conf.starts_with(&local)
                && !local.starts_with(&data)
                && !local.starts_with(&conf),
            "A pasta de sincronização não pode conter dados/configuração do próprio aplicativo"
        );
        let root = Root::open(&local, None)?;
        let previous = self.load().ok();
        let credentials = credentials.canonicalize()?;
        ensure!(
            !credentials.starts_with(&local),
            "Guarde o arquivo OAuth fora da pasta sincronizada"
        );
        let account_profile = self
            .account()
            .ok()
            .filter(|a| a.credentials == credentials)
            .map(|a| a.profile);
        let profile = previous
            .as_ref()
            .filter(|c| {
                c.local == local
                    && c.remote_id == remote_id
                    && c.credentials == credentials
                    && c.root_identity == root.identity
            })
            .map(|c| c.profile.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let _previous_lock = if let Some(previous) = previous {
            let directory = self.profile_dir(&previous)?;
            let guard = lock(&directory)?;
            let database = directory.join("state.sqlite");
            if previous.profile != profile && database.exists() {
                let mut store = Store::open(database)?;
                store.init_mirror()?;
                ensure!(
                    store.intents()?.is_empty(),
                    "Há operações pendentes no perfil atual. Recupere as pendências antes de trocar as pastas ou o cliente OAuth."
                );
            }
            Some(guard)
        } else {
            None
        };
        let config = Config {
            profile,
            local,
            root_identity: root.identity,
            remote_id: remote_id.into(),
            remote_name: remote_name.map(str::to_owned),
            credentials,
            account_profile,
            interval_seconds: 30,
            propagate_deletions,
        };
        self.save(&config)?;
        Ok(config)
    }
    fn profile_dir(&self, config: &Config) -> Result<PathBuf> {
        ensure!(Uuid::parse_str(&config.profile).is_ok(), "Perfil inválido");
        let path = self.data.join(&config.profile);
        private_dir(&path)?;
        Ok(path)
    }
    pub fn login(&self) -> Result<()> {
        self.connect(&self.application_credentials()?)
    }
    pub fn disconnect(&self) -> Result<()> {
        let account = self.account()?;
        oauth::disconnect(&account.profile)
    }
    pub fn drive(&self, config: &Config) -> Result<Drive> {
        let mut drive = Drive::new(
            config.account_profile().into(),
            oauth::Credentials::load(&config.credentials)?,
            config.remote_id.clone(),
        )?;
        let dir = self.profile_dir(config)?;
        let mut store = Store::open(dir.join("state.sqlite"))?;
        store.init_mirror()?;
        drive.set_upload_sessions(Box::new(SessionStore(store)));
        let mut change_store = Store::open(dir.join("state.sqlite"))?;
        change_store.init_mirror()?;
        drive.set_change_state(Box::new(ChangeStore(change_store)));
        Ok(drive)
    }
    pub fn run(&self, dry_run: bool) -> Result<Report> {
        self.run_with_progress(dry_run, None)
    }
    pub fn run_with_progress(
        &self,
        dry_run: bool,
        progress: Option<drive_api::ProgressSink>,
    ) -> Result<Report> {
        let config = self.load()?;
        let dir = self.profile_dir(&config)?;
        let _lock = lock(&dir)?;
        ensure!(
            self.load()?.profile == config.profile,
            "A configuração mudou; execute novamente com o perfil atual"
        );
        let root = Root::open(&config.local, Some(&config.root_identity))?;
        let mut store = Store::open(dir.join("state.sqlite"))?;
        store.init_mirror()?;
        let mut remote = self.drive(&config)?;
        remote.set_progress(progress);
        synchronize_with_pause(
            &root,
            &mut remote,
            &mut store,
            dry_run,
            config.propagate_deletions,
            &|| if dry_run { Ok(false) } else { self.paused() },
        )
    }
    pub fn history(&self) -> Result<Vec<String>> {
        let config = self.load()?;
        let dir = self.profile_dir(&config)?;
        let _lock = lock(&dir)?;
        let mut store = Store::open(dir.join("state.sqlite"))?;
        store.init_mirror()?;
        Ok(store.history()?)
    }
}
struct SessionStore(Store);
impl drive_api::UploadSessionStore for SessionStore {
    fn load(&mut self, id: &str) -> Result<Option<sync_core::mirror::UploadSession>> {
        Ok(self.0.upload_session(id)?)
    }
    fn save(&mut self, id: &str, session: &sync_core::mirror::UploadSession) -> Result<()> {
        Ok(self.0.save_upload_session(id, session)?)
    }
    fn clear(&mut self, id: &str) -> Result<()> {
        Ok(self.0.clear_upload_session(id)?)
    }
}
struct ChangeStore(Store);
impl drive_api::ChangeStateStore for ChangeStore {
    fn load(&mut self) -> Result<Option<(String, BTreeMap<String, Entry>)>> {
        Ok(self.0.remote_snapshot()?)
    }
    fn advance(&mut self, cursor: &str) -> Result<()> {
        Ok(self.0.advance_remote_cursor(cursor)?)
    }
    fn update(&mut self, cursor: &str, updates: &BTreeMap<String, Option<Entry>>) -> Result<()> {
        Ok(self.0.update_remote_snapshot(cursor, updates)?)
    }
    fn save(&mut self, cursor: &str, inventory: &BTreeMap<String, Entry>) -> Result<()> {
        Ok(self.0.save_remote_snapshot(cursor, inventory)?)
    }
}
fn private_dir(path: &Path) -> Result<()> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)?;
    ensure!(
        !std::fs::symlink_metadata(path)?.file_type().is_symlink(),
        "Diretório de dados não pode ser link simbólico"
    );
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}
fn lock(dir: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(dir.join("daemon.lock"))?;
    file.try_lock_exclusive()
        .context("Outra sincronização está em execução")?;
    Ok(file)
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct Report {
    pub completed: usize,
    pub planned: Vec<String>,
    pub conflicts: Vec<String>,
    pub blocked: Vec<String>,
    pub recovered: usize,
    pub paused: bool,
}
/// Scheduling policy only; callers supply failure count and server minimum.
pub fn retry_delay(failures: u32, minimum: std::time::Duration) -> std::time::Duration {
    std::time::Duration::from_secs((30_u64 << failures.saturating_sub(1).min(4)).min(300))
        .max(minimum)
}
fn status_entry(status: Status) -> Option<Entry> {
    match status {
        Status::Present(e) => Some(e),
        _ => None,
    }
}

/// One complete reconciliation cycle. Any failed inventory stops before planning.
pub fn synchronize(
    root: &Root,
    remote: &mut impl Remote,
    store: &mut Store,
    dry_run: bool,
    deletions: bool,
) -> Result<Report> {
    synchronize_with_pause(root, remote, store, dry_run, deletions, &|| Ok(false))
}
/// Pause is checked between effects, never by interrupting an in-flight file replacement.
pub fn synchronize_with_pause(
    root: &Root,
    remote: &mut impl Remote,
    store: &mut Store,
    dry_run: bool,
    deletions: bool,
    paused: &dyn Fn() -> Result<bool>,
) -> Result<Report> {
    let mut total = Report::default();
    for _ in 0..1024 {
        let (report, changed) = synchronize_pass(root, remote, store, dry_run, deletions, paused)?;
        total.paused = report.paused;
        total.completed += report.completed;
        total.recovered += report.recovered;
        total.planned.extend(report.planned);
        total.conflicts.extend(report.conflicts);
        total.blocked.extend(report.blocked);
        if !changed {
            total.conflicts.sort();
            total.conflicts.dedup();
            total.blocked.sort();
            total.blocked.dedup();
            return Ok(total);
        }
    }
    total
        .blocked
        .push("Limite de alterações de diretórios neste ciclo; continue no próximo ciclo".into());
    Ok(total)
}
fn synchronize_pass(
    root: &Root,
    remote: &mut impl Remote,
    store: &mut Store,
    dry_run: bool,
    deletions: bool,
    paused: &dyn Fn() -> Result<bool>,
) -> Result<(Report, bool)> {
    let mut report = Report::default();
    if paused()? {
        report.paused = true;
        return Ok((report, false));
    }
    let mut remote_files = remote.scan()?;
    let intents = store.intents()?;
    if dry_run && !intents.is_empty() {
        report
            .blocked
            .push("Há operações pendentes; execute um ciclo para inspecioná-las".into());
        return Ok((report, false));
    }
    for intent in intents {
        if paused()? {
            report.paused = true;
            return Ok((report, false));
        }
        match execute(root, remote, store, &intent) {
            Ok(()) => report.recovered += 1,
            Err(e) if drive_api::retry_minimum(&e).is_some() => {
                return Err(e.context("Retomada interrompida; intenção preservada"));
            }
            Err(e) => report
                .blocked
                .push(format!("Operação {}: {e:#}", intent.operation.operation_id)),
        }
    }
    // Never plan around an unresolved running effect.
    if !store.intents()?.is_empty() {
        return Ok((report, false));
    }
    if report.recovered > 0 {
        remote_files = remote.scan()?;
    }
    let mut local_files = root.scan()?;
    let links = store.links()?;
    let directories = synchronize_directories(
        root,
        remote,
        store,
        &local_files,
        &remote_files,
        &links,
        dry_run,
        deletions,
        &mut report,
        paused,
    );
    let (changed, blocked) = match directories {
        Ok(result) => result,
        Err(error) if error.is::<PauseRequested>() => {
            report.paused = true;
            return Ok((report, false));
        }
        Err(error) => return Err(error),
    };
    if changed {
        return Ok((report, true));
    }
    local_files.retain(|path, entry| !entry.directory && !blocked_path(path, &blocked));
    remote_files.retain(|_, entry| !entry.directory && !blocked_path(&entry.path, &blocked));
    let links: Vec<_> = links
        .into_iter()
        .filter(|link| {
            !link.local.directory
                && !link.remote.directory
                && !blocked_path(&link.local.path, &blocked)
                && !blocked_path(&link.remote.path, &blocked)
        })
        .collect();
    let mut handled = BTreeSet::new();
    for base in &links {
        if paused()? {
            report.paused = true;
            return Ok((report, false));
        }
        handled.insert(base.id.clone());
        let remote_entry = remote_files.remove(&base.id);
        // Match identity first to track local renames; fallback handles atomic-save editors.
        let mut matching = local_files
            .iter()
            .filter(|(_, e)| e.identity == base.local.identity);
        let first = matching.next().map(|(p, _)| p.clone());
        if matching.next().is_some() {
            report
                .conflicts
                .push(format!("Hard links ambíguos: {}", base.local.path));
            continue;
        }
        let path = first.unwrap_or_else(|| base.local.path.clone());
        let local = local_files.remove(&path);
        let trashed = if remote_entry.is_none() {
            matches!(remote.status(&base.id)?, Status::Trashed)
        } else {
            false
        };
        let mut choice = reconcile(Some(base), local.as_ref(), remote_entry.as_ref(), trashed);
        if choice == Choice::Act(Kind::Upload)
            && local
                .as_ref()
                .is_some_and(|l| drive_api::is_reference_path(&l.path))
            && local
                .as_ref()
                .zip(remote_entry.as_ref())
                .is_some_and(|(l, r)| !l.same_content(r))
        {
            choice = Choice::Conflict;
        }
        let operation = Operation {
            operation_id: Uuid::new_v4().to_string(),
            object_id: base.id.clone(),
            kind: Kind::Checkpoint,
            local,
            remote: remote_entry,
            base: Some(base.clone()),
        };
        dispatch(
            root,
            remote,
            store,
            operation,
            choice,
            dry_run,
            deletions,
            &mut report,
        )?;
    }
    // Initial association by path requires equal content; unequal content is a conflict.
    let mut claimed = BTreeSet::new();
    for (id, remote_entry) in remote_files {
        if paused()? {
            report.paused = true;
            return Ok((report, false));
        }
        if handled.contains(&id) {
            continue;
        }
        claimed.insert(remote_entry.path.clone());
        let local = local_files.remove(&remote_entry.path);
        let choice = reconcile(None, local.as_ref(), Some(&remote_entry), false);
        let operation = Operation {
            operation_id: Uuid::new_v4().to_string(),
            object_id: id,
            kind: Kind::Checkpoint,
            local,
            remote: Some(remote_entry),
            base: None,
        };
        dispatch(
            root,
            remote,
            store,
            operation,
            choice,
            dry_run,
            deletions,
            &mut report,
        )?;
    }
    for (_, local) in local_files {
        if paused()? {
            report.paused = true;
            return Ok((report, false));
        }
        if drive_api::is_reference_path(&local.path) {
            report.blocked.push(format!("Referência Google sem vínculo: {}. Abra no navegador ou remova a cópia local antes de sincronizar.",local.path));
            continue;
        }
        // A tracked file moved remotely may now occupy this path; never overwrite.
        if claimed.contains(&local.path) {
            report.conflicts.push(local.path.clone());
            continue;
        }
        let id = if dry_run {
            format!("unreserved-{}", Uuid::new_v4())
        } else {
            remote.reserve_id()?
        };
        let operation = Operation {
            operation_id: Uuid::new_v4().to_string(),
            object_id: id,
            kind: Kind::Upload,
            local: Some(local),
            remote: None,
            base: None,
        };
        dispatch(
            root,
            remote,
            store,
            operation,
            Choice::Act(Kind::Upload),
            dry_run,
            deletions,
            &mut report,
        )?;
    }
    Ok((report, false))
}
#[derive(Debug)]
struct PauseRequested;
impl std::fmt::Display for PauseRequested {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Pausa solicitada")
    }
}
impl std::error::Error for PauseRequested {}
fn blocked_path(path: &str, prefixes: &[String]) -> bool {
    prefixes.iter().any(|prefix| {
        path == prefix
            || path
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with('/'))
    })
}
#[allow(clippy::too_many_arguments)]
fn synchronize_directories(
    root: &Root,
    remote: &mut impl Remote,
    store: &mut Store,
    local: &std::collections::BTreeMap<String, Entry>,
    files: &std::collections::BTreeMap<String, Entry>,
    links: &[Link],
    dry_run: bool,
    deletions: bool,
    report: &mut Report,
    paused: &dyn Fn() -> Result<bool>,
) -> Result<(bool, Vec<String>)> {
    struct Candidate {
        base: Option<Link>,
        id: Option<String>,
        local: Option<Entry>,
        remote: Option<Entry>,
        choice: Choice,
    }
    let mut candidates = Vec::new();
    let mut local_claimed = BTreeSet::new();
    let mut remote_claimed = BTreeSet::new();
    for base in links {
        let l = local
            .values()
            .find(|e| e.identity == base.local.identity)
            .or_else(|| local.get(&base.local.path))
            .cloned();
        let r = files.get(&base.id).cloned();
        if !base.local.directory
            && !base.remote.directory
            && !l.as_ref().is_some_and(|e| e.directory)
            && !r.as_ref().is_some_and(|e| e.directory)
        {
            continue;
        }
        if let Some(l) = &l {
            local_claimed.insert(l.path.clone());
        }
        remote_claimed.insert(base.id.clone());
        let trashed = r.is_none() && matches!(remote.status(&base.id)?, Status::Trashed);
        let choice = reconcile(Some(base), l.as_ref(), r.as_ref(), trashed);
        candidates.push(Candidate {
            base: Some(base.clone()),
            id: Some(base.id.clone()),
            local: l,
            remote: r,
            choice,
        });
    }
    for (id, entry) in files {
        if !entry.directory || remote_claimed.contains(id) {
            continue;
        }
        let l = local.get(&entry.path).cloned();
        if let Some(l) = &l {
            local_claimed.insert(l.path.clone());
        }
        remote_claimed.insert(id.clone());
        let choice = reconcile(None, l.as_ref(), Some(entry), false);
        candidates.push(Candidate {
            base: None,
            id: Some(id.clone()),
            local: l,
            remote: Some(entry.clone()),
            choice,
        });
    }
    for entry in local.values() {
        if !entry.directory || local_claimed.contains(&entry.path) {
            continue;
        }
        let r = files.iter().find(|(_, r)| r.path == entry.path);
        let choice = reconcile(None, Some(entry), r.map(|(_, r)| r), false);
        candidates.push(Candidate {
            base: None,
            id: r.map(|(id, _)| id.clone()),
            local: Some(entry.clone()),
            remote: r.map(|(_, r)| r.clone()),
            choice,
        });
    }
    let target_path = |c: &Candidate| -> String {
        let target = if c.choice == Choice::Act(Kind::Download) {
            c.remote.as_ref()
        } else {
            c.local.as_ref().or(c.remote.as_ref())
        };
        target
            .map(|e| e.path.clone())
            .or_else(|| c.base.as_ref().map(|b| b.local.path.clone()))
            .unwrap_or_default()
    };
    candidates.sort_by_key(|c| {
        let path = target_path(c);
        (path.matches('/').count(), path, c.id.clone())
    });
    let mut blocked = Vec::new();
    for candidate in candidates {
        if paused()? {
            return Err(PauseRequested.into());
        }
        let path = target_path(&candidate);
        if blocked_path(&path, &blocked) {
            continue;
        }
        let supported = matches!(
            candidate.choice,
            Choice::Act(Kind::Upload | Kind::Download | Kind::Checkpoint | Kind::Forget)
        );
        if !supported || (candidate.choice == Choice::Act(Kind::Forget) && !deletions) {
            report.blocked.push(format!(
                "Pasta bloqueada (conflito, acesso ou exclusão de árvore): {path}"
            ));
            blocked.push(path);
            if let Some(base) = &candidate.base {
                blocked.push(base.local.path.clone());
                blocked.push(base.remote.path.clone());
            }
            if let Some(remote) = &candidate.remote {
                blocked.push(remote.path.clone());
            }
            continue;
        }
        let id = match candidate.id {
            Some(id) => id,
            None if dry_run => "unreserved-directory".into(),
            None => remote.reserve_id()?,
        };
        let kind = match candidate.choice {
            Choice::Act(kind) => kind,
            _ => continue,
        };
        let operation = Operation {
            operation_id: Uuid::new_v4().to_string(),
            object_id: id,
            kind,
            local: candidate.local,
            remote: candidate.remote,
            base: candidate.base,
        };
        let before = report.completed;
        dispatch(
            root,
            remote,
            store,
            operation,
            candidate.choice,
            dry_run,
            deletions,
            report,
        )?;
        if !dry_run && report.completed > before && matches!(kind, Kind::Upload | Kind::Download) {
            return Ok((true, blocked));
        }
    }
    Ok((false, blocked))
}
#[allow(clippy::too_many_arguments)]
fn dispatch(
    root: &Root,
    remote: &mut impl Remote,
    store: &mut Store,
    mut op: Operation,
    choice: Choice,
    dry_run: bool,
    deletions: bool,
    report: &mut Report,
) -> Result<()> {
    let path = op
        .local
        .as_ref()
        .or(op.remote.as_ref())
        .map(|e| e.path.clone())
        .unwrap_or_else(|| op.object_id.clone());
    match choice {
        Choice::Conflict => report.conflicts.push(path),
        Choice::Unknown => report
            .blocked
            .push(format!("Sem evidência de exclusão ou sem acesso: {path}")),
        Choice::Act(kind) => {
            if matches!(
                kind,
                Kind::TrashRemote | Kind::QuarantineLocal | Kind::Forget
            ) && !deletions
            {
                report
                    .blocked
                    .push(format!("Propagação de exclusões desativada: {path}"));
                return Ok(());
            }
            if kind == Kind::Checkpoint
                && op.base.as_ref().is_some_and(|b| {
                    Some(&b.local) == op.local.as_ref() && Some(&b.remote) == op.remote.as_ref()
                })
            {
                return Ok(());
            }
            op.kind = kind;
            report.planned.push(format!("{kind:?}: {path}"));
            if !dry_run {
                store.stage(&op)?;
                match execute(
                    root,
                    remote,
                    store,
                    &Intent {
                        operation: op,
                        running: false,
                    },
                ) {
                    Ok(()) => report.completed += 1,
                    Err(e) => {
                        report.blocked.push(format!("{path}: {e:#}"));
                        return Err(e.context(format!(
                            "Ciclo interrompido para preservar operação pendente: {path}"
                        )));
                    }
                }
            }
        }
    }
    Ok(())
}
fn observed(
    root: &Root,
    remote: &mut impl Remote,
    op: &Operation,
) -> Result<(Option<Entry>, Status)> {
    let target = match op.kind {
        Kind::Download | Kind::KeepBoth => op.remote.as_ref(),
        _ => op.local.as_ref(),
    };
    let local = if let Some(target) = target {
        root.inspect(&target.path)?
    } else {
        None
    };
    Ok((local, remote.status(&op.object_id)?))
}
fn completed_link(op: &Operation, local: Entry, remote: Entry) -> Link {
    Link {
        id: op.object_id.clone(),
        local,
        remote,
    }
}
fn execute_inner(
    root: &Root,
    remote: &mut impl Remote,
    store: &mut Store,
    intent: &Intent,
) -> Result<()> {
    let op = &intent.operation;
    if matches!(
        op.kind,
        Kind::TrashRemote | Kind::QuarantineLocal | Kind::KeepBoth
    ) {
        ensure!(
            !op.local.as_ref().is_some_and(|e| e.directory)
                && !op.remote.as_ref().is_some_and(|e| e.directory),
            "Efeito de exclusão/conflito de diretório não suportado"
        );
    }
    // Inspect first: an interrupted effect may already be complete.
    let (current_local, current_remote) = observed(root, remote, op)?;
    let remote_entry = status_entry(current_remote.clone());
    match op.kind {
        Kind::Upload => {
            let desired = op.local.as_ref().context("Upload sem origem")?;
            if let Some(r) = &remote_entry
                && r.same_file(desired)
            {
                let link = completed_link(op, desired.clone(), r.clone());
                store.finish(op, Some(&link), "upload verificado")?;
                return Ok(());
            }
        }
        Kind::Download | Kind::KeepBoth => {
            let desired = op.remote.as_ref().context("Download sem origem")?;
            if let Some(l) = &current_local {
                let copy_ready = if op.kind == Kind::KeepBoth {
                    let copy = conflict_copy(op)?;
                    root.inspect(&copy.path)?
                        .is_some_and(|e| e.same_file(&copy))
                } else {
                    true
                };
                let identity_matches = !desired.directory
                    || op
                        .local
                        .as_ref()
                        .is_none_or(|old| old.identity == l.identity);
                if l.same_file(desired) && copy_ready && identity_matches {
                    let link = completed_link(op, l.clone(), desired.clone());
                    store.finish(op, Some(&link), "download verificado")?;
                    return Ok(());
                }
            }
        }
        Kind::TrashRemote if matches!(current_remote, Status::Trashed) => {
            store.finish(op, None, "arquivo remoto na lixeira")?;
            return Ok(());
        }
        Kind::QuarantineLocal if current_local.is_none() => {
            // Missing name alone is insufficient; verify the recovery copy.
            root.quarantine(
                &op.operation_id,
                op.local.as_ref().context("Quarentena sem origem")?,
            )?;
            store.finish(op, None, "arquivo local preservado em recuperação")?;
            return Ok(());
        }
        _ => {}
    }
    let old_local = if let Some(old) = &op.local {
        root.inspect(&old.path)?
    } else {
        if let Some(target) = &op.remote {
            root.inspect(&target.path)?
        } else {
            None
        }
    };
    let mut local_matches = old_local == op.local;
    if !local_matches
        && intent.running
        && matches!(op.kind, Kind::Download | Kind::KeepBoth)
        && old_local.is_none()
        && let Some(old) = &op.local
    {
        // A prior attempt may have retained the old name before installing.
        root.quarantine(&op.operation_id, old)?;
        local_matches = true;
    }
    let remote_matches = match (&op.remote, &current_remote) {
        (Some(expected), Status::Present(actual)) => expected == actual,
        (None, Status::Unknown) => op.base.is_none(),
        (None, Status::Trashed) => true,
        _ => false,
    };
    if !(local_matches && remote_matches) {
        if !intent.running {
            store.abandon_unstarted(op)?;
        }
        bail!(
            "Precondições mudaram; nenhum efeito repetido. Revise o conflito e a pasta de recuperação."
        );
    }
    if !intent.running {
        store.mark_running(&op.operation_id)?;
    }
    match op.kind {
        Kind::Upload => {
            let local = op.local.as_ref().context("Origem ausente")?;
            let uploaded = if local.directory {
                remote.put_directory(&op.object_id, op.remote.as_ref(), local)?
            } else {
                let source = root.spool(&op.operation_id, local)?;
                remote.upload(&op.object_id, op.remote.as_ref(), local, source)?
            };
            let link = completed_link(op, local.clone(), uploaded);
            store.finish(op, Some(&link), "upload concluído")?;
        }
        Kind::KeepBoth => {
            let copy = conflict_copy(op)?;
            if let Some(existing) = root.inspect(&copy.path)? {
                ensure!(
                    existing.same_file(&copy),
                    "Nome da cópia de conflito já ocupado"
                );
            } else {
                let original = op.local.as_ref().context("Conflito sem origem local")?;
                let source = root.spool(&op.operation_id, original)?;
                root.discard_partial_download(&op.operation_id)?;
                root.install(&op.operation_id, &copy, None, source)?;
            }
            let target = op.remote.as_ref().context("Conflito sem origem remota")?;
            root.discard_partial_download(&op.operation_id)?;
            let source = remote.download(&op.object_id, target)?;
            let installed = root.install(&op.operation_id, target, op.local.as_ref(), source)?;
            let link = completed_link(op, installed, target.clone());
            store.finish(
                op,
                Some(&link),
                "ambas as versões preservadas; cópia local será enviada no próximo ciclo",
            )?;
        }
        Kind::Download => {
            let target = op.remote.as_ref().context("Origem remota ausente")?;
            let installed = if target.directory {
                root.install_directory(target, op.local.as_ref())?
            } else if drive_api::is_reference_path(&target.path) {
                root.discard_partial_download(&op.operation_id)?;
                let source = remote.download(&op.object_id, target)?;
                root.install(&op.operation_id, target, op.local.as_ref(), source)?
            } else {
                let offset = root.partial_download_len(&op.operation_id, target)?;
                let source: Box<dyn std::io::Read> = if offset == target.size {
                    Box::new(std::io::empty())
                } else {
                    remote.download_from(&op.object_id, target, offset)?
                };
                root.install_resuming(&op.operation_id, target, op.local.as_ref(), offset, source)?
            };
            let link = completed_link(op, installed, target.clone());
            store.finish(op, Some(&link), "download concluído")?;
        }
        Kind::TrashRemote => {
            remote.trash(
                &op.object_id,
                op.remote.as_ref().context("Origem remota ausente")?,
            )?;
            store.finish(op, None, "remoto enviado à lixeira")?;
        }
        Kind::QuarantineLocal => {
            root.quarantine(
                &op.operation_id,
                op.local.as_ref().context("Origem local ausente")?,
            )?;
            store.finish(op, None, "local preservado em recuperação")?;
        }
        Kind::Checkpoint => {
            let local = op.local.clone().context("Checkpoint sem estado local")?;
            let remote = op.remote.clone().context("Checkpoint sem estado remoto")?;
            ensure!(local.same_file(&remote), "Checkpoint divergente");
            let link = completed_link(op, local, remote);
            store.finish(op, Some(&link), "estado confirmado")?;
        }
        Kind::Forget => {
            ensure!(
                matches!(current_remote, Status::Trashed) && current_local.is_none(),
                "Exclusão não confirmada"
            );
            store.finish(op, None, "exclusão confirmada nos dois lados")?;
        }
    }
    Ok(())
}

impl Paths {
    pub fn set_paused(&self, paused: bool) -> Result<()> {
        let config = self.load()?;
        let dir = self.profile_dir(&config)?;
        let flag = dir.join("paused");
        if paused {
            OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(flag)?
                .sync_all()?;
        } else {
            match std::fs::remove_file(flag) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
    pub fn paused(&self) -> Result<bool> {
        let config = self.load()?;
        Ok(self.profile_dir(&config)?.join("paused").exists())
    }
    pub fn status_json(&self) -> Result<serde_json::Value> {
        let config = self.load()?;
        let dir = self.profile_dir(&config)?;
        let last = std::fs::read(dir.join("status.json"))
            .ok()
            .and_then(|v| serde_json::from_slice::<serde_json::Value>(&v).ok());
        Ok(
            serde_json::json!({"configured":true,"local":config.local,"remote_id":config.remote_id,"paused":self.paused()?,"last_cycle":last}),
        )
    }
    pub fn cycle(&self, dry_run: bool) -> Result<Report> {
        self.cycle_with_progress(dry_run, None)
    }
    pub fn cycle_with_progress(
        &self,
        dry_run: bool,
        progress: Option<drive_api::ProgressSink>,
    ) -> Result<Report> {
        if !dry_run {
            ensure!(!self.paused()?, "Sincronização pausada");
        }
        let result = self.run_with_progress(dry_run, progress);
        if !dry_run {
            let config = self.load()?;
            let dir = self.profile_dir(&config)?;
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs();
            let status = match &result {
                Ok(report) => serde_json::json!({"unix_time":timestamp,"report":report}),
                Err(e) => serde_json::json!({"unix_time":timestamp,"error":format!("{e:#}")}),
            };
            let temp = dir.join(format!("status-{}.tmp", Uuid::new_v4()));
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temp)?;
            file.write_all(&serde_json::to_vec_pretty(&status)?)?;
            file.sync_all()?;
            std::fs::rename(temp, dir.join("status.json"))?;
        }
        result
    }
}

fn conflict_copy(op: &Operation) -> Result<Entry> {
    let mut entry = op.local.clone().context("Conflito sem versão local")?;
    entry.path = format!("{}.conflict-{}", entry.path, op.operation_id);
    Ok(entry)
}
/// Explicit user command. Preserves the local version under a new visible name,
/// then installs the remote version at its projected path, retaining a recovery copy.
pub fn resolve_both(
    root: &Root,
    remote: &mut impl Remote,
    store: &mut Store,
    path: &str,
) -> Result<()> {
    ensure!(
        store.intents()?.is_empty(),
        "Resolva as operações pendentes antes de resolver conflitos"
    );
    let remote_files = remote.scan()?;
    let local_files = root.scan()?;
    let links = store.links()?;
    let base = links
        .iter()
        .find(|b| b.local.path == path || remote_files.get(&b.id).is_some_and(|e| e.path == path));
    let id = if let Some(base) = base {
        base.id.clone()
    } else {
        remote_files
            .iter()
            .find(|(_, e)| e.path == path)
            .map(|(id, _)| id.clone())
            .context("Arquivo remoto não encontrado")?
    };
    let remote_entry = remote_files
        .get(&id)
        .context("Arquivo remoto indisponível")?
        .clone();
    let local = base
        .and_then(|b| {
            local_files
                .values()
                .find(|e| e.identity == b.local.identity)
        })
        .or_else(|| local_files.get(path))
        .context("Versão local não encontrada")?
        .clone();
    ensure!(
        !local.directory && !remote_entry.directory,
        "Resolva conflitos de pastas por renomeação explícita; KeepBoth aceita somente arquivos"
    );
    ensure!(
        reconcile(base, Some(&local), Some(&remote_entry), false) == Choice::Conflict
            || (drive_api::is_reference_path(&local.path) && !local.same_content(&remote_entry)),
        "Este item não tem um conflito de conteúdo pendente"
    );
    let op = Operation {
        operation_id: Uuid::new_v4().to_string(),
        object_id: id,
        kind: Kind::KeepBoth,
        local: Some(local),
        remote: Some(remote_entry),
        base: base.cloned(),
    };
    store.stage(&op)?;
    execute(
        root,
        remote,
        store,
        &Intent {
            operation: op,
            running: false,
        },
    )
}
impl Paths {
    pub fn keep_both(&self, path: &str) -> Result<()> {
        let config = self.load()?;
        let dir = self.profile_dir(&config)?;
        let _lock = lock(&dir)?;
        ensure!(
            self.load()?.profile == config.profile,
            "A configuração mudou; execute novamente com o perfil atual"
        );
        let root = Root::open(&config.local, Some(&config.root_identity))?;
        let mut store = Store::open(dir.join("state.sqlite"))?;
        store.init_mirror()?;
        let mut remote = self.drive(&config)?;
        resolve_both(&root, &mut remote, &mut store, path)
    }
}

impl Paths {
    /// Explicit recovery command; never retires a running create whose remote ID
    /// cannot be inspected. Old base is retained, so ambiguities become conflicts.
    pub fn replan_pending(&self) -> Result<usize> {
        let config = self.load()?;
        let dir = self.profile_dir(&config)?;
        let _lock = lock(&dir)?;
        ensure!(
            self.load()?.profile == config.profile,
            "A configuração mudou; execute novamente com o perfil atual"
        );
        let root = Root::open(&config.local, Some(&config.root_identity))?;
        let mut store = Store::open(dir.join("state.sqlite"))?;
        store.init_mirror()?;
        let mut remote = self.drive(&config)?;
        remote.scan()?;
        root.scan()?;
        let mut count = 0;
        for intent in store.intents()? {
            if !intent.running {
                store.abandon_unstarted(&intent.operation)?;
                count += 1;
                continue;
            }
            if execute(&root, &mut remote, &mut store, &intent).is_ok() {
                count += 1;
                continue;
            }
            let op = &intent.operation;
            let status = remote.status(&op.object_id)?;
            ensure!(
                !matches!(status, Status::Unknown),
                "Sem acesso ao ID pendente; mantenha o journal e reconecte a conta"
            );
            if op.base.is_none() && op.kind == Kind::Upload {
                // A reserved ID now exists; associate it instead of generating a
                // second object. The old intended state remains the merge base.
                let desired = op.local.clone().context("Criação sem estado local")?;
                let link = Link {
                    id: op.object_id.clone(),
                    local: desired.clone(),
                    remote: desired,
                };
                store.finish(
                    op,
                    Some(&link),
                    "ID criado associado; divergências serão replanejadas como conflito",
                )?;
            } else {
                store.retire(
                    op,
                    "replanejamento solicitado; base anterior conservada e arquivos preservados",
                )?;
            }
            count += 1;
        }
        Ok(count)
    }
}

fn execute(
    root: &Root,
    remote: &mut impl Remote,
    store: &mut Store,
    intent: &Intent,
) -> Result<()> {
    execute_inner(root, remote, store, intent)?;
    root.cleanup_upload(&intent.operation.operation_id)?;
    Ok(())
}
