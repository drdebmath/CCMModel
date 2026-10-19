//! `GeneralDisp` tests: hand traces, collisions, probing rules and the
//! paper's properties over seeded multi-start sweeps.

mod common;

use std::collections::HashMap;

use ccm_algorithms::general_disp::{
    simulate, slot_of, GeneralError, GeneralResult, GeneralRole, Group, GroupEvent, ShownRole,
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

/// Definition 1: every agent alone on its node, as a settler or a waiting leader.
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

/// Path 0 – 1 – 2 with both agents on node 0, traced by hand: 23 steps.
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

/// Two groups meet on a path (see `docs/general-disp.md` for the trace).
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
    assert_eq!(result.agents[4].memory.group(), a);
    assert_eq!(result.stats.level_ups, 1);
}

/// The role log replays to the final roles and is empty without a recorder.
#[test]
fn the_role_log_replays_to_the_final_roles() {
    let starts = nodes(&[0, 0, 0, 0, 3, 3]);
    let (result, trace) = traced(&path(6), &starts);
    let mut shown = vec![None; result.agents.len()];
    for change in &result.roles {
        shown[change.agent.index()] = Some((change.role, change.group));
    }
    for (index, agent) in result.agents.iter().enumerate() {
        let role = match agent.role {
            GeneralRole::Settler => ShownRole::Settler,
            GeneralRole::WaitingLeader => ShownRole::WaitingLeader,
            other => panic!("agent {index} ends as {other:?}"),
        };
        assert_eq!(shown[index], Some((role, agent.memory.group())));
    }
    assert!(result.roles.windows(2).all(|p| p[0].step <= p[1].step));
    assert!(result
        .roles
        .iter()
        .all(|change| trace.checkpoints.iter().any(|c| c.step == change.step)));
    assert!(result
        .roles
        .iter()
        .any(|change| change.role == ShownRole::StrongZombie));
    assert!(run(&path(6), &starts).unwrap().roles.is_empty());
}

fn label(level: u32, leader: u32) -> Group {
    Group {
        level,
        leader: AgentId(leader),
    }
}

fn generated(family: GraphFamily, n: usize) -> PortGraph {
    generate_graph(&GraphSpec::new(family, n), PortAssignment::Canonical, 0).unwrap()
}

/// Three groups on a cycle: a higher level wins, equal levels go to the larger
/// ID, weak zombies chase in slot 9 and strong ones only in slot 10.
#[test]
fn three_groups_collide_and_zombies_chase_by_strength() {
    let g = generated(GraphFamily::Cycle, 8);
    let (result, _) = traced(&g, &nodes(&[1, 5, 5, 4, 4, 1, 4, 1]));
    assert_dispersed(&result, "three groups");
    let zombies: Vec<(u64, AgentId, NodeId, Group)> = result
        .events
        .iter()
        .filter_map(|event| match *event {
            GroupEvent::BecameZombie {
                step,
                agent,
                node,
                stronger,
            } if step > 0 => Some((step, agent, node, stronger)),
            _ => None,
        })
        .collect();
    assert_eq!(
        zombies,
        vec![
            (24, AgentId(2), NodeId(4), label(2, 6)),
            (72, AgentId(6), NodeId(1), label(2, 7)),
        ]
    );
    let chases: Vec<(u64, u8, AgentId, NodeId, bool)> = result
        .events
        .iter()
        .filter_map(|event| match *event {
            GroupEvent::Chased {
                step,
                zombie,
                to,
                weak,
                ..
            } => Some((step, slot_of(step), zombie, to, weak)),
            _ => None,
        })
        .collect();
    assert_eq!(
        chases,
        vec![
            (32, 9, AgentId(2), NodeId(3), true),
            (81, 10, AgentId(6), NodeId(0), false),
            (93, 10, AgentId(6), NodeId(7), false),
        ]
    );
    assert!(result.events.contains(&GroupEvent::ZombieLevels {
        step: 32,
        zombie: AgentId(2),
        location: 2,
        swarm: 1,
    }));
    assert!(result.events.contains(&GroupEvent::Settled {
        step: 97,
        agent: AgentId(6),
        node: NodeId(7),
        group: label(2, 7),
    }));
    assert_eq!(
        (
            result.stats.weak_chase_moves,
            result.stats.strong_chase_moves
        ),
        (1, 2)
    );
}

