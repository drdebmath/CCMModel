//! `RootedAsync` against the paper's worked example and across schedules.

mod common;

use ccm_algorithms::rooted_async::{
    simulate, AgentState, Position, ProbeRule, RootedAsyncError, RootedAsyncResult, Schedule,
};
use ccm_core::{
    AgentId, ComplexityMetrics, NodeId, PortAssignment, PortEdge, PortGraph, PortId, Termination,
};
use ccm_experiments::{generate_graph, GraphFamily, GraphSpec};
use ccm_trace::NoTrace;
use common::rooted;

fn run(g: &PortGraph, agents: usize, schedule: Schedule) -> (RootedAsyncResult, ComplexityMetrics) {
    let (result, metrics, _) = simulate(
        g,
        &rooted(agents),
        schedule,
        1_000_000,
        ComplexityMetrics::default(),
        NoTrace,
    )
    .unwrap_or_else(|error| panic!("{schedule:?}: {error}"));
    (result, metrics)
}

/// Each agent's home, in agent order.
fn homes(result: &RootedAsyncResult) -> Vec<NodeId> {
    result
        .agents
        .iter()
        .map(|agent| agent.home.expect("every agent settles"))
        .collect()
}

fn assert_dispersed(result: &RootedAsyncResult, case: &str) {
    assert_eq!(result.termination, Termination::Completed, "{case}");
    let mut seen = std::collections::BTreeSet::new();
    for (index, agent) in result.agents.iter().enumerate() {
        assert_eq!(
            agent.memory.state,
            AgentState::Settled,
            "{case}: agent {index}"
        );
        let home = agent.home.expect("settled");
        assert_eq!(
            agent.position,
            Position::At(home),
            "{case}: agent {index} is not home"
        );
        assert!(
            seen.insert(home),
            "{case}: node {} holds two agents",
            home.0
        );
    }
}

fn schedules() -> Vec<Schedule> {
    let mut all = vec![Schedule::Synchronous];
    all.extend((0..6).map(|seed| Schedule::Random { seed }));
    all.extend((0..6).map(|seed| Schedule::Interleaved { seed }));
    all
}

/// Figure 5 of the paper, ports renumbered from 0. Nodes: v=0, u=1, w=2, x=3,
/// y=4, z=5. The figure omits x's fourth port, to y; step 6 of the example
/// probes that edge, so it is included here.
fn figure_five() -> PortGraph {
    let e = |neighbor: u32, remote: u16| PortEdge {
        neighbor: NodeId(neighbor),
        remote_port: PortId(remote),
    };
    PortGraph::from_port_tables(vec![
        vec![e(1, 0), e(3, 2)],
        vec![e(0, 0), e(2, 0), e(3, 0)],
        vec![e(1, 1), e(3, 1), e(4, 2), e(5, 1)],
        vec![e(1, 2), e(2, 1), e(0, 1), e(4, 1)],
        vec![e(5, 0), e(3, 3), e(2, 2)],
        vec![e(4, 0), e(2, 3)],
    ])
    .unwrap()
}

#[test]
fn the_papers_example_run_ends_as_appendix_b_says() {
    for schedule in schedules() {
        let (result, metrics) = run(&figure_five(), 6, schedule);
        assert_dispersed(&result, &format!("{schedule:?}"));
        // a1..a6 are agents 0..5. Final settlement: psi(v)=a6, psi(u)=a5,
        // psi(w)=a4, psi(x)=a3, psi(y)=a2, psi(z)=a1.
        assert_eq!(
            homes(&result),
            [5, 4, 3, 2, 1, 0].map(NodeId).to_vec(),
            "{schedule:?}"
        );
        // u is vacated by (V2) and x by (V4); both come home in Retrace.
        assert_eq!(metrics.vacates, 2, "{schedule:?}");
    }
}

#[test]
fn the_papers_example_resolves_its_vacated_probes_by_the_named_rules() {
    let (result, _) = run(&figure_five(), 6, Schedule::Synchronous);
    let rule_at = |head: u32, port: u16| {
        result
            .probes
            .iter()
            .find(|p| p.head == NodeId(head) && p.port == PortId(port))
            .map(|p| (p.rule, p.owner))
    };
    // Step 1: from v, x is empty; the chain x -> u -> v finds a6 but no claim.
    assert_eq!(rule_at(0, 1), Some((ProbeRule::R3cIIBeta, None)));
    // Step 4: from x, port 1 reaches the vacated u, claimed by a5 (R3a-i).
    assert_eq!(rule_at(3, 0), Some((ProbeRule::R3aI, Some(AgentId(4)))));
    // Step 6: from y, port 2 reaches the vacated x through u and v (R3c-ii(a)).
    assert_eq!(
        rule_at(4, 1),
        Some((ProbeRule::R3cIIAlpha, Some(AgentId(2))))
    );
}

#[test]
fn every_schedule_disperses_and_reaches_the_same_placement() {
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
    let ports = [
        PortAssignment::Canonical,
        PortAssignment::Random,
        PortAssignment::Adversarial,
    ];
    for family in &families {
        for assignment in ports {
            for seed in 0..3 {
                let g =
                    generate_graph(&GraphSpec::new(family.clone(), 16), assignment, seed).unwrap();
                for agents in [5, 11, 16] {
                    let case = format!(
                        "{} {assignment:?} seed {seed}, {agents} agents",
                        family.label()
                    );
                    let mut placements = Vec::new();
                    for schedule in schedules() {
                        let (result, _) = run(&g, agents, schedule);
                        assert_dispersed(&result, &format!("{case}, {schedule:?}"));
                        placements.push(homes(&result));
                    }
                    // Decisions depend only on what scouts find, never on
                    // timing, so the schedule cannot change where agents end.
                    assert!(
                        placements.windows(2).all(|pair| pair[0] == pair[1]),
                        "{case}: the schedule changed the placement"
                    );
                }
            }
        }
    }
}

#[test]
fn a_slow_scout_or_a_slow_lead_only_delays_the_run() {
    let g = generate_graph(
        &GraphSpec::new(GraphFamily::RandomConnected { extra_edges: 12 }, 14),
        PortAssignment::Random,
        7,
    )
    .unwrap();
    let (fast, _) = run(&g, 12, Schedule::Random { seed: 1 });
    for slow in [0, 3, 11] {
        let (result, _) = run(
            &g,
            12,
            Schedule::Starve {
                seed: 1,
                agent: AgentId(slow),
                period: 40,
            },
        );
        assert_dispersed(&result, &format!("agent {slow} starved"));
        assert_eq!(homes(&result), homes(&fast), "agent {slow} starved");
    }
}

#[test]
fn a_scattered_placement_is_rejected() {
    let g = common::graph(&[(0, 1), (1, 2)], 3);
    let error = simulate(
        &g,
        &[NodeId(0), NodeId(2)],
        Schedule::Synchronous,
        100,
        ComplexityMetrics::default(),
        NoTrace,
    )
    .unwrap_err();
    assert_eq!(error, RootedAsyncError::NonRootedStart);
}

#[test]
fn a_single_agent_settles_where_it_stands() {
    let g = common::graph(&[(0, 1)], 2);
    let (result, _) = run(&g, 1, Schedule::Synchronous);
    assert_dispersed(&result, "one agent");
    assert_eq!(homes(&result), vec![NodeId(0)]);
}
