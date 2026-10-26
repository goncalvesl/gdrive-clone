//! Google Drive v3 adapter. Tokens never appear in errors or persisted metadata.
pub mod oauth;
mod progress;
use anyhow::{Context, Result, ensure};
use md5::{Digest, Md5};
use progress::ProgressReader;
pub use progress::{ProgressSink, TransferProgress};
use reqwest::{
    Method,
    blocking::{Client, Response},
    header::{CONTENT_RANGE, ETAG, IF_MATCH, LOCATION, RANGE},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    time::Duration,
};
use sync_core::mirror::{Entry, UploadSession};

#[derive(Debug)]
pub enum ApiFailure {
    Network(&'static str),
    Http {
        status: u16,
        retry_after: Option<u64>,
    },
}
impl std::fmt::Display for ApiFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(message) => f.write_str(message),
            Self::Http { status, .. } => write!(
                f,
                "Serviço Google retornou HTTP {status}; nenhuma exclusão será inferida desse erro"
            ),
        }
    }
}
impl std::error::Error for ApiFailure {}
pub fn retry_minimum(error: &anyhow::Error) -> Option<Duration> {
    let failure = error.chain().find_map(|cause| {
        cause.downcast_ref::<ApiFailure>().or_else(|| {
            cause
                .downcast_ref::<io::Error>()
                .and_then(io::Error::get_ref)
                .and_then(|inner| inner.downcast_ref::<ApiFailure>())
        })
    })?;
    match failure {
        ApiFailure::Network(_) => Some(Duration::ZERO),
        ApiFailure::Http {
            status,
            retry_after,
        } if *status == 429 || (500..=599).contains(status) => {
            Some(Duration::from_secs(retry_after.unwrap_or(0)))
        }
        _ => None,
    }
}
struct NetworkReader<R>(R);
impl<R: Read> Read for NetworkReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.0.read(buffer).map_err(|_| {
            io::Error::other(ApiFailure::Network(
                "Download interrompido; bytes parciais preservados para retomada",
            ))
        })
    }
}
const API: &str = "https://www.googleapis.com/drive/v3";
const UPLOAD: &str = "https://www.googleapis.com/upload/drive/v3";
const FIELDS: &str = "id,name,mimeType,parents,md5Checksum,size,version,trashed";
#[derive(Clone, Debug)]
pub enum Status {
    Present(Entry),
    Trashed,
    Unknown,
}
pub trait Remote {
    fn set_progress(&mut self, _sink: Option<ProgressSink>) {}