/// Unlike `RootedDisp`, only a settler of the leader's own group marks a node
/// visited.
#[test]
fn a_probe_counts_only_settlers_of_the_leaders_group() {
    let (result, _) = traced(&path(6), &nodes(&[0, 0, 0, 0, 3, 3]));
    let at = |wanted: &GroupEvent| {
        result
            .events
            .iter()
            .position(|event| event == wanted)
            .unwrap_or_else(|| panic!("missing {wanted:?}"))
    };
    let into_b = at(&GroupEvent::ForwardMove {
        step: 70,
        leader: AgentId(3),
        from: NodeId(2),
        to: NodeId(3),
    });
    let taken = at(&GroupEvent::Incorporated {
        step: 73,
        settler: AgentId(4),
        node: NodeId(3),
        from: label(1, 5),
        to: label(2, 3),
    });
    assert!(into_b < taken);
}

/// Helpers walk over through `help`; after a level-up, the group's older
/// helpers count as weaker and are sent home.
#[test]
fn helpers_follow_help_and_weaker_ones_are_dismissed() {
    let (result, trace) = traced(&path(4), &nodes(&[0, 1, 1, 0]));
    assert_dispersed(&result, "dismissal");
    assert!(result.events.contains(&GroupEvent::LevelUp {
        step: 37,
        leader: AgentId(3),
        zombie: AgentId(2),
        node: NodeId(1),
        level: 2,
    }));
    let position = |after_step: u64| {
        trace
            .checkpoints
            .iter()
            .find(|checkpoint| checkpoint.step == after_step)
            .map(|checkpoint| checkpoint.agent_nodes[0])
            .unwrap()
    };
    assert_eq!(position(38), NodeId(0), "home before slot 3");
    assert_eq!(position(39), NodeId(1), "at the centre after slot 3");
    assert_eq!(position(40), NodeId(0), "dismissed in slot 4");
    assert_eq!(result.stats.helpers_dismissed, 1);
    assert_eq!(result.stats.stolen_helpers, 0);
    assert!(result.events.contains(&GroupEvent::Incorporated {
        step: 61,
        settler: AgentId(0),
        node: NodeId(0),
        from: label(1, 3),
        to: label(2, 3),
    }));
}

/// A recruited helper taken over by a stronger group marks its port as
/// leading out of the territory.
#[test]
fn a_helper_taken_by_a_stronger_group_is_detected_as_stolen() {
    let g = generated(GraphFamily::Tree { branching: 2 }, 10);
    let (result, _) = traced(&g, &nodes(&[7, 7, 1, 4, 7, 4, 7, 1, 7]));
    assert_dispersed(&result, "stolen helper");
    assert_eq!(result.stats.stolen_helpers, 1);
    let order: Vec<&GroupEvent> = result
        .events
        .iter()
        .filter(|event| {
            matches!(
                event,
                GroupEvent::Incorporated { step: 49, .. }
                    | GroupEvent::ForwardMove { step: 58, .. }
                    | GroupEvent::BecameZombie { step: 60, .. }
            )
        })
        .collect();
    assert_eq!(
        order,
        vec![
            &GroupEvent::Incorporated {
                step: 49,
                settler: AgentId(2),
                node: NodeId(1),
                from: label(1, 7),
                to: label(2, 8),
            },
            &GroupEvent::ForwardMove {
                step: 58,
                leader: AgentId(7),
                from: NodeId(0),
                to: NodeId(1),
            },
            &GroupEvent::BecameZombie {
                step: 60,
                agent: AgentId(7),
                node: NodeId(1),
                stronger: label(2, 8),
            },
        ]
    );
}

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

    let bound = (usize::BITS - k.leading_zeros()) - 1 + 1;
    assert!(
        result.stats.max_level <= bound,
        "{label}: level {} exceeds log2 k + 1 = {bound}",
        result.stats.max_level
    );
    assert_eq!(
        result.stats.stuck_chases, 0,
        "{label}: a zombie had no port"
    );

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
    assert!(result.events.is_empty());
}
