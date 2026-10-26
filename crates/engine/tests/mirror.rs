#![allow(clippy::unwrap_used, clippy::panic)]
use anyhow::{Result, bail};
use drive_api::{Remote, Status};
use engine::synchronize;
use local_fs::Root;
use state_store::Store;
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Cursor, Read},
    path::PathBuf,
};
use sync_core::mirror::Entry;
use uuid::Uuid;

struct Fixture {
    dir: PathBuf,
    root: Root,
    store: Store,
    remote: Mock,
}
impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("drive-e2e-{}", Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("local")).unwrap();
        let root = Root::open(&dir.join("local"), None).unwrap();
        let mut store = Store::open(dir.join("state.sqlite")).unwrap();
        store.init_mirror().unwrap();
        Self {
            dir,
            root,
            store,
            remote: Mock::default(),
        }
    }
    fn write(&self, path: &str, bytes: &[u8]) {
        let full = self.dir.join("local").join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, bytes).unwrap();
    }
    fn run(&mut self) -> Result<engine::Report> {
        synchronize(&self.root, &mut self.remote, &mut self.store, false, true)
    }
    fn seed(&mut self) {
        self.write("a.txt", b"first");
        self.run().unwrap();
    }
    fn remote_entry(&self, path: &str) -> Entry {
        self.root.inspect(path).unwrap().unwrap()
    }
    fn change_remote(&mut self, id: &str, bytes: &[u8], path: &str) {
        self.write("hash-fixture", bytes);
        let mut entry = self.remote_entry("hash-fixture");
        std::fs::remove_file(self.dir.join("local/hash-fixture")).unwrap();
        entry.path = path.into();
        entry.identity = Uuid::new_v4().to_string();
        self.remote.files.insert(id.into(), (entry, bytes.to_vec()));
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
#[derive(Default)]
struct Mock {
    files: BTreeMap<String, (Entry, Vec<u8>)>,
    trash: BTreeMap<String, ()>,
    uploads: usize,
    fail_after_upload: bool,
    fail_scan: bool,
    fail_before_upload: bool,
    directory_uploads: usize,
    fail_directory_before: bool,
    fail_directory_after: bool,
    reserved: usize,
    pause_after_upload: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    fail_download_once: bool,
    download_offsets: Vec<u64>,
}
impl Remote for Mock {
    fn scan(&mut self) -> Result<BTreeMap<String, Entry>> {
        if self.fail_scan {
            bail!("incomplete scan");
        }
        Ok(self
            .files
            .iter()
            .map(|(k, (e, _))| (k.clone(), e.clone()))
            .collect())
    }
    fn status(&mut self, id: &str) -> Result<Status> {
        Ok(if let Some((e, _)) = self.files.get(id) {
            Status::Present(e.clone())
        } else if self.trash.contains_key(id) {
            Status::Trashed
        } else {
            Status::Unknown
        })
    }
    fn reserve_id(&mut self) -> Result<String> {
        self.reserved += 1;
        Ok(Uuid::new_v4().to_string())
    }
    fn download(&mut self, id: &str, _: &Entry) -> Result<Box<dyn Read>> {
        Ok(Box::new(Cursor::new(self.files.get(id).unwrap().1.clone())))
    }
    fn download_from(&mut self, id: &str, _: &Entry, offset: u64) -> Result<Box<dyn Read>> {
        self.download_offsets.push(offset);
        let bytes = self.files.get(id).unwrap().1.clone();
        if self.fail_download_once {
            self.fail_download_once = false;
            struct Interrupted(Cursor<Vec<u8>>);
            impl Read for Interrupted {
                fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                    if self.0.position() >= 2 {
                        return Err(std::io::Error::other("interrupted response"));
                    }
                    let count = buffer.len().min(2);
                    self.0.read(&mut buffer[..count])
                }
            }
            return Ok(Box::new(Interrupted(Cursor::new(bytes))));
        }
        Ok(Box::new(Cursor::new(bytes[offset as usize..].to_vec())))
    }
    fn upload(
        &mut self,
        id: &str,
        _: Option<&Entry>,
        target: &Entry,
        mut source: File,
    ) -> Result<Entry> {
        if self.fail_before_upload {
            bail!("offline before effect");
        }
        let mut bytes = Vec::new();
        source.read_to_end(&mut bytes)?;
        let mut entry = target.clone();
        entry.identity = Uuid::new_v4().to_string();
        self.files.insert(id.into(), (entry.clone(), bytes));
        self.uploads += 1;
        if let Some(paused) = &self.pause_after_upload {
            paused.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        if self.fail_after_upload {
            self.fail_after_upload = false;
            bail!("lost response after upload");
        }
        Ok(entry)
    }
    fn put_directory(
        &mut self,
        id: &str,
        expected: Option<&Entry>,
        target: &Entry,
    ) -> Result<Entry> {
        if self.fail_directory_before {
            bail!("offline before directory effect");
        }
        if let Some(old) = expected {
            let prefix = format!("{}/", old.path);
            for (entry, _) in self.files.values_mut() {
                if let Some(rest) = entry.path.strip_prefix(&prefix) {
                    entry.path = format!("{}/{}", target.path, rest);
                }
            }
        }
        let mut entry = target.clone();
        entry.identity = Uuid::new_v4().to_string();
        self.files.insert(id.into(), (entry.clone(), Vec::new()));
        self.directory_uploads += 1;
        if self.fail_directory_after {
            self.fail_directory_after = false;
            bail!("lost response after folder effect");
        }
        Ok(entry)
    }
    fn trash(&mut self, id: &str, _: &Entry) -> Result<()> {
        self.files.remove(id);
        self.trash.insert(id.into(), ());
        Ok(())
    }
}
#[test]
fn upload_download_edits_and_checkpoint_converge() {
    let mut f = Fixture::new();
    f.seed();
    let id = f.remote.files.keys().next().unwrap().clone();
    assert_eq!(f.remote.files.get(&id).unwrap().1, b"first");
    f.change_remote(&id, b"remote edit", "a.txt");
    f.run().unwrap();
    assert_eq!(
        std::fs::read(f.dir.join("local/a.txt")).unwrap(),
        b"remote edit"
    );
    f.write("a.txt", b"local edit");
    f.run().unwrap();
    assert_eq!(f.remote.files.get(&id).unwrap().1, b"local edit");
    assert!(f.run().unwrap().planned.is_empty());
    assert!(f.store.intents().unwrap().is_empty());
}
#[test]
fn interrupted_download_resumes_from_private_staging_after_reopen() {
    let mut f = Fixture::new();
    f.change_remote("remote1", b"abcdef", "resume.bin");
    f.remote.fail_download_once = true;
    assert!(f.run().is_err());
    let operation = f.store.intents().unwrap().remove(0).operation;
    assert_eq!(
        f.root
            .partial_download_len(&operation.operation_id, operation.remote.as_ref().unwrap())
            .unwrap(),
        2
    );
    assert!(!f.dir.join("local/resume.bin").exists());
    f.store = Store::open(f.dir.join("state.sqlite")).unwrap();
    assert_eq!(f.run().unwrap().recovered, 1);
    assert_eq!(f.remote.download_offsets, vec![0, 2]);
    assert_eq!(
        std::fs::read(f.dir.join("local/resume.bin")).unwrap(),
        b"abcdef"
    );
    assert!(f.store.intents().unwrap().is_empty());
}
#[test]
fn interrupted_accepted_upload_is_inspected_not_created_twice() {
    let mut f = Fixture::new();
    f.write("a.txt", b"first");
    f.remote.fail_after_upload = true;
    assert!(f.run().is_err());
    assert_eq!(f.remote.uploads, 1);
    assert!(f.store.links().unwrap().is_empty());
    f.store = Store::open(f.dir.join("state.sqlite")).unwrap();
    let report = f.run().unwrap();
    assert_eq!(report.recovered, 1);
    assert_eq!(f.remote.uploads, 1);
    assert_eq!(f.store.links().unwrap().len(), 1);
}
#[test]
fn interruption_before_effect_revalidates_and_retries_same_reserved_id() {
    let mut f = Fixture::new();
    f.write("a.txt", b"first");
    f.remote.fail_before_upload = true;
    assert!(f.run().is_err());
    let id = f
        .store
        .intents()
        .unwrap()
        .first()
        .unwrap()
        .operation
        .object_id
        .clone();
    f.remote.fail_before_upload = false;
    f.run().unwrap();
    assert!(f.remote.files.contains_key(&id));
    assert_eq!(f.remote.uploads, 1);
}
#[test]
fn source_changed_while_offline_stays_blocked_without_upload() {
    let mut f = Fixture::new();
    f.write("a.txt", b"first");
    f.remote.fail_before_upload = true;
    assert!(f.run().is_err());
    f.write("a.txt", b"changed");
    f.remote.fail_before_upload = false;
    let report = f.run().unwrap();
    assert!(!report.blocked.is_empty());
    assert_eq!(f.remote.uploads, 0);
    assert!(!f.store.intents().unwrap().is_empty());
}
#[test]
fn concurrent_edits_preserve_both_versions() {
    let mut f = Fixture::new();
    f.seed();
    let id = f.remote.files.keys().next().unwrap().clone();
    f.write("a.txt", b"local");
    f.change_remote(&id, b"remote", "a.txt");
    let report = f.run().unwrap();
    assert_eq!(report.conflicts, vec!["a.txt"]);
    assert_eq!(std::fs::read(f.dir.join("local/a.txt")).unwrap(), b"local");
    assert_eq!(f.remote.files.get(&id).unwrap().1, b"remote");
}
#[test]
fn local_rename_preserves_remote_id() {
    let mut f = Fixture::new();
    f.seed();
    let id = f.remote.files.keys().next().unwrap().clone();
    std::fs::rename(f.dir.join("local/a.txt"), f.dir.join("local/b.txt")).unwrap();
    f.run().unwrap();
    assert_eq!(f.remote.files.len(), 1);
    assert_eq!(f.remote.files.get(&id).unwrap().0.path, "b.txt");
}
#[test]
fn remote_rename_preserves_old_local_content_in_recovery() {
    let mut f = Fixture::new();
    f.seed();
    let id = f.remote.files.keys().next().unwrap().clone();
    f.change_remote(&id, b"first", "sub/b.txt");
    f.run().unwrap();
    assert!(!f.dir.join("local/a.txt").exists());
    assert_eq!(
        std::fs::read(f.dir.join("local/sub/b.txt")).unwrap(),
        b"first"
    );
    assert!(
        std::fs::read_dir(f.dir.join("local/.gdrive-clone-recovery"))
            .unwrap()
            .any(|e| e.unwrap().file_name().to_string_lossy().ends_with("-old"))
    );
}
#[test]
fn local_deletion_uses_remote_trash() {
    let mut f = Fixture::new();
    f.seed();
    let id = f.remote.files.keys().next().unwrap().clone();
    std::fs::remove_file(f.dir.join("local/a.txt")).unwrap();
    f.run().unwrap();
    assert!(f.remote.trash.contains_key(&id));
    assert!(f.store.links().unwrap().is_empty());
}
#[test]
fn explicit_remote_trash_quarantines_local_file() {
    let mut f = Fixture::new();
    f.seed();
    let id = f.remote.files.keys().next().unwrap().clone();
    f.remote.files.remove(&id);
    f.remote.trash.insert(id, ());
    f.run().unwrap();
    assert!(!f.dir.join("local/a.txt").exists());
    assert!(f.store.links().unwrap().is_empty());
}
#[test]
fn loss_of_remote_access_is_not_deletion() {
    let mut f = Fixture::new();
    f.seed();
    f.remote.files.clear();
    let report = f.run().unwrap();
    assert!(!report.blocked.is_empty());
    assert!(f.dir.join("local/a.txt").exists());
    assert_eq!(f.store.links().unwrap().len(), 1);
}
#[test]
fn failed_inventory_never_applies_local_deletion() {
    let mut f = Fixture::new();
    f.seed();
    std::fs::remove_file(f.dir.join("local/a.txt")).unwrap();
    f.remote.fail_scan = true;
    assert!(f.run().is_err());
    assert!(f.remote.trash.is_empty());
}
#[test]
fn preview_has_no_effects_or_reserved_remote_ids() {
    let mut f = Fixture::new();
    f.write("a.txt", b"first");
    let report = synchronize(&f.root, &mut f.remote, &mut f.store, true, true).unwrap();
    assert_eq!(report.planned.len(), 1);
    assert!(f.remote.files.is_empty());
    assert!(f.store.intents().unwrap().is_empty());
}
#[test]
fn mismatched_initial_files_conflict_instead_of_overwriting() {
    let mut f = Fixture::new();
    f.write("a.txt", b"local");
    f.change_remote("remote-id", b"remote", "a.txt");
    let report = f.run().unwrap();
    assert_eq!(report.conflicts.len(), 1);
    assert_eq!(f.remote.uploads, 0);
}
#[test]
fn identical_initial_files_are_adopted_and_future_edits_work() {
    let mut f = Fixture::new();
    f.write("a.txt", b"same");
    f.change_remote("remote-id", b"same", "a.txt");
    f.run().unwrap();
    assert_eq!(f.store.links().unwrap().len(), 1);
    assert_eq!(f.remote.uploads, 0);
    f.write("a.txt", b"edited");
    f.run().unwrap();
    assert_eq!(f.remote.uploads, 1);
}

#[test]
fn resolving_conflict_keeps_both_versions_and_uploads_visible_copy() {
    let mut f = Fixture::new();
    f.seed();
    let id = f.remote.files.keys().next().unwrap().clone();
    f.write("a.txt", b"local-conflict");
    f.change_remote(&id, b"remote-conflict", "a.txt");
    engine::resolve_both(&f.root, &mut f.remote, &mut f.store, "a.txt").unwrap();
    assert_eq!(
        std::fs::read(f.dir.join("local/a.txt")).unwrap(),
        b"remote-conflict"
    );
    let scan = f.root.scan().unwrap();
    assert!(scan.values().any(|e| e.path.starts_with("a.txt.conflict-")));
    f.run().unwrap();
    assert_eq!(f.remote.files.len(), 2);
    assert!(f.remote.files.values().any(|(_, b)| b == b"local-conflict"));
    assert!(
        f.remote
            .files
            .values()
            .any(|(_, b)| b == b"remote-conflict")
    );
}

#[test]
fn interrupted_download_after_quarantine_resumes_without_losing_original() {
    use sync_core::mirror::{Kind, Operation};
    let mut f = Fixture::new();
    f.seed();
    let id = f.remote.files.keys().next().unwrap().clone();
    let base = f.store.links().unwrap().remove(0);
    f.change_remote(&id, b"remote edit", "a.txt");
    let target = f.remote.files.get(&id).unwrap().0.clone();
    let op = Operation {
        operation_id: Uuid::new_v4().to_string(),
        object_id: id,
        kind: Kind::Download,
        local: Some(base.local.clone()),
        remote: Some(target),
        base: Some(base.clone()),
    };
    f.store.stage(&op).unwrap();
    f.store.mark_running(&op.operation_id).unwrap();
    f.root.quarantine(&op.operation_id, &base.local).unwrap();
    f.store = Store::open(f.dir.join("state.sqlite")).unwrap();
    let report = f.run().unwrap();
    assert_eq!(report.recovered, 1);
    assert_eq!(
        std::fs::read(f.dir.join("local/a.txt")).unwrap(),
        b"remote edit"
    );
    assert_eq!(
        std::fs::read(f.dir.join(format!(
            "local/.gdrive-clone-recovery/{}-old",
            op.operation_id
        )))
        .unwrap(),
        b"first"
    );
}
#[test]
fn deletion_propagation_can_be_disabled_without_erasing_base() {
    let mut f = Fixture::new();
    f.seed();
    std::fs::remove_file(f.dir.join("local/a.txt")).unwrap();
    let report = synchronize(&f.root, &mut f.remote, &mut f.store, false, false).unwrap();
    assert_eq!(report.blocked.len(), 1);
    assert!(f.remote.trash.is_empty());
    assert_eq!(f.store.links().unwrap().len(), 1);
}
#[test]
fn equal_concurrent_edits_advance_checkpoint_for_next_change() {
    let mut f = Fixture::new();
    f.seed();
    let id = f.remote.files.keys().next().unwrap().clone();
    f.write("a.txt", b"same new");
    f.change_remote(&id, b"same new", "a.txt");
    f.run().unwrap();
    f.write("a.txt", b"later local");
    let report = f.run().unwrap();
    assert!(report.conflicts.is_empty());
    assert_eq!(f.remote.files.get(&id).unwrap().1, b"later local");
}

#[test]
fn unlinked_reference_is_blocked_before_creating_a_remote_intent() {
    let mut f = Fixture::new();
    f.write(
        "Copy.gdoc",
        br#"{"doc_id":"doc","url":"https://drive.google.com/open?id=doc"}"#,
    );
    let report = f.run().unwrap();
    assert_eq!(report.blocked.len(), 1);
    assert!(f.store.intents().unwrap().is_empty());
    assert_eq!(f.remote.uploads, 0);
}
#[test]
fn editing_reference_metadata_becomes_resolvable_conflict_not_document_upload() {
    let mut f = Fixture::new();
    f.change_remote(
        "doc",
        br#"{"doc_id":"doc","url":"https://drive.google.com/open?id=doc"}"#,
        "Doc.gdoc",
    );
    f.run().unwrap();
    f.write("Doc.gdoc", b"edited reference");
    let report = f.run().unwrap();
    assert_eq!(report.conflicts, vec!["Doc.gdoc"]);
    assert!(f.store.intents().unwrap().is_empty());
    assert_eq!(f.remote.uploads, 0);
    engine::resolve_both(&f.root, &mut f.remote, &mut f.store, "Doc.gdoc").unwrap();
    assert_eq!(
        std::fs::read(f.dir.join("local/Doc.gdoc")).unwrap(),
        f.remote.files.get("doc").unwrap().1
    );
}

fn directory(path: &str) -> Entry {
    Entry {
        directory: true,
        path: path.into(),
        hash: String::new(),
        size: 0,
        identity: "remote-revision-1".into(),
    }
}
#[test]
fn local_empty_directory_tree_is_created_and_adopted_without_file_uploads() {
    let mut f = Fixture::new();
    std::fs::create_dir_all(f.dir.join("local/a/b/empty")).unwrap();
    f.run().unwrap();
    assert_eq!(f.remote.directory_uploads, 3);
    assert_eq!(f.remote.uploads, 0);
    assert_eq!(f.store.links().unwrap().len(), 3);
    assert!(f.run().unwrap().planned.is_empty());
    assert!(f.remote.files.values().all(|(entry, _)| entry.directory));
}
#[test]
fn remote_empty_directory_tree_is_created_locally_parent_first() {
    let mut f = Fixture::new();
    // IDs intentionally put the child first in lexical order.
    f.remote
        .files
        .insert("z-parent".into(), (directory("a"), Vec::new()));
    f.remote
        .files
        .insert("a-child".into(), (directory("a/empty"), Vec::new()));
    f.run().unwrap();
    assert!(f.dir.join("local/a/empty").is_dir());
    assert_eq!(f.store.links().unwrap().len(), 2);
    assert_eq!(f.remote.directory_uploads, 0);
    assert!(f.run().unwrap().planned.is_empty());
}
#[test]
fn local_directory_rename_moves_remote_tree_and_preserves_ids_and_bytes() {
    let mut f = Fixture::new();
    f.write("old/child.txt", b"keep");
    f.run().unwrap();
    let ids: Vec<_> = f.remote.files.keys().cloned().collect();
    std::fs::rename(f.dir.join("local/old"), f.dir.join("local/new")).unwrap();
    let report = f.run().unwrap();
    assert!(report.conflicts.is_empty());
    assert!(report.blocked.is_empty());
    assert_eq!(f.remote.files.keys().cloned().collect::<Vec<_>>(), ids);
    assert!(
        f.remote
            .files
            .values()
            .all(|(entry, _)| entry.path.starts_with("new"))
    );
    assert_eq!(f.remote.uploads, 1);
    assert_eq!(
        std::fs::read(f.dir.join("local/new/child.txt")).unwrap(),
        b"keep"
    );
    assert!(f.run().unwrap().planned.is_empty());
}
#[test]
fn remote_directory_rename_moves_local_tree_without_replacing_files() {
    let mut f = Fixture::new();
    f.write("old/child.txt", b"keep");
    f.run().unwrap();
    for (entry, _) in f.remote.files.values_mut() {
        entry.path = entry.path.replacen("old", "new", 1);
        entry.identity = "changed".into();
    }
    let report = f.run().unwrap();
    assert!(report.conflicts.is_empty());
    assert!(report.blocked.is_empty());
    assert!(!f.dir.join("local/old").exists());
    assert_eq!(
        std::fs::read(f.dir.join("local/new/child.txt")).unwrap(),
        b"keep"
    );
    assert!(f.run().unwrap().planned.is_empty());
}
#[test]
fn interrupted_folder_creation_reuses_reserved_id_before_and_after_effect() {
    for after_effect in [false, true] {
        let mut f = Fixture::new();
        std::fs::create_dir(f.dir.join("local/empty")).unwrap();
        f.remote.fail_directory_before = !after_effect;
        f.remote.fail_directory_after = after_effect;
        assert!(f.run().is_err());
        let id = f.store.intents().unwrap()[0].operation.object_id.clone();
        assert_eq!(f.remote.reserved, 1);
        f.remote.fail_directory_before = false;
        f.store = Store::open(f.dir.join("state.sqlite")).unwrap();
        assert_eq!(f.run().unwrap().recovered, 1);
        assert_eq!(f.remote.directory_uploads, 1);
        assert_eq!(f.remote.reserved, 1);
        assert!(f.remote.files.contains_key(&id));
        assert!(f.store.intents().unwrap().is_empty());
    }
}
#[test]
fn interrupted_local_directory_move_is_recognized_by_inode() {
    use sync_core::mirror::{Kind, Operation};
    let mut f = Fixture::new();
    f.write("old/keep.txt", b"keep");
    f.run().unwrap();
    let base = f
        .store
        .links()
        .unwrap()
        .into_iter()
        .find(|b| b.local.directory)
        .unwrap();
    for (entry, _) in f.remote.files.values_mut() {
        entry.path = entry.path.replacen("old", "new", 1);
    }
    let desired = f.remote.files.get(&base.id).unwrap().0.clone();
    let op = Operation {
        operation_id: Uuid::new_v4().to_string(),
        object_id: base.id.clone(),
        kind: Kind::Download,
        local: Some(base.local.clone()),
        remote: Some(desired.clone()),
        base: Some(base.clone()),
    };
    f.store.stage(&op).unwrap();
    f.store.mark_running(&op.operation_id).unwrap();
    f.root
        .install_directory(&desired, Some(&base.local))
        .unwrap();
    f.store = Store::open(f.dir.join("state.sqlite")).unwrap();
    assert_eq!(f.run().unwrap().recovered, 1);
    assert_eq!(
        std::fs::read(f.dir.join("local/new/keep.txt")).unwrap(),
        b"keep"
    );
    assert!(f.store.intents().unwrap().is_empty());
}
#[test]
fn directory_collision_does_not_replace_a_local_file_or_upload_descendants() {
    let mut f = Fixture::new();
    f.write("occupied", b"original");
    f.remote
        .files
        .insert("dir".into(), (directory("occupied"), Vec::new()));
    f.change_remote("child", b"remote", "occupied/child.txt");
    let report = f.run().unwrap();
    assert!(!report.blocked.is_empty());
    assert_eq!(
        std::fs::read(f.dir.join("local/occupied")).unwrap(),
        b"original"
    );
    assert_eq!(f.remote.uploads, 0);
    assert!(f.store.intents().unwrap().is_empty());
}
#[test]
fn deleting_a_local_directory_blocks_the_entire_remote_tree() {
    let mut f = Fixture::new();
    f.write("folder/keep.txt", b"keep");
    f.run().unwrap();
    std::fs::remove_dir_all(f.dir.join("local/folder")).unwrap();
    let report = f.run().unwrap();
    assert!(!report.blocked.is_empty());
    assert_eq!(f.remote.files.len(), 2);
    assert!(f.remote.trash.is_empty());
}
#[test]
fn loss_of_folder_access_preserves_local_descendants() {
    let mut f = Fixture::new();
    f.write("folder/keep.txt", b"keep");
    f.run().unwrap();
    f.remote.files.clear();
    let report = f.run().unwrap();
    assert!(!report.blocked.is_empty());
    assert_eq!(
        std::fs::read(f.dir.join("local/folder/keep.txt")).unwrap(),
        b"keep"
    );
    assert_eq!(f.remote.uploads, 1);
    assert!(f.remote.trash.is_empty());
}
#[test]
fn directory_preview_does_not_create_folders_or_reserve_ids() {
    let mut f = Fixture::new();
    std::fs::create_dir(f.dir.join("local/local-empty")).unwrap();
    f.remote.files.insert(
        "remote-empty".into(),
        (directory("remote-empty"), Vec::new()),
    );
    let report = synchronize(&f.root, &mut f.remote, &mut f.store, true, true).unwrap();
    assert_eq!(report.planned.len(), 2);
    assert!(!f.dir.join("local/remote-empty").exists());
    assert_eq!(f.remote.reserved, 0);
    assert_eq!(f.remote.directory_uploads, 0);
    assert!(f.store.links().unwrap().is_empty());
}

#[test]
fn pause_between_files_commits_the_current_file_and_leaves_the_next_unstarted() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let mut f = Fixture::new();
    f.write("a.txt", b"one");
    f.write("b.txt", b"two");
    let paused = Arc::new(AtomicBool::new(false));
    f.remote.pause_after_upload = Some(paused.clone());
    let report =
        engine::synchronize_with_pause(&f.root, &mut f.remote, &mut f.store, false, true, &|| {
            Ok(paused.load(Ordering::SeqCst))
        })
        .unwrap();
    assert!(report.paused);
    assert_eq!(report.completed, 1);
    assert_eq!(f.remote.uploads, 1);
    assert_eq!(f.remote.reserved, 1);
    assert!(f.store.intents().unwrap().is_empty());
    f.remote.pause_after_upload = None;
    paused.store(false, Ordering::SeqCst);
    let report =
        engine::synchronize_with_pause(&f.root, &mut f.remote, &mut f.store, false, true, &|| {
            Ok(paused.load(Ordering::SeqCst))
        })
        .unwrap();
    assert!(!report.paused);
    assert_eq!(f.remote.uploads, 2);
    assert_eq!(f.store.links().unwrap().len(), 2);
}

