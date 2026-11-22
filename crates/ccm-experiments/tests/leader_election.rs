//! Leader election by agents (Kshemkalyani et al., arXiv:2403.13716, Section 2)
//! tests: the problem page's example traced round by round as in
//! `docs/leader-election.md`, the task's three properties (one leader, the
//! smallest ID, known to everyone; the round bound; no visit meets an agent
//! away from home), Lemma 4's claim that every edge carries a message in every
//! simulated round, the rooted mode, and rejected input.

mod common;

use ccm_algorithms::leader_election::{
    id_bits, memory_bits_per_agent, schedule_rounds, simulate, ElectionError, ElectionResult,
    StartMode, MEMORY_WORDS_PER_AGENT,
};
use ccm_core::{
    AgentId, AgentOutput, AgentStatus, Algorithm, ComplexityMetrics, NodeId, PortAssignment,
    PortGraph, SchedulerModel, Termination,
};
use ccm_experiments::{
    execute_one, generate_graph, leader_election_round_limit, place_agents, prepare_run,
    BuiltinRunner, GraphFamily, GraphSpec, PlacementSpec, RunError, RunRequest,
};
use ccm_trace::{FullTrace, NoTrace, Phase, SimulationEvent, Trace};
use common::{graph, rooted};
use std::collections::BTreeSet;

fn run(g: &PortGraph, starts: &[NodeId]) -> (ElectionResult, ComplexityMetrics) {
    let (result, metrics, _) =
        simulate(g, starts, u64::MAX, ComplexityMetrics::default(), NoTrace).unwrap();
    (result, metrics)
}

fn traced(g: &PortGraph, starts: &[NodeId]) -> (ElectionResult, ComplexityMetrics, Trace) {
    let (result, metrics, recorder) = simulate(
        g,
        starts,
        u64::MAX,
        ComplexityMetrics::default(),
        FullTrace::new(),
    )
    .unwrap();
    (result, metrics, recorder.into_trace())
}

/// The problem page's path: node 0 – node 1 – node 2, with ID 3 (agent 2) on
/// node 0, ID 2 (agent 1) on node 1 and ID 1 (agent 0) on node 2.
fn page_example() -> (PortGraph, Vec<NodeId>) {
    (
        graph(&[(0, 1), (1, 2)], 3),
        vec![NodeId(2), NodeId(1), NodeId(0)],
    )
}

fn max_degree(g: &PortGraph) -> u16 {
    (0..g.node_count())
        .map(|node| g.degree(node_at(node)).unwrap())
        .max()
        .unwrap_or(0)
}

fn node_at(index: usize) -> NodeId {
    NodeId(u32::try_from(index).unwrap())
}

/// Shortest-path distances from `source`, by a plain BFS.
fn distances(g: &PortGraph, source: NodeId) -> Vec<u64> {
    let mut distance = vec![u64::MAX; g.node_count()];
    distance[source.index()] = 0;
    let mut queue = std::collections::VecDeque::from([source]);
    while let Some(node) = queue.pop_front() {
        for (_, edge) in g.ports(node) {
            if distance[edge.neighbor.index()] == u64::MAX {
                distance[edge.neighbor.index()] = distance[node.index()] + 1;
                queue.push_back(edge.neighbor);
            }
        }
    }
    distance
}

fn eccentricity(g: &PortGraph, source: NodeId) -> u64 {
    distances(g, source).into_iter().max().unwrap()
}

fn diameter(g: &PortGraph) -> u64 {
    (0..g.node_count())
        .map(|node| eccentricity(g, node_at(node)))
        .max()
        .unwrap()
}

/// `(agent, out port)` of every move made in round `step`.
fn moves_at(trace: &Trace, step: u64) -> Vec<(u32, u16)> {
    let mut moves: Vec<_> = trace
        .events
        .iter()
        .filter(|event| event.step == step)
        .filter_map(|event| match event.event {
            SimulationEvent::AgentMoved {
                agent, out_port, ..
            } => Some((agent.0, out_port.0)),
            _ => None,
        })
        .collect();
    moves.sort_unstable();
    moves
}

