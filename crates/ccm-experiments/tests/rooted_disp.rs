//! `RootedDisp` (Sudo et al., Section 3) tests: the two worked traces of
//! `docs/rooted-disp.md`, the task card's four properties (permanent homes,
//! temporary helping, return from probes, termination), Theorem 1's bounds,
//! and what separates it from the legacy baselines.

mod common;

use ccm_algorithms::rooted_disp::{simulate, ProbeStats, RootedError, RootedResult, RootedRole};
use ccm_core::{
    AgentStatus, ComplexityMetrics, NodeId, PortAssignment, PortGraph, PortId, Termination,
};
use ccm_experiments::{generate_graph, GraphFamily, GraphSpec};
use ccm_trace::{Checkpoint, FullTrace, NoTrace, Phase, SimulationEvent, Trace};
use common::{graph, random_port_graph, rooted};

fn run(
    g: &PortGraph,
    starts: &[NodeId],
    round_limit: u64,
) -> Result<(RootedResult, ComplexityMetrics), RootedError> {
    simulate(
        g,
        starts,
        round_limit,
        ComplexityMetrics::default(),
        NoTrace,
    )
    .map(|(result, metrics, _)| (result, metrics))
}

fn traced(g: &PortGraph, agents: usize) -> (RootedResult, ComplexityMetrics, Trace) {
    let (result, metrics, recorder) = simulate(
        g,
        &rooted(agents),
        1_000_000,
        ComplexityMetrics::default(),
        FullTrace::new(),
    )
    .unwrap();
    (result, metrics, recorder.into_trace())
}

fn path4() -> PortGraph {
    graph(&[(0, 1), (1, 2), (2, 3)], 4)
}

/// Hub 0, leaves 1–8; canonical ports put leaf `i` on hub port `i − 1`.
fn star9() -> PortGraph {
    let edges: Vec<(u32, u32)> = (1..9).map(|leaf| (0, leaf)).collect();
    graph(&edges, 9)
}

/// The phase of every step after step 0, in order.
fn phases(trace: &Trace) -> Vec<Phase> {
    trace
        .events
        .iter()
        .filter(|event| event.step > 0)
        .filter_map(|event| match event.event {
            SimulationEvent::PhaseStarted { phase } => Some(phase),
            _ => None,
        })
        .collect()
}

/// The phase recorded at `step`.
fn phase_at(trace: &Trace, step: u64) -> Phase {
    trace
        .events
        .iter()
        .find_map(|event| match event.event {
            SimulationEvent::PhaseStarted { phase } if event.step == step => Some(phase),
            _ => None,
        })
        .expect("every step records its phase")
}

/// `(agent, out port)` of every single-agent move made at `step`.
fn moves_at(trace: &Trace, step: u64) -> Vec<(u32, u16)> {
    trace
        .events
        .iter()
        .filter(|event| event.step == step)
        .filter_map(|event| match event.event {
            SimulationEvent::AgentMoved {
                agent, out_port, ..
            } => Some((agent.0, out_port.0)),
            _ => None,
        })
        .collect()
}

/// Agents standing on `node` other than `excluded`, at a checkpoint.
fn others_on(checkpoint: &Checkpoint, node: u32, excluded: u32) -> usize {
    checkpoint
        .agent_nodes
        .iter()
        .enumerate()
        .filter(|&(agent, at)| at.0 == node && u32::try_from(agent).unwrap() != excluded)
        .count()
}

fn homes(result: &RootedResult) -> Vec<u32> {
    result
        .agents
        .iter()
        .map(|agent| agent.home.expect("every agent settles").0)
        .collect()
}

fn max_degree(g: &PortGraph) -> u64 {
    (0..g.node_count())
        .map(|node| u64::from(g.degree(NodeId(u32::try_from(node).unwrap())).unwrap()))
        .max()
        .unwrap_or(0)
}