#[test]
fn directory_move_recovery_does_not_adopt_an_unrelated_destination_inode() {
    use sync_core::mirror::{Kind, Operation};
    let mut f = Fixture::new();
    f.write("old/keep.txt", b"original");
    f.run().unwrap();
    let base = f
        .store
        .links()
        .unwrap()
        .into_iter()
        .find(|b| b.local.directory)
        .unwrap();
    for (entry, _) in f.remote.files.values_mut() {
        entry.path = entry.path.replacen("old", "new", 1);
    }
    let desired = f.remote.files[&base.id].0.clone();
    let op = Operation {
        operation_id: Uuid::new_v4().to_string(),
        object_id: base.id.clone(),
        kind: Kind::Download,
        local: Some(base.local.clone()),
        remote: Some(desired),
        base: Some(base),
    };
    f.store.stage(&op).unwrap();
    f.store.mark_running(&op.operation_id).unwrap();
    f.write("new/unrelated.txt", b"concurrent");
    assert!(!f.run().unwrap().blocked.is_empty());
    assert_eq!(
        std::fs::read(f.dir.join("local/old/keep.txt")).unwrap(),
        b"original"
    );
    assert_eq!(
        std::fs::read(f.dir.join("local/new/unrelated.txt")).unwrap(),
        b"concurrent"
    );
    assert_eq!(f.store.intents().unwrap().len(), 1);
}