/// `(round, visitor, host, node)` of every message exchange.
fn exchanges(trace: &Trace) -> Vec<(u64, u32, u32, u32)> {
    trace
        .events
        .iter()
        .filter_map(|event| match event.event {
            SimulationEvent::MessageExchanged {
                visitor,
                host,
                node,
            } => Some((event.step, visitor.0, host.0, node.0)),
            _ => None,
        })
        .collect()
}

/// `(round, agent, new min)` of every change of an agent's `min`.
fn minimum_changes(trace: &Trace) -> Vec<(u64, u32, u32)> {
    trace
        .events
        .iter()
        .filter_map(|event| match event.event {
            SimulationEvent::MinimumChanged { agent, min } => Some((event.step, agent.0, min.0)),
            _ => None,
        })
        .collect()
}

/// Asserts the task's output property: exactly one agent says leader, it is
/// the smallest ID (`AgentId(0)`), and every agent's output names it.
fn assert_elected(result: &ElectionResult, case: &str) {
    assert_eq!(result.termination, Termination::Completed, "{case}");
    let outputs = result.outputs().expect("a completed run has outputs");
    let leaders: Vec<usize> = outputs
        .iter()
        .enumerate()
        .filter(|(_, output)| {
            matches!(
                output,
                AgentOutput::Leader {
                    is_leader: true,
                    ..
                }
            )
        })
        .map(|(index, _)| index)
        .collect();
    assert_eq!(leaders, vec![0], "{case}: the leader is ID 1 alone");
    for (index, output) in outputs.iter().enumerate() {
        let AgentOutput::Leader { leader, .. } = output;
        assert_eq!(
            *leader,
            AgentId(0),
            "{case}: agent {index} names the leader"
        );
        assert_eq!(result.agents[index].memory.min, 1, "{case}");
        assert_eq!(
            result.agents[index].home,
            Some(result.agents[index].node),
            "{case}: agent {index} ends at home"
        );
    }
}

#[test]
fn the_problem_pages_example_round_by_round() {
    let (g, starts) = page_example();
    let (result, metrics, trace) = traced(&g, &starts);
    assert_elected(&result, "page example");
    assert_eq!(result.mode, StartMode::Dispersed);

    // n = 3, Δ = 2, b = 2: blocks of 4 rounds, 2 simulated rounds, 16 rounds.
    assert_eq!(result.stats.id_bits, 2);
    assert_eq!(result.stats.max_degree, 2);
    assert_eq!(result.rounds, 16);
    assert_eq!(result.stats.election_rounds, 16);
    assert_eq!(result.stats.simulated_rounds, 2);
    assert_eq!(result.stats.blocks, 4);
    assert_eq!(result.stats.visits, 10);
    assert_eq!(result.stats.meetings, 6);
    assert_eq!(result.stats.visits_without_message, 4);
    assert_eq!(result.stats.rounds_until_all_knew, Some(16));
    assert_eq!(result.stats.memory_bits_per_agent, 16);
    assert_eq!(metrics.agent_moves, 20);

    // The moves of simulated round 1 (agent = ID − 1), repeated in round 2.
    // A move back leaves through the port the visitor arrived by.
    for offset in [0, 8] {
        assert_eq!(moves_at(&trace, offset + 1), vec![(1, 0), (2, 0)]);
        assert_eq!(moves_at(&trace, offset + 2), vec![(1, 0), (2, 0)]);
        assert_eq!(moves_at(&trace, offset + 3), vec![(1, 1)]);
        assert_eq!(moves_at(&trace, offset + 4), vec![(1, 0)]);
        assert_eq!(moves_at(&trace, offset + 5), vec![(0, 0), (2, 0)]);
        assert_eq!(moves_at(&trace, offset + 6), vec![(0, 1), (2, 0)]);
        assert!(moves_at(&trace, offset + 7).is_empty());
        assert!(moves_at(&trace, offset + 8).is_empty());
    }

    // Round 4: ID 2 meets ID 1 on node 2. Round 6: ID 1 and ID 3 both meet
    // ID 2 on node 1 and take nothing from each other.
    assert_eq!(
        exchanges(&trace),
        vec![
            (4, 1, 0, 2),
            (6, 0, 1, 1),
            (6, 2, 1, 1),
            (12, 1, 0, 2),
            (14, 0, 1, 1),
            (14, 2, 1, 1),
        ]
    );

    // ID 3's min goes 3 → 2 → 1 and ID 2's goes 2 → 1, as in the animation.
    assert_eq!(
        minimum_changes(&trace),
        vec![(8, 1, 0), (8, 2, 1), (16, 2, 0)]
    );

    // Every round alternates out and back.
    let phases: Vec<(u64, Phase)> = trace
        .events
        .iter()
        .filter_map(|event| match event.event {
            SimulationEvent::PhaseStarted { phase } => Some((event.step, phase)),
            _ => None,
        })
        .collect();
    assert_eq!(phases[0], (0, Phase::Other));
    for round in 1..=16 {
        let expected = if round % 2 == 1 {
            Phase::VisitOut
        } else {
            Phase::VisitBack
        };
        assert_eq!(phases[usize::try_from(round).unwrap()], (round, expected));
    }

    // Each agent's output is decided once, in the last round.
    let decided: Vec<_> = trace
        .events
        .iter()
        .filter_map(|event| match event.event {
            SimulationEvent::OutputDecided { agent, output } => Some((event.step, agent.0, output)),
            _ => None,
        })
        .collect();
    assert_eq!(decided.len(), 3);
    for (index, &(step, agent, output)) in decided.iter().enumerate() {
        assert_eq!(step, 16);
        assert_eq!(agent as usize, index);
        assert_eq!(
            output,
            AgentOutput::Leader {
                is_leader: index == 0,
                leader: AgentId(0)
            }
        );
    }
}

