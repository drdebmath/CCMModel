//! Help-by-Scouts regression tests, moved out of the algorithm source.

mod common;

use ccm_algorithms::help_by_scouts::{simulate, HelpError, HelpResult, HelpStatus};
use ccm_core::{ComplexityMetrics, NodeId, PortEdge, PortGraph, PortId, Termination};
use ccm_trace::NoTrace;
use common::{graph, rooted};

fn run(graph: &PortGraph, starts: &[NodeId], round_limit: u64) -> Result<HelpResult, HelpError> {
    simulate(
        graph,
        starts,
        round_limit,
        ComplexityMetrics::default(),
        NoTrace,
    )
    .map(|(result, _, _)| result)
}

fn homes(result: &HelpResult) -> Vec<Option<NodeId>> {
    result.agents.iter().map(|agent| agent.home).collect()
}

fn some(nodes: &[u32]) -> Vec<Option<NodeId>> {
    nodes.iter().map(|&node| Some(NodeId(node))).collect()
}

#[test]
fn single_agent_settles_without_moving() {
    let result = run(&graph(&[], 1), &[NodeId(0)], 40).unwrap();
    assert_eq!(result.termination, Termination::Completed);
    assert_eq!(result.agents[0].status, HelpStatus::Settled);
    assert_eq!(result.agents[0].home, Some(NodeId(0)));
}

#[test]
fn rooted_path_matches_python_final_fixture() {
    let result = run(&graph(&[(0, 1), (1, 2)], 3), &rooted(3), 200).unwrap();
    assert_eq!(homes(&result), some(&[2, 1, 0]));
    let mut nodes: Vec<NodeId> = result.agents.iter().map(|agent| agent.node).collect();
    nodes.sort_unstable();
    assert_eq!(nodes, vec![NodeId(0), NodeId(1), NodeId(2)]);
}

#[test]
fn explicit_cycle_matches_python_final_fixture() {
    let edge = |neighbor, remote_port| PortEdge {
        neighbor: NodeId(neighbor),
        remote_port: PortId(remote_port),
    };
    let graph = PortGraph::from_port_tables(vec![
        vec![edge(1, 0), edge(2, 1)],
        vec![edge(0, 0), edge(2, 0)],
        vec![edge(1, 1), edge(0, 1)],
    ])
    .unwrap();
    let result = run(&graph, &rooted(3), 200).unwrap();
    assert_eq!(homes(&result), some(&[2, 1, 0]));
}

#[test]
fn canonical_star_matches_python_final_fixture() {
    let result = run(&graph(&[(0, 1), (0, 2), (0, 3)], 4), &rooted(4), 400).unwrap();
    assert_eq!(homes(&result), some(&[1, 3, 2, 0]));
}

#[test]
fn canonical_tree_matches_python_final_fixture() {
    let g = graph(&[(0, 1), (0, 2), (1, 3), (1, 4)], 5);
    let result = run(&g, &rooted(5), 600).unwrap();
    assert_eq!(homes(&result), some(&[4, 3, 1, 2, 0]));
}

#[test]
fn canonical_complete_four_matches_python_final_fixture() {
    let g = graph(&[(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)], 4);
    let result = run(&g, &rooted(4), 600).unwrap();
    assert_eq!(homes(&result), some(&[3, 1, 2, 0]));
}

#[test]
fn documents_python_multi_start_failure() {
    assert!(matches!(
        run(&graph(&[(0, 1), (1, 2)], 3), &[NodeId(0), NodeId(2)], 200),
        Err(HelpError::MissingAgentAtNode { .. })
    ));
}

#[test]
fn round_limit_returns_partial_state_and_metrics() {
    let (result, metrics, _) = simulate(
        &graph(&[(0, 1), (1, 2)], 3),
        &rooted(3),
        1,
        ComplexityMetrics::default(),
        NoTrace,
    )
    .unwrap();
    assert_eq!(
        result.termination,
        Termination::RoundLimitReached { limit: 1 }
    );
    assert_eq!(metrics.macro_rounds, 1);
    assert_eq!(metrics.settlements, 1);
}
