//! `GeneralDisp` (Sudo et al., Section 4, Algorithms 2–5) tests: small cases
//! traced by hand from the pseudocode, two groups meeting on a path, and
//! properties of the paper (Lemmas 3 and 4, the slot discipline, dispersion)
//! over seeded graphs and multi-start placements.

mod common;

use std::collections::HashMap;

use ccm_algorithms::general_disp::{
    simulate, slot_of, GeneralError, GeneralResult, GeneralRole, Group, GroupEvent,
};
use ccm_core::{AgentId, ComplexityMetrics, NodeId, PortAssignment, PortGraph, Termination};
use ccm_experiments::{generate_graph, place_agents, GraphFamily, GraphSpec, PlacementSpec};
use ccm_trace::{FullTrace, NoTrace, Trace};
use common::graph;

fn run(g: &PortGraph, starts: &[NodeId]) -> Result<GeneralResult, GeneralError> {
    simulate(g, starts, 10_000_000, ComplexityMetrics::default(), NoTrace)
        .map(|(result, _, _)| result)
}

fn traced(g: &PortGraph, starts: &[NodeId]) -> (GeneralResult, Trace) {
    let (result, _, recorder) = simulate(
        g,
        starts,
        10_000_000,
        ComplexityMetrics::default(),
        FullTrace::new(),
    )
    .unwrap();
    (result, recorder.into_trace())
}

fn nodes(list: &[u32]) -> Vec<NodeId> {
    list.iter().copied().map(NodeId).collect()
}

fn path(n: u32) -> PortGraph {
    let edges: Vec<(u32, u32)> = (1..n).map(|v| (v - 1, v)).collect();
    graph(&edges, usize::try_from(n).unwrap())
}

/// Definition 1 at the end: every agent on its own node, none a zombie, and
/// every agent a settler on its home or a leader waiting alone.
fn assert_dispersed(result: &GeneralResult, label: &str) {
    assert_eq!(result.termination, Termination::Completed, "{label}");
    let mut seen = HashMap::new();
    for (index, agent) in result.agents.iter().enumerate() {
        if let Some(other) = seen.insert(agent.node, index) {
            panic!(
                "{label}: agents {other} and {index} share node {}",
                agent.node.0
            );
        }
        match agent.role {
            GeneralRole::Settler => assert_eq!(agent.home, Some(agent.node), "{label}"),
            GeneralRole::WaitingLeader => assert_eq!(agent.home, None, "{label}"),
            role => panic!("{label}: agent {index} ends as {role:?}"),
        }
        assert_eq!(
            agent.memory.help, None,
            "{label}: agent {index} still helps"
        );
    }
}

// ----- Hand traces -----

#[test]
fn an_already_dispersed_start_takes_no_step() {
    let result = run(&path(2), &nodes(&[0, 1])).unwrap();
    assert_dispersed(&result, "path of two");
    assert_eq!(result.steps, 0);
    assert!(result
        .agents
        .iter()
        .all(|agent| agent.role == GeneralRole::WaitingLeader));
}

#[test]
fn a_single_agent_takes_no_step() {
    let result = run(&path(4), &nodes(&[2])).unwrap();
    assert_dispersed(&result, "one agent");
    assert_eq!(result.steps, 0);
}

/// Path 0 – 1 – 2, both agents on node 0, traced from the pseudocode:
///
/// - step 0 (slot 1): agent 1 is stronger, so agent 0 becomes a zombie;
/// - step 1 (slot 2): node 0 is unsettled, so zombie 0 settles there;
/// - steps 5–7 (slots 6–8): leader 1 probes port 0, finds no settler of its
///   group at node 1, and sets `next = 0`, `checked = 0`;
/// - step 15 (slot 4 of unit 2): port 0 is checked and has no helper, so the
///   probe is over; step 16 (slot 5) sets `done`;
/// - step 22 (slot 11): leader 1 moves forward to node 1 and waits there.
///
/// After step 22 every agent is on its own node, so the run ends at 23 steps.
#[test]
fn two_agents_on_one_node_match_the_hand_trace() {
    let (result, _) = traced(&path(3), &nodes(&[0, 0]));
    assert_dispersed(&result, "two agents");
    assert_eq!(result.steps, 23);
    assert_eq!(result.agents[0].role, GeneralRole::Settler);
    assert_eq!(result.agents[0].node, NodeId(0));
    assert_eq!(result.agents[1].role, GeneralRole::WaitingLeader);
    assert_eq!(result.agents[1].node, NodeId(1));
    let leader = Group {
        level: 1,
        leader: AgentId(1),
    };
    assert_eq!(
        result.events,
        vec![
            GroupEvent::BecameZombie {
                step: 0,
                agent: AgentId(0),
                node: NodeId(0),
                stronger: leader,
            },
            GroupEvent::Settled {
                step: 1,
                agent: AgentId(0),
                node: NodeId(0),
                // A zombie carries the ID of the leader that beat it.
                group: leader,
            },
            GroupEvent::ForwardMove {
                step: 22,
                leader: AgentId(1),
                from: NodeId(0),
                to: NodeId(1),
            },
        ]
    );
    assert_eq!(result.stats.probe_passes, 1);
    assert_eq!(
        (result.stats.forward_moves, result.stats.backward_moves),
        (1, 0)
    );
}