/// The graph families and sizes of the task: 2–200 nodes, every family,
/// every port assignment, several seeds, from a dispersed start.
fn dispersed_cases() -> Vec<(String, PortGraph, Vec<NodeId>)> {
    let families = [
        GraphFamily::Path,
        GraphFamily::Cycle,
        GraphFamily::Star,
        GraphFamily::Complete,
        GraphFamily::Tree { branching: 2 },
        GraphFamily::Tree { branching: 3 },
        GraphFamily::Grid { columns: 3 },
        GraphFamily::Grid { columns: 5 },
        GraphFamily::RandomConnected { extra_edges: 6 },
        GraphFamily::RandomBoundedDegree { max_degree: 3 },
    ];
    let assignments = [
        PortAssignment::Canonical,
        PortAssignment::Random,
        PortAssignment::Adversarial,
    ];
    let mut sizes: Vec<(usize, u64)> = (2..=24).map(|n| (n, 3)).collect();
    sizes.extend([(32, 2), (50, 2), (64, 1), (100, 1), (128, 1), (200, 1)]);
    let mut cases = Vec::new();
    for family in &families {
        for &(nodes, seeds) in &sizes {
            // A complete graph or a star of 200 nodes has Δ = 199: one run of
            // each is enough, and the largest sizes use one port assignment.
            for &assignment in &assignments {
                if nodes >= 100 && assignment != PortAssignment::Random {
                    continue;
                }
                for seed in 0..seeds {
                    let Ok(g) =
                        generate_graph(&GraphSpec::new(family.clone(), nodes), assignment, seed)
                    else {
                        continue;
                    };
                    let starts = place_agents(nodes, nodes, &PlacementSpec::Dispersed, seed)
                        .expect("n agents fit on n nodes");
                    let case = format!(
                        "{} with {nodes} nodes, {assignment:?} ports, seed {seed}",
                        family.label()
                    );
                    cases.push((case, g, starts));
                }
            }
        }
    }
    cases
}

