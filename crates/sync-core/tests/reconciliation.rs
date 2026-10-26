#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use sync_core::{
    BlockReason, Decision, Direction, FileState, Inventory, Observation, SimulationError, Snapshot,
    plan, simulate,
};

fn file(version: &str) -> FileState {
    FileState {
        path: "report.txt".into(),
        fingerprint: version.into(),
    }
}
fn present(version: &str) -> Observation {
    Observation::Present(file(version))
}
fn snapshot(base: bool, local: Observation, remote: Observation) -> Snapshot {
    Snapshot {
        base: if base {
            BTreeMap::from([("id-1".into(), file("base"))])
        } else {
            BTreeMap::new()
        },
        local: Inventory {
            complete: true,
            objects: BTreeMap::from([("id-1".into(), local)]),
        },
        remote: Inventory {
            complete: true,
            objects: BTreeMap::from([("id-1".into(), remote)]),
        },
    }
}

#[test]
fn unilateral_creation_and_edits_converge_and_second_plan_is_empty() {
    for has_base in [false, true] {
        for direction in [Direction::Upload, Direction::Download] {
            let unchanged = if has_base {
                present("base")
            } else {
                Observation::Absent
            };
            let (local, remote) = match direction {
                Direction::Upload => (present("new"), unchanged),
                Direction::Download => (unchanged, present("new")),
            };
            let initial = snapshot(has_base, local.clone(), remote.clone());
            let planned = plan(&initial);
            assert!(matches!(planned.decisions(), [Decision::Transfer(t)]
                if t.direction == direction && t.expected_local == local && t.expected_remote == remote));
            let final_state = simulate(&planned, &initial).unwrap();
            assert_eq!(final_state.local.objects.get("id-1"), Some(&present("new")));
            assert_eq!(
                final_state.remote.objects.get("id-1"),
                Some(&present("new"))
            );
            assert_eq!(final_state.base.get("id-1"), Some(&file("new")));
            assert!(plan(&final_state).decisions().is_empty());
        }
    }
}

#[test]
fn concurrent_edits_and_delete_edit_conflicts_preserve_both_sides() {
    for (local, remote) in [
        (present("local"), present("remote")),
        (Observation::Deleted, present("remote")),
        (present("local"), Observation::Deleted),
    ] {
        let initial = snapshot(true, local, remote);
        let planned = plan(&initial);
        assert!(matches!(planned.decisions(), [Decision::Conflict { .. }]));
        assert_eq!(simulate(&planned, &initial).unwrap(), initial);
    }
}

#[test]
fn simultaneous_creations_do_not_assume_same_name_means_same_content() {
    let initial = snapshot(false, present("local"), present("remote"));
    assert!(matches!(
        plan(&initial).decisions(),
        [Decision::Conflict { .. }]
    ));
}

#[test]
fn equal_states_need_no_transfer() {
    for state in [present("base"), present("same-new"), Observation::Deleted] {
        assert!(
            plan(&snapshot(true, state.clone(), state))
                .decisions()
                .is_empty()
        );
    }
}

#[test]
fn incomplete_inventory_blocks_every_effect_on_either_side() {
    for local_incomplete in [true, false] {
        let mut initial = snapshot(true, present("new"), present("base"));
        if local_incomplete {
            initial.local.complete = false;
        } else {
            initial.remote.complete = false;
        }
        let planned = plan(&initial);
        assert!(matches!(
            planned.decisions(),
            [Decision::Blocked {
                reason: BlockReason::IncompleteInventory,
                ..
            }]
        ));
        assert_eq!(simulate(&planned, &initial).unwrap(), initial);
    }
}

#[test]
fn omission_or_loss_of_access_never_becomes_deletion() {
    for local_missing in [true, false] {
        let mut initial = snapshot(true, present("base"), present("base"));
        if local_missing {
            initial.local.objects.clear();
        } else {
            initial.remote.objects.clear();
        }
        let planned = plan(&initial);
        assert!(matches!(
            planned.decisions(),
            [Decision::Blocked {
                reason: BlockReason::UnknownObject,
                ..
            }]
        ));
        assert_eq!(simulate(&planned, &initial).unwrap(), initial);
    }
}

#[test]
fn deletion_propagation_waits_for_recovery_support() {
    for (local, remote) in [
        (Observation::Deleted, present("base")),
        (present("base"), Observation::Deleted),
    ] {
        let initial = snapshot(true, local, remote);
        let planned = plan(&initial);
        assert!(matches!(
            planned.decisions(),
            [Decision::Blocked {
                reason: BlockReason::DeletionNeedsRecovery,
                ..
            }]
        ));
        assert_eq!(simulate(&planned, &initial).unwrap(), initial);
    }
}

#[test]
fn changed_source_destination_or_inventory_rejects_whole_plan() {
    let initial = snapshot(true, present("new"), present("base"));
    let planned = plan(&initial);
    let mut mutations = Vec::new();
    let mut source = initial.clone();
    source.local.objects.insert("id-1".into(), present("newer"));
    mutations.push(source);
    let mut destination = initial.clone();
    destination
        .remote
        .objects
        .insert("id-1".into(), present("remote-edit"));
    mutations.push(destination);
    let mut incomplete = initial.clone();
    incomplete.remote.complete = false;
    mutations.push(incomplete);
    let mut base = initial.clone();
    base.base.clear();
    mutations.push(base);
    for changed in mutations {
        let before = changed.clone();
        assert_eq!(
            simulate(&planned, &changed),
            Err(SimulationError::StaleSnapshot)
        );
        assert_eq!(changed, before);
    }
}

#[test]
fn duplicate_paths_block_overwriting_other_ids() {
    let mut initial = snapshot(false, present("new"), Observation::Absent);
    initial
        .remote
        .objects
        .insert("different-id".into(), present("other"));
    let planned = plan(&initial);
    assert!(planned.decisions().iter().any(|d| matches!(d,
        Decision::Blocked { id, reason: BlockReason::PathCollision } if id == "id-1")));
    assert_eq!(simulate(&planned, &initial).unwrap(), initial);
}

#[test]
fn rename_retains_id_and_transfers_projected_state() {
    let mut initial = snapshot(true, present("base"), present("base"));
    let renamed = FileState {
        path: "renamed.txt".into(),
        fingerprint: "base".into(),
    };
    initial
        .local
        .objects
        .insert("id-1".into(), Observation::Present(renamed.clone()));
    let final_state = simulate(&plan(&initial), &initial).unwrap();
    assert_eq!(
        final_state.remote.objects.get("id-1"),
        Some(&Observation::Present(renamed))
    );
    assert_eq!(final_state.remote.objects.len(), 1);
}

#[test]
fn ordering_is_stable_and_independent_of_insertion_order() {
    let mut first = snapshot(false, present("new"), Observation::Absent);
    first
        .local
        .objects
        .insert("aaa".into(), Observation::Unknown);
    let mut second = first.clone();
    second.local.objects = first.local.objects.clone().into_iter().rev().collect();
    assert_eq!(plan(&first), plan(&second));
    assert!(
        matches!(plan(&first).decisions().first(), Some(Decision::Blocked { id, .. }) if id == "aaa")
    );
}