/// The card's first exercise: two groups meet on a path.
///
/// Path 0 – … – 5. Agents 0–3 start on node 0, agents 4–5 on node 3.
///
/// - Group A: leader 3 settles zombie 0 at node 0 and meets zombie 1 of its own
///   level, so it rises to level 2 and zombie 1 drops to level 0. It settles
///   zombie 1 at node 1 and walks on with zombie 2.
/// - Group B: leader 5 settles zombie 4 at node 3 (group `(1, 5)`), probes,
///   and walks alone to node 2, where it waits.
/// - Group A reaches node 2, where B's leader waits. A (level 2) is stronger,
///   so leader 5 becomes a zombie and joins A. A settles zombie 2 there.
/// - A probes from node 2: node 3 holds settler 4 of the weaker group B, so it
///   counts as outside A's territory. A moves there and incorporates settler
///   4 into its own group, then settles zombie 5 at node 4 and walks alone to
///   node 5, where it waits.
#[test]
fn two_groups_meet_on_a_path() {
    use GeneralRole::{Settler, WaitingLeader};
    let (result, _) = traced(&path(6), &nodes(&[0, 0, 0, 0, 3, 3]));
    assert_dispersed(&result, "two groups");
    let finals: Vec<(u32, GeneralRole)> = result
        .agents
        .iter()
        .map(|agent| (agent.node.0, agent.role))
        .collect();
    assert_eq!(
        finals,
        vec![
            (0, Settler),
            (1, Settler),
            (2, Settler),
            (5, WaitingLeader),
            (3, Settler),
            (4, Settler),
        ]
    );
    let a = Group {
        level: 2,
        leader: AgentId(3),
    };
    let became_zombie: Vec<(AgentId, NodeId, Group)> = result
        .events
        .iter()
        .filter_map(|event| match *event {
            GroupEvent::BecameZombie {
                agent,
                node,
                stronger,
                ..
            } => Some((agent, node, stronger)),
            _ => None,
        })
        .collect();
    // At the start, the weaker agents on each start node; then leader 5 at
    // node 2, beaten by group A.
    assert_eq!(
        became_zombie.last(),
        Some(&(AgentId(5), NodeId(2), a)),
        "{became_zombie:?}"
    );
    assert!(!became_zombie
        .iter()
        .any(|&(agent, _, _)| agent == AgentId(3)));
    let incorporated: Vec<(AgentId, NodeId, Group, Group)> = result
        .events
        .iter()
        .filter_map(|event| match *event {
            GroupEvent::Incorporated {
                settler,
                node,
                from,
                to,
                ..
            } => Some((settler, node, from, to)),
            _ => None,
        })
        .collect();
    assert_eq!(
        incorporated,
        vec![(
            AgentId(4),
            NodeId(3),
            Group {
                level: 1,
                leader: AgentId(5),
            },
            a
        )]
    );
    // Settler 4 ends in group A.
    assert_eq!(result.agents[4].memory.group(), a);
    assert_eq!(result.stats.level_ups, 1);
}

// ----- Properties over a sweep -----