#[test]
fn one_leader_with_the_smallest_id_known_to_every_agent() {
    let cases = dispersed_cases();
    assert!(cases.len() > 1_000, "only {} cases generated", cases.len());
    for (case, g, starts) in &cases {
        let (result, metrics) = run(g, starts);
        assert_elected(&result, case);
        assert_eq!(result.mode, StartMode::Dispersed, "{case}");

        let n = g.node_count();
        let delta = max_degree(g);
        let b = u64::from(id_bits(n));
        let n_u64 = u64::try_from(n).unwrap();

        // The round count: exactly (n − 1) · b · 2Δ, which is Theorem 1's
        // O(Δ T log n) with T = n − 1 and constant 2(log₂ n + 1).
        assert_eq!(
            result.rounds,
            (n_u64 - 1) * b * 2 * u64::from(delta),
            "{case}"
        );
        assert_eq!(result.rounds, schedule_rounds(n, delta), "{case}");
        #[allow(clippy::cast_precision_loss)]
        let bound = 2.0 * f64::from(delta) * (n as f64 - 1.0) * ((n as f64).log2() + 1.0);
        #[allow(clippy::cast_precision_loss)]
        let rounds = result.rounds as f64;
        assert!(rounds <= bound, "{case}: {rounds} rounds above {bound}");

        // Every agent knows the leader after ecc(leader's home) simulated
        // rounds, so within the page's D · b · 2Δ.
        let leader_home = starts[0];
        let per_simulated_round = b * 2 * u64::from(delta);
        assert_eq!(
            result.stats.rounds_until_all_knew,
            Some(eccentricity(g, leader_home) * per_simulated_round),
            "{case}"
        );
        assert!(
            result.stats.rounds_until_all_knew.unwrap() <= diameter(g) * per_simulated_round,
            "{case}"
        );

        // Visits: in every simulated round, each agent visits all its
        // neighbours once per 1 bit of its ID.
        let visits_per_simulated_round: u64 = starts
            .iter()
            .enumerate()
            .map(|(index, &home)| {
                let id = u32::try_from(index + 1).unwrap();
                u64::from(g.degree(home).unwrap()) * u64::from(id.count_ones())
            })
            .sum();
        assert_eq!(
            result.stats.visits,
            (n_u64 - 1) * visits_per_simulated_round,
            "{case}"
        );
        assert_eq!(
            result.stats.meetings + result.stats.visits_without_message,
            result.stats.visits,
            "{case}"
        );

        // Counters.
        assert_eq!(metrics.rounds, result.rounds, "{case}");
        assert_eq!(metrics.probe_rounds, result.rounds, "{case}");
        assert_eq!(metrics.movement_rounds, 0, "{case}");
        assert_eq!(metrics.macro_rounds, n_u64 - 1, "{case}");
        assert_eq!(metrics.port_probes, result.stats.visits, "{case}");
        assert_eq!(metrics.probe_out_traversals, result.stats.visits, "{case}");
        assert_eq!(metrics.probe_back_traversals, result.stats.visits, "{case}");
        assert_eq!(metrics.agent_moves, 2 * result.stats.visits, "{case}");
        assert_eq!(metrics.settlements, 0, "{case}");
        assert_eq!(
            metrics.peak_logical_memory_words,
            MEMORY_WORDS_PER_AGENT * n_u64,
            "{case}"
        );

        // Memory: O(log n) bits per agent. Every variable fits in the bits of
        // n or of 2Δ < 2n, so nine of them take at most 9 (b + 1) bits.
        assert_eq!(
            result.stats.memory_bits_per_agent,
            memory_bits_per_agent(n, delta),
            "{case}"
        );
        assert!(result.stats.memory_bits_per_agent <= 9 * (b + 1), "{case}");
    }
}

