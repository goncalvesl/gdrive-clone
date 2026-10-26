#![allow(clippy::unwrap_used)]
use state_store::{Error, Phase, Store};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use sync_core::{Direction, FileState, Observation, Transfer};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Database(PathBuf);
impl Database {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "gdrive-store-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path.join("state.sqlite"))
    }
}
impl Drop for Database {
    fn drop(&mut self) {
        if let Some(parent) = self.0.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }
}
fn transfer() -> Transfer {
    let file = FileState {
        path: "synthetic.txt".into(),
        fingerprint: "v1".into(),
    };
    Transfer {
        id: "remote-or-synthetic-id".into(),
        direction: Direction::Upload,
        expected_local: Observation::Present(file.clone()),
        expected_remote: Observation::Absent,
        file,
    }
}
fn observe(store: &mut Store) {
    let observed = Observation::Present(transfer().file);
    store.observe("op-1", &observed, &observed).unwrap();
}

#[test]
fn reopen_preserves_payload_and_each_phase_until_atomic_confirmation() {
    let db = Database::new();
    {
        let mut store = Store::open(&db.0).unwrap();
        store.enqueue("op-1", &transfer(), None).unwrap();
    }
    {
        let mut store = Store::open(&db.0).unwrap();
        let pending = store.pending().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending.first().unwrap().transfer, transfer());
        assert_eq!(pending.first().unwrap().phase, Phase::Planned);
        store.start("op-1").unwrap();
    }
    {
        let mut store = Store::open(&db.0).unwrap();
        assert_eq!(
            store.pending().unwrap().first().unwrap().phase,
            Phase::Running
        );
        assert!(store.confirmed().unwrap().is_empty());
        assert!(matches!(store.start("op-1"), Err(Error::InvalidTransition)));
        observe(&mut store);
    }
    {
        let mut store = Store::open(&db.0).unwrap();
        assert_eq!(
            store.pending().unwrap().first().unwrap().phase,
            Phase::EffectObserved
        );
        assert!(store.confirmed().unwrap().is_empty());
        store.commit("op-1").unwrap();
    }
    let store = Store::open(&db.0).unwrap();
    assert!(store.pending().unwrap().is_empty());
    assert_eq!(
        store.confirmed().unwrap().get(&transfer().id),
        Some(&transfer().file)
    );
}

#[test]
fn invalid_transitions_and_wrong_observations_leave_intent_pending() {
    let db = Database::new();
    let mut store = Store::open(&db.0).unwrap();
    store.enqueue("op-1", &transfer(), None).unwrap();
    assert!(matches!(
        store.commit("op-1"),
        Err(Error::InvalidTransition)
    ));
    store.start("op-1").unwrap();
    assert!(matches!(
        store.observe("op-1", &Observation::Unknown, &Observation::Absent),
        Err(Error::ObservationMismatch)
    ));
    assert_eq!(
        store.pending().unwrap().first().unwrap().phase,
        Phase::Running
    );
    assert!(store.confirmed().unwrap().is_empty());
}

#[test]
fn duplicate_ids_active_objects_and_stale_base_are_rejected() {
    let db = Database::new();
    let mut store = Store::open(&db.0).unwrap();
    store.enqueue("op-1", &transfer(), None).unwrap();
    assert!(store.enqueue("op-1", &transfer(), None).is_err());
    assert!(store.enqueue("op-2", &transfer(), None).is_err());
    store.start("op-1").unwrap();
    observe(&mut store);
    store.commit("op-1").unwrap();
    assert!(matches!(
        store.enqueue("op-2", &transfer(), None),
        Err(Error::StaleBase)
    ));
    assert!(store.pending().unwrap().is_empty());
}

#[test]
fn sql_failure_between_base_write_and_journal_completion_rolls_back_both() {
    let db = Database::new();
    let mut store = Store::open(&db.0).unwrap();
    store.enqueue("op-1", &transfer(), None).unwrap();
    store.start("op-1").unwrap();
    observe(&mut store);
    let raw = rusqlite::Connection::open(&db.0).unwrap();
    raw.execute_batch("CREATE TRIGGER fail_commit BEFORE UPDATE OF phase ON operations WHEN NEW.phase='committed' BEGIN SELECT RAISE(ABORT,'injected failure'); END;").unwrap();
    assert!(store.commit("op-1").is_err());
    drop(store);
    let mut reopened = Store::open(&db.0).unwrap();
    assert!(reopened.confirmed().unwrap().is_empty());
    assert_eq!(
        reopened.pending().unwrap().first().unwrap().phase,
        Phase::EffectObserved
    );
    raw.execute_batch("DROP TRIGGER fail_commit").unwrap();
    reopened.commit("op-1").unwrap();
    assert!(reopened.pending().unwrap().is_empty());
}