fn families() -> Vec<(&'static str, GraphFamily)> {
    vec![
        ("path", GraphFamily::Path),
        ("cycle", GraphFamily::Cycle),
        ("star", GraphFamily::Star),
        ("complete", GraphFamily::Complete),
        ("tree:2", GraphFamily::Tree { branching: 2 }),
        ("tree:4", GraphFamily::Tree { branching: 4 }),
        ("grid:5", GraphFamily::Grid { columns: 5 }),
        ("random+0", GraphFamily::RandomConnected { extra_edges: 0 }),
        (
            "random+10",
            GraphFamily::RandomConnected { extra_edges: 10 },
        ),
        (
            "random+60",
            GraphFamily::RandomConnected { extra_edges: 60 },
        ),
        (
            "bounded:4",
            GraphFamily::RandomBoundedDegree { max_degree: 4 },
        ),
    ]
}

fn placements(n: usize) -> Vec<(String, PlacementSpec)> {
    let mut list = vec![
        (
            "single".to_string(),
            PlacementSpec::SingleNode { node: NodeId(0) },
        ),
        (
            "uniform:2".to_string(),
            PlacementSpec::UniformRandom { distinct_nodes: 2 },
        ),
        (
            "uniform:4".to_string(),
            PlacementSpec::UniformRandom { distinct_nodes: 4 },
        ),
        (
            "uniform:k".to_string(),
            PlacementSpec::UniformRandom { distinct_nodes: n },
        ),
    ];
    let far_end = NodeId(u32::try_from(n - 1).unwrap());
    let middle = NodeId(u32::try_from(n / 2).unwrap());
    list.push((
        "clustered:ends+middle".to_string(),
        PlacementSpec::Clustered {
            centers: vec![NodeId(0), middle, far_end],
        },
    ));
    list
}

fn max_degree(g: &PortGraph) -> usize {
    (0..g.node_count())
        .map(|v| usize::from(g.degree(NodeId(u32::try_from(v).unwrap())).unwrap()))
        .max()
        .unwrap_or(0)
}

#[derive(Default, Debug)]
struct Totals {
    runs: usize,
    zombifications: u64,
    incorporations: u64,
    level_ups: u64,
    weak_moves: u64,
    strong_moves: u64,
    backward: u64,
    worst_ratio: f64,
}

/// Every property checked on one run from its trace.
fn check_run(g: &PortGraph, starts: &[NodeId], label: &str, totals: &mut Totals) {
    let (result, trace) = traced(g, starts);
    assert_dispersed(&result, label);
    let k = starts.len();

    // Lemma 3: a level never exceeds log2 k + 1.
    let bound = (usize::BITS - k.leading_zeros()) - 1 + 1; // floor(log2 k) + 1
    assert!(
        result.stats.max_level <= bound,
        "{label}: level {} exceeds log2 k + 1 = {bound}",
        result.stats.max_level
    );
    assert_eq!(
        result.stats.stuck_chases, 0,
        "{label}: a zombie had no port"
    );

    // Slot 9 moves only weak zombies; slot 10 moves both kinds.
    for event in &result.events {
        if let GroupEvent::Chased {
            step, weak, zombie, ..
        } = *event
        {
            let slot = slot_of(step);
            assert!(slot == 9 || slot == 10, "{label}: chase in slot {slot}");
            assert!(
                slot == 10 || weak,
                "{label}: strong zombie {} moved in slot 9",
                zombie.0
            );
        }
    }

    // Lemma 4: a zombie's location level never decreases.
    let mut location: HashMap<AgentId, u32> = HashMap::new();
    for event in &result.events {
        if let GroupEvent::ZombieLevels {
            zombie,
            location: level,
            step,
            ..
        } = *event
        {
            if let Some(&before) = location.get(&zombie) {
                assert!(
                    level >= before,
                    "{label}: zombie {} location level fell {before} -> {level} at step {step}",
                    zombie.0
                );
            }
            location.insert(zombie, level);
        }
    }

    // Settlers are home in slots 9–12 and 1–2 (Section 4.2), read from the
    // trace: the checkpoint after step t is the state before step t + 1.
    for checkpoint in &trace.checkpoints {
        let slot_ahead = slot_of(checkpoint.step);
        if matches!(slot_ahead, 9..=12 | 1..=3) {
            for (agent, home) in checkpoint.home_nodes.iter().enumerate() {
                if let Some(home) = home {
                    assert_eq!(
                        checkpoint.agent_nodes[agent], *home,
                        "{label}: settler {agent} away before slot {slot_ahead}"
                    );
                }
            }
        }
    }

    // Theorem 2's shape: steps against k log k log τ, recorded for the report.
    let tau = k.min(max_degree(g)).max(2);
    let k_log = f64::from(u32::try_from(k.max(2)).unwrap());
    let scale = k_log * k_log.log2() * f64::from(u32::try_from(tau).unwrap()).log2();
    let ratio = f64::from(u32::try_from(result.steps).unwrap()) / scale;
    totals.worst_ratio = totals.worst_ratio.max(ratio);

    totals.runs += 1;
    totals.zombifications += result.stats.zombifications;
    totals.incorporations += result.stats.incorporations;
    totals.level_ups += result.stats.level_ups;
    totals.weak_moves += result.stats.weak_chase_moves;
    totals.strong_moves += result.stats.strong_chase_moves;
    totals.backward += result.stats.backward_moves;
}