/// The problem page's warning: two visitors may stand on one node, so a
/// visitor takes a message only from the agent whose current bit is 0. That
/// agent must always be at home, and every meeting pairs a bit-1 visitor with a
/// bit-0 host.
#[test]
fn a_visit_never_meets_an_agent_away_from_home() {
    let mut checked = 0;
    for (case, g, starts) in dispersed_cases()
        .into_iter()
        .filter(|(_, g, _)| g.node_count() <= 16)
    {
        let (result, _, trace) = traced(&g, &starts);
        assert_elected(&result, &case);
        let n = g.node_count();
        let b = id_bits(n);
        let block_length = 2 * u64::from(max_degree(&g));
        let bit = |agent: u32, round: u64| {
            let block = u32::try_from((round - 1) / block_length).unwrap() % b;
            ((agent + 1) >> (b - 1 - block)) & 1 == 1
        };
        for (round, visitor, host, node) in exchanges(&trace) {
            // The state after the out round, before the visitor looks. From a
            // dispersed start, checkpoint `r` is the state after round `r`.
            let before = &trace.checkpoints[usize::try_from(round - 1).unwrap()];
            assert_eq!(before.step, round - 1, "{case}");
            let (v, h) = (visitor as usize, host as usize);
            assert_eq!(before.agent_nodes[h], NodeId(node), "{case}");
            assert_eq!(before.home_nodes[h], Some(NodeId(node)), "{case}");
            assert_eq!(before.agent_statuses[h], AgentStatus::Settled, "{case}");
            assert_eq!(before.agent_nodes[v], NodeId(node), "{case}");
            assert_eq!(
                before.agent_statuses[v],
                AgentStatus::SettledScout,
                "{case}"
            );
            assert!(bit(visitor, round), "{case}: visitor's bit is 1");
            assert!(!bit(host, round), "{case}: host's bit is 0");
            checked += 1;
        }
    }
    assert!(checked > 10_000, "only {checked} meetings checked");
}

/// Lemma 4: two neighbours have different IDs, so in every simulated round
/// one of them visits the other at home. Every edge carries its messages.
#[test]
fn every_edge_carries_a_message_in_every_simulated_round() {
    for (case, g, starts) in dispersed_cases()
        .into_iter()
        .filter(|(_, g, _)| g.node_count() <= 12)
    {
        let (_, _, trace) = traced(&g, &starts);
        let n = g.node_count();
        let per_simulated_round = u64::from(id_bits(n)) * 2 * u64::from(max_degree(&g));
        let mut edges = BTreeSet::new();
        for node in 0..n {
            for (_, edge) in g.ports(node_at(node)) {
                edges.insert((
                    node.min(edge.neighbor.index()),
                    node.max(edge.neighbor.index()),
                ));
            }
        }
        let homes: Vec<usize> = starts.iter().map(|node| node.index()).collect();
        let all = exchanges(&trace);
        for simulated in 0..u64::try_from(n - 1).unwrap() {
            let first = simulated * per_simulated_round + 1;
            let last = (simulated + 1) * per_simulated_round;
            let met: BTreeSet<(usize, usize)> = all
                .iter()
                .copied()
                .filter(|&(round, ..)| (first..=last).contains(&round))
                .map(|(_, visitor, host, _)| {
                    let (a, b) = (homes[visitor as usize], homes[host as usize]);
                    (a.min(b), a.max(b))
                })
                .collect();
            assert_eq!(met, edges, "{case}: simulated round {}", simulated + 1);
        }
    }
}

#[test]
fn tracing_does_not_change_the_result_or_the_metrics() {
    for (case, g, starts) in dispersed_cases()
        .into_iter()
        .filter(|(_, g, _)| g.node_count() <= 10)
    {
        let plain = run(&g, &starts);
        let (result, metrics, _) = traced(&g, &starts);
        assert_eq!(plain, (result, metrics), "{case}");
    }
}

