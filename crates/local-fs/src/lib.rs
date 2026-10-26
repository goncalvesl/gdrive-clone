//! Linux filesystem adapter: anchored directory descriptors and no symlink traversal.
use anyhow::{Context, Result, bail, ensure};
use md5::{Digest, Md5};
use rustix::fs::{Mode, OFlags, RenameFlags, mkdirat, openat, renameat_with};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    mem::MaybeUninit,
    os::fd::AsRawFd,
    os::unix::fs::MetadataExt,
    path::{Component, Path},
};
use sync_core::mirror::Entry;

pub const RECOVERY: &str = ".gdrive-clone-recovery";

/// Linux filesystem change notifications used only to wake the periodic sync loop.
pub struct LocalWatcher {
    fd: rustix::fd::OwnedFd,
}
impl LocalWatcher {
    pub fn open(root: &Path) -> Result<Self> {
        let fd = rustix::fs::inotify::init(
            rustix::fs::inotify::CreateFlags::CLOEXEC | rustix::fs::inotify::CreateFlags::NONBLOCK,
        )?;
        let root = root.canonicalize()?;
        let device = std::fs::metadata(&root)?.dev();
        Self::add_tree(&fd, &root, &root, device, 0)?;
        Ok(Self { fd })
    }
    fn add_tree(
        fd: &rustix::fd::OwnedFd,
        root: &Path,
        path: &Path,
        device: u64,
        depth: usize,
    ) -> Result<()> {
        ensure!(depth < 128, "Árvore de observação profunda demais");
        rustix::fs::inotify::add_watch(
            fd,
            path,
            rustix::fs::inotify::WatchFlags::ATTRIB
                | rustix::fs::inotify::WatchFlags::CLOSE_WRITE
                | rustix::fs::inotify::WatchFlags::CREATE
                | rustix::fs::inotify::WatchFlags::DELETE
                | rustix::fs::inotify::WatchFlags::DELETE_SELF
                | rustix::fs::inotify::WatchFlags::MODIFY
                | rustix::fs::inotify::WatchFlags::MOVE_SELF
                | rustix::fs::inotify::WatchFlags::MOVED_FROM
                | rustix::fs::inotify::WatchFlags::MOVED_TO
                | rustix::fs::inotify::WatchFlags::DONT_FOLLOW
                | rustix::fs::inotify::WatchFlags::ONLYDIR,
        )?;
        for item in std::fs::read_dir(path)? {
            let item = item?;
            if path == root && item.file_name() == RECOVERY {
                continue;
            }
            if item.file_type()?.is_dir() {
                let child = item.path();
                if std::fs::metadata(&child)?.dev() == device {
                    Self::add_tree(fd, root, &child, device, depth + 1)?;
                }
            }
        }
        Ok(())
    }
    /// Returns true when an event arrived; the caller should rebuild watches after it.
    pub fn wait(&self, timeout: std::time::Duration) -> Result<bool> {
        let timespec = rustix::event::Timespec::try_from(timeout)
            .map_err(|_| anyhow::anyhow!("Intervalo de observação inválido"))?;
        let mut descriptors = [rustix::event::PollFd::new(
            &self.fd,
            rustix::event::PollFlags::IN,
        )];
        if rustix::event::poll(&mut descriptors, Some(&timespec))? == 0 {
            return Ok(false);
        }
        let mut storage = [MaybeUninit::uninit(); 4096];
        let mut reader = rustix::fs::inotify::Reader::new(&self.fd, &mut storage);
        let mut changed = false;
        loop {
            match reader.next() {
                Ok(_) => changed = true,
                Err(error) if error == rustix::io::Errno::AGAIN => break,
                Err(error) => return Err(error.into()),
            }
        }
        Ok(changed)
    }
}