#[test]
fn every_run_disperses_and_keeps_the_papers_invariants() {
    let ports = [
        PortAssignment::Canonical,
        PortAssignment::Random,
        PortAssignment::Adversarial,
    ];
    let mut totals = Totals::default();
    for (family_name, family) in families() {
        for n in [3, 6, 12, 25] {
            for assignment in ports {
                for seed in 0..2_u64 {
                    let Ok(g) =
                        generate_graph(&GraphSpec::new(family.clone(), n), assignment, seed)
                    else {
                        continue;
                    };
                    for (placement_name, placement) in placements(n) {
                        for k in [2, n / 2, n] {
                            let Ok(starts) = place_agents(n, k, &placement, seed) else {
                                continue;
                            };
                            let label = format!(
                                "{family_name}, n={n}, {assignment:?}, seed {seed}, {placement_name}, k={k}"
                            );
                            check_run(&g, &starts, &label, &mut totals);
                        }
                    }
                }
            }
        }
    }
    println!("{totals:?}");
    // The sweep must reach the parts of the algorithm it claims to test.
    assert!(totals.runs > 1_000, "{totals:?}");
    assert!(totals.zombifications > 0, "{totals:?}");
    assert!(totals.incorporations > 0, "{totals:?}");
    assert!(totals.level_ups > 0, "{totals:?}");
    assert!(totals.strong_moves > 0, "{totals:?}");
    assert!(totals.backward > 0, "{totals:?}");
}

#[test]
fn larger_multi_start_runs_disperse() {
    let mut totals = Totals::default();
    for seed in 0..12_u64 {
        let n = 80 + usize::try_from(seed).unwrap() * 10;
        let g = generate_graph(
            &GraphSpec::new(GraphFamily::RandomConnected { extra_edges: 2 * n }, n),
            PortAssignment::Random,
            seed,
        )
        .unwrap();
        for distinct in [3, 10, n / 4] {
            let starts = place_agents(
                n,
                n,
                &PlacementSpec::UniformRandom {
                    distinct_nodes: distinct,
                },
                seed,
            )
            .unwrap();
            check_run(
                &g,
                &starts,
                &format!("large seed {seed}, n={n}, {distinct} starts"),
                &mut totals,
            );
        }
    }
    println!("{totals:?}");
}

// ----- Inputs -----

#[test]
fn inputs_outside_the_model_are_rejected() {
    let g = path(3);
    assert_eq!(run(&g, &[]).unwrap_err(), GeneralError::NoAgents);
    assert_eq!(
        run(&g, &nodes(&[0, 0, 0, 0])).unwrap_err(),
        GeneralError::TooManyAgents {
            agents: 4,
            nodes: 3
        }
    );
    assert_eq!(
        run(&g, &nodes(&[7])).unwrap_err(),
        GeneralError::InvalidStart(NodeId(7))
    );
}

#[test]
fn the_round_limit_stops_the_run_and_reports_it() {
    let g = path(8);
    let (result, _, _) = simulate(
        &g,
        &nodes(&[0, 0, 0, 0]),
        30,
        ComplexityMetrics::default(),
        NoTrace,
    )
    .unwrap();
    assert_eq!(
        result.termination,
        Termination::RoundLimitReached { limit: 30 }
    );
    assert_eq!(result.steps, 30);
}

#[test]
fn every_step_is_one_round_in_the_metrics() {
    let g = path(6);
    let (result, metrics, _) = simulate(
        &g,
        &nodes(&[0, 0, 0, 0, 3, 3]),
        1_000_000,
        ComplexityMetrics::default(),
        NoTrace,
    )
    .unwrap();
    assert_eq!(metrics.rounds, result.steps);
    assert_eq!(metrics.macro_rounds, result.stats.units);
    // Without a recorder, no events are kept.
    assert!(result.events.is_empty());
}
