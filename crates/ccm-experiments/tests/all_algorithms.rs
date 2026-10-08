//! Properties every algorithm must have, checked once for all three.

mod common;

use ccm_algorithms::drop_and_freeze::DropStatus;
use ccm_algorithms::help_by_scouts::HelpStatus;
use ccm_algorithms::p1tree::P1Status;
use ccm_core::{ComplexityMetrics, NodeId, PortAssignment, PortGraph, Termination};
use ccm_experiments::{generate_graph, GraphFamily, GraphSpec};
use ccm_trace::{FullTrace, NoTrace, Recorder};
use common::{graph, rooted};

/// Runs one algorithm with the given recorder and returns its final state,
/// rendered for comparison, and its metrics.
fn summary<R: Recorder>(
    algorithm: &str,
    g: &PortGraph,
    starts: &[NodeId],
    recorder: R,
) -> (String, ComplexityMetrics) {
    match algorithm {
        "drop_and_freeze" => {
            let mut metrics = ComplexityMetrics::default();
            let run = ccm_algorithms::drop_and_freeze::simulate_with(
                g,
                starts,
                1_000,
                &mut metrics,
                ccm_algorithms::drop_and_freeze::SharedRecorder::new(recorder),
            )
            .unwrap();
            (
                format!("{:?} {:?}", run.final_state, run.termination),
                metrics,
            )
        }
        "help_by_scouts" => {
            let (result, metrics, _) = ccm_algorithms::help_by_scouts::simulate(
                g,
                starts,
                1_000,
                ComplexityMetrics::default(),
                recorder,
            )
            .unwrap();
            (format!("{result:?}"), metrics)
        }
        "p1tree" => {
            let (result, metrics, _) = ccm_algorithms::p1tree::simulate(
                g,
                starts,
                1_000,
                ComplexityMetrics::default(),
                recorder,
            )
            .unwrap();
            (format!("{result:?}"), metrics)
        }
        other => unreachable!("unknown algorithm {other}"),
    }
}

const ALGORITHMS: [&str; 3] = ["drop_and_freeze", "help_by_scouts", "p1tree"];

fn cases() -> Vec<PortGraph> {
    vec![
        graph(&[(0, 1), (1, 2), (2, 3)], 4),
        graph(&[(0, 1), (1, 2), (2, 3), (3, 0), (0, 2)], 4),
        graph(&[(0, 1), (0, 2), (1, 3), (1, 4)], 5),
    ]
}

#[test]
fn the_same_input_gives_the_same_result() {
    for algorithm in ALGORITHMS {
        for g in cases() {
            let starts = rooted(g.node_count());
            let first = summary(algorithm, &g, &starts, NoTrace);
            let second = summary(algorithm, &g, &starts, NoTrace);
            assert_eq!(first, second, "{algorithm}");
        }
    }
}

/// Where each agent ended, whether it settled, and the home it claims. A
/// Drop-and-Freeze agent never leaves the node it settles at, so its node is
/// its home.
fn final_placement(
    algorithm: &str,
    g: &PortGraph,
    starts: &[NodeId],
) -> (Termination, Vec<(NodeId, bool, Option<NodeId>)>) {
    const LIMIT: u64 = 1_000_000;
    match algorithm {
        "drop_and_freeze" => {
            let result = ccm_algorithms::drop_and_freeze::simulate(g, starts, LIMIT).unwrap();
            let agents = result.final_state.agents.iter().map(|agent| {
                let settled = agent.status == DropStatus::Settled;
                (agent.node, settled, settled.then_some(agent.node))
            });
            (result.termination, agents.collect())
        }
        "help_by_scouts" => {
            let (result, _, _) = ccm_algorithms::help_by_scouts::simulate(
                g,
                starts,
                LIMIT,
                ComplexityMetrics::default(),
                NoTrace,
            )
            .unwrap();
            let agents = result
                .agents
                .iter()
                .map(|agent| (agent.node, agent.status == HelpStatus::Settled, agent.home));
            (result.termination, agents.collect())
        }
        "p1tree" => {
            let (result, _, _) = ccm_algorithms::p1tree::simulate(
                g,
                starts,
                LIMIT,
                ComplexityMetrics::default(),
                NoTrace,
            )
            .unwrap();
            let agents = result
                .agents
                .iter()
                .map(|agent| (agent.node, agent.status == P1Status::Settled, agent.home));
            (result.termination, agents.collect())
        }
        other => unreachable!("unknown algorithm {other}"),
    }
}

/// The dispersion property itself: every run ends with each agent settled,
/// standing on its own home, and no node holding more than one agent.
#[test]
fn every_run_ends_with_at_most_one_agent_per_node() {
    let families = [
        GraphFamily::Path,
        GraphFamily::Cycle,
        GraphFamily::Star,
        GraphFamily::Complete,
        GraphFamily::Tree { branching: 3 },
        GraphFamily::Grid { columns: 3 },
        GraphFamily::RandomConnected { extra_edges: 8 },
        GraphFamily::RandomBoundedDegree { max_degree: 3 },
    ];
    let ports = [
        PortAssignment::Canonical,
        PortAssignment::Random,
        PortAssignment::Adversarial,
    ];
    let nodes = 10;
    for algorithm in ALGORITHMS {
        for family in &families {
            for assignment in ports {
                for seed in 0..3 {
                    let g =
                        generate_graph(&GraphSpec::new(family.clone(), nodes), assignment, seed)
                            .unwrap();
                    for agents in [4, nodes] {
                        let case = format!(
                            "{algorithm} on {} with {assignment:?} ports, seed {seed}, \
                             {agents} agents",
                            family.label()
                        );
                        let (termination, placed) = final_placement(algorithm, &g, &rooted(agents));
                        assert_eq!(termination, Termination::Completed, "{case}");
                        let mut occupied = vec![false; nodes];
                        for (index, &(node, settled, home)) in placed.iter().enumerate() {
                            assert!(settled, "{case}: agent {index} never settled");
                            assert_eq!(home, Some(node), "{case}: agent {index} is not home");
                            assert!(
                                !occupied[node.index()],
                                "{case}: node {} holds two agents",
                                node.0
                            );
                            occupied[node.index()] = true;
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn tracing_does_not_change_results_or_metrics() {
    for algorithm in ALGORITHMS {
        for g in cases() {
            let starts = rooted(g.node_count());
            let plain = summary(algorithm, &g, &starts, NoTrace);
            let traced = summary(algorithm, &g, &starts, FullTrace::new());
            assert_eq!(plain, traced, "{algorithm}");
        }
    }
}