/// Graphs for the property tests: connected, with seeded random port labels,
/// and an agent count from 1 up to every node.
fn random_cases() -> Vec<(u64, PortGraph, usize)> {
    (0..60_u64)
        .map(|seed| {
            let nodes = 4 + usize::try_from(seed % 9).unwrap();
            let g = random_port_graph(nodes, usize::try_from(seed % 6).unwrap(), seed);
            let agents = 1 + usize::try_from(seed % 7).unwrap().min(nodes - 1);
            let agents = if seed % 3 == 0 { nodes } else { agents };
            (seed, g, agents)
        })
        .collect()
}

// ----- Hand traces (docs/rooted-disp.md, "Worked examples") -----

#[test]
fn the_path_hand_trace_is_reproduced_step_for_step() {
    let (result, metrics, trace) = traced(&path4(), 4);
    assert_eq!(result.termination, Termination::Completed);
    assert_eq!(result.steps, 13);
    assert_eq!(metrics.rounds, 13);
    assert_eq!(
        phases(&trace),
        vec![
            Phase::ProbeOut,
            Phase::ProbeBack,
            Phase::Movement,
            Phase::ProbeOut,
            Phase::ProbeBack,
            Phase::HelperReturn,
            Phase::Movement,
            Phase::ProbeOut,
            Phase::ProbeBack,
            Phase::ProbeOut,
            Phase::ProbeBack,
            Phase::HelperReturn,
            Phase::Movement,
        ]
    );
    assert_eq!(moves_at(&trace, 1), vec![(1, 0)]);
    assert_eq!(moves_at(&trace, 4), vec![(2, 0), (3, 1)]);
    // Step 10: the recruited helper, the smallest ID at node 2, finds node 3.
    assert_eq!(moves_at(&trace, 10), vec![(1, 1)]);
    assert_eq!(
        result.stats,
        ProbeStats {
            probe_calls: 3,
            probe_passes: 4,
            max_passes_in_one_probe: 2,
            helper_recruitments: 2,
            helper_return_steps: 2,
            forward_moves: 3,
            backward_moves: 0,
        }
    );
    assert_eq!(metrics.port_probes, 5);
    assert_eq!(metrics.agent_moves, 20);
    assert_eq!(homes(&result), vec![0, 1, 2, 3]);
    let parents: Vec<Option<PortId>> = result.agents.iter().map(|a| a.memory.parent).collect();
    assert_eq!(
        parents,
        vec![None, Some(PortId(0)), Some(PortId(0)), Some(PortId(0))]
    );
}

#[test]
fn the_star_hand_trace_is_reproduced() {
    let (result, metrics) = run(&star9(), &rooted(9), 1_000).unwrap();
    assert_eq!(result.termination, Termination::Completed);
    assert_eq!(result.steps, 73);
    assert_eq!(
        result.stats,
        ProbeStats {
            probe_calls: 15,
            probe_passes: 22,
            max_passes_in_one_probe: 4,
            helper_recruitments: 35,
            helper_return_steps: 14,
            forward_moves: 8,
            backward_moves: 7,
        }
    );
    assert_eq!(metrics.rounds, 73);
    assert_eq!(metrics.probe_rounds, 44);
    assert_eq!(metrics.movement_rounds, 29);
    assert_eq!(metrics.port_probes, 65);
    assert_eq!(metrics.agent_moves, 264);
    assert_eq!(metrics.settlements, 9);
    assert_eq!(metrics.backtracks, 7);
    assert_eq!(homes(&result), (0..9).collect::<Vec<u32>>());
}

#[test]
fn the_thirteenth_probe_on_the_star_is_figure_one() {
    let (_, _, trace) = traced(&star9(), 9);
    let at = |step: usize| others_on(&trace.checkpoints[step], 0, 0);
    // Probe 13 starts after step 51; its passes end at steps 53 and 55.
    assert_eq!((at(51), at(53), at(55)), (2, 4, 8));
    // Probe 15, the last, doubles from a_max alone: 1 -> 2 -> 4 -> 8.
    assert_eq!((at(63), at(65), at(67), at(69)), (1, 2, 4, 8));
}

