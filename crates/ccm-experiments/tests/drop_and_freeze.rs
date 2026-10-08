//! Drop-and-Freeze regression tests, moved out of the algorithm source.

mod common;

use ccm_algorithms::{simulate, simulate_with, DropStatus, SharedRecorder};
use ccm_core::{ComplexityMetrics, NodeId, PortEdge, PortGraph, PortId, Termination};
use ccm_trace::{FullTrace, Phase, SimulationEvent};
use common::graph;

fn statuses(state: &ccm_algorithms::SimulationState) -> Vec<DropStatus> {
    state.agents.iter().map(|agent| agent.status).collect()
}

#[test]
fn path3_matches_python_final_state_and_round_phases() {
    let mut metrics = ComplexityMetrics::default();
    let run = simulate_with(
        &graph(&[(0, 1), (1, 2)], 3),
        &[NodeId(0), NodeId(0), NodeId(0)],
        3,
        &mut metrics,
        SharedRecorder::new(FullTrace::new()),
    )
    .unwrap();
    assert_eq!(
        run.final_state.positions(),
        vec![NodeId(0), NodeId(1), NodeId(2)]
    );
    assert_eq!(statuses(&run.final_state), vec![DropStatus::Settled; 3]);

    let trace = run.trace.into_trace();
    let phases: Vec<(u64, Phase)> = trace
        .events
        .iter()
        .filter_map(|recorded| match recorded.event {
            SimulationEvent::PhaseStarted { phase } => Some((recorded.step, phase)),
            _ => None,
        })
        .collect();
    let mut expected = vec![(0, Phase::Other)];
    for round in 0..3_u64 {
        expected.push((round * 3 + 1, Phase::ProbeOut));
        expected.push((round * 3 + 2, Phase::ProbeBack));
        expected.push((round * 3 + 3, Phase::Movement));
    }
    assert_eq!(phases, expected);

    assert_eq!(metrics.macro_rounds, 3);
    assert_eq!(metrics.rounds, 9);
    assert_eq!(metrics.port_probes, 3);
    assert_eq!(metrics.agent_moves, 9);
    assert_eq!(metrics.settlements, 3);
    assert_eq!(metrics.group_moves, 2);
    assert_eq!(metrics.maximum_group_size, 2);
    assert_eq!(run.termination, Termination::Completed);
}

#[test]
fn structured_final_positions_match_reference_cases() {
    let cases = [
        (
            graph(&[(0, 1), (1, 2)], 3),
            vec![NodeId(0), NodeId(0), NodeId(0)],
            vec![NodeId(0), NodeId(1), NodeId(2)],
        ),
        (
            graph(&[(0, 1), (1, 2), (2, 0)], 3),
            vec![NodeId(0), NodeId(0), NodeId(0)],
            vec![NodeId(0), NodeId(2), NodeId(1)],
        ),
        (
            graph(&[(0, 1), (0, 2), (0, 3)], 4),
            vec![NodeId(0); 4],
            vec![NodeId(0), NodeId(2), NodeId(3), NodeId(1)],
        ),
        (
            graph(&[(0, 1), (0, 2), (1, 3), (1, 4)], 5),
            vec![NodeId(0); 5],
            vec![NodeId(0), NodeId(2), NodeId(1), NodeId(3), NodeId(4)],
        ),
        (
            graph(&[(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)], 4),
            vec![NodeId(0); 4],
            vec![NodeId(0), NodeId(2), NodeId(1), NodeId(3)],
        ),
    ];
    for (graph, starts, expected) in cases {
        let result = simulate(&graph, &starts, 10).unwrap();
        assert_eq!(result.final_state.positions(), expected);
        assert_eq!(result.termination, Termination::Completed);
    }
}

#[test]
fn round_limit_is_not_success() {
    let result = simulate(&graph(&[(0, 1), (1, 2)], 3), &[NodeId(0); 3], 1).unwrap();
    assert_eq!(
        statuses(&result.final_state),
        vec![
            DropStatus::Settled,
            DropStatus::Unsettled,
            DropStatus::Unsettled
        ]
    );
    assert_eq!(
        result.termination,
        Termination::RoundLimitReached { limit: 1 }
    );
}

#[test]
fn randomized_input_ports_are_normalized_like_python_init_ports() {
    let edge = |neighbor, remote_port| PortEdge {
        neighbor: NodeId(neighbor),
        remote_port: PortId(remote_port),
    };
    let randomized = PortGraph::from_port_tables(vec![
        vec![edge(2, 0), edge(1, 0)],
        vec![edge(0, 1), edge(2, 1)],
        vec![edge(0, 0), edge(1, 1)],
    ])
    .unwrap();
    let canonical = graph(&[(0, 1), (0, 2), (1, 2)], 3);
    let a = simulate(&randomized, &[NodeId(0); 3], 5).unwrap();
    let b = simulate(&canonical, &[NodeId(0); 3], 5).unwrap();
    assert_eq!(a.final_state, b.final_state);
    assert_eq!(a.metrics, b.metrics);
}

#[test]
fn two_start_nodes_settle_independently() {
    let result = simulate(&graph(&[(0, 1), (1, 2)], 3), &[NodeId(0), NodeId(2)], 1).unwrap();
    assert_eq!(result.final_state.positions(), vec![NodeId(0), NodeId(2)]);
    assert_eq!(statuses(&result.final_state), vec![DropStatus::Settled; 2]);
}
