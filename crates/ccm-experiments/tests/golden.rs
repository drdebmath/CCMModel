//! Golden-file check: a fixed sweep over every algorithm must reproduce
//! `tests/golden/sweep.csv` byte for byte.
//!
//! A refactor passes only if it changes no result and no metric. A change that
//! is meant to alter behaviour regenerates the file in the same commit, with
//! `CCM_UPDATE_GOLDEN=1 cargo test -p ccm-experiments --test golden`, so the
//! difference is reviewed rather than discovered.

use ccm_core::{Algorithm, NodeId, PortAssignment};
use ccm_experiments::{
    results_to_csv, run_sweep_parallel, BuiltinRunner, GraphFamily, GraphSpec, PlacementSpec,
    RunRequest, SweepSpec,
};
use std::path::PathBuf;

const ALGORITHMS: [Algorithm; 3] = [
    Algorithm::DropAndFreeze,
    Algorithm::HelpByScouts,
    Algorithm::P1Tree,
];

const FAMILIES: [GraphFamily; 8] = [
    GraphFamily::Path,
    GraphFamily::Cycle,
    GraphFamily::Star,
    GraphFamily::Complete,
    GraphFamily::Tree { branching: 2 },
    GraphFamily::Grid { columns: 4 },
    GraphFamily::RandomConnected { extra_edges: 6 },
    GraphFamily::RandomBoundedDegree { max_degree: 4 },
];

const PORTS: [PortAssignment; 3] = [
    PortAssignment::Canonical,
    PortAssignment::Random,
    PortAssignment::Adversarial,
];

fn sweep_csv() -> String {
    let mut results = Vec::new();
    for algorithm in ALGORITHMS {
        for family in FAMILIES {
            for port_assignment in PORTS {
                let base = RunRequest {
                    algorithm,
                    graph: GraphSpec::new(family.clone(), 12),
                    port_assignment,
                    placement: PlacementSpec::SingleNode { node: NodeId(0) },
                    agents: 6,
                    seed: 1,
                    round_limit: None,
                };
                let spec = SweepSpec::new(base)
                    .with_node_counts(vec![12, 40])
                    .with_agent_counts(vec![6, 12])
                    .with_seeds(vec![1, 2, 3]);
                for result in run_sweep_parallel(&BuiltinRunner, &spec, 4) {
                    let mut result = result.expect("every golden configuration runs");
                    // The build commit is not a simulation result.
                    result.git_commit = "unknown";
                    results.push(result);
                }
            }
        }
    }
    results_to_csv(&results)
}

#[test]
fn sweep_matches_the_golden_file() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/sweep.csv");
    let actual = sweep_csv();
    if std::env::var_os("CCM_UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, &actual).expect("golden file is writable");
        return;
    }
    // A Windows checkout may convert the file to CRLF; the rows are what count.
    let expected = std::fs::read_to_string(&path)
        .expect("tests/golden/sweep.csv exists; regenerate it with CCM_UPDATE_GOLDEN=1")
        .replace("\r\n", "\n");
    if actual != expected {
        let line = actual
            .lines()
            .zip(expected.lines())
            .position(|(a, e)| a != e)
            .map_or_else(
                || "a missing or extra row".to_owned(),
                |i| format!("line {}", i + 1),
            );
        panic!(
            "sweep output differs from tests/golden/sweep.csv at {line}. If the change is \
             intended, regenerate with CCM_UPDATE_GOLDEN=1 and commit the new file."
        );
    }
}