    fn scan(&mut self) -> Result<BTreeMap<String, Entry>>;
    fn status(&mut self, id: &str) -> Result<Status>;
    fn reserve_id(&mut self) -> Result<String>;
    fn download(&mut self, id: &str, expected: &Entry) -> Result<Box<dyn Read>>;
    fn download_from(&mut self, id: &str, expected: &Entry, offset: u64) -> Result<Box<dyn Read>> {
        if offset == 0 {
            self.download(id, expected)
        } else {
            anyhow::bail!("Adaptador não oferece retomada de download")
        }
    }
    fn upload(
        &mut self,
        id: &str,
        expected: Option<&Entry>,
        target: &Entry,
        source: File,
    ) -> Result<Entry>;
    fn put_directory(
        &mut self,
        _id: &str,
        _expected: Option<&Entry>,
        _target: &Entry,
    ) -> Result<Entry> {
        anyhow::bail!("Adaptador não oferece operações de diretório")
    }
    fn trash(&mut self, id: &str, expected: &Entry) -> Result<()>;
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    id: String,
    name: String,
    mime_type: String,
    #[serde(default)]
    parents: Vec<String>,
    md5_checksum: Option<String>,
    size: Option<String>,
    version: Option<String>,
    #[serde(default)]
    trashed: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    #[serde(default)]
    files: Vec<Meta>,
    next_page_token: Option<String>,
    #[serde(default)]
    incomplete_search: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChangePage {
    #[serde(default)]
    changes: Vec<Change>,
    next_page_token: Option<String>,
    new_start_page_token: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Change {
    file_id: Option<String>,
    change_type: Option<String>,
    #[serde(default)]
    removed: bool,
    file: Option<Meta>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AccountInfo {
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub usage: Option<u64>,
    pub limit: Option<u64>,
}
pub trait UploadSessionStore: Send {
    fn load(&mut self, object_id: &str) -> Result<Option<UploadSession>>;
    fn save(&mut self, object_id: &str, session: &UploadSession) -> Result<()>;
    fn clear(&mut self, object_id: &str) -> Result<()>;
}
pub trait ChangeStateStore: Send {
    fn load(&mut self) -> Result<Option<(String, BTreeMap<String, Entry>)>>;
    fn advance(&mut self, cursor: &str) -> Result<()>;
    fn update(&mut self, cursor: &str, updates: &BTreeMap<String, Option<Entry>>) -> Result<()>;
    fn save(&mut self, cursor: &str, inventory: &BTreeMap<String, Entry>) -> Result<()>;
}
pub struct Drive {
    upload_sessions: Option<Box<dyn UploadSessionStore>>,
    change_state: Option<Box<dyn ChangeStateStore>>,
    progress: Option<ProgressSink>,
    client: Client,
    session: oauth::Session,
    root: String,
    paths: BTreeMap<String, String>,
    directories: BTreeMap<String, String>,
    base_url: String,
    upload_url: String,
}
impl Drive {
    pub fn new(profile: String, credentials: oauth::Credentials, root: String) -> Result<Self> {
        ensure!(!root.is_empty(), "Escolha uma pasta remota");
        let client = Client::builder()
            .timeout(Duration::from_secs(300))
            .connect_timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            upload_sessions: None,
            change_state: None,
            progress: None,
            client,
            session: oauth::Session::new(profile, credentials),
            root,
            paths: BTreeMap::new(),
            directories: BTreeMap::new(),
            base_url: API.into(),
            upload_url: UPLOAD.into(),
        })
    }
    pub fn set_upload_sessions(&mut self, sessions: Box<dyn UploadSessionStore>) {
        self.upload_sessions = Some(sessions);
    }
    pub fn set_change_state(&mut self, state: Box<dyn ChangeStateStore>) {
        self.change_state = Some(state);
    }
    fn validate_upload_url(&self, location: &str) -> Result<()> {
        let parsed =
            url::Url::parse(location).map_err(|_| anyhow::anyhow!("URL de sessão inválida"))?;
        let allowed = parsed.scheme() == "https"
            && parsed.port().is_none_or(|port| port == 443)
            && parsed.host_str().is_some_and(|host| {
                host == "www.googleapis.com" || host == "content.googleapis.com"
            });
        #[cfg(test)]
        let allowed = allowed
            || (parsed.scheme() == "http"
                && parsed.host_str() == Some("127.0.0.1")
                && location.starts_with(&format!("{}/", self.base_url)));
        ensure!(
            allowed
                && parsed.username().is_empty()
                && parsed.password().is_none()
                && parsed.fragment().is_none(),
            "Destino inesperado de upload"
        );
        Ok(())
    }
    fn finish_upload(&mut self, id: &str, target: &Entry, response: Response) -> Result<Entry> {
        let meta: Meta = Self::checked(response)?.json().map_err(|_| {
            anyhow::anyhow!("Resposta final do upload inválida; intenção preservada para inspeção")
        })?;
        let result = entry(
            &meta,
            self.project_path(&meta)?
                .context("Resultado do upload fora da raiz")?,
        )?;
        ensure!(
            meta.id == id && !meta.trashed && !result.directory && result.same_file(target),
            "Resposta final do upload diverge da intenção"
        );
        self.paths.insert(id.into(), target.path.clone());
        // Persistent session removal belongs to Store::finish with the link transaction.
        Ok(result)
    }
    fn send_upload_chunks(
        &mut self,
        id: &str,
        target: &Entry,
        source: &mut File,
        session: &mut UploadSession,
        tag: Option<&str>,
    ) -> Result<Entry> {
        self.validate_upload_url(&session.url)?;
        const CHUNK: u64 = 8 * 1024 * 1024;
        let mut offset = session.acknowledged;
        loop {
            if offset >= target.size && target.size > 0 {
                return Err(ApiFailure::Network(
                    "Bytes recebidos pelo servidor; aguardando confirmação final da sessão",
                )
                .into());
            }
            let length = CHUNK.min(target.size.saturating_sub(offset));
            source.seek(SeekFrom::Start(offset))?;
            let mut bytes = vec![0_u8; length as usize];
            source
                .read_exact(&mut bytes)
                .context("Snapshot de upload incompleto")?;
            let body = ProgressReader::new_at(
                std::io::Cursor::new(bytes),
                &target.path,
                true,
                target.size,
                self.progress.clone(),
                offset,
            );
            let mut request = self
                .request(Method::PUT, session.url.clone())?
                .header("Content-Type", "application/octet-stream")
                .header("Content-Length", length)
                .body(reqwest::blocking::Body::sized(body, length));
            if length > 0 {
                request = request.header(
                    "Content-Range",
                    format!("bytes {}-{}/{}", offset, offset + length - 1, target.size),
                );
            }
            if let Some(tag) = tag {
                request = request.header(IF_MATCH, tag);
            }
            let response = request.send().map_err(|_| {
                ApiFailure::Network(
                    "Upload interrompido; a sessão persistida será consultada antes de repetir",
                )
            })?;
            if response.status().as_u16() != 308 {
                return self.finish_upload(id, target, response);
            }
            let acknowledged = acknowledged_bytes(&response, target.size)?;
            ensure!(
                acknowledged > offset && acknowledged <= offset + length,
                "Range remoto inválido ou sem progresso"
            );
            offset = acknowledged;
            session.acknowledged = acknowledged;
            if let Some(store) = &mut self.upload_sessions {
                store.save(id, session)?;
            }
        }
    }
    pub fn account_info(&mut self) -> Result<AccountInfo> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct User {
            display_name: Option<String>,
            email_address: Option<String>,
        }
        #[derive(Deserialize)]
        struct Quota {
            usage: Option<String>,
            limit: Option<String>,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct About {
            user: Option<User>,
            storage_quota: Option<Quota>,
        }
        let response = self
            .request(Method::GET, format!("{}/about", self.base_url))?
            .query(&[(
                "fields",
                "user(displayName,emailAddress),storageQuota(usage,limit)",
            )])
            .send()
            .map_err(|_| ApiFailure::Network("Não foi possível consultar dados da conta"))?;
        let about: About = Self::checked(response)?
            .json()
            .context("Dados da conta inválidos")?;
        let user = about.user.context("Drive não retornou dados do usuário")?;
        let usage = about
            .storage_quota
            .as_ref()
            .and_then(|q| q.usage.as_ref())
            .map(|v| v.parse())
            .transpose()
            .context("Uso de armazenamento inválido")?;
        let limit = about
            .storage_quota
            .as_ref()
            .and_then(|q| q.limit.as_ref())
            .map(|v| v.parse())
            .transpose()
            .context("Limite de armazenamento inválido")?;
        Ok(AccountInfo {
            display_name: user.display_name,
            email: user.email_address,
            usage,
            limit,
        })
    }
    fn request(
        &mut self,
        method: Method,
        url: String,
    ) -> Result<reqwest::blocking::RequestBuilder> {
        let token = self.session.bearer(&self.client)?;
        Ok(self.client.request(method, url).bearer_auth(token))
    }
    fn checked(response: Response) -> Result<Response> {
        let status = response.status();
        if !status.is_success() {
            let retry_after = response
                .headers()
                .get("Retry-After")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse().ok());
            return Err(ApiFailure::Http {
                status: status.as_u16(),
                retry_after,
            }
            .into());
        }
        Ok(response)
    }
    fn metadata(&mut self, id: &str) -> Result<Option<(Meta, Option<String>)>> {
        let url = format!("{}/files/{}", self.base_url, escape_id(id)?);
        let response = self
            .request(Method::GET, url)?
            .query(&[("fields", FIELDS)])
            .send()
            .map_err(|_| ApiFailure::Network("Falha de conexão ao consultar arquivo"))?;
        if matches!(response.status().as_u16(), 403 | 404) {
            return Ok(None);
        }
        let response = Self::checked(response)?;
        let etag = response
            .headers()
            .get(ETAG)
            .and_then(|s| s.to_str().ok())
            .map(str::to_string);
        Ok(Some((
            response.json().context("Metadados inválidos do Drive")?,
            etag,
        )))
    }
    fn project_path(&self, meta: &Meta) -> Result<Option<String>> {
        let Some(parent) = meta.parents.first() else {
            return Ok(None);
        };
        let prefix = if parent == &self.root {
            Some(String::new())
        } else {
            self.directories
                .iter()
                .find(|(_, v)| *v == parent)
                .map(|(p, _)| p.clone())
        };
        let Some(prefix) = prefix else {
            return Ok(None);
        };
        validate_name(&meta.name)?;
        Ok(Some(if prefix.is_empty() {
            projected_name(meta)
        } else {
            format!("{prefix}/{}", projected_name(meta))
        }))
    }
    fn children(&mut self, parent: &str) -> Result<Vec<Meta>> {
        escape_id(parent)?;
        let mut files = Vec::new();
        let mut token = String::new();
        let mut seen = BTreeSet::new();
        loop {
            ensure!(seen.insert(token.clone()), "Paginação repetida pelo Drive");
            let url = format!("{}/files", self.base_url);
            let mut request = self.request(Method::GET, url)?.query(&[
                ("q", format!("'{parent}' in parents and trashed = false")),
                (
                    "fields",
                    format!("nextPageToken,incompleteSearch,files({FIELDS})"),
                ),
                ("pageSize", "1000".into()),
            ]);
            if !token.is_empty() {
                request = request.query(&[("pageToken", token.as_str())]);
            }
            let page: Page = Self::checked(
                request
                    .send()
                    .map_err(|_| ApiFailure::Network("Falha de conexão durante inventário"))?,
            )?
            .json()
            .context("Inventário remoto inválido")?;
            ensure!(
                !page.incomplete_search,
                "Google retornou inventário incompleto"
            );
            files.extend(page.files);
            match page.next_page_token {
                Some(next) => token = next,
                None => break,
            }
        }
        Ok(files)
    }
    fn start_page_token(&mut self) -> Result<String> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct StartToken {
            start_page_token: String,
        }
        let response = self
            .request(
                Method::GET,
                format!("{}/changes/startPageToken", self.base_url),
            )?
            .query(&[("fields", "startPageToken")])
            .send()
            .map_err(|_| ApiFailure::Network("Falha ao obter cursor inicial de mudanças"))?;
        let token: StartToken = Self::checked(response)?
            .json()
            .context("Cursor inicial de mudanças inválido")?;
        ensure!(!token.start_page_token.is_empty(), "Cursor inicial vazio");
        Ok(token.start_page_token)
    }
    fn list_changes(&mut self, cursor: &str) -> Result<(Vec<Change>, String)> {
        let mut token = cursor.to_owned();
        let mut seen = BTreeSet::new();
        let mut changes = Vec::new();
        for _ in 0..10_000 {
            ensure!(
                seen.insert(token.clone()),
                "Paginação repetida no log de mudanças"
            );
            let response = self
                .request(Method::GET, format!("{}/changes", self.base_url))?
                .query(&[
                    ("pageToken", token.as_str()),
                    ("spaces", "drive"),
                    ("includeRemoved", "true"),
                    ("pageSize", "1000"),
                    ("fields", "nextPageToken,newStartPageToken,changes(fileId,removed,file(id,name,mimeType,parents,md5Checksum,size,version,trashed))"),
                ])
                .send()
                .map_err(|_| ApiFailure::Network("Falha ao consultar mudanças do Drive"))?;
            let page: ChangePage = Self::checked(response)?
                .json()
                .context("Página de mudanças inválida")?;
            changes.extend(page.changes);
            match page.next_page_token {
                Some(next) if !next.is_empty() => token = next,
                Some(_) => anyhow::bail!("Token de continuação vazio"),
                None => {
                    let next = page
                        .new_start_page_token
                        .context("Última página não trouxe novo cursor")?;
                    ensure!(!next.is_empty(), "Novo cursor de mudanças vazio");
                    return Ok((changes, next));
                }
            }
        }
        anyhow::bail!("Log de mudanças excede limite de páginas")
    }
    fn seed_directory_paths(&mut self, inventory: &BTreeMap<String, Entry>) {
        self.paths.clear();
        self.directories.clear();
        for (id, entry) in inventory {
            self.paths.insert(id.clone(), entry.path.clone());
            if entry.directory {
                self.directories.insert(entry.path.clone(), id.clone());
            }
        }
    }
    /// Applies safe file events and in-tree directory moves to a cache.
    fn apply_file_changes(
        &mut self,
        inventory: &mut BTreeMap<String, Entry>,
        changes: Vec<Change>,
    ) -> Result<Option<BTreeMap<String, Option<Entry>>>> {
        self.seed_directory_paths(inventory);
        let mut updates = BTreeMap::new();
        for change in changes {
            if change.change_type.as_deref() == Some("drive") {
                return Ok(None);
            }
            let Some(id) = change.file_id else {
                return Ok(None);
            };
            escape_id(&id)?;
            let was_directory = inventory.get(&id).is_some_and(|entry| entry.directory);
            let Some(meta) = change.file else {
                if change.removed {
                    if was_directory {
                        return Ok(None);
                    }
                    inventory.remove(&id);
                    updates.insert(id, None);
                    continue;
                }
                return Ok(None);
            };
            if meta.id != id {
                return Ok(None);
            }
            for parent in &meta.parents {
                escape_id(parent)?;
            }
            if meta.mime_type == "application/vnd.google-apps.folder" {
                if !was_directory || change.removed || meta.trashed {
                    // A new/deleted folder may bring or hide an entire subtree.
                    return Ok(None);
                }
                let old_path = inventory.get(&id).map(|entry| entry.path.clone());
                let Some(old_path) = old_path else {
                    return Ok(None);
                };
                let Some(new_path) = self.project_path(&meta)? else {
                    return Ok(None);
                };
                if new_path.starts_with(&format!("{old_path}/")) {
                    return Ok(None);
                }
                let prefix = format!("{old_path}/");
                for (descendant_id, descendant) in inventory.iter_mut() {
                    if descendant.path == old_path {
                        descendant.path = new_path.clone();
                        updates.insert(descendant_id.clone(), Some(descendant.clone()));
                    } else if let Some(suffix) = descendant.path.strip_prefix(&prefix) {
                        descendant.path = format!("{new_path}/{suffix}");
                        updates.insert(descendant_id.clone(), Some(descendant.clone()));
                    }
                }
                let folder = entry(&meta, new_path)?;
                inventory.insert(id.clone(), folder.clone());
                updates.insert(id.clone(), Some(folder));
                self.seed_directory_paths(inventory);
                continue;
            }
            if was_directory {
                return Ok(None);
            }
            if change.removed || meta.trashed {
                inventory.remove(&id);
                updates.insert(id, None);
                continue;
            }
            if inventory.contains_key(&id)
                && meta.parents.first().is_some_and(|parent| {
                    parent != &self.root && !self.directories.values().any(|id| id == parent)
                })
            {
                // A known file moving under a new/unknown parent may belong to a new subtree.
                return Ok(None);
            }
            match self.project_path(&meta)? {
                Some(path) => {
                    let entry = entry(&meta, path)?;
                    inventory.insert(id.clone(), entry.clone());
                    updates.insert(id, Some(entry));
                }
                None => {
                    inventory.remove(&id);
                    updates.insert(id, None);
                }
            }
        }
        let mut paths = BTreeSet::new();
        if inventory
            .values()
            .any(|entry| !paths.insert(entry.path.clone()))
        {
            return Ok(None);
        }
        self.seed_directory_paths(inventory);
        Ok(Some(updates))
    }
    fn full_scan(&mut self) -> Result<BTreeMap<String, Entry>> {
        self.paths.clear();
        self.directories.clear();
        let root = self.root.clone();
        let (meta, _) = self
            .metadata(&root)?
            .context("Sem acesso à pasta remota; sincronização bloqueada")?;
        ensure!(
            !meta.trashed && meta.mime_type == "application/vnd.google-apps.folder",
            "Raiz remota indisponível"
        );
        self.root = meta.id.clone();
        let root = self.root.clone();
        let mut files = BTreeMap::new();
        self.walk(&root, "", 0, &mut BTreeSet::new(), &mut files)?;
        Ok(files)
    }
    fn walk(
        &mut self,
        parent: &str,
        prefix: &str,
        depth: usize,
        seen: &mut BTreeSet<String>,
        files: &mut BTreeMap<String, Entry>,
    ) -> Result<()> {
        ensure!(
            depth < 128 && seen.insert(parent.into()),
            "Árvore remota profunda ou cíclica"
        );
        let mut names = BTreeSet::new();
        for item in self.children(parent)? {
            validate_name(&item.name)?;
            ensure!(
                names.insert(projected_name(&item)),
                "Nomes duplicados na pasta remota: {}. Renomeie pelo site do Drive.",
                item.name
            );
            let path = if prefix.is_empty() {
                projected_name(&item)
            } else {
                format!("{prefix}/{}", projected_name(&item))
            };
            self.paths.insert(item.id.clone(), path.clone());
            if item.mime_type == "application/vnd.google-apps.folder" {
                self.directories.insert(path.clone(), item.id.clone());
                files.insert(item.id.clone(), entry(&item, path.clone())?);
                self.walk(&item.id, &path, depth + 1, seen, files)?;
            } else {
                files.insert(item.id.clone(), entry(&item, path)?);
            }
        }
        Ok(())
    }
    pub fn folders(&mut self, parent: &str) -> Result<Vec<(String, String)>> {
        let (meta, _) = self
            .metadata(parent)?
            .context("Sem acesso à pasta; a lista anterior deve ser preservada")?;
        ensure!(
            !meta.trashed && meta.mime_type == "application/vnd.google-apps.folder",
            "A pasta selecionada está indisponível"
        );
        let mut folders: Vec<_> = self
            .children(&meta.id)?
            .into_iter()
            .filter(|m| m.mime_type == "application/vnd.google-apps.folder")
            .map(|m| (m.id, m.name))
            .collect();
        folders.sort_by(|a, b| {
            a.1.to_lowercase()
                .cmp(&b.1.to_lowercase())
                .then_with(|| a.1.cmp(&b.1))
                .then_with(|| a.0.cmp(&b.0))
        });
        Ok(folders)
    }
    fn parent_for(&mut self, path: &str) -> Result<(String, String)> {
        let mut parts: Vec<_> = path.split('/').collect();
        let name = parts.pop().context("Nome ausente")?.to_string();
        validate_name(&name)?;
        let mut parent = self.root.clone();
        let mut current = String::new();
        for part in parts {
            validate_name(part)?;
            if !current.is_empty() {
                current.push('/');
            }
            current.push_str(part);
            if let Some(id) = self.directories.get(&current) {
                parent = id.clone();
                continue;
            }
            anyhow::bail!(
                "Pasta pai não inventariada; execute novo ciclo antes de criar descendentes"
            );
        }
        Ok((parent, name))
    }
}
fn acknowledged_bytes(response: &Response, total: u64) -> Result<u64> {
    let Some(range) = response.headers().get("Range") else {
        return Ok(0);
    };
    let value = range
        .to_str()
        .map_err(|_| anyhow::anyhow!("Range de upload inválido"))?;
    let last: u64 = value
        .strip_prefix("bytes=0-")
        .context("Range de upload inválido")?
        .parse()
        .context("Range de upload inválido")?;
    let next = last
        .checked_add(1)
        .context("Range de upload excede tamanho suportado")?;
    ensure!(next <= total, "Range de upload excede o arquivo");
    Ok(next)
}
fn escape_id(id: &str) -> Result<&str> {
    ensure!(
        !id.is_empty()
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
        "ID de arquivo inválido"
    );
    Ok(id)
}
fn validate_name(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty()
            && name != "."
            && name != ".."
            && name != ".gdrive-clone-recovery"
            && !name.contains(['/', '\\', '\0']),
        "Nome remoto inseguro ou incompatível"
    );
    Ok(())
}
fn native_suffix(mime: &str) -> Option<&'static str> {
    match mime {
        "application/vnd.google-apps.folder" => None,
        "application/vnd.google-apps.document" => Some(".gdoc"),
        "application/vnd.google-apps.spreadsheet" => Some(".gsheet"),
        "application/vnd.google-apps.presentation" => Some(".gslides"),
        m if m.starts_with("application/vnd.google-apps.") => Some(".gdrive-link"),
        _ => None,
    }
}
fn projected_name(meta: &Meta) -> String {
    format!(
        "{}{}",
        meta.name,
        native_suffix(&meta.mime_type).unwrap_or("")
    )
}
fn document_bytes(id: &str) -> Result<Vec<u8>> {
    escape_id(id)?;
    Ok(serde_json::to_vec(
        &json!({"doc_id":id,"url":format!("https://drive.google.com/open?id={id}")}),
    )?)
}
pub fn is_reference_path(path: &str) -> bool {
    [".gdoc", ".gsheet", ".gslides", ".gdrive-link"]
        .iter()
        .any(|suffix| path.ends_with(suffix))
}
/// Validates a local reference without accepting arbitrary URL schemes or hosts.
pub fn reference_url(bytes: &[u8]) -> Result<String> {
    #[derive(Deserialize)]
    struct Reference {
        doc_id: String,
        url: String,
    }
    ensure!(bytes.len() <= 16384, "Referência grande demais");
    let reference: Reference =
        serde_json::from_slice(bytes).context("Referência Google inválida")?;
    escape_id(&reference.doc_id)?;
    let url = format!("https://drive.google.com/open?id={}", reference.doc_id);
    ensure!(
        reference.url == url,
        "Referência não aponta para o Drive esperado"
    );
    Ok(url)
}
fn entry(meta: &Meta, path: String) -> Result<Entry> {
    let directory = meta.mime_type == "application/vnd.google-apps.folder";
    let (hash, size) = if directory {
        (String::new(), 0)
    } else if native_suffix(&meta.mime_type).is_some() {
        let bytes = document_bytes(&meta.id)?;
        (format!("{:x}", Md5::digest(&bytes)), bytes.len() as u64)
    } else {
        (
            meta.md5_checksum
                .clone()
                .context("Arquivo remoto sem MD5 suportado")?,
            meta.size.as_ref().context("Arquivo sem tamanho")?.parse()?,
        )
    };
    Ok(Entry {
        directory,
        path,
        hash,
        size,
        identity: meta.version.clone().context("Arquivo sem revisão")?,
    })
}
impl Remote for Drive {
    fn set_progress(&mut self, sink: Option<ProgressSink>) {
        self.progress = sink;
    }

