//! Pure, conservative reconciliation and an in-memory simulator.
//!
//! Callers assign stable object keys (remote IDs for linked objects). Absence in
//! an inventory is unknown, even when complete: only explicit tombstones prove
//! deletion. This module does not infer identity from file names.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A synthetic, already-projected regular file. Content is an opaque fingerprint,
/// not bytes. Real adapters must validate paths and supply comparable fingerprints.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileState {
    pub path: String,
    pub fingerprint: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Observation {
    Present(FileState),
    /// Explicit evidence of deletion, never just omission from a listing.
    Deleted,
    /// An authoritative absence for a not-yet-created object.
    Absent,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inventory {
    pub complete: bool,
    pub objects: BTreeMap<String, Observation>,
}

impl Inventory {
    fn get(&self, id: &str) -> Observation {
        self.objects
            .get(id)
            .cloned()
            .unwrap_or(Observation::Unknown)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub base: BTreeMap<String, FileState>,
    pub local: Inventory,
    pub remote: Inventory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Upload,
    Download,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transfer {
    pub id: String,
    pub direction: Direction,
    pub expected_local: Observation,
    pub expected_remote: Observation,
    pub file: FileState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockReason {
    IncompleteInventory,
    UnknownObject,
    /// Delete propagation needs a quarantine/trash executor, not implemented yet.
    DeletionNeedsRecovery,
    /// Name collisions require a projection policy, not implicit overwriting.
    PathCollision,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    Transfer(Transfer),
    Conflict { id: String },
    Blocked { id: String, reason: BlockReason },
}

/// Immutable plan: execution must validate it against the snapshot it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    source: Snapshot,
    decisions: Vec<Decision>,
}

impl Plan {
    pub fn decisions(&self) -> &[Decision] {
        &self.decisions
    }
}

fn path_collides(snapshot: &Snapshot, id: &str, file: &FileState) -> bool {
    [&snapshot.local, &snapshot.remote].iter().any(|inventory| {
        inventory.objects.iter().any(|(other, observation)| {
            other != id
                && matches!(observation, Observation::Present(state) if state.path == file.path)
        })
    })
}

/// Produces stable decisions ordered by object key. Incomplete scans block all
/// effects, including uploads: an omitted target could otherwise be overwritten.
pub fn plan(snapshot: &Snapshot) -> Plan {
    let ids: BTreeSet<_> = snapshot
        .base
        .keys()
        .chain(snapshot.local.objects.keys())
        .chain(snapshot.remote.objects.keys())
        .cloned()
        .collect();
    let mut decisions = Vec::new();
    for id in ids {
        let blocked = |reason| Decision::Blocked {
            id: id.clone(),
            reason,
        };
        if !snapshot.local.complete || !snapshot.remote.complete {
            decisions.push(blocked(BlockReason::IncompleteInventory));
            continue;
        }
        let local = snapshot.local.get(&id);
        let remote = snapshot.remote.get(&id);
        if matches!(local, Observation::Unknown) || matches!(remote, Observation::Unknown) {
            decisions.push(blocked(BlockReason::UnknownObject));
            continue;
        }
        let base = snapshot
            .base
            .get(&id)
            .cloned()
            .map(Observation::Present)
            .unwrap_or(Observation::Absent);
        // Explicit deletion of an object unknown to the base is ambiguous.
        if !snapshot.base.contains_key(&id)
            && (local == Observation::Deleted || remote == Observation::Deleted)
        {
            decisions.push(blocked(BlockReason::DeletionNeedsRecovery));
            continue;
        }
        if local == remote {
            continue;
        }
        let (direction, source) = if remote == base {
            (Direction::Upload, &local)
        } else if local == base {
            (Direction::Download, &remote)
        } else {
            decisions.push(Decision::Conflict { id });
            continue;
        };
        match source {
            Observation::Present(file) if path_collides(snapshot, &id, file) => {
                decisions.push(blocked(BlockReason::PathCollision));
            }
            Observation::Present(file) => decisions.push(Decision::Transfer(Transfer {
                id,
                direction,
                file: file.clone(),
                expected_local: local,
                expected_remote: remote,
            })),
            _ => decisions.push(blocked(BlockReason::DeletionNeedsRecovery)),
        }
    }
    Plan {
        source: snapshot.clone(),
        decisions,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SimulationError {
    StaleSnapshot,
}

/// Applies only transfers to a clone. No file contents, I/O, journal or remote
/// revisions are simulated. Conflicts and blocked decisions leave both sides intact.
/// A changed snapshot rejects the entire plan before any simulated effect.
pub fn simulate(plan: &Plan, current: &Snapshot) -> Result<Snapshot, SimulationError> {
    if current != &plan.source {
        return Err(SimulationError::StaleSnapshot);
    }
    let mut result = current.clone();
    for decision in &plan.decisions {
        if let Decision::Transfer(transfer) = decision {
            let destination = match transfer.direction {
                Direction::Upload => &mut result.remote,
                Direction::Download => &mut result.local,
            };
            destination.objects.insert(
                transfer.id.clone(),
                Observation::Present(transfer.file.clone()),
            );
            result
                .base
                .insert(transfer.id.clone(), transfer.file.clone());
        }
    }
    Ok(result)
}

pub mod mirror;