/// The second mode: every agent on one node, `RootedDisp` first, then the
/// election from the homes it gives.
#[test]
fn the_rooted_mode_disperses_with_rooted_disp_then_elects() {
    let families = [
        GraphFamily::Path,
        GraphFamily::Star,
        GraphFamily::Complete,
        GraphFamily::Tree { branching: 3 },
        GraphFamily::Grid { columns: 4 },
        GraphFamily::RandomConnected { extra_edges: 10 },
    ];
    for family in &families {
        for nodes in [2, 3, 7, 16, 40] {
            for seed in 0..3 {
                // Some families have no graph at the smallest sizes.
                let Ok(g) = generate_graph(
                    &GraphSpec::new(family.clone(), nodes),
                    PortAssignment::Random,
                    seed,
                ) else {
                    assert!(nodes < 7, "{} with {nodes} nodes", family.label());
                    continue;
                };
                let case = format!("{} with {nodes} nodes, seed {seed}", family.label());
                let starts = rooted(nodes);
                let (result, metrics) = run(&g, &starts);
                assert_elected(&result, &case);
                assert_eq!(result.mode, StartMode::Rooted, "{case}");

                let (dispersal, dispersal_metrics, _) = ccm_algorithms::rooted_disp::simulate(
                    &g,
                    &starts,
                    u64::MAX,
                    ComplexityMetrics::default(),
                    NoTrace,
                )
                .unwrap();
                assert_eq!(result.stats.dispersion_rounds, dispersal.steps, "{case}");
                let election = schedule_rounds(nodes, max_degree(&g));
                assert_eq!(result.stats.election_rounds, election, "{case}");
                assert_eq!(result.rounds, dispersal.steps + election, "{case}");
                assert_eq!(metrics.rounds, result.rounds, "{case}");
                assert_eq!(metrics.settlements, u64::try_from(nodes).unwrap(), "{case}");
                assert_eq!(
                    metrics.agent_moves,
                    dispersal_metrics.agent_moves + 2 * result.stats.visits,
                    "{case}"
                );
                for (index, agent) in result.agents.iter().enumerate() {
                    assert_eq!(agent.home, dispersal.agents[index].home, "{case}");
                }
            }
        }
    }
}

#[test]
fn a_single_agent_is_its_own_leader_in_no_rounds() {
    let g = graph(&[], 1);
    let (result, metrics, trace) = traced(&g, &[NodeId(0)]);
    assert_elected(&result, "one node");
    assert_eq!(result.rounds, 0);
    assert_eq!(metrics.rounds, 0);
    assert_eq!(result.stats.rounds_until_all_knew, Some(0));
    assert!(trace.events.iter().any(|event| matches!(
        event.event,
        SimulationEvent::OutputDecided {
            output: AgentOutput::Leader {
                is_leader: true,
                ..
            },
            ..
        }
    )));
}

#[test]
fn the_round_limit_stops_the_run_without_outputs() {
    let (g, starts) = page_example();
    let (result, metrics, _) =
        simulate(&g, &starts, 10, ComplexityMetrics::default(), NoTrace).unwrap();
    assert_eq!(
        result.termination,
        Termination::RoundLimitReached { limit: 10 }
    );
    assert_eq!(result.rounds, 10);
    assert_eq!(metrics.rounds, 10);
    assert!(result.outputs().is_none());
    // Round 10 is a back round, so every visitor is home again.
    assert!(result
        .agents
        .iter()
        .all(|agent| agent.home == Some(agent.node)));

    // In rooted mode the limit counts RootedDisp's steps too.
    let g = graph(&[(0, 1), (1, 2), (2, 3)], 4);
    let (result, _, _) =
        simulate(&g, &rooted(4), 5, ComplexityMetrics::default(), NoTrace).unwrap();
    assert_eq!(
        result.termination,
        Termination::RoundLimitReached { limit: 5 }
    );
    assert_eq!(result.mode, StartMode::Rooted);
    assert_eq!(result.rounds, 5);
    assert_eq!(result.stats.election_rounds, 0);
    assert!(result.outputs().is_none());
}

#[test]
fn input_the_model_does_not_cover_is_rejected() {
    let path = graph(&[(0, 1), (1, 2)], 3);
    let reject = |g: &PortGraph, starts: &[NodeId]| {
        simulate(g, starts, u64::MAX, ComplexityMetrics::default(), NoTrace).unwrap_err()
    };
    assert_eq!(reject(&path, &[]), ElectionError::NoAgents);
    assert_eq!(
        reject(&path, &[NodeId(0), NodeId(1)]),
        ElectionError::AgentCountMismatch {
            agents: 2,
            nodes: 3
        }
    );
    assert_eq!(
        reject(&path, &[NodeId(0), NodeId(1), NodeId(5)]),
        ElectionError::InvalidStart(NodeId(5))
    );
    assert_eq!(
        reject(&path, &[NodeId(0), NodeId(0), NodeId(1)]),
        ElectionError::NeitherDispersedNorRooted
    );
    let split = graph(&[(0, 1), (2, 3)], 4);
    assert_eq!(
        reject(&split, &[NodeId(0), NodeId(1), NodeId(2), NodeId(3)]),
        ElectionError::Disconnected
    );
    assert_eq!(
        ElectionError::NeitherDispersedNorRooted.to_string(),
        "leader election starts dispersed (one agent per node) or rooted (all agents on one node)"
    );
}