#[test]
fn newer_schema_is_refused_without_downgrade() {
    let db = Database::new();
    let raw = rusqlite::Connection::open(&db.0).unwrap();
    raw.pragma_update(None, "user_version", 99).unwrap();
    assert!(matches!(
        Store::open(&db.0),
        Err(Error::UnsupportedSchema(99))
    ));
    assert_eq!(
        raw.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        99
    );
}

// Invoked in an isolated process; exit bypasses destructors to model process loss.
#[test]
fn child_exits_after_durable_start() {
    let Some(path) = std::env::var_os("GDRIVE_STORE_CRASH_DB") else {
        return;
    };
    let mut store = Store::open(path).unwrap();
    store.enqueue("op-1", &transfer(), None).unwrap();
    store.start("op-1").unwrap();
    std::process::exit(23);
}

#[test]
fn process_loss_keeps_running_intent_for_inspection_not_automatic_replay() {
    let db = Database::new();
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_exits_after_durable_start"])
        .env("GDRIVE_STORE_CRASH_DB", &db.0)
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(23));
    let mut store = Store::open(&db.0).unwrap();
    assert_eq!(
        store.pending().unwrap().first().unwrap().phase,
        Phase::Running
    );
    assert!(store.confirmed().unwrap().is_empty());
    assert!(matches!(store.start("op-1"), Err(Error::InvalidTransition)));
}

#[test]
fn runtime_migration_preserves_original_journal_and_reopens_v5() {
    let db = Database::new();
    let mut store = Store::open(&db.0).unwrap();
    store.enqueue("op-1", &transfer(), None).unwrap();
    store.start("op-1").unwrap();
    observe(&mut store);
    store.commit("op-1").unwrap();
    let before = store.confirmed().unwrap();
    store.init_mirror().unwrap();
    drop(store);
    let mut reopened = Store::open(&db.0).unwrap();
    reopened.init_mirror().unwrap();
    assert_eq!(before, reopened.confirmed().unwrap());
    let raw = rusqlite::Connection::open(&db.0).unwrap();
    assert_eq!(
        raw.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        5
    );
}

#[test]
fn remote_inventory_and_cursor_commit_atomically_and_survive_reopen() {
    use std::collections::BTreeMap;
    use sync_core::mirror::Entry;
    let db = Database::new();
    let mut store = Store::open(&db.0).unwrap();
    store.init_mirror().unwrap();
    assert!(store.remote_snapshot().unwrap().is_none());
    let mut inventory = BTreeMap::new();
    inventory.insert(
        "remote-id".into(),
        Entry {
            directory: false,
            path: "folder/file.txt".into(),
            hash: "abc".into(),
            size: 3,
            identity: "revision".into(),
        },
    );
    assert!(store.save_remote_snapshot("", &inventory).is_err());
    assert!(store.remote_snapshot().unwrap().is_none());
    store
        .save_remote_snapshot("opaque-token", &inventory)
        .unwrap();
    drop(store);
    let mut reopened = Store::open(&db.0).unwrap();
    let (cursor, saved) = reopened.remote_snapshot().unwrap().unwrap();
    assert_eq!(cursor, "opaque-token");
    assert_eq!(saved, inventory);
    reopened.advance_remote_cursor("cursor-advanced").unwrap();
    let mut changed_entry = inventory.get("remote-id").unwrap().clone();
    changed_entry.path = "folder/renamed.txt".into();
    changed_entry.hash = "def".into();
    reopened
        .update_remote_snapshot(
            "cursor-delta",
            &BTreeMap::from([
                ("remote-id".into(), Some(changed_entry.clone())),
                ("removed-id".into(), None),
            ]),
        )
        .unwrap();
    let expected = BTreeMap::from([("remote-id".into(), changed_entry)]);
    assert_eq!(
        reopened.remote_snapshot().unwrap().unwrap(),
        ("cursor-delta".into(), expected.clone())
    );
    let raw = rusqlite::Connection::open(&db.0).unwrap();
    raw.execute_batch("CREATE TRIGGER fail_snapshot_cursor BEFORE UPDATE ON remote_cursor WHEN NEW.token IN ('cursor-reject','delta-reject') BEGIN SELECT RAISE(ABORT,'injected'); END;")
        .unwrap();
    assert!(
        reopened
            .update_remote_snapshot(
                "delta-reject",
                &BTreeMap::from([("remote-id".into(), None)])
            )
            .is_err()
    );
    let replacement = BTreeMap::new();
    assert!(
        reopened
            .save_remote_snapshot("cursor-reject", &replacement)
            .is_err()
    );
    assert_eq!(
        reopened.remote_snapshot().unwrap().unwrap(),
        ("cursor-delta".into(), expected)
    );
    drop(reopened);
    raw.execute_batch("DROP TABLE remote_inventory; DROP TABLE remote_cursor;")
        .unwrap();
    raw.pragma_update(None, "user_version", 4).unwrap();
    drop(raw);
    let mut upgraded = Store::open(&db.0).unwrap();
    upgraded.init_mirror().unwrap();
    assert!(upgraded.remote_snapshot().unwrap().is_none());
}