#[test]
fn several_finds_in_one_pass_take_the_smallest_port() {
    // Probe 9 on the star: its second pass sends four probers to ports 4-7,
    // all unsettled, and the group goes through port 4 to leaf 5.
    let (_, _, trace) = traced(&star9(), 9);
    assert_eq!(moves_at(&trace, 34), vec![(1, 4), (2, 5), (3, 6), (4, 7)]);
    let forward = trace
        .events
        .iter()
        .find_map(|event| match &event.event {
            SimulationEvent::GroupMoved { to, out_port, .. } if event.step == 37 => {
                Some((to.0, out_port.0))
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(forward, (5, 4));
}

// ----- Permanent settler homes -----

#[test]
fn settler_homes_never_change_and_match_the_ownership_index() {
    for (seed, g, agents) in random_cases() {
        let (result, _, trace) = traced(&g, agents);
        assert_eq!(result.termination, Termination::Completed, "seed {seed}");
        let mut first_home: Vec<Option<NodeId>> = vec![None; agents];
        for checkpoint in &trace.checkpoints {
            for (agent, home) in checkpoint.home_nodes.iter().enumerate() {
                if let Some(earlier) = first_home[agent] {
                    assert_eq!(
                        *home,
                        Some(earlier),
                        "seed {seed}: agent {agent} moved home"
                    );
                } else {
                    first_home[agent] = *home;
                }
            }
            let mut owned = vec![false; g.node_count()];
            for home in checkpoint.home_nodes.iter().flatten() {
                assert!(
                    !owned[home.index()],
                    "seed {seed}: node {} has two homes",
                    home.0
                );
                owned[home.index()] = true;
            }
            for (node, settler) in checkpoint.settled_agents.iter().enumerate() {
                if let Some(settler) = settler {
                    let at = checkpoint.agent_nodes[settler.index()];
                    assert_eq!(at.index(), node, "seed {seed}: settler not on its node");
                    assert_eq!(
                        checkpoint.home_nodes[settler.index()],
                        Some(at),
                        "seed {seed}"
                    );
                }
            }
        }
    }
}

// ----- Temporary helping -----

#[test]
fn helpers_stand_next_to_their_homes_at_the_probe_centre() {
    for (seed, g, agents) in random_cases() {
        let (_, _, trace) = traced(&g, agents);
        let leader = agents - 1;
        for checkpoint in &trace.checkpoints {
            if phase_at(&trace, checkpoint.step) != Phase::ProbeBack {
                continue;
            }
            let centre = checkpoint.agent_nodes[leader];
            for (agent, status) in checkpoint.agent_statuses.iter().enumerate() {
                if *status != AgentStatus::SettledScout {
                    continue;
                }
                let home = checkpoint.home_nodes[agent].expect("a helper is a settler");
                assert_eq!(checkpoint.agent_nodes[agent], centre, "seed {seed}");
                assert!(
                    g.ports(home).any(|(_, edge)| edge.neighbor == centre),
                    "seed {seed}: helper {agent} is not one edge from home"
                );
            }
        }
    }
}

#[test]
fn no_prober_lands_on_a_home_whose_settler_is_away() {
    for (seed, g, agents) in random_cases() {
        let (_, _, trace) = traced(&g, agents);
        for checkpoint in &trace.checkpoints {
            if phase_at(&trace, checkpoint.step) != Phase::ProbeOut {
                continue;
            }
            for node in 0..g.node_count() {
                let owned = checkpoint
                    .home_nodes
                    .iter()
                    .any(|h| h.map(NodeId::index) == Some(node));
                let visited = checkpoint
                    .agent_nodes
                    .iter()
                    .zip(&checkpoint.agent_statuses)
                    .any(|(at, status)| at.index() == node && *status != AgentStatus::Settled);
                if owned && visited {
                    assert!(
                        checkpoint.settled_agents[node].is_some(),
                        "seed {seed}: a prober reached node {node} while its settler was away"
                    );
                }
            }
        }
    }
}

#[test]
fn every_probe_starts_again_from_port_zero() {
    // Line 14 resets `checked`: no cursor survives from an earlier call, so
    // the first pass of every Probe() includes port 0.
    for (seed, g, agents) in random_cases() {
        let (_, _, trace) = traced(&g, agents);
        let mut previous = Phase::Other;
        for checkpoint in trace.checkpoints.iter().skip(1) {
            let phase = phase_at(&trace, checkpoint.step);
            if phase == Phase::ProbeOut && matches!(previous, Phase::Other | Phase::Movement) {
                let ports: Vec<u16> = moves_at(&trace, checkpoint.step)
                    .iter()
                    .map(|&(_, port)| port)
                    .collect();
                assert_eq!(
                    ports.iter().min(),
                    Some(&0),
                    "seed {seed}, step {}",
                    checkpoint.step
                );
            }
            previous = phase;
        }
    }
}

// ----- Return from probes -----

#[test]
fn every_helper_is_home_before_each_dfs_move_and_at_the_end() {
    for (seed, g, agents) in random_cases() {
        let (result, _, trace) = traced(&g, agents);
        for checkpoint in &trace.checkpoints {
            if phase_at(&trace, checkpoint.step) != Phase::Movement {
                continue;
            }
            assert!(
                !checkpoint
                    .agent_statuses
                    .contains(&AgentStatus::SettledScout),
                "seed {seed}: a helper is away at step {}",
                checkpoint.step
            );
        }
        for agent in &result.agents {
            assert_eq!(agent.role, RootedRole::Settler, "seed {seed}");
            assert_eq!(Some(agent.node), agent.home, "seed {seed}");
            assert_eq!(agent.memory.return_port, None, "seed {seed}");
        }
    }
}

// ----- Termination -----

#[test]
fn the_leader_settles_last_and_ends_the_run() {
    for (seed, g, agents) in random_cases() {
        let (result, _, trace) = traced(&g, agents);
        let last = trace
            .events
            .iter()
            .filter_map(|event| match event.event {
                SimulationEvent::AgentSettled { agent, .. } => Some(agent.0),
                _ => None,
            })
            .next_back();
        assert_eq!(
            last,
            Some(u32::try_from(agents - 1).unwrap()),
            "seed {seed}"
        );
        assert_eq!(
            phase_at(&trace, result.steps),
            if agents == 1 {
                Phase::Other
            } else {
                Phase::Movement
            }
        );
    }
}

#[test]
fn a_single_agent_settles_at_once() {
    let (result, metrics) = run(&path4(), &rooted(1), 10).unwrap();
    assert_eq!(result.termination, Termination::Completed);
    assert_eq!(result.steps, 0);
    assert_eq!(metrics.rounds, 0);
    assert_eq!(homes(&result), vec![0]);
}

#[test]
fn the_round_limit_stops_the_run_and_reports_it() {
    let (result, metrics) = run(&path4(), &rooted(4), 5).unwrap();
    assert_eq!(
        result.termination,
        Termination::RoundLimitReached { limit: 5 }
    );
    assert_eq!(result.steps, 5);
    assert_eq!(metrics.rounds, 5);
    assert!(result
        .agents
        .iter()
        .any(|agent| agent.role != RootedRole::Settler));
    // Exactly enough steps is a completion, not a limit.
    assert_eq!(
        run(&path4(), &rooted(4), 13).unwrap().0.termination,
        Termination::Completed
    );
}

#[test]
fn inputs_outside_the_rooted_setting_are_rejected() {
    let g = path4();
    assert_eq!(
        run(&g, &[NodeId(0), NodeId(2)], 100),
        Err(RootedError::NonRootedStart)
    );
    assert_eq!(run(&g, &[], 100), Err(RootedError::NoAgents));
    assert_eq!(
        run(&g, &rooted(5), 100),
        Err(RootedError::TooManyAgents {
            agents: 5,
            nodes: 4
        })
    );
    assert_eq!(
        run(&g, &[NodeId(7)], 100),
        Err(RootedError::InvalidStart(NodeId(7)))
    );
    // Two components: three agents cannot all settle in the root's two nodes.
    let split = graph(&[(0, 1), (2, 3)], 4);
    assert_eq!(
        run(&split, &rooted(3), 100),
        Err(RootedError::RootExhausted)
    );
}

// ----- Lemma 1 and Theorem 1 -----

#[test]
fn costs_respect_lemma_one_and_theorem_one() {
    for (seed, g, agents) in random_cases() {
        let (result, metrics) = run(&g, &rooted(agents), 1_000_000).unwrap();
        let k = u64::try_from(agents).unwrap();
        let stats = result.stats;
        let tau = k.min(max_degree(&g));
        let log = u64::from((tau + 1).ilog2());
        assert_eq!(stats.forward_moves, k - 1, "seed {seed}");
        assert!(stats.backward_moves <= k.saturating_sub(1), "seed {seed}");
        assert_eq!(
            stats.probe_calls,
            stats.forward_moves + stats.backward_moves,
            "seed {seed}"
        );
        assert!(stats.max_passes_in_one_probe <= log + 1, "seed {seed}");
        assert!(
            stats.helper_return_steps <= stats.probe_calls,
            "seed {seed}"
        );
        assert_eq!(
            result.steps,
            2 * stats.probe_passes + stats.helper_return_steps + stats.probe_calls,
            "seed {seed}: every step is a probe step, a helpers-home step or a DFS move"
        );
        assert!(result.steps <= 2 * (k - 1) * (2 * log + 4), "seed {seed}");
        assert_eq!(metrics.rounds, result.steps, "seed {seed}");
        assert_eq!(metrics.probe_rounds, 2 * stats.probe_passes, "seed {seed}");
    }
}

#[test]
fn probing_cost_grows_with_log_degree_not_degree() {
    // A star rooted at a leaf: the hub's degree grows 16-fold, the agent
    // count stays at 8, and the most passes any Probe() needs stays bounded
    // by log of the agent count.
    let mut passes = Vec::new();
    for leaves in [16_u32, 256] {
        let edges: Vec<(u32, u32)> = (1..=leaves).map(|leaf| (0, leaf)).collect();
        let g = graph(&edges, usize::try_from(leaves).unwrap() + 1);
        let (result, _) = run(&g, &[NodeId(1); 8], 1_000_000).unwrap();
        assert_eq!(result.termination, Termination::Completed);
        passes.push(result.stats.max_passes_in_one_probe);
    }
    assert_eq!(passes[0], passes[1]);
    assert!(passes[1] <= u64::from(9_u32.ilog2()) + 1);
}

#[test]
fn parent_ports_form_a_dfs_tree_rooted_at_the_start() {
    for (seed, g, agents) in random_cases() {
        let (result, _) = run(&g, &rooted(agents), 1_000_000).unwrap();
        let settler_at = |node: NodeId| result.agents.iter().find(|a| a.home == Some(node));
        for agent in &result.agents {
            let mut at = agent.home.unwrap();
            let mut hops = 0;
            while let Some(port) = settler_at(at).unwrap().memory.parent {
                at = g
                    .traverse(at, port)
                    .expect("parent port is a real port")
                    .neighbor;
                assert!(
                    settler_at(at).is_some(),
                    "seed {seed}: parent node unsettled"
                );
                hops += 1;
                assert!(hops < agents, "seed {seed}: parent pointers loop");
            }
            assert_eq!(at, NodeId(0), "seed {seed}: does not reach the root");
        }
    }
}

// ----- Against the legacy baselines -----

#[test]
fn supplied_port_labels_are_used_as_given_unlike_drop_and_freeze() {
    // Reversing the hub's port order sends RootedDisp's second agent to a
    // different leaf. Drop-and-Freeze rewrites ports to canonical order, so
    // its result cannot change.
    let star = |ports| generate_graph(&GraphSpec::new(GraphFamily::Star, 5), ports, 0).unwrap();
    let canonical = star(PortAssignment::Canonical);
    let reversed = star(PortAssignment::Adversarial);
    let ours = |g: &PortGraph| homes(&run(g, &rooted(2), 100).unwrap().0);
    assert_eq!(ours(&canonical), vec![0, 1]);
    assert_eq!(ours(&reversed), vec![0, 4]);
    let legacy = |g: &PortGraph| {
        ccm_algorithms::drop_and_freeze::simulate(g, &rooted(2), 100)
            .unwrap()
            .final_state
            .positions()
    };
    assert_eq!(legacy(&canonical), legacy(&reversed));
}
