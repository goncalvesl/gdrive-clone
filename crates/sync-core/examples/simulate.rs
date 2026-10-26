//! Synthetic upload demo. No filesystem or network synchronization.
use std::collections::BTreeMap;
use sync_core::{FileState, Inventory, Observation, Snapshot, plan, simulate};

fn main() {
    let initial = Snapshot {
        base: BTreeMap::new(),
        local: Inventory {
            complete: true,
            objects: BTreeMap::from([(
                "synthetic-id".into(),
                Observation::Present(FileState {
                    path: "example.txt".into(),
                    fingerprint: "synthetic-content-v1".into(),
                }),
            )]),
        },
        remote: Inventory {
            complete: true,
            objects: BTreeMap::from([("synthetic-id".into(), Observation::Absent)]),
        },
    };
    let planned = plan(&initial);
    println!("SIMULATION ONLY: {:#?}", planned.decisions());
    match simulate(&planned, &initial) {
        Ok(result) => println!("Remaining decisions: {}", plan(&result).decisions().len()),
        Err(error) => eprintln!("Simulation failed: {error:?}"),
    }
}