    fn scan(&mut self) -> Result<BTreeMap<String, Entry>> {
        let Some(state) = &mut self.change_state else {
            return self.full_scan();
        };
        let saved = state.load()?;
        match saved {
            None => {
                // The token precedes the baseline inventory so concurrent edits are replayed next cycle.
                let cursor = self.start_page_token()?;
                let inventory = self.full_scan()?;
                self.change_state
                    .as_mut()
                    .context("Persistência do log de mudanças ausente")?
                    .save(&cursor, &inventory)?;
                Ok(inventory)
            }
            Some((cursor, inventory)) => {
                let (changes, next_cursor) = match self.list_changes(&cursor) {
                    Ok(page) => page,
                    Err(error)
                        if error.downcast_ref::<ApiFailure>().is_some_and(|failure| {
                            matches!(
                                failure,
                                ApiFailure::Http {
                                    status: 400 | 410,
                                    ..
                                }
                            )
                        }) =>
                    {
                        // A rejected cursor is only replaced after a fresh complete inventory.
                        let fresh_cursor = self.start_page_token()?;
                        let refreshed = self.full_scan()?;
                        self.change_state
                            .as_mut()
                            .context("Persistência do log de mudanças ausente")?
                            .save(&fresh_cursor, &refreshed)?;
                        return Ok(refreshed);
                    }
                    Err(error) => return Err(error),
                };
                if changes.is_empty() {
                    if next_cursor != cursor {
                        self.change_state
                            .as_mut()
                            .context("Persistência do log de mudanças ausente")?
                            .advance(&next_cursor)?;
                    }
                    self.seed_directory_paths(&inventory);
                    Ok(inventory)
                } else {
                    let mut updated = inventory;
                    match self.apply_file_changes(&mut updated, changes)? {
                        Some(updates) => {
                            self.change_state
                                .as_mut()
                                .context("Persistência do log de mudanças ausente")?
                                .update(&next_cursor, &updates)?;
                            Ok(updated)
                        }
                        None => {
                            // Structural or ambiguous events require a complete, deletion-safe inventory.
                            let refreshed = self.full_scan()?;
                            self.change_state
                                .as_mut()
                                .context("Persistência do log de mudanças ausente")?
                                .save(&next_cursor, &refreshed)?;
                            Ok(refreshed)
                        }
                    }
                }
            }
        }
    }
    fn status(&mut self, id: &str) -> Result<Status> {
        let Some((meta, _)) = self.metadata(id)? else {
            return Ok(Status::Unknown);
        };
        if meta.trashed {
            return Ok(Status::Trashed);
        }
        // Never pull an object moved out of the configured tree back into it.
        let Some(path) = self.project_path(&meta)? else {
            return Ok(Status::Unknown);
        };
        Ok(Status::Present(entry(&meta, path)?))
    }
    fn reserve_id(&mut self) -> Result<String> {
        #[derive(Deserialize)]
        struct Ids {
            ids: Vec<String>,
        }
        let url = format!("{}/files/generateIds", self.base_url);
        let response = self
            .request(Method::GET, url)?
            .query(&[("count", "1"), ("space", "drive"), ("type", "files")])
            .send()
            .map_err(|_| ApiFailure::Network("Falha ao reservar ID remoto"))?;
        let ids: Ids = Self::checked(response)?.json()?;
        ids.ids.into_iter().next().context("Drive não retornou ID")
    }
    fn download(&mut self, id: &str, expected: &Entry) -> Result<Box<dyn Read>> {
        self.download_from(id, expected, 0)
    }
    fn download_from(&mut self, id: &str, expected: &Entry, offset: u64) -> Result<Box<dyn Read>> {
        ensure!(!expected.directory, "Diretório não tem corpo de download");
        ensure!(
            offset < expected.size || offset == 0,
            "Offset de download inválido"
        );
        let (meta, _) = self.metadata(id)?.context("Arquivo remoto indisponível")?;
        ensure!(
            !meta.trashed
                && entry(
                    &meta,
                    self.project_path(&meta)?.context("Arquivo fora da raiz")?
                )? == *expected,
            "Arquivo remoto mudou; replaneje"
        );
        if native_suffix(&meta.mime_type).is_some() {
            ensure!(
                offset == 0,
                "Referência Google não suporta download parcial"
            );
            return Ok(Box::new(ProgressReader::new(
                std::io::Cursor::new(document_bytes(id)?),
                &expected.path,
                false,
                expected.size,
                self.progress.clone(),
            )));
        }
        let url = format!("{}/files/{}", self.base_url, escape_id(id)?);
        let mut request = self.request(Method::GET, url)?.query(&[("alt", "media")]);
        if offset > 0 {
            request = request.header(RANGE, format!("bytes={offset}-"));
        }
        let response = request
            .send()
            .map_err(|_| ApiFailure::Network("Falha no download"))?;
        let response = Self::checked(response)?;
        if offset > 0 {
            ensure!(
                response.status().as_u16() == 206,
                "Servidor não confirmou download parcial"
            );
            let expected_range = format!("bytes {offset}-{}/{}", expected.size - 1, expected.size);
            ensure!(
                response
                    .headers()
                    .get(CONTENT_RANGE)
                    .and_then(|range| range.to_str().ok())
                    == Some(expected_range.as_str()),
                "Range de download diverge do arquivo esperado"
            );
        } else {
            ensure!(
                response.status().as_u16() == 200,
                "Resposta inesperada ao iniciar download"
            );
        }
        Ok(Box::new(ProgressReader::new_at(
            NetworkReader(response),
            &expected.path,
            false,
            expected.size,
            self.progress.clone(),
            offset,
        )))
    }
    fn upload(
        &mut self,
        id: &str,
        expected: Option<&Entry>,
        target: &Entry,
        mut source: File,
    ) -> Result<Entry> {
        ensure!(
            !target.directory && expected.is_none_or(|e| !e.directory),
            "Use operações de diretório para pastas"
        );
        let existing = self.metadata(id)?;
        if let Some((meta, etag)) = &existing
            && let Some(suffix) = native_suffix(&meta.mime_type)
        {
            let expected = expected.context("Referência já existe; inspecione antes de repetir")?;
            ensure!(
                !meta.trashed
                    && entry(
                        meta,
                        self.project_path(meta)?.context("Documento fora da raiz")?
                    )? == *expected,
                "Documento remoto mudou"
            );
            ensure!(
                target.same_content(expected),
                "Edite o documento Google pelo navegador; não altere o arquivo de referência"
            );
            let raw_path = target
                .path
                .strip_suffix(suffix)
                .context("Mantenha a extensão da referência Google ao renomear")?;
            let (parent, name) = self.parent_for(raw_path)?;
            let url = format!("{}/files/{}", self.base_url, escape_id(id)?);
            let mut request = self
                .request(Method::PATCH, url)?
                .query(&[("fields", FIELDS)])
                .header(
                    IF_MATCH,
                    etag.as_ref()
                        .context("ETag ausente; renomeação bloqueada")?,
                )
                .json(&json!({"name":name}));
            if !meta.parents.contains(&parent) {
                request = request.query(&[
                    ("addParents", parent.as_str()),
                    ("removeParents", meta.parents.join(",").as_str()),
                ]);
            }
            let response = Self::checked(
                request
                    .send()
                    .map_err(|_| ApiFailure::Network("Falha ao renomear documento Google"))?,
            )?;
            let changed: Meta = response.json().context("Resposta de documento inválida")?;
            self.paths.insert(id.into(), target.path.clone());
            return entry(&changed, target.path.clone());
        }
        if expected.is_none() {
            ensure!(
                ![".gdoc", ".gsheet", ".gslides", ".gdrive-link"]
                    .iter()
                    .any(|suffix| target.path.ends_with(suffix)),
                "Referência Google local sem vínculo; não será enviada como arquivo binário"
            );
        }
        let (parent, name) = self.parent_for(&target.path)?;
        let (method, url, metadata, etag) = if let Some(expected) = expected {
            let (meta, etag) = existing.context("Arquivo remoto perdeu acesso")?;
            ensure!(!meta.trashed, "Arquivo foi para a lixeira");
            ensure!(
                entry(
                    &meta,
                    self.project_path(&meta)?
                        .context("Arquivo movido para fora da raiz")?
                )? == *expected,
                "Arquivo remoto mudou antes do upload"
            );
            let tag = etag.context(
                "Drive não retornou ETag; atualização bloqueada para preservar edição concorrente",
            )?;
            (
                Method::PATCH,
                format!("{}/files/{}", self.upload_url, escape_id(id)?),
                json!({"name":name}),
                Some((tag, meta.parents)),
            )
        } else {
            ensure!(
                existing.is_none(),
                "ID reservado já existe; inspecione a operação antes de repetir"
            );
            (
                Method::POST,
                format!("{}/files", self.upload_url),
                json!({"id":id,"name":name,"parents":[parent]}),
                None,
            )
        };
        let saved = match &mut self.upload_sessions {
            Some(store) => store.load(id)?,
            None => None,
        };
        if let Some(mut saved) = saved {
            ensure!(
                saved.target == *target && saved.expected.as_ref() == expected,
                "Sessão pendente pertence a outro snapshot; revise o journal"
            );
            self.validate_upload_url(&saved.url)?;
            let mut request = self
                .request(Method::PUT, saved.url.clone())?
                .header("Content-Length", 0)
                .header("Content-Range", format!("bytes */{}", target.size));
            if let Some((tag, _)) = &etag {
                request = request.header(IF_MATCH, tag);
            }
            let response = request.send().map_err(|_| {
                ApiFailure::Network(
                    "Não foi possível consultar sessão de upload; intenção preservada",
                )
            })?;
            match response.status().as_u16() {
                200 | 201 => return self.finish_upload(id, target, response),
                308 => {
                    saved.acknowledged = acknowledged_bytes(&response, target.size)?;
                    if let Some(store) = &mut self.upload_sessions {
                        store.save(id, &saved)?;
                    }
                    return self.send_upload_chunks(
                        id,
                        target,
                        &mut source,
                        &mut saved,
                        etag.as_ref().map(|(tag, _)| tag.as_str()),
                    );
                }
                404 | 410 => {
                    if let Some(store) = &mut self.upload_sessions {
                        store.clear(id)?;
                    }
                }
                _ => {
                    Self::checked(response)?;
                    anyhow::bail!("Resposta inesperada ao consultar upload");
                }
            }
        }
        let mut request = self
            .request(method, url)?
            .query(&[("uploadType", "resumable"), ("fields", FIELDS)])
            .header("X-Upload-Content-Type", "application/octet-stream")
            .header("X-Upload-Content-Length", target.size)
            .json(&metadata);
        if let Some((tag, parents)) = &etag {
            request = request.header(IF_MATCH, tag);
            if !parents.contains(&parent) {
                request = request.query(&[
                    ("addParents", parent.as_str()),
                    ("removeParents", parents.join(",").as_str()),
                ]);
            }
        }
        let response = Self::checked(
            request
                .send()
                .map_err(|_| ApiFailure::Network("Falha ao iniciar upload"))?,
        )?;
        let location = response
            .headers()
            .get(LOCATION)
            .context("Sessão de upload ausente")?
            .to_str()?
            .to_string();
        self.validate_upload_url(&location)?;
        let mut session = UploadSession {
            url: location,
            acknowledged: 0,
            target: target.clone(),
            expected: expected.cloned(),
        };
        if let Some(store) = &mut self.upload_sessions {
            store.save(id, &session)?;
        }
        self.send_upload_chunks(
            id,
            target,
            &mut source,
            &mut session,
            etag.as_ref().map(|(tag, _)| tag.as_str()),
        )
    }

