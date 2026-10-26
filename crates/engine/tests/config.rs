#![allow(clippy::unwrap_used)]
use engine::Paths;
use std::{os::unix::fs::PermissionsExt, path::PathBuf};
use uuid::Uuid;
struct Fixture {
    dir: PathBuf,
    paths: Paths,
}
impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("drive-config-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("oauth.json"),
            br#"{"installed":{"client_id":"synthetic","client_secret":"synthetic-secret"}}"#,
        )
        .unwrap();
        Self {
            paths: Paths {
                config: dir.join("config"),
                data: dir.join("data"),
            },
            dir,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
#[test]
fn configuration_is_private_and_does_not_copy_oauth_secrets() {
    let f = Fixture::new();
    let c = f
        .paths
        .configure(
            &f.dir.join("files"),
            "remoteid",
            &f.dir.join("oauth.json"),
            false,
        )
        .unwrap();
    let loaded = f.paths.load().unwrap();
    assert_eq!(loaded.profile, c.profile);
    let file = f.paths.config.join("config.json");
    assert_eq!(
        std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(
        !std::fs::read_to_string(file)
            .unwrap()
            .contains("synthetic-secret")
    );
}
#[test]
fn same_roots_keep_profile_and_new_roots_create_independent_profile() {
    let f = Fixture::new();
    let a = f
        .paths
        .configure(
            &f.dir.join("files"),
            "remoteid",
            &f.dir.join("oauth.json"),
            false,
        )
        .unwrap();
    let b = f
        .paths
        .configure(
            &f.dir.join("files"),
            "remoteid",
            &f.dir.join("oauth.json"),
            true,
        )
        .unwrap();
    assert_eq!(a.profile, b.profile);
    let c = f
        .paths
        .configure(
            &f.dir.join("other"),
            "remoteid",
            &f.dir.join("oauth.json"),
            false,
        )
        .unwrap();
    assert_ne!(a.profile, c.profile);
}
#[test]
fn configuration_and_credential_directories_cannot_be_synchronized() {
    let f = Fixture::new();
    assert!(
        f.paths
            .configure(&f.dir, "remoteid", &f.dir.join("oauth.json"), false)
            .is_err()
    );
    let files = f.dir.join("files");
    std::fs::create_dir_all(&files).unwrap();
    std::fs::copy(f.dir.join("oauth.json"), files.join("oauth.json")).unwrap();
    assert!(
        f.paths
            .configure(&files, "remoteid", &files.join("oauth.json"), false)
            .is_err()
    );
}
#[test]
fn pause_is_persistent_without_accessing_google_or_keyring() {
    let f = Fixture::new();
    f.paths
        .configure(
            &f.dir.join("files"),
            "remoteid",
            &f.dir.join("oauth.json"),
            false,
        )
        .unwrap();
    assert!(!f.paths.paused().unwrap());
    f.paths.set_paused(true).unwrap();
    assert!(f.paths.paused().unwrap());
    assert!(f.paths.cycle(false).is_err());
    f.paths.set_paused(false).unwrap();
    assert!(!f.paths.paused().unwrap());
}

#[test]
fn connection_can_be_prepared_before_any_sync_root_and_survives_root_changes() {
    let f = Fixture::new();
    let credentials = f.dir.join("oauth.json");
    let account = f.paths.prepare_account(&credentials).unwrap();
    assert!(!f.paths.configured());
    assert!(!f.dir.join("files").exists());
    assert_eq!(f.paths.account().unwrap().profile, account.profile);
    let first = f
        .paths
        .configure(&f.dir.join("files"), "folder1", &credentials, false)
        .unwrap();
    let second = f
        .paths
        .configure(&f.dir.join("files"), "folder2", &credentials, false)
        .unwrap();
    assert_ne!(first.profile, second.profile);
    assert_eq!(first.account_profile(), account.profile);
    assert_eq!(second.account_profile(), account.profile);
    assert_eq!(
        f.paths.prepare_account(&credentials).unwrap().profile,
        account.profile
    );
    let file = f.paths.config.join("account.json");
    assert_eq!(
        std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(
        !std::fs::read_to_string(file)
            .unwrap()
            .contains("synthetic-secret")
    );
}

#[test]
fn legacy_connection_is_reused_without_reading_or_copying_keyring_secrets() {
    let f = Fixture::new();
    let credentials = f.dir.join("oauth.json");
    let original = f
        .paths
        .configure(&f.dir.join("files"), "folder1", &credentials, false)
        .unwrap();
    let file = f.paths.config.join("config.json");
    let mut legacy: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    legacy.as_object_mut().unwrap().remove("account_profile");
    legacy.as_object_mut().unwrap().remove("remote_name");
    std::fs::write(&file, serde_json::to_vec(&legacy).unwrap()).unwrap();
    assert_eq!(f.paths.load().unwrap().account_profile(), original.profile);
    let account = f.paths.prepare_account(&credentials).unwrap();
    assert_eq!(account.profile, original.profile);
    let selected = f
        .paths
        .configure(&f.dir.join("files"), "folder2", &credentials, false)
        .unwrap();
    assert_eq!(selected.account_profile(), original.profile);
    assert_ne!(selected.profile, original.profile);
}

#[test]
fn invalid_credentials_do_not_replace_the_saved_connection() {
    let f = Fixture::new();
    let original = f.paths.prepare_account(&f.dir.join("oauth.json")).unwrap();
    std::fs::write(f.dir.join("invalid.json"), b"{}").unwrap();
    assert!(
        f.paths
            .prepare_account(&f.dir.join("invalid.json"))
            .is_err()
    );
    assert_eq!(f.paths.account().unwrap().profile, original.profile);
    std::fs::copy(f.dir.join("oauth.json"), f.dir.join("other.json")).unwrap();
    let other = f.paths.prepare_account(&f.dir.join("other.json")).unwrap();
    assert_ne!(original.profile, other.profile);
}

#[test]
fn changing_roots_cannot_hide_a_pending_remote_effect() {
    use sync_core::mirror::{Entry, Kind, Operation};
    let f = Fixture::new();
    let credentials = f.dir.join("oauth.json");
    let original = f
        .paths
        .configure(&f.dir.join("files"), "folder1", &credentials, false)
        .unwrap();
    let directory = f.paths.data.join(&original.profile);
    std::fs::create_dir_all(&directory).unwrap();
    let mut store = state_store::Store::open(directory.join("state.sqlite")).unwrap();
    store.init_mirror().unwrap();
    let op = Operation {
        operation_id: Uuid::new_v4().to_string(),
        object_id: "reserved".into(),
        kind: Kind::Upload,
        local: Some(Entry {
            directory: false,
            path: "file.txt".into(),
            hash: "synthetic".into(),
            size: 3,
            identity: "synthetic-inode".into(),
        }),
        remote: None,
        base: None,
    };
    store.stage(&op).unwrap();
    store.mark_running(&op.operation_id).unwrap();
    assert!(
        f.paths
            .configure(&f.dir.join("files"), "folder2", &credentials, false)
            .is_err()
    );
    assert_eq!(f.paths.load().unwrap().profile, original.profile);
    assert_eq!(store.intents().unwrap().len(), 1);
}

#[test]
fn managed_application_client_can_be_provided_without_a_user_setup_or_roots() {
    let f = Fixture::new();
    std::fs::create_dir_all(&f.paths.config).unwrap();
    let managed = f.paths.config.join("oauth-client.json");
    std::fs::copy(f.dir.join("oauth.json"), &managed).unwrap();
    assert_eq!(f.paths.application_credentials().unwrap(), managed);
    assert!(!f.paths.configured());
    assert!(!f.paths.config.join("account.json").exists());
}

#[test]
fn configuration_changes_respect_the_active_executor_lock() {
    use fs2::FileExt;
    let f = Fixture::new();
    let credentials = f.dir.join("oauth.json");
    let original = f
        .paths
        .configure(&f.dir.join("files"), "folder1", &credentials, false)
        .unwrap();
    let directory = f.paths.data.join(&original.profile);
    std::fs::create_dir_all(&directory).unwrap();
    let guard = std::fs::File::create(directory.join("daemon.lock")).unwrap();
    guard.try_lock_exclusive().unwrap();
    assert!(
        f.paths
            .configure(&f.dir.join("files"), "folder2", &credentials, false)
            .is_err()
    );
    assert!(
        f.paths
            .configure(&f.dir.join("files"), "folder1", &credentials, true)
            .is_err()
    );
    assert!(!f.paths.load().unwrap().propagate_deletions);
    assert_eq!(f.paths.load().unwrap().profile, original.profile);
    drop(guard);
    assert!(
        f.paths
            .configure(&f.dir.join("files"), "folder2", &credentials, false)
            .is_ok()
    );
}

#[test]
fn retry_schedule_is_bounded_and_respects_server_minimum() {
    use std::time::Duration;
    let delays: Vec<_> = (1..=7)
        .map(|n| engine::retry_delay(n, Duration::ZERO).as_secs())
        .collect();
    assert_eq!(delays, vec![30, 60, 120, 240, 300, 300, 300]);
    assert_eq!(
        engine::retry_delay(1, Duration::from_secs(600)).as_secs(),
        600
    );
    assert_eq!(engine::retry_delay(u32::MAX, Duration::ZERO).as_secs(), 300);
}