#[test]
fn runtime_finish_rolls_back_link_when_journal_delete_fails() {
    use sync_core::mirror::{Entry, Kind, Link, Operation};
    let db = Database::new();
    let mut store = Store::open(&db.0).unwrap();
    store.init_mirror().unwrap();
    let entry = Entry {
        directory: false,
        path: "file".into(),
        hash: "hash".into(),
        size: 4,
        identity: "id".into(),
    };
    let op = Operation {
        operation_id: "op-runtime".into(),
        object_id: "remote-id".into(),
        kind: Kind::Upload,
        local: Some(entry.clone()),
        remote: None,
        base: None,
    };
    let link = Link {
        id: op.object_id.clone(),
        local: entry.clone(),
        remote: entry,
    };
    store.stage(&op).unwrap();
    store.mark_running(&op.operation_id).unwrap();
    let session = sync_core::mirror::UploadSession {
        url: "https://www.googleapis.com/upload/synthetic".into(),
        acknowledged: 2,
        target: link.local.clone(),
        expected: None,
    };
    store.save_upload_session(&op.object_id, &session).unwrap();
    let raw = rusqlite::Connection::open(&db.0).unwrap();
    raw.execute_batch("CREATE TRIGGER fail_runtime BEFORE DELETE ON mirror_journal BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(store.finish(&op, Some(&link), "test").is_err());
    drop(store);
    let mut reopened = Store::open(&db.0).unwrap();
    assert!(reopened.links().unwrap().is_empty());
    assert_eq!(reopened.intents().unwrap().len(), 1);
    assert!(reopened.history().unwrap().is_empty());
    assert!(reopened.upload_session(&op.object_id).unwrap().is_some());
    raw.execute_batch("DROP TRIGGER fail_runtime").unwrap();
    reopened.finish(&op, Some(&link), "test").unwrap();
    assert_eq!(reopened.links().unwrap(), vec![link]);
    assert!(reopened.intents().unwrap().is_empty());
    assert!(reopened.upload_session(&op.object_id).unwrap().is_none());
}

#[test]
fn v2_runtime_payloads_without_directory_field_migrate_without_changing_pending_effects() {
    use sync_core::mirror::{Entry, Kind, Operation};
    let db = Database::new();
    let mut store = Store::open(&db.0).unwrap();
    store.init_mirror().unwrap();
    let op = Operation {
        operation_id: "legacy-op".into(),
        object_id: "reserved".into(),
        kind: Kind::Upload,
        local: Some(Entry {
            directory: false,
            path: "file.txt".into(),
            hash: "abc".into(),
            size: 3,
            identity: "inode".into(),
        }),
        remote: None,
        base: None,
    };
    store.stage(&op).unwrap();
    store.mark_running(&op.operation_id).unwrap();
    drop(store);
    let raw = rusqlite::Connection::open(&db.0).unwrap();
    let mut payload = serde_json::to_value(&op).unwrap();
    payload["local"]
        .as_object_mut()
        .unwrap()
        .remove("directory");
    raw.execute(
        "UPDATE mirror_journal SET payload=?1",
        [serde_json::to_string(&payload).unwrap()],
    )
    .unwrap();
    raw.execute_batch(
        "DROP TABLE upload_sessions; DROP TABLE remote_inventory; DROP TABLE remote_cursor;",
    )
    .unwrap();
    raw.pragma_update(None, "user_version", 2).unwrap();
    drop(raw);
    let mut store = Store::open(&db.0).unwrap();
    store.init_mirror().unwrap();
    let intents = store.intents().unwrap();
    assert_eq!(intents.len(), 1);
    assert_eq!(intents[0].operation, op);
    assert!(intents[0].running);
}

#[test]
fn upload_sessions_require_a_running_intent_and_survive_reopen() {
    use sync_core::mirror::{Entry, Kind, Operation, UploadSession};
    let db = Database::new();
    let mut store = Store::open(&db.0).unwrap();
    store.init_mirror().unwrap();
    let target = Entry {
        directory: false,
        path: "file".into(),
        hash: "abc".into(),
        size: 3,
        identity: "inode".into(),
    };
    let session = UploadSession {
        url: "https://www.googleapis.com/upload/synthetic".into(),
        acknowledged: 2,
        target: target.clone(),
        expected: None,
    };
    let op = Operation {
        operation_id: "upload".into(),
        object_id: "reserved".into(),
        kind: Kind::Upload,
        local: Some(target),
        remote: None,
        base: None,
    };
    assert!(store.save_upload_session("reserved", &session).is_err());
    store.stage(&op).unwrap();
    assert!(store.save_upload_session("reserved", &session).is_err());
    store.mark_running("upload").unwrap();
    store.save_upload_session("reserved", &session).unwrap();
    drop(store);
    let store = Store::open(&db.0).unwrap();
    assert!(
        store
            .upload_session("reserved")
            .unwrap()
            .is_some_and(|saved| saved == session)
    );
}
