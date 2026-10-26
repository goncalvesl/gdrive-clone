#![allow(clippy::unwrap_used)]
use local_fs::{Root, validate_path};
use std::{io::Cursor, path::PathBuf};
use uuid::Uuid;
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("drive-local-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn root(&self) -> Root {
        Root::open(&self.0, None).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[test]
fn rejects_escape_paths_and_reserved_names() {
    for p in [
        "",
        "/absolute",
        "../escape",
        "a/../b",
        "a//b",
        "a/./b",
        "a\\b",
        ".gdrive-clone-recovery/a",
        "a\0b",
    ] {
        assert!(validate_path(p).is_err(), "{p}");
    }
    assert!(validate_path("fotos/ação.txt").is_ok());
}
#[test]
fn symlink_is_never_followed_for_reads_or_scans() {
    let f = Fixture::new();
    let outside = Fixture::new();
    std::fs::write(outside.0.join("secret"), b"outside").unwrap();
    std::os::unix::fs::symlink(&outside.0, f.0.join("link")).unwrap();
    let root = f.root();
    assert!(root.read("link/secret").is_err());
    assert!(root.scan().is_err());
}
#[test]
fn mismatched_root_identity_blocks_open() {
    let f = Fixture::new();
    assert!(Root::open(&f.0, Some("0:0")).is_err());
}
#[test]
fn download_checksum_failure_never_truncates_original() {
    let f = Fixture::new();
    std::fs::write(f.0.join("a"), b"original").unwrap();
    let root = f.root();
    let expected = root.inspect("a").unwrap().unwrap();
    assert!(
        root.install("op1", &expected, Some(&expected), Cursor::new(b"bad bytes"))
            .is_err()
    );
    assert_eq!(std::fs::read(f.0.join("a")).unwrap(), b"original");
}
#[test]
fn changed_destination_is_retained_in_recovery() {
    let f = Fixture::new();
    std::fs::write(f.0.join("a"), b"old").unwrap();
    let root = f.root();
    let old = root.inspect("a").unwrap().unwrap();
    std::fs::write(f.0.join("a"), b"concurrent").unwrap();
    assert!(root.quarantine("op1", &old).is_err());
    assert_eq!(
        std::fs::read(f.0.join(".gdrive-clone-recovery/op1-old")).unwrap(),
        b"concurrent"
    );
}
#[test]
fn private_spool_is_immutable_when_original_is_edited() {
    use std::io::Read;
    let f = Fixture::new();
    std::fs::write(f.0.join("a"), b"original").unwrap();
    let root = f.root();
    let old = root.inspect("a").unwrap().unwrap();
    let mut spool = root.spool("op1", &old).unwrap();
    std::fs::write(f.0.join("a"), b"edited").unwrap();
    let mut bytes = Vec::new();
    spool.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"original");
}

#[test]
fn directories_are_scanned_and_moves_never_replace_an_occupied_destination() {
    let f = Fixture::new();
    std::fs::create_dir_all(f.0.join("old/empty")).unwrap();
    std::fs::write(f.0.join("old/keep"), b"keep").unwrap();
    std::fs::write(f.0.join("occupied"), b"original").unwrap();
    let root = f.root();
    let old = root.inspect("old").unwrap().unwrap();
    assert!(old.directory);
    assert!(root.scan().unwrap()["old/empty"].directory);
    let mut target = old.clone();
    target.path = "occupied".into();
    assert!(root.install_directory(&target, Some(&old)).is_err());
    assert_eq!(std::fs::read(f.0.join("occupied")).unwrap(), b"original");
    assert!(f.0.join("old/keep").exists());
    target.path = "new".into();
    let moved = root.install_directory(&target, Some(&old)).unwrap();
    assert_eq!(moved.identity, old.identity);
    assert_eq!(std::fs::read(f.0.join("new/keep")).unwrap(), b"keep");
}
#[test]
fn directory_install_and_quarantine_refuse_symlinks_and_recursive_deletion() {
    let f = Fixture::new();
    let outside = Fixture::new();
    std::os::unix::fs::symlink(&outside.0, f.0.join("link")).unwrap();
    let root = f.root();
    let target = sync_core::mirror::Entry {
        directory: true,
        path: "link/child".into(),
        hash: String::new(),
        size: 0,
        identity: "unused".into(),
    };
    assert!(root.install_directory(&target, None).is_err());
    assert!(!outside.0.join("child").exists());
    std::fs::create_dir_all(f.0.join("folder/child")).unwrap();
    let old = root.inspect("folder").unwrap().unwrap();
    assert!(root.quarantine("op", &old).is_err());
    assert!(f.0.join("folder/child").is_dir());
}
#[test]
fn corrupted_resume_never_replaces_old_file_and_discards_bad_partial() {
    let f = Fixture::new();
    std::fs::write(f.0.join("a"), b"original").unwrap();
    std::fs::write(f.0.join("expected"), b"abcdef").unwrap();
    let root = f.root();
    let old = root.inspect("a").unwrap().unwrap();
    let mut target = root.inspect("expected").unwrap().unwrap();
    target.path = "a".into();
    std::fs::remove_file(f.0.join("expected")).unwrap();
    assert_eq!(root.partial_download_len("op2", &target).unwrap(), 0);
    std::fs::write(f.0.join(".gdrive-clone-recovery/op2-download"), b"xx").unwrap();
    assert!(
        root.install_resuming("op2", &target, Some(&old), 2, Cursor::new(b"cdef"))
            .is_err()
    );
    assert_eq!(std::fs::read(f.0.join("a")).unwrap(), b"original");
    assert_eq!(root.partial_download_len("op2", &target).unwrap(), 0);
}
#[test]
fn partial_download_symlink_is_rejected() {
    let f = Fixture::new();
    let outside = Fixture::new();
    std::fs::write(outside.0.join("secret"), b"secret").unwrap();
    let root = f.root();
    std::fs::write(f.0.join("a"), b"secret").unwrap();
    let target = root.inspect("a").unwrap().unwrap();
    root.partial_download_len("op3", &target).unwrap();
    std::os::unix::fs::symlink(
        outside.0.join("secret"),
        f.0.join(".gdrive-clone-recovery/op3-download"),
    )
    .unwrap();
    assert!(root.partial_download_len("op3", &target).is_err());
    assert!(
        root.install_resuming("op3", &target, None, 0, Cursor::new(b"secret"))
            .is_err()
    );
    assert_eq!(std::fs::read(outside.0.join("secret")).unwrap(), b"secret");
    std::fs::hard_link(
        outside.0.join("secret"),
        f.0.join(".gdrive-clone-recovery/op4-download"),
    )
    .unwrap();
    assert!(root.partial_download_len("op4", &target).is_err());
    assert!(
        root.install_resuming("op4", &target, None, 6, Cursor::new(b""))
            .is_err()
    );
}
#[test]
fn local_watcher_wakes_for_nested_changes_but_ignores_recovery_staging() {
    let f = Fixture::new();
    std::fs::create_dir_all(f.0.join("nested")).unwrap();
    std::fs::create_dir(f.0.join(".gdrive-clone-recovery")).unwrap();
    let watcher = local_fs::LocalWatcher::open(&f.0).unwrap();
    assert!(!watcher.wait(std::time::Duration::from_millis(10)).unwrap());
    std::fs::write(f.0.join("nested/file"), b"change").unwrap();
    assert!(watcher.wait(std::time::Duration::from_secs(1)).unwrap());
    std::fs::write(f.0.join(".gdrive-clone-recovery/partial"), b"staging").unwrap();
    assert!(!watcher.wait(std::time::Duration::from_millis(20)).unwrap());
}
