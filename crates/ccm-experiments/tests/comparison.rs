//! Multi-start comparison through the runner: one prepared input per case,
//! given to every algorithm. Rooted-only algorithms must be refused before
//! they run; `GeneralDisp` must disperse; and the role-transition trace must
//! come from the same preparation as the CSV row.

use ccm_core::{Algorithm, NodeId, PortAssignment, Termination};
use ccm_experiments::{
    default_round_limit, execute_one, general_disp_transitions, prepare_run, requires_rooted_start,
    BuiltinRunner, GraphFamily, GraphSpec, PlacementSpec, RunError, RunRequest,
};

const ALGORITHMS: [Algorithm; 5] = [
    Algorithm::DropAndFreeze,
    Algorithm::HelpByScouts,
    Algorithm::P1Tree,
    Algorithm::RootedDisp,
    Algorithm::GeneralDisp,
];

fn request(
    algorithm: Algorithm,
    family: GraphFamily,
    nodes: usize,
    agents: usize,
    placement: PlacementSpec,
    seed: u64,
) -> RunRequest {
    RunRequest {
        algorithm,
        graph: GraphSpec::new(family, nodes),
        port_assignment: PortAssignment::Random,
        placement,
        agents,
        seed,
        round_limit: None,
    }
}

fn scattered() -> Vec<PlacementSpec> {
    vec![
        PlacementSpec::UniformRandom { distinct_nodes: 2 },
        PlacementSpec::UniformRandom { distinct_nodes: 5 },
        PlacementSpec::Clustered {
            centers: vec![NodeId(0), NodeId(5), NodeId(11)],
        },
    ]
}

#[test]
fn only_rooted_only_algorithms_are_refused_on_scattered_starts() {
    assert!(requires_rooted_start(Algorithm::HelpByScouts));
    assert!(requires_rooted_start(Algorithm::P1Tree));
    assert!(requires_rooted_start(Algorithm::RootedDisp));
    assert!(!requires_rooted_start(Algorithm::DropAndFreeze));
    assert!(!requires_rooted_start(Algorithm::GeneralDisp));

    for placement in scattered() {
        for seed in 0..5 {
            for algorithm in ALGORITHMS {
                let req = request(
                    algorithm,
                    GraphFamily::RandomConnected { extra_edges: 10 },
                    12,
                    8,
                    placement.clone(),
                    seed,
                );
                let starts = prepare_run(req.clone()).unwrap().starts;
                let scattered = starts.windows(2).any(|pair| pair[0] != pair[1]);
                let outcome = execute_one(&BuiltinRunner, req);
                if requires_rooted_start(algorithm) && scattered {
                    assert_eq!(
                        outcome,
                        Err(RunError::RequiresRootedStart {
                            algorithm,
                            placement: placement.label(),
                        }),
                        "{algorithm:?} {}",
                        placement.label()
                    );
                } else {
                    assert!(outcome.is_ok(), "{algorithm:?}: {outcome:?}");
                }
            }
        }
    }
}

#[test]
fn every_algorithm_sees_the_same_starts_for_the_same_request() {
    for placement in scattered() {
        let base = |algorithm| {
            request(
                algorithm,
                GraphFamily::Grid { columns: 4 },
                12,
                8,
                placement.clone(),
                3,
            )
        };
        let reference = prepare_run(base(Algorithm::GeneralDisp)).unwrap();
        for algorithm in ALGORITHMS {
            let prepared = prepare_run(base(algorithm)).unwrap();
            assert_eq!(prepared.graph, reference.graph, "{algorithm:?}");
            assert_eq!(prepared.starts, reference.starts, "{algorithm:?}");
        }
    }
}

#[test]
fn general_disp_completes_every_scattered_case_within_its_default_limit() {
    let families = [
        GraphFamily::Path,
        GraphFamily::Cycle,
        GraphFamily::Star,
        GraphFamily::Complete,
        GraphFamily::Tree { branching: 3 },
        GraphFamily::Grid { columns: 4 },
        GraphFamily::RandomConnected { extra_edges: 10 },
        GraphFamily::RandomBoundedDegree { max_degree: 4 },
    ];
    for family in families {
        for placement in scattered() {
            for agents in [4, 8, 12] {
                for seed in 0..3 {
                    let req = request(
                        Algorithm::GeneralDisp,
                        family.clone(),
                        12,
                        agents,
                        placement.clone(),
                        seed,
                    );
                    let result = execute_one(&BuiltinRunner, req).unwrap();
                    assert_eq!(
                        result.termination,
                        Termination::Completed,
                        "{} {} k={agents} seed {seed}",
                        family.label(),
                        placement.label()
                    );
                    assert!(
                        result.metrics.rounds
                            < default_round_limit(Algorithm::GeneralDisp, agents) / 10,
                        "the default limit leaves a wide margin"
                    );
                }
            }
        }
    }
}

#[test]
fn the_default_limit_is_unchanged_for_the_other_algorithms() {
    for algorithm in [
        Algorithm::DropAndFreeze,
        Algorithm::HelpByScouts,
        Algorithm::P1Tree,
        Algorithm::RootedDisp,
    ] {
        assert_eq!(default_round_limit(algorithm, 9), 360, "{algorithm:?}");
    }
    assert!(default_round_limit(Algorithm::GeneralDisp, 9) > 360);
}

#[test]
fn the_transition_trace_matches_the_csv_row() {
    let req = request(
        Algorithm::GeneralDisp,
        GraphFamily::RandomConnected { extra_edges: 6 },
        20,
        12,
        PlacementSpec::UniformRandom { distinct_nodes: 4 },
        7,
    );
    let row = execute_one(&BuiltinRunner, req.clone()).unwrap();
    let trace = general_disp_transitions(req).unwrap();
    assert_eq!(trace.termination, row.termination);
    assert_eq!(trace.steps, row.metrics.rounds);
    let settled = trace
        .transitions
        .iter()
        .filter(|t| t.kind == "settled")
        .count();
    assert_eq!(u64::try_from(settled).unwrap(), row.metrics.settlements);
    assert!(trace
        .transitions
        .windows(2)
        .all(|pair| pair[0].step <= pair[1].step));
}

#[test]
fn transitions_refuse_other_algorithms_and_rooted_only_rules_still_apply() {
    let req = request(
        Algorithm::RootedDisp,
        GraphFamily::Path,
        6,
        3,
        PlacementSpec::SingleNode { node: NodeId(0) },
        1,
    );
    assert_eq!(
        general_disp_transitions(req),
        Err(RunError::AlgorithmUnavailable(Algorithm::RootedDisp))
    );
}