    fn put_directory(
        &mut self,
        id: &str,
        expected: Option<&Entry>,
        target: &Entry,
    ) -> Result<Entry> {
        ensure!(target.directory, "Origem não é diretório");
        let existing = self.metadata(id)?;
        let (parent, name) = self.parent_for(&target.path)?;
        let response = if let Some(expected) = expected {
            ensure!(expected.directory, "Tipo remoto mudou");
            let (meta, tag) = existing.context("Pasta remota indisponível")?;
            ensure!(
                !meta.trashed
                    && entry(
                        &meta,
                        self.project_path(&meta)?.context("Pasta fora da raiz")?
                    )? == *expected,
                "Pasta remota mudou; replaneje"
            );
            let mut request = self
                .request(
                    Method::PATCH,
                    format!("{}/files/{}", self.base_url, escape_id(id)?),
                )?
                .query(&[("fields", FIELDS)])
                .header(
                    IF_MATCH,
                    tag.context("ETag ausente; movimento de pasta bloqueado")?,
                )
                .json(&json!({"name":name}));
            if !meta.parents.contains(&parent) {
                request = request.query(&[
                    ("addParents", parent.as_str()),
                    ("removeParents", meta.parents.join(",").as_str()),
                ]);
            }
            request.send().map_err(|_| {
                ApiFailure::Network("Movimento de pasta interrompido; inspecione o journal")
            })?
        } else {
            ensure!(
                existing.is_none(),
                "ID reservado já existe; inspecione antes de repetir"
            );
            self.request(Method::POST, format!("{}/files", self.base_url))?
                .query(&[("fields", FIELDS)])
                .json(&json!({"id":id,"name":name,"mimeType":"application/vnd.google-apps.folder","parents":[parent]}))
                .send().map_err(|_| ApiFailure::Network("Criação de pasta interrompida; o mesmo ID será inspecionado"))?
        };
        let meta: Meta = Self::checked(response)?
            .json()
            .context("Resposta de pasta inválida")?;
        ensure!(
            meta.id == id
                && !meta.trashed
                && meta.mime_type == "application/vnd.google-apps.folder"
                && meta.name == name
                && meta.parents.contains(&parent),
            "Resultado remoto da pasta diverge da intenção"
        );
        self.directories.insert(target.path.clone(), id.into());
        self.paths.insert(id.into(), target.path.clone());
        entry(&meta, target.path.clone())
    }
    fn trash(&mut self, id: &str, expected: &Entry) -> Result<()> {
        ensure!(
            !expected.directory,
            "Exclusão de pastas bloqueada para preservar descendentes concorrentes"
        );
        let (meta, etag) = self
            .metadata(id)?
            .context("Sem acesso ao arquivo remoto; não é exclusão")?;
        ensure!(
            !meta.trashed
                && entry(
                    &meta,
                    self.project_path(&meta)?
                        .context("Arquivo movido para fora da raiz")?
                )? == *expected,
            "Arquivo remoto mudou antes da exclusão"
        );
        let url = format!("{}/files/{}", self.base_url, escape_id(id)?);
        let response = self
            .request(Method::PATCH, url)?
            .header(IF_MATCH, etag.context("ETag ausente; exclusão bloqueada")?)
            .json(&json!({"trashed":true}))
            .send()
            .map_err(|_| ApiFailure::Network("Exclusão interrompida; confira a lixeira"))?;
        Self::checked(response)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use std::{io::Write, net::TcpListener, thread};
    fn fake(responses: Vec<(u16, String)>) -> (Drive, thread::JoinHandle<Vec<String>>) {
        fake_headers(
            responses
                .into_iter()
                .map(|(status, body)| (status, String::new(), body))
                .collect(),
        )
    }
    fn fake_headers(
        responses: Vec<(u16, String, String)>,
    ) -> (Drive, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server_base = base.clone();
        let handle = thread::spawn(move || {
            let mut requests = Vec::new();
            for (status, headers, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut request = Vec::new();
                let mut buf = [0u8; 1];
                while !request.ends_with(b"\r\n\r\n") && request.len() < 16384 {
                    if stream.read(&mut buf).unwrap() == 0 {
                        break;
                    }
                    request.push(buf[0]);
                }
                let header = String::from_utf8(request.clone()).unwrap();
                let length: usize = header
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|n| n.trim().parse().ok())
                    })
                    .unwrap_or(0);
                let mut payload = vec![0; length];
                stream.read_exact(&mut payload).unwrap();
                request.extend(payload);
                requests.push(String::from_utf8(request).unwrap());
                let headers = headers.replace("{BASE}", &server_base);
                let reply = format!(
                    "HTTP/1.1 {status} Test\r\n{headers}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(reply.as_bytes()).unwrap();
            }
            requests
        });
        let drive = Drive {
            upload_sessions: None,
            change_state: None,
            progress: None,
            client: Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(3))
                .build()
                .unwrap(),
            session: oauth::Session::synthetic(),
            root: "rootid".into(),
            paths: BTreeMap::new(),
            directories: BTreeMap::new(),
            base_url: base.clone(),
            upload_url: base,
        };
        (drive, handle)
    }
    fn root() -> String {
        json!({"id":"rootid","name":"Root","mimeType":"application/vnd.google-apps.folder"})
            .to_string()
    }
    fn binary(id: &str, name: &str) -> serde_json::Value {
        json!({"id":id,"name":name,"mimeType":"text/plain","parents":["rootid"],"md5Checksum":"abc","size":"3","version":"1"})
    }
    #[derive(Clone, Default)]
    struct MemorySessions(std::sync::Arc<std::sync::Mutex<BTreeMap<String, UploadSession>>>);
    impl UploadSessionStore for MemorySessions {
        fn load(&mut self, id: &str) -> Result<Option<UploadSession>> {
            Ok(self.0.lock().unwrap().get(id).cloned())
        }
        fn save(&mut self, id: &str, session: &UploadSession) -> Result<()> {
            self.0.lock().unwrap().insert(id.into(), session.clone());
            Ok(())
        }
        fn clear(&mut self, id: &str) -> Result<()> {
            self.0.lock().unwrap().remove(id);
            Ok(())
        }
    }
    type CachedInventory = Option<(String, BTreeMap<String, Entry>)>;
    #[derive(Clone, Default)]
    struct MemoryChanges(std::sync::Arc<std::sync::Mutex<CachedInventory>>);
    impl ChangeStateStore for MemoryChanges {
        fn load(&mut self) -> Result<Option<(String, BTreeMap<String, Entry>)>> {
            Ok(self.0.lock().unwrap().clone())
        }
        fn advance(&mut self, cursor: &str) -> Result<()> {
            let mut state = self.0.lock().unwrap();
            let (_, inventory) = state.take().context("Snapshot ausente")?;
            *state = Some((cursor.into(), inventory));
            Ok(())
        }
        fn update(
            &mut self,
            cursor: &str,
            updates: &BTreeMap<String, Option<Entry>>,
        ) -> Result<()> {
            let mut state = self.0.lock().unwrap();
            let (_, mut inventory) = state.take().context("Snapshot ausente")?;
            for (id, entry) in updates {
                if let Some(entry) = entry {
                    inventory.insert(id.clone(), entry.clone());
                } else {
                    inventory.remove(id);
                }
            }
            *state = Some((cursor.into(), inventory));
            Ok(())
        }
        fn save(&mut self, cursor: &str, inventory: &BTreeMap<String, Entry>) -> Result<()> {
            *self.0.lock().unwrap() = Some((cursor.into(), inventory.clone()));
            Ok(())
        }
    }
    struct UploadFile {
        path: std::path::PathBuf,
        target: Entry,
        meta: String,
    }
    impl UploadFile {
        fn new(bytes: &[u8]) -> Self {
            let path = std::env::temp_dir().join(format!("drive-resume-{}", uuid::Uuid::new_v4()));
            std::fs::write(&path, bytes).unwrap();
            let target = Entry {
                directory: false,
                path: "file.bin".into(),
                hash: format!("{:x}", Md5::digest(bytes)),
                size: bytes.len() as u64,
                identity: "synthetic-inode".into(),
            };
            let meta = json!({"id":"reserved","name":"file.bin","mimeType":"application/octet-stream","parents":["rootid"],"md5Checksum":target.hash,"size":target.size.to_string(),"version":"1"}).to_string();
            Self { path, target, meta }
        }
        fn open(&self) -> File {
            File::open(&self.path).unwrap()
        }
    }
    impl Drop for UploadFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
    #[test]
    fn restarted_upload_queries_server_range_and_sends_only_remaining_bytes() {
        let bytes = vec![b'x'; 8 * 1024 * 1024 + 3];
        let file = UploadFile::new(&bytes);
        let (mut drive, handle) = fake_headers(vec![
            (404, String::new(), "{}".into()),
            (200, "Location: {BASE}/session\r\n".into(), String::new()),
            (503, String::new(), "{}".into()),
            (404, String::new(), "{}".into()),
            (308, "Range: bytes=0-262143\r\n".into(), String::new()),
            (200, String::new(), file.meta.clone()),
        ]);
        let mut sessions = MemorySessions::default();
        drive.set_upload_sessions(Box::new(sessions.clone()));
        assert!(
            drive
                .upload("reserved", None, &file.target, file.open())
                .is_err()
        );
        assert_eq!(sessions.load("reserved").unwrap().unwrap().acknowledged, 0);
        // Rebuild the adapter with the same durable state; no in-memory HTTP session retained.
        let mut restarted = Drive {
            upload_sessions: Some(Box::new(sessions.clone())),
            change_state: None,
            progress: None,
            client: drive.client.clone(),
            session: oauth::Session::synthetic(),
            root: drive.root.clone(),
            paths: BTreeMap::new(),
            directories: BTreeMap::new(),
            base_url: drive.base_url.clone(),
            upload_url: drive.upload_url.clone(),
        };
        drop(drive);
        restarted
            .upload("reserved", None, &file.target, file.open())
            .unwrap();
        let requests = handle.join().unwrap();
        assert!(
            requests[4]
                .to_ascii_lowercase()
                .contains(&format!("content-range: bytes */{}", bytes.len()))
        );
        assert!(requests[5].to_ascii_lowercase().contains(&format!(
            "content-range: bytes 262144-{}/{}",
            bytes.len() - 1,
            bytes.len()
        )));
        assert!(requests[5].as_bytes().ends_with(&bytes[262144..]));
        assert_eq!(
            requests.iter().filter(|r| r.starts_with("POST ")).count(),
            1
        );
        assert_eq!(
            sessions.load("reserved").unwrap().unwrap().acknowledged,
            262144
        );
    }
    #[test]
    fn invalid_final_upload_response_does_not_expose_the_private_session_url() {
        let file = UploadFile::new(b"abc");
        let (mut drive, handle) = fake_headers(vec![
            (404, String::new(), "{}".into()),
            (
                200,
                "Location: {BASE}/private-session-capability\r\n".into(),
                String::new(),
            ),
            (200, String::new(), "invalid json".into()),
        ]);
        let error = drive
            .upload("reserved", None, &file.target, file.open())
            .unwrap_err();
        assert!(!format!("{error:#}").contains("private-session-capability"));
        handle.join().unwrap();
    }
    #[test]
    fn zero_byte_upload_finishes_without_a_chunk_loop() {
        let file = UploadFile::new(b"");
        let (mut drive, handle) = fake_headers(vec![
            (404, String::new(), "{}".into()),
            (200, "Location: {BASE}/session\r\n".into(), String::new()),
            (200, String::new(), file.meta.clone()),
        ]);
        assert_eq!(
            drive
                .upload("reserved", None, &file.target, file.open())
                .unwrap()
                .size,
            0
        );
        let requests = handle.join().unwrap();
        assert!(
            requests[2]
                .to_ascii_lowercase()
                .contains("content-length: 0")
        );
    }
    #[test]
    fn expired_upload_session_restarts_under_the_same_reserved_id() {
        let file = UploadFile::new(b"abc");
        let (mut drive, handle) = fake_headers(vec![
            (404, String::new(), "{}".into()),
            (404, String::new(), "{}".into()),
            (200, "Location: {BASE}/fresh\r\n".into(), String::new()),
            (200, String::new(), file.meta.clone()),
        ]);
        let mut sessions = MemorySessions::default();
        sessions
            .save(
                "reserved",
                &UploadSession {
                    url: format!("{}/old", drive.base_url),
                    acknowledged: 2,
                    target: file.target.clone(),
                    expected: None,
                },
            )
            .unwrap();
        drive.set_upload_sessions(Box::new(sessions));
        drive
            .upload("reserved", None, &file.target, file.open())
            .unwrap();
        let requests = handle.join().unwrap();
        assert!(requests[1].starts_with("PUT /old "));
        assert!(requests[2].contains("\"id\":\"reserved\""));
        assert!(requests[3].starts_with("PUT /fresh "));
    }
    #[test]
    fn invalid_server_range_blocks_resuming_without_sending_new_content() {
        let file = UploadFile::new(b"abc");
        let (mut drive, handle) = fake_headers(vec![
            (404, String::new(), "{}".into()),
            (308, "Range: bytes=0-99\r\n".into(), String::new()),
        ]);
        let mut sessions = MemorySessions::default();
        sessions
            .save(
                "reserved",
                &UploadSession {
                    url: format!("{}/session", drive.base_url),
                    acknowledged: 0,
                    target: file.target.clone(),
                    expected: None,
                },
            )
            .unwrap();
        drive.set_upload_sessions(Box::new(sessions));
        assert!(
            drive
                .upload("reserved", None, &file.target, file.open())
                .is_err()
        );
        assert_eq!(handle.join().unwrap().len(), 2);
    }
    #[test]
    fn hostile_persisted_upload_destination_is_rejected_and_not_in_error_text() {
        let file = UploadFile::new(b"abc");
        let (mut drive, handle) = fake(vec![(404, "{}".into())]);
        let mut sessions = MemorySessions::default();
        sessions
            .save(
                "reserved",
                &UploadSession {
                    url: "https://example.invalid/private-session".into(),
                    acknowledged: 0,
                    target: file.target.clone(),
                    expected: None,
                },
            )
            .unwrap();
        drive.set_upload_sessions(Box::new(sessions));
        let error = drive
            .upload("reserved", None, &file.target, file.open())
            .unwrap_err();
        assert!(!format!("{error:#}").contains("private-session"));
        assert_eq!(handle.join().unwrap().len(), 1);
    }
    #[test]
    fn multi_chunk_upload_uses_acknowledged_range_and_verifies_final_checksum() {
        let bytes = vec![b'a'; 8 * 1024 * 1024 + 3];
        let file = UploadFile::new(&bytes);
        let (mut drive, handle) = fake_headers(vec![
            (404, String::new(), "{}".into()),
            (200, "Location: {BASE}/session\r\n".into(), String::new()),
            (308, "Range: bytes=0-8388607\r\n".into(), String::new()),
            (200, String::new(), file.meta.clone()),
        ]);
        drive
            .upload("reserved", None, &file.target, file.open())
            .unwrap();
        let requests = handle.join().unwrap();
        assert!(requests[3].as_bytes().ends_with(b"aaa"));
        assert!(
            requests[3]
                .to_ascii_lowercase()
                .contains("content-range: bytes 8388608-8388610/8388611")
        );
    }
    fn folder(id: &str, name: &str, version: &str) -> serde_json::Value {
        json!({"id":id,"name":name,"version":version,"mimeType":"application/vnd.google-apps.folder","parents":["rootid"]})
    }
    #[test]
    fn retry_advice_preserves_rate_limit_delay_and_refuses_permission_retries() {
        let (mut drive, handle) = fake_headers(vec![
            (429, "Retry-After: 600\r\n".into(), "{}".into()),
            (403, String::new(), "{}".into()),
        ]);
        let error = drive
            .account_info()
            .unwrap_err()
            .context("wrapped operation");
        assert_eq!(retry_minimum(&error), Some(Duration::from_secs(600)));
        assert_eq!(retry_minimum(&drive.account_info().unwrap_err()), None);
        handle.join().unwrap();
    }
    #[test]
    fn account_metadata_and_storage_come_from_about_without_inventing_a_missing_limit() {
        let (mut drive, handle) = fake(vec![(200, json!({"user":{"displayName":"Pessoa de teste","emailAddress":"person@example.invalid"},"storageQuota":{"usage":"1024","limit":"4096"}}).to_string()),
            (200, json!({"user":{},"storageQuota":{"usage":"2048"}}).to_string())]);
        let info = drive.account_info().unwrap();
        assert_eq!(info.usage, Some(1024));
        assert_eq!(info.limit, Some(4096));
        assert_eq!(info.email.as_deref(), Some("person@example.invalid"));
        let info = drive.account_info().unwrap();
        assert_eq!(info.limit, None);
        assert_eq!(info.display_name, None);
        let requests = handle.join().unwrap();
        assert!(requests.iter().all(|r| r.starts_with("GET /about?")));
    }
    #[test]
    fn account_storage_invalid_values_are_errors_not_zero_usage() {
        let (mut drive, handle) = fake(vec![(
            200,
            json!({"user":{},"storageQuota":{"usage":"invalid"}}).to_string(),
        )]);
        assert!(drive.account_info().is_err());
        handle.join().unwrap();
    }
    #[test]
    fn empty_folders_are_inventory_objects_without_md5_or_media() {
        let (mut drive, handle) = fake(vec![
            (200, root()),
            (
                200,
                json!({"files":[folder("dir", "empty", "1")]}).to_string(),
            ),
            (200, json!({"files":[]}).to_string()),
        ]);
        let scan = drive.scan().unwrap();
        assert!(scan["dir"].directory);
        assert_eq!(scan["dir"].size, 0);
        assert_eq!(scan["dir"].path, "empty");
        handle.join().unwrap();
    }
    #[test]
    fn directory_creation_uses_reserved_id_and_metadata_endpoint() {
        let meta = folder("reserved", "empty", "1");
        let (mut drive, handle) = fake(vec![(404, "{}".into()), (200, meta.to_string())]);
        let target = entry(&serde_json::from_value(meta).unwrap(), "empty".into()).unwrap();
        assert!(
            drive
                .put_directory("reserved", None, &target)
                .unwrap()
                .same_file(&target)
        );
        let requests = handle.join().unwrap();
        assert!(requests[1].starts_with("POST /files?"));
        assert!(requests[1].contains("\"id\":\"reserved\""));
        assert!(requests[1].contains("application/vnd.google-apps.folder"));
        assert!(!requests[1].contains("uploadType"));
    }
    #[test]
    fn directory_rename_checks_revision_and_sends_if_match() {
        let old = folder("dir", "old", "1");
        let new = folder("dir", "new", "2");
        let (mut drive, handle) = fake_headers(vec![
            (200, "ETag: \"v1\"\r\n".into(), old.to_string()),
            (200, String::new(), new.to_string()),
        ]);
        let old = entry(&serde_json::from_value(old).unwrap(), "old".into()).unwrap();
        let mut target = old.clone();
        target.path = "new".into();
        drive.put_directory("dir", Some(&old), &target).unwrap();
        let requests = handle.join().unwrap();
        assert!(requests[1].starts_with("PATCH /files/dir?"));
        assert!(
            requests[1]
                .to_ascii_lowercase()
                .contains("if-match: \"v1\"")
        );
        assert!(requests[1].contains("\"name\":\"new\""));
    }
    #[test]
    fn directory_rename_without_etag_and_directory_trash_are_refused() {
        let old = folder("dir", "old", "1");
        let (mut drive, handle) = fake(vec![(200, old.to_string())]);
        let old = entry(&serde_json::from_value(old).unwrap(), "old".into()).unwrap();
        let mut target = old.clone();
        target.path = "new".into();
        assert!(drive.put_directory("dir", Some(&old), &target).is_err());
        assert!(drive.trash("dir", &old).is_err());
        assert_eq!(handle.join().unwrap().len(), 1);
    }
    #[test]
    fn folder_browser_resolves_root_and_keeps_duplicate_names_by_id() {
        let folder = |id: &str, name: &str| json!({"id":id,"name":name,"mimeType":"application/vnd.google-apps.folder"});
        let (mut drive, handle) = fake(vec![
            (200, root()),
            (
                200,
                json!({"files":[folder("z","Zulu"),binary("file","a.txt")],"nextPageToken":"next"})
                    .to_string(),
            ),
            (
                200,
                json!({"files":[folder("b","Alpha"),folder("a","Alpha")]}).to_string(),
            ),
        ]);
        assert_eq!(
            drive.folders("root").unwrap(),
            vec![
                ("a".into(), "Alpha".into()),
                ("b".into(), "Alpha".into()),
                ("z".into(), "Zulu".into())
            ]
        );
        let requests = handle.join().unwrap();
        assert!(requests[0].starts_with("GET /files/root?"));
        assert!(requests[1].contains("rootid"));
    }
    #[test]
    fn folder_browser_rejects_inaccessible_or_incomplete_lists() {
        let (mut drive, handle) = fake(vec![(404, "{}".into())]);
        assert!(drive.folders("missing").is_err());
        handle.join().unwrap();
        let (mut drive, handle) = fake(vec![
            (200, root()),
            (200, json!({"files":[],"incompleteSearch":true}).to_string()),
        ]);
        assert!(drive.folders("root").is_err());
        handle.join().unwrap();
    }
    #[test]
    fn pagination_collects_every_page_before_returning_inventory() {
        let (mut drive, handle) = fake(vec![
            (200, root()),
            (
                200,
                json!({"files":[binary("a","a.txt")],"nextPageToken":"page2"}).to_string(),
            ),
            (200, json!({"files":[binary("b","b.txt")]}).to_string()),
        ]);
        let files = drive.scan().unwrap();
        assert_eq!(files.len(), 2);
        let requests = handle.join().unwrap();
        assert!(requests.last().unwrap().contains("pageToken=page2"));
        assert!(requests.iter().all(|s| {
            s.to_ascii_lowercase()
                .contains("authorization: bearer synthetic-access-token")
        }));
    }
    #[test]
    fn incomplete_inventory_is_an_error_not_empty_state() {
        let (mut drive, handle) = fake(vec![
            (200, root()),
            (200, json!({"files":[],"incompleteSearch":true}).to_string()),
        ]);
        assert!(drive.scan().is_err());
        handle.join().unwrap();
    }
    #[test]
    fn repeated_page_token_is_rejected() {
        let (mut drive, handle) = fake(vec![
            (200, root()),
            (200, json!({"files":[],"nextPageToken":"same"}).to_string()),
            (200, json!({"files":[],"nextPageToken":"same"}).to_string()),
        ]);
        assert!(drive.scan().is_err());
        handle.join().unwrap();
    }
    #[test]
    fn duplicate_remote_names_are_rejected_without_silent_overwrite() {
        let (mut drive, handle) = fake(vec![
            (200, root()),
            (
                200,
                json!({"files":[binary("a","same.txt"),binary("b","same.txt")]}).to_string(),
            ),
        ]);
        assert!(drive.scan().is_err());
        handle.join().unwrap();
    }
    #[test]
    fn remote_path_traversal_is_rejected() {
        let (mut drive, handle) = fake(vec![
            (200, root()),
            (200, json!({"files":[binary("a","../outside")]}).to_string()),
        ]);
        assert!(drive.scan().is_err());
        handle.join().unwrap();
    }
    #[test]
    fn native_google_docs_are_projected_as_browser_references() {
        let(mut drive,handle)=fake(vec![(200,root()),(200,json!({"files":[{"id":"doc","name":"Doc","mimeType":"application/vnd.google-apps.document","parents":["rootid"],"version":"1"}]}).to_string())]);
        let files = drive.scan().unwrap();
        assert_eq!(files.get("doc").unwrap().path, "Doc.gdoc");
        assert!(files.get("doc").unwrap().size > 0);
        handle.join().unwrap();
    }
    #[test]
    fn native_download_returns_safe_reference_instead_of_media_export() {
        let meta = json!({"id":"doc","name":"Doc","mimeType":"application/vnd.google-apps.document","parents":["rootid"],"version":"2"});
        let (mut drive, handle) = fake(vec![(200, meta.to_string())]);
        let state = entry(
            &serde_json::from_value::<Meta>(meta).unwrap(),
            "Doc.gdoc".into(),
        )
        .unwrap();
        let mut bytes = Vec::new();
        drive
            .download("doc", &state)
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(
            reference_url(&bytes).unwrap(),
            "https://drive.google.com/open?id=doc"
        );
        assert_eq!(bytes.len() as u64, state.size);
        handle.join().unwrap();
    }
    #[test]
    fn partial_download_requires_matching_content_range() {
        let state = entry(
            &serde_json::from_value::<Meta>(binary("a", "a.bin")).unwrap(),
            "a.bin".into(),
        )
        .unwrap();
        for (status, range, valid) in [
            (206, "Content-Range: bytes 1-2/3\r\n", true),
            (200, "", false),
            (206, "Content-Range: bytes 0-2/3\r\n", false),
            (206, "Content-Range: bytes 1-2/4\r\n", false),
        ] {
            let (mut drive, handle) = fake_headers(vec![
                (200, String::new(), binary("a", "a.bin").to_string()),
                (status, range.into(), "bc".into()),
            ]);
            let result = drive.download_from("a", &state, 1);
            assert_eq!(result.is_ok(), valid);
            if let Ok(mut source) = result {
                let mut content = Vec::new();
                source.read_to_end(&mut content).unwrap();
                assert_eq!(content, b"bc");
            }
            let requests = handle.join().unwrap();
            assert!(
                requests[1].contains("Range: bytes=1-") || requests[1].contains("range: bytes=1-")
            );
        }
    }
    #[test]
    fn empty_change_feed_reuses_inventory_and_advances_cursor() {
        let (mut drive, handle) = fake(vec![(
            200,
            json!({"changes":[],"newStartPageToken":"cursor-2"}).to_string(),
        )]);
        let cached = Entry {
            directory: false,
            path: "cached.txt".into(),
            hash: "abc".into(),
            size: 3,
            identity: "revision-1".into(),
        };
        let mut state = MemoryChanges::default();
        *state.0.lock().unwrap() = Some((
            "cursor-1".into(),
            BTreeMap::from([("id1".into(), cached.clone())]),
        ));
        drive.set_change_state(Box::new(state.clone()));
        let inventory = drive.scan().unwrap();
        assert_eq!(inventory, BTreeMap::from([("id1".into(), cached)]));
        assert_eq!(state.load().unwrap().unwrap().0, "cursor-2");
        let requests = handle.join().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].contains("/changes?"));
        assert!(requests[0].contains("pageToken=cursor-1"));
        assert!(requests[0].contains("includeRemoved=true"));
    }
    #[test]
    fn changed_feed_is_fully_paginated_before_atomic_full_inventory_refresh() {
        let (mut drive, handle) = fake(vec![
            (
                200,
                json!({"changes":[{"fileId":"folder","changeType":"file","file":{"id":"folder","name":"new-dir","mimeType":"application/vnd.google-apps.folder","parents":["rootid"],"version":"1"}}],"nextPageToken":"next"}).to_string(),
            ),
            (
                200,
                json!({"changes":[],"newStartPageToken":"cursor-3"}).to_string(),
            ),
            (200, root()),
            (200, json!({"files":[binary("a","file.txt")]}).to_string()),
        ]);
        let mut state = MemoryChanges::default();
        *state.0.lock().unwrap() = Some(("cursor-1".into(), BTreeMap::new()));
        drive.set_change_state(Box::new(state.clone()));
        let inventory = drive.scan().unwrap();
        assert_eq!(inventory.get("a").unwrap().path, "file.txt");
        let (cursor, saved) = state.load().unwrap().unwrap();
        assert_eq!(cursor, "cursor-3");
        assert_eq!(saved, inventory);
        let requests = handle.join().unwrap();
        assert_eq!(requests.len(), 4);
        assert!(requests[1].contains("pageToken=next"));
        assert!(requests[2].contains("/files/rootid?"));
    }
    #[test]
    fn file_changes_update_only_cached_ids_and_preserve_parent_projection() {
        let changed = json!({"fileId":"a","changeType":"file","file":{"id":"a","name":"renamed.txt","mimeType":"text/plain","parents":["folder-id"],"md5Checksum":"def","size":"3","version":"2"}});
        let removed = json!({"fileId":"gone","changeType":"file","removed":true});
        let (mut drive, handle) = fake(vec![(
            200,
            json!({"changes":[changed,removed],"newStartPageToken":"cursor-new"}).to_string(),
        )]);
        let folder = Entry {
            directory: true,
            path: "folder".into(),
            hash: String::new(),
            size: 0,
            identity: "folder-revision".into(),
        };
        let old = Entry {
            directory: false,
            path: "old.txt".into(),
            hash: "abc".into(),
            size: 3,
            identity: "revision-1".into(),
        };
        let gone = Entry {
            path: "gone.txt".into(),
            ..old.clone()
        };
        let original = BTreeMap::from([
            ("folder-id".into(), folder),
            ("a".into(), old),
            ("gone".into(), gone),
        ]);
        let mut state = MemoryChanges::default();
        *state.0.lock().unwrap() = Some(("cursor-old".into(), original));
        drive.set_change_state(Box::new(state.clone()));
        let inventory = drive.scan().unwrap();
        assert_eq!(inventory["a"].path, "folder/renamed.txt");
        assert_eq!(inventory["a"].hash, "def");
        assert!(!inventory.contains_key("gone"));
        assert!(inventory["folder-id"].directory);
        let (cursor, saved) = state.load().unwrap().unwrap();
        assert_eq!(cursor, "cursor-new");
        assert_eq!(saved, inventory);
        let requests = handle.join().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].contains("/changes?"));
    }
    #[test]
    fn known_directory_renames_and_moves_rewrite_cached_subtree_paths_by_id() {
        let target_move = json!({"fileId":"target-id","changeType":"file","file":{"id":"target-id","name":"dest","mimeType":"application/vnd.google-apps.folder","parents":["rootid"],"version":"2"}});
        let source_move = json!({"fileId":"source-id","changeType":"file","file":{"id":"source-id","name":"renamed","mimeType":"application/vnd.google-apps.folder","parents":["target-id"],"version":"3"}});
        let (mut drive, handle) = fake(vec![(
            200,
            json!({"changes":[target_move,source_move],"newStartPageToken":"cursor-new"})
                .to_string(),
        )]);
        let directory = |path: &str, identity: &str| Entry {
            directory: true,
            path: path.into(),
            hash: String::new(),
            size: 0,
            identity: identity.into(),
        };
        let file = |path: &str, identity: &str| Entry {
            directory: false,
            path: path.into(),
            hash: "abc".into(),
            size: 3,
            identity: identity.into(),
        };
        let original = BTreeMap::from([
            ("target-id".into(), directory("target", "target-v1")),
            ("source-id".into(), directory("source", "source-v1")),
            ("child-dir".into(), directory("source/sub", "child-v1")),
            ("file-id".into(), file("source/sub/file.txt", "file-v1")),
        ]);
        let mut state = MemoryChanges::default();
        *state.0.lock().unwrap() = Some(("cursor-old".into(), original));
        drive.set_change_state(Box::new(state.clone()));
        let inventory = drive.scan().unwrap();
        assert_eq!(inventory["target-id"].path, "dest");
        assert_eq!(inventory["source-id"].path, "dest/renamed");
        assert_eq!(inventory["child-dir"].path, "dest/renamed/sub");
        assert_eq!(inventory["file-id"].path, "dest/renamed/sub/file.txt");
        assert_eq!(inventory["source-id"].identity, "3");
        assert_eq!(state.load().unwrap().unwrap().1, inventory);
        assert_eq!(handle.join().unwrap().len(), 1);
    }
    #[test]
    fn incremental_name_collision_falls_back_to_safe_full_scan() {
        let changed = json!({"fileId":"b","changeType":"file","file":{"id":"b","name":"same.txt","mimeType":"text/plain","parents":["rootid"],"md5Checksum":"def","size":"3","version":"2"}});
        let (mut drive, handle) = fake(vec![
            (
                200,
                json!({"changes":[changed],"newStartPageToken":"cursor-new"}).to_string(),
            ),
            (200, root()),
            (
                200,
                json!({"files":[binary("a","same.txt"),binary("b","same.txt")]}).to_string(),
            ),
        ]);
        let original = Entry {
            directory: false,
            path: "same.txt".into(),
            hash: "abc".into(),
            size: 3,
            identity: "revision-1".into(),
        };
        let mut state = MemoryChanges::default();
        *state.0.lock().unwrap() = Some((
            "cursor-old".into(),
            BTreeMap::from([("a".into(), original.clone())]),
        ));
        drive.set_change_state(Box::new(state.clone()));
        assert!(drive.scan().is_err());
        assert_eq!(
            state.load().unwrap().unwrap(),
            (
                "cursor-old".into(),
                BTreeMap::from([("a".into(), original)])
            )
        );
        let requests = handle.join().unwrap();
        assert_eq!(requests.len(), 3);
    }
    #[test]
    fn failed_refresh_never_advances_change_cursor_or_replaces_cache() {
        let (mut drive, handle) = fake(vec![
            (
                200,
                json!({"changes":[{"fileId":"a"}],"newStartPageToken":"cursor-new"}).to_string(),
            ),
            (500, "{}".into()),
        ]);
        let cached = Entry {
            directory: false,
            path: "cached.txt".into(),
            hash: "abc".into(),
            size: 3,
            identity: "revision-1".into(),
        };
        let mut state = MemoryChanges::default();
        *state.0.lock().unwrap() = Some((
            "cursor-old".into(),
            BTreeMap::from([("old".into(), cached.clone())]),
        ));
        drive.set_change_state(Box::new(state.clone()));
        assert!(drive.scan().is_err());
        assert_eq!(
            state.load().unwrap().unwrap(),
            (
                "cursor-old".into(),
                BTreeMap::from([("old".into(), cached)])
            )
        );
        handle.join().unwrap();
    }
    #[test]
    fn initial_snapshot_gets_cursor_before_full_inventory() {
        let (mut drive, handle) = fake(vec![
            (200, json!({"startPageToken":"cursor-initial"}).to_string()),
            (200, root()),
            (200, json!({"files":[]}).to_string()),
        ]);
        let mut state = MemoryChanges::default();
        drive.set_change_state(Box::new(state.clone()));
        assert!(drive.scan().unwrap().is_empty());
        assert_eq!(
            state.load().unwrap().unwrap(),
            ("cursor-initial".into(), BTreeMap::new())
        );
        let requests = handle.join().unwrap();
        assert_eq!(requests.len(), 3);
        assert!(requests[0].contains("/changes/startPageToken?"));
    }
    #[test]
    fn rejected_cursor_is_rebased_only_after_a_complete_inventory() {
        let (mut drive, handle) = fake(vec![
            (400, "{}".into()),
            (200, json!({"startPageToken":"cursor-fresh"}).to_string()),
            (200, root()),
            (200, json!({"files":[]}).to_string()),
        ]);
        let mut state = MemoryChanges::default();
        *state.0.lock().unwrap() = Some(("cursor-stale".into(), BTreeMap::new()));
        drive.set_change_state(Box::new(state.clone()));
        assert!(drive.scan().unwrap().is_empty());
        assert_eq!(state.load().unwrap().unwrap().0, "cursor-fresh");
        let requests = handle.join().unwrap();
        assert_eq!(requests.len(), 4);
        assert!(requests[1].contains("/changes/startPageToken?"));
    }
    #[test]
    fn interrupted_media_body_is_retryable_without_exposing_transport_details() {
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("private transport details"))
            }
        }
        let mut reader = NetworkReader(Broken);
        let error = anyhow::Error::from(reader.read(&mut [0; 1]).unwrap_err())
            .context("Staging parcial preservado");
        assert_eq!(retry_minimum(&error), Some(Duration::ZERO));
        assert!(!format!("{error:#}").contains("private transport details"));
    }
    #[test]
    fn reference_handler_rejects_external_urls_and_invalid_ids() {
        assert!(reference_url(br#"{"doc_id":"doc","url":"https://evil.example/"}"#).is_err());
        assert!(
            reference_url(br#"{"doc_id":"../x","url":"https://drive.google.com/open?id=../x"}"#)
                .is_err()
        );
    }
    #[test]
    fn projected_native_name_collision_blocks_inventory() {
        let(mut drive,handle)=fake(vec![(200,root()),(200,json!({"files":[{"id":"doc","name":"Doc","mimeType":"application/vnd.google-apps.document","parents":["rootid"],"version":"1"},binary("binary","Doc.gdoc")]}).to_string())]);
        assert!(drive.scan().is_err());
        handle.join().unwrap();
    }
    #[test]
    fn missing_permission_is_unknown_not_deleted() {
        for code in [403, 404] {
            let (mut drive, handle) = fake(vec![(code, "{}".into())]);
            assert!(matches!(drive.status("missing").unwrap(), Status::Unknown));
            handle.join().unwrap();
        }
    }
    #[test]
    fn only_explicit_trashed_metadata_proves_deletion() {
        let (mut drive, handle) = fake(vec![(
            200,
            json!({"id":"a","name":"a","mimeType":"text/plain","trashed":true}).to_string(),
        )]);
        assert!(matches!(drive.status("a").unwrap(), Status::Trashed));
        handle.join().unwrap();
    }
    #[test]
    fn quota_errors_do_not_become_absence() {
        let (mut drive, handle) = fake(vec![(429, "{}".into())]);
        assert!(drive.status("a").is_err());
        handle.join().unwrap();
    }
    #[test]
    fn status_detects_name_changes_after_scan() {
        let (mut drive, handle) = fake(vec![(200, binary("a", "renamed.txt").to_string())]);
        drive.paths.insert("a".into(), "old.txt".into());
        assert!(matches!(drive.status("a").unwrap(),Status::Present(e) if e.path=="renamed.txt"));
        handle.join().unwrap();
    }
}

#[cfg(test)]
mod mutation_tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use std::{
        io::{Seek, SeekFrom, Write},
        net::TcpListener,
        thread,
    };
    fn upload_fixture(
        existing: bool,
    ) -> (
        Drive,
        File,
        Entry,
        thread::JoinHandle<Vec<Vec<u8>>>,
        std::path::PathBuf,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server_url = base.clone();
        let target = Entry {
            directory: false,
            path: "a.txt".into(),
            hash: "900150983cd24fb0d6963f7d28e17f72".into(),
            size: 3,
            identity: "1".into(),
        };
        let meta=json!({"id":"reservedid","name":"a.txt","mimeType":"text/plain","parents":["rootid"],"md5Checksum":target.hash,"size":"3","version":"1"}).to_string();
        let handle = thread::spawn(move || {
            let mut requests = Vec::new();
            for index in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut header = Vec::new();
                let mut byte = [0u8; 1];
                while !header.ends_with(b"\r\n\r\n") {
                    stream.read_exact(&mut byte).unwrap();
                    header.push(byte[0]);
                }
                let text = String::from_utf8(header.clone()).unwrap();
                let length = text
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|s| s.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                let mut body = vec![0u8; length];
                stream.read_exact(&mut body).unwrap();
                header.extend(body);
                requests.push(header);
                let (status, extra, body) = match index {
                    0 if existing => (200, "ETag: \"v1\"\r\n".to_string(), meta.clone()),
                    0 => (404, String::new(), "{}".into()),
                    1 => (
                        200,
                        format!("Location: {server_url}/session\r\n"),
                        String::new(),
                    ),
                    _ => (200, String::new(), meta.clone()),
                };
                let response = format!(
                    "HTTP/1.1 {status} Test\r\n{extra}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
            requests
        });
        let path = std::env::temp_dir().join(format!("drive-http-{}", uuid::Uuid::new_v4()));
        let mut file = File::create(&path).unwrap();
        file.write_all(b"abc").unwrap();
        drop(file);
        let mut file = File::open(&path).unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        let drive = Drive {
            upload_sessions: None,
            change_state: None,
            progress: None,
            client: Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap(),
            session: oauth::Session::synthetic(),
            root: "rootid".into(),
            paths: BTreeMap::new(),
            directories: BTreeMap::new(),
            base_url: base.clone(),
            upload_url: base,
        };
        (drive, file, target, handle, path)
    }
    #[test]
    fn new_upload_uses_reserved_id_and_streams_exact_bytes() {
        let (mut drive, file, target, server, path) = upload_fixture(false);
        let uploaded = drive.upload("reservedid", None, &target, file).unwrap();
        assert!(uploaded.same_file(&target));
        let requests = server.join().unwrap();
        let initiation = String::from_utf8_lossy(requests.get(1).unwrap());
        assert!(initiation.starts_with("POST /files?"));
        assert!(initiation.contains("\"id\":\"reservedid\""));
        assert!(requests.get(2).unwrap().ends_with(b"abc"));
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn update_upload_checks_revision_and_sends_if_match_on_both_requests() {
        let (mut drive, file, target, server, path) = upload_fixture(true);
        drive
            .upload("reservedid", Some(&target), &target, file)
            .unwrap();
        let requests = server.join().unwrap();
        for request in requests.iter().skip(1) {
            assert!(
                String::from_utf8_lossy(request)
                    .to_ascii_lowercase()
                    .contains("if-match: \"v1\"")
            );
        }
        assert!(
            String::from_utf8_lossy(requests.get(1).unwrap())
                .starts_with("PATCH /files/reservedid?")
        );
        std::fs::remove_file(path).unwrap();
    }
}