pub fn validate_path(path: &str) -> Result<()> {
    ensure!(
        !path.is_empty() && !path.contains('\0') && !path.contains('\\'),
        "Caminho inválido"
    );
    ensure!(
        path.split('/')
            .all(|s| !s.is_empty() && s != "." && s != ".." && s != RECOVERY),
        "Nome reservado ou caminho inseguro"
    );
    ensure!(
        Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_))),
        "Caminho fora da raiz"
    );
    Ok(())
}
fn identity(file: &File) -> Result<String> {
    let m = file.metadata()?;
    Ok(format!("{}:{}", m.dev(), m.ino()))
}
fn digest(file: &mut File) -> Result<(String, u64)> {
    file.seek(SeekFrom::Start(0))?;
    let mut hash = Md5::new();
    let mut count = 0u64;
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(buf.get(..n).context("Buffer inválido")?);
        count = count
            .checked_add(n as u64)
            .context("Arquivo grande demais")?;
    }
    file.seek(SeekFrom::Start(0))?;
    Ok((format!("{:x}", hash.finalize()), count))
}
pub struct Root {
    dir: File,
    pub identity: String,
}
impl Root {
    pub fn open(path: &Path, expected: Option<&str>) -> Result<Self> {
        ensure!(path.is_absolute(), "Escolha um caminho local absoluto");
        let dir = File::from(openat(
            rustix::fs::CWD,
            path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?);
        let id = identity(&dir)?;
        if let Some(expected) = expected {
            ensure!(
                id == expected,
                "A pasta local mudou ou foi desmontada; sincronização bloqueada"
            );
        }
        Ok(Self { dir, identity: id })
    }
    fn parent(&self, path: &str, create: bool) -> Result<(File, String)> {
        validate_path(path)?;
        let mut parts: Vec<_> = path.split('/').collect();
        let name = parts.pop().context("Nome ausente")?.to_string();
        let mut parent = self.dir.try_clone()?;
        for part in parts {
            if create {
                match mkdirat(&parent, part, Mode::from_bits_truncate(0o700)) {
                    Ok(()) => {}
                    Err(e) if e == rustix::io::Errno::EXIST => {}
                    Err(e) => return Err(e.into()),
                }
            }
            parent = File::from(openat(
                &parent,
                part,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )?);
            ensure!(
                parent.metadata()?.dev() == self.dir.metadata()?.dev(),
                "Submontagem não suportada"
            );
        }
        Ok((parent, name))
    }
    pub fn read(&self, path: &str) -> Result<File> {
        let (parent, name) = self.parent(path, false)?;
        let file = File::from(openat(
            &parent,
            name.as_str(),
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
        )?);
        ensure!(file.metadata()?.is_file(), "Tipo de arquivo não suportado");
        ensure!(
            file.metadata()?.dev() == self.dir.metadata()?.dev(),
            "Submontagem não suportada"
        );
        Ok(file)
    }
    fn open_directory(&self, path: &str) -> Result<File> {
        let (parent, name) = self.parent(path, false)?;
        let file = File::from(openat(
            &parent,
            name.as_str(),
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?);
        ensure!(
            file.metadata()?.dev() == self.dir.metadata()?.dev(),
            "Submontagem não suportada"
        );
        Ok(file)
    }
    pub fn install_directory(&self, target: &Entry, old: Option<&Entry>) -> Result<Entry> {
        ensure!(target.directory, "Destino não é diretório");
        let (parent, name) = self.parent(&target.path, false)?;
        if let Some(old) = old {
            ensure!(
                old.directory && self.inspect(&old.path)?.as_ref() == Some(old),
                "Pasta de origem mudou"
            );
            if old.path != target.path {
                let (source, source_name) = self.parent(&old.path, false)?;
                renameat_with(
                    &source,
                    source_name.as_str(),
                    &parent,
                    name.as_str(),
                    RenameFlags::NOREPLACE,
                )
                .context("Destino ocupado; nenhuma pasta substituída")?;
                source.sync_all()?;
            }
        } else {
            mkdirat(&parent, name.as_str(), Mode::from_bits_truncate(0o700))?;
        }
        parent.sync_all()?;
        self.inspect(&target.path)?
            .context("Pasta instalada desapareceu")
    }
    pub fn inspect(&self, path: &str) -> Result<Option<Entry>> {
        match self.open_directory(path) {
            Ok(file) => {
                return Ok(Some(Entry {
                    directory: true,
                    path: path.into(),
                    hash: String::new(),
                    size: 0,
                    identity: identity(&file)?,
                }));
            }
            Err(error)
                if error.downcast_ref::<rustix::io::Errno>()
                    == Some(&rustix::io::Errno::NOTDIR) => {}
            Err(error)
                if error.downcast_ref::<rustix::io::Errno>() == Some(&rustix::io::Errno::NOENT) =>
            {
                return Ok(None);
            }
            Err(error) => return Err(error),
        }
        let mut file = match self.read(path) {
            Ok(f) => f,
            Err(e) if e.downcast_ref::<rustix::io::Errno>() == Some(&rustix::io::Errno::NOENT) => {
                return Ok(None);
            }
            Err(e) => return Err(e),
        };
        let before = file.metadata()?;
        let (hash, size) = digest(&mut file)?;
        let after = file.metadata()?;
        ensure!(
            before.len() == after.len()
                && before.mtime() == after.mtime()
                && before.mtime_nsec() == after.mtime_nsec()
                && before.ctime() == after.ctime()
                && before.ctime_nsec() == after.ctime_nsec(),
            "Arquivo mudou durante leitura"
        );
        Ok(Some(Entry {
            directory: false,
            path: path.into(),
            hash,
            size,
            identity: identity(&file)?,
        }))
    }
    pub fn scan(&self) -> Result<BTreeMap<String, Entry>> {
        let mut found = BTreeMap::new();
        self.walk(&self.dir, "", 0, &mut found)?;
        let mut identities = std::collections::BTreeSet::new();
        ensure!(
            found
                .values()
                .all(|e| identities.insert(e.identity.clone())),
            "Hard links não suportados: inventário bloqueado"
        );
        Ok(found)
    }
    fn walk(
        &self,
        dir: &File,
        prefix: &str,
        depth: usize,
        found: &mut BTreeMap<String, Entry>,
    ) -> Result<()> {
        ensure!(depth < 128, "Árvore de pastas profunda demais");
        for item in std::fs::read_dir(format!("/proc/self/fd/{}", dir.as_raw_fd()))? {
            let item = item?;
            let name = item
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("Nome não UTF-8 não suportado"))?;
            if prefix.is_empty() && name == RECOVERY {
                continue;
            }
            let path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            validate_path(&path)?;
            let kind = item.file_type()?;
            if kind.is_dir() {
                let child = File::from(openat(
                    dir,
                    name.as_str(),
                    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                    Mode::empty(),
                )?);
                ensure!(
                    child.metadata()?.dev() == self.dir.metadata()?.dev(),
                    "Submontagem não suportada"
                );
                found.insert(
                    path.clone(),
                    Entry {
                        directory: true,
                        path: path.clone(),
                        hash: String::new(),
                        size: 0,
                        identity: identity(&child)?,
                    },
                );
                self.walk(&child, &path, depth + 1, found)?;
            } else if kind.is_file() {
                let file = self
                    .inspect(&path)?
                    .context("Arquivo desapareceu durante inventário")?;
                found.insert(path, file);
            } else {
                bail!("Link simbólico ou tipo especial bloqueia inventário: {path}");
            }
        }
        Ok(())
    }
    fn recovery(&self) -> Result<File> {
        match mkdirat(&self.dir, RECOVERY, Mode::from_bits_truncate(0o700)) {
            Ok(()) => {}
            Err(e) if e == rustix::io::Errno::EXIST => {}
            Err(e) => return Err(e.into()),
        }
        Ok(File::from(openat(
            &self.dir,
            RECOVERY,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?))
    }
    fn recovery_name(op: &str, suffix: &str) -> Result<String> {
        ensure!(
            !op.is_empty() && op.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
            "ID de operação inválido"
        );
        Ok(format!("{op}-{suffix}"))
    }
    /// Retains the old file before any replacement; never replaces a newer name.
    pub fn quarantine(&self, op: &str, old: &Entry) -> Result<()> {
        ensure!(!old.directory, "Quarentena de diretórios não suportada");
        let recovery = self.recovery()?;
        let backup = Self::recovery_name(op, "old")?;
        let (parent, name) = self.parent(&old.path, false)?;
        match renameat_with(
            &parent,
            name.as_str(),
            &recovery,
            backup.as_str(),
            RenameFlags::NOREPLACE,
        ) {
            Ok(()) => {}
            Err(e) if e == rustix::io::Errno::EXIST || e == rustix::io::Errno::NOENT => {}
            Err(e) => return Err(e.into()),
        }
        let mut saved = File::from(openat(
            &recovery,
            backup.as_str(),
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?);
        ensure!(!old.directory, "Quarentena de diretórios não suportada");
        ensure!(saved.metadata()?.is_file(), "Backup inesperado");
        let (hash, size) = digest(&mut saved)?;
        saved.sync_all()?;
        recovery.sync_all()?;
        parent.sync_all()?;
        ensure!(
            hash == old.hash && size == old.size,
            "Edição concorrente preservada na pasta de recuperação; revise antes de continuar"
        );
        Ok(())
    }
    pub fn install(
        &self,
        op: &str,
        target: &Entry,
        old: Option<&Entry>,
        source: impl Read,
    ) -> Result<Entry> {
        ensure!(
            !target.directory && old.is_none_or(|e| !e.directory),
            "Transferência de arquivo não aceita diretório"
        );
        let recovery = self.recovery()?;
        let temp = Self::recovery_name(op, "download")?;
        let mut output = File::from(openat(
            &recovery,
            temp.as_str(),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_bits_truncate(0o600),
        )?);
        let limit = target
            .size
            .checked_add(1)
            .context("Tamanho de download inválido")?;
        std::io::copy(&mut source.take(limit), &mut output)?;
        output.sync_all()?;
        drop(output);
        self.finish_download(&recovery, &temp, op, target, old)
    }
    pub fn partial_download_len(&self, op: &str, target: &Entry) -> Result<u64> {
        ensure!(!target.directory, "Diretório não tem download parcial");
        let recovery = self.recovery()?;
        let name = Self::recovery_name(op, "download")?;
        let file = match openat(
            &recovery,
            name.as_str(),
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        ) {
            Ok(fd) => File::from(fd),
            Err(e) if e == rustix::io::Errno::NOENT => return Ok(0),
            Err(e) => return Err(e.into()),
        };
        let meta = file.metadata()?;
        ensure!(
            meta.is_file() && meta.nlink() == 1,
            "Download parcial não é arquivo privado regular"
        );
        ensure!(
            meta.dev() == self.dir.metadata()?.dev(),
            "Submontagem não suportada"
        );
        ensure!(
            meta.len() <= target.size,
            "Download parcial excede tamanho esperado"
        );
        Ok(meta.len())
    }
    /// Appends only the missing suffix to a private staging file owned by this operation.
    pub fn install_resuming(
        &self,
        op: &str,
        target: &Entry,
        old: Option<&Entry>,
        offset: u64,
        source: impl Read,
    ) -> Result<Entry> {
        ensure!(
            !target.directory && old.is_none_or(|e| !e.directory),
            "Transferência de arquivo não aceita diretório"
        );
        ensure!(offset <= target.size, "Offset de download inválido");
        let recovery = self.recovery()?;
        let temp = Self::recovery_name(op, "download")?;
        let mut output = if offset == 0 {
            match openat(
                &recovery,
                temp.as_str(),
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::from_bits_truncate(0o600),
            ) {
                Ok(fd) => File::from(fd),
                Err(e) if e == rustix::io::Errno::EXIST => File::from(openat(
                    &recovery,
                    temp.as_str(),
                    OFlags::WRONLY | OFlags::APPEND | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                    Mode::empty(),
                )?),
                Err(e) => return Err(e.into()),
            }
        } else {
            File::from(openat(
                &recovery,
                temp.as_str(),
                OFlags::WRONLY | OFlags::APPEND | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )?)
        };
        let meta = output.metadata()?;
        ensure!(
            meta.is_file() && meta.nlink() == 1 && meta.len() == offset,
            "Download parcial mudou ou não é privado"
        );
        ensure!(
            meta.dev() == self.dir.metadata()?.dev(),
            "Submontagem não suportada"
        );
        let limit = (target.size - offset)
            .checked_add(1)
            .context("Tamanho de download inválido")?;
        std::io::copy(&mut source.take(limit), &mut output)?;
        output.sync_all()?;
        drop(output);
        self.finish_download(&recovery, &temp, op, target, old)
    }
    fn finish_download(
        &self,
        recovery: &File,
        temp: &str,
        op: &str,
        target: &Entry,
        old: Option<&Entry>,
    ) -> Result<Entry> {
        let mut check = File::from(openat(
            recovery,
            temp,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?);
        let (hash, size) = digest(&mut check)?;
        if hash != target.hash || size != target.size {
            if size >= target.size {
                self.discard_partial_download(op)?;
            }
            bail!("Download com checksum ou tamanho divergente");
        }
        if let Some(old) = old {
            self.quarantine(op, old)?;
        }
        let (parent, name) = self.parent(&target.path, true)?;
        renameat_with(
            recovery,
            temp,
            &parent,
            name.as_str(),
            RenameFlags::NOREPLACE,
        )
        .context("Destino já existe; arquivo preservado em recuperação")?;
        parent.sync_all()?;
        recovery.sync_all()?;
        self.inspect(&target.path)?
            .context("Arquivo instalado desapareceu")
    }
    /// Private immutable upload snapshot; original edits never alter the HTTP body.
    pub fn spool(&self, op: &str, expected: &Entry) -> Result<File> {
        ensure!(!expected.directory, "Diretório não tem corpo de upload");
        let recovery = self.recovery()?;
        let name = Self::recovery_name(op, "upload")?;
        let opened = openat(
            &recovery,
            name.as_str(),
            OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_bits_truncate(0o600),
        );
        let mut file = match opened {
            Ok(fd) => {
                let mut f = File::from(fd);
                std::io::copy(&mut self.read(&expected.path)?, &mut f)?;
                f.sync_all()?;
                recovery.sync_all()?;
                f
            }
            Err(e) if e == rustix::io::Errno::EXIST => File::from(openat(
                &recovery,
                name.as_str(),
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )?),
            Err(e) => return Err(e.into()),
        };
        let (hash, size) = digest(&mut file)?;
        ensure!(
            hash == expected.hash && size == expected.size,
            "Origem mudou; upload bloqueado"
        );
        Ok(file)
    }
    pub fn discard_partial_download(&self, op: &str) -> Result<()> {
        let recovery = self.recovery()?;
        let name = Self::recovery_name(op, "download")?;
        match rustix::fs::unlinkat(&recovery, name.as_str(), rustix::fs::AtFlags::empty()) {
            Ok(()) => Ok(()),
            Err(e) if e == rustix::io::Errno::NOENT => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

impl Root {
    pub fn cleanup_upload(&self, op: &str) -> Result<()> {
        let recovery = self.recovery()?;
        let name = Self::recovery_name(op, "upload")?;
        match rustix::fs::unlinkat(&recovery, name.as_str(), rustix::fs::AtFlags::empty()) {
            Ok(()) => {
                recovery.sync_all()?;
                Ok(())
            }
            Err(e) if e == rustix::io::Errno::NOENT => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}