fn request(placement: PlacementSpec, agents: usize) -> RunRequest {
    RunRequest {
        algorithm: Algorithm::LeaderElection,
        graph: GraphSpec::new(GraphFamily::Grid { columns: 4 }, 12),
        port_assignment: PortAssignment::Random,
        placement,
        agents,
        seed: 5,
        round_limit: None,
        scheduler: SchedulerModel::Synchronous,
        labels: Vec::new(),
    }
}

/// Through the runner: the per-agent output reaches `RunResult`, the default
/// limit is the exact schedule, and other starts are refused before the run.
#[test]
fn the_runner_carries_the_outputs_and_refuses_other_starts() {
    let dispersed = execute_one(&BuiltinRunner, request(PlacementSpec::Dispersed, 12)).unwrap();
    assert!(dispersed.completed());
    let outputs = dispersed
        .outputs
        .as_ref()
        .expect("leader election has outputs");
    assert_eq!(outputs.len(), 12);
    assert_eq!(
        outputs[0],
        AgentOutput::Leader {
            is_leader: true,
            leader: AgentId(0)
        }
    );
    assert!(outputs[1..].iter().all(|output| *output
        == AgentOutput::Leader {
            is_leader: false,
            leader: AgentId(0)
        }));
    let prepared = prepare_run(request(PlacementSpec::Dispersed, 12)).unwrap();
    assert_eq!(
        leader_election_round_limit(&prepared),
        dispersed.metrics.rounds,
        "with no limit set, the schedule fits exactly"
    );

    let rooted = execute_one(
        &BuiltinRunner,
        request(PlacementSpec::SingleNode { node: NodeId(3) }, 12),
    )
    .unwrap();
    assert!(rooted.completed());
    assert_eq!(rooted.outputs, dispersed.outputs);

    // The dispersion algorithms have no output.
    let mut other = request(PlacementSpec::SingleNode { node: NodeId(0) }, 12);
    other.algorithm = Algorithm::RootedDisp;
    assert_eq!(execute_one(&BuiltinRunner, other).unwrap().outputs, None);

    for (placement, agents) in [
        (PlacementSpec::Dispersed, 6),
        (
            PlacementSpec::Clustered {
                centers: vec![NodeId(0), NodeId(11)],
            },
            12,
        ),
    ] {
        let label = placement.label();
        assert_eq!(
            execute_one(&BuiltinRunner, request(placement, agents)),
            Err(RunError::RequiresDispersedOrRootedStart {
                algorithm: Algorithm::LeaderElection,
                placement: label,
                agents,
                nodes: 12,
            })
        );
    }
}

#[test]
fn memory_follows_the_table_in_the_write_up() {
    // n = 3, Δ = 2: 2+2+2+2+2 (ID, n, Δ, min, received) + 2 (simulated
    // round) + 1 (block) + 2 (round in block) + 1 (leader flag).
    assert_eq!(memory_bits_per_agent(3, 2), 16);
    // One agent: every field takes one bit.
    assert_eq!(memory_bits_per_agent(1, 0), 9);
    // n = 200, Δ = 199: b = 8.
    assert_eq!(id_bits(200), 8);
    assert_eq!(memory_bits_per_agent(200, 199), 4 * 8 + 8 + 8 + 3 + 9 + 1);
    // ⌊log₂ n⌋ + 1 bits fit every ID, also when n is a power of two.
    for n in 1..=1_024_usize {
        assert!(n < 1 << id_bits(n), "{n}");
        assert!(n >= 1 << (id_bits(n) - 1), "{n}");
    }
}
