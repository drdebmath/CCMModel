//! Rendezvous of two labelled agents in a tree (Dessmark et al. 2006): the
//! hand traces of `docs/rendezvous.md`, the meeting guarantee over every label
//! pair up to 64 and every tree family up to 200 nodes, the exact round limit,
//! and the rule that crossing inside an edge is not a meeting.

mod common;

use ccm_algorithms::rendezvous::{
    block_len, round_bound, simulate, string, string_len, string_text, symbol, Activity,
    RendezvousError, RendezvousResult, Stage, Walker,
};
use ccm_core::{
    ComplexityMetrics, DeterministicRng, NodeId, PortAssignment, PortGraph, PortId, Termination,
};
use ccm_experiments::{generate_graph, GraphFamily, GraphSpec};
use ccm_trace::{FullTrace, NoTrace, Trace};
use common::graph;

fn run(g: &PortGraph, starts: [u32; 2], labels: [u32; 2]) -> RendezvousResult {
    let starts = starts.map(NodeId);
    let limit = 2 * round_bound(g.node_count(), &labels);
    simulate(
        g,
        &starts,
        &labels,
        limit,
        ComplexityMetrics::default(),
        NoTrace,
    )
    .unwrap()
    .0
}

fn traced(g: &PortGraph, starts: [u32; 2], labels: [u32; 2]) -> (RendezvousResult, Trace) {
    let starts = starts.map(NodeId);
    let limit = 2 * round_bound(g.node_count(), &labels);
    let (result, _, recorder) = simulate(
        g,
        &starts,
        &labels,
        limit,
        ComplexityMetrics::default(),
        FullTrace::new(),
    )
    .unwrap();
    (result, recorder.into_trace())
}

/// Each agent's node at the end of every round, from round 1.
fn walks(trace: &Trace) -> [Vec<u32>; 2] {
    let mut walks = [Vec::new(), Vec::new()];
    for checkpoint in trace.checkpoints.iter().filter(|c| c.step > 0) {
        for (agent, walk) in walks.iter_mut().enumerate() {
            walk.push(checkpoint.agent_nodes[agent].0);
        }
    }
    walks
}

/// The hand trace's tree: 0 - 1 - 2, 1 - 3, 3 - 4, 3 - 5, canonical ports.
fn six() -> PortGraph {
    graph(&[(0, 1), (1, 2), (1, 3), (3, 4), (3, 5)], 6)
}

fn tree(family: &GraphFamily, nodes: usize, ports: PortAssignment, seed: u64) -> PortGraph {
    generate_graph(&GraphSpec::new(family.clone(), nodes), ports, seed).unwrap()
}

const TREES: [GraphFamily; 5] = [
    GraphFamily::Path,
    GraphFamily::Star,
    GraphFamily::Tree { branching: 2 },
    GraphFamily::Tree { branching: 3 },
    GraphFamily::RandomTree,
];

const PORTS: [PortAssignment; 3] = [
    PortAssignment::Canonical,
    PortAssignment::Random,
    PortAssignment::Adversarial,
];

#[test]
fn strings_follow_the_task_examples() {
    assert_eq!(string_text(5), "11 00 11 01");
    assert_eq!(string_text(1), "11 01");
    assert_eq!(string_text(3), "11 11 01");
    assert_eq!(string_text(2), "11 00 01");
    for label in 1..=1000 {
        assert_eq!(string(label).len(), string_len(label) as usize);
        assert_eq!(symbol(label, string_len(label)), None);
    }
}

/// Step 4 of the algorithm: for two different labels there is a position,
/// before the shorter string ends, where one string has 1 and the other 0.
#[test]
fn two_strings_disagree_before_the_shorter_one_ends() {
    for a in 1..=64_u32 {
        for b in (a + 1)..=64 {
            let (x, y) = (string(a), string(b));
            let shorter = x.len().min(y.len());
            assert!(
                (0..shorter).any(|i| x[i] != y[i]),
                "{a} and {b} agree on the first {shorter} symbols"
            );
        }
    }
}

/// The first tour on the hand trace's tree, round by round, from each start.
#[test]
fn the_first_tour_is_the_hand_traced_depth_first_walk() {
    let (result, trace) = traced(&six(), [0, 5], [1, 3]);
    assert_eq!(result.block_len, 10);
    let [a, b] = walks(&trace);
    assert_eq!(a[..10], [1, 2, 1, 3, 4, 3, 5, 3, 1, 0]);
    assert_eq!(b[..10], [3, 1, 0, 1, 2, 1, 3, 4, 3, 5]);
    // Blocks 1 and 2 have symbol 1 for both, so both repeat the tour.
    assert_eq!(a[10..20], a[..10]);
    assert_eq!(b[20..30], b[..10]);
}

/// Labels 1 and 3: `11 01` and `11 11 01`. Symbol 3 (block 3) is 0 for label 1
/// and 1 for label 3, so label 1 waits on node 0 while label 3's tour passes
/// through it in the block's third round.
#[test]
fn labels_one_and_three_meet_in_block_three() {
    let (result, trace) = traced(&six(), [0, 5], [1, 3]);
    let meeting = result.meeting.expect("they meet");
    assert_eq!(result.termination, Termination::Completed);
    assert_eq!(meeting.block, 3);
    assert_eq!(meeting.round, 33);
    assert_eq!(meeting.node, NodeId(0));
    assert_eq!(result.rounds, 33);
    assert_eq!(result.bound, 50);
    let [a, b] = walks(&trace);
    assert_eq!(a[30..33], [0, 0, 0], "label 1 waits at its start");
    assert_eq!(b[30..33], [3, 1, 0], "label 3 explores");
    // Nowhere earlier on one node at the end of a round.
    assert!((0..32).all(|round| a[round] != b[round]));
}

/// On a single edge the agents cross each other inside it in every round of a
/// joint tour. That is not a meeting; they meet when one waits and the other
/// comes to it.
#[test]
fn crossing_inside_an_edge_is_not_a_meeting() {
    let g = graph(&[(0, 1)], 2);
    let (result, trace) = traced(&g, [0, 1], [1, 2]);
    let [a, b] = walks(&trace);
    // Blocks 0-2 (rounds 1-6): both explore, swapping ends each odd round.
    assert_eq!(a[..6], [1, 0, 1, 0, 1, 0]);
    assert_eq!(b[..6], [0, 1, 0, 1, 0, 1]);
    // Block 3 (rounds 7-8): both strings have 0, both wait. Block 4: label 1
    // has 1 and label 2 has 0, so label 1 walks onto node 1.
    let meeting = result.meeting.unwrap();
    assert_eq!(
        (meeting.round, meeting.node, meeting.block),
        (9, NodeId(1), 4)
    );
    assert!(result.rounds <= result.bound);
}

/// The main guarantee: every pair of labels up to 64 meets, within the exact
/// limit, from several pairs of starts on several trees.
#[test]
fn every_label_pair_up_to_sixty_four_meets_within_the_limit() {
    let trees = [
        six(),
        tree(&GraphFamily::RandomTree, 9, PortAssignment::Random, 3),
        tree(&GraphFamily::Star, 7, PortAssignment::Adversarial, 0),
    ];
    for g in &trees {
        let n = u32::try_from(g.node_count()).unwrap();
        for starts in [[0, n - 1], [n - 1, 1], [2, 0]] {
            for a in 1..=64 {
                for b in 1..=64 {
                    if a == b {
                        continue;
                    }
                    let result = run(g, starts, [a, b]);
                    let case = format!("labels {a},{b} from {starts:?} on {n} nodes");
                    assert!(result.meeting.is_some(), "{case}: no meeting");
                    assert!(
                        result.rounds <= result.bound,
                        "{case}: {} rounds",
                        result.rounds
                    );
                }
            }
        }
    }
}

/// Every tree family and size up to 200, every port assignment, many seeds
/// and start pairs, with sampled label pairs.
#[test]
fn every_tree_family_and_size_meets_within_the_limit() {
    let mut rng = DeterministicRng::new(2006);
    for family in &TREES {
        for nodes in [2, 3, 4, 5, 8, 13, 30, 64, 120, 200] {
            for ports in PORTS {
                for seed in 0..4 {
                    let shape = tree(family, nodes, ports, seed);
                    for _ in 0..3 {
                        let first = rng.index(nodes).unwrap();
                        let mut second = rng.index(nodes - 1).unwrap();
                        if second >= first {
                            second += 1;
                        }
                        let one = u32::try_from(rng.index(64).unwrap()).unwrap() + 1;
                        let mut two = u32::try_from(rng.index(63).unwrap()).unwrap() + 1;
                        if two >= one {
                            two += 1;
                        }
                        let starts = [first, second].map(|v| u32::try_from(v).unwrap());
                        let result = run(&shape, starts, [one, two]);
                        let case = format!(
                            "{} n={nodes} {ports:?} seed {seed}, labels {one},{two} from {starts:?}",
                            family.label()
                        );
                        let meeting = result.meeting.unwrap_or_else(|| panic!("{case}"));
                        assert_eq!(result.termination, Termination::Completed, "{case}");
                        // The exact limit: 2(n - 1) rounds of the first tour
                        // plus 2(n - 1) per symbol of the shorter string.
                        let shorter = u64::from(string_len(one).min(string_len(two)));
                        let limit = block_len(nodes) * (1 + shorter);
                        assert_eq!(result.bound, limit);
                        assert!(meeting.round <= limit, "{case}: round {}", meeting.round);
                        // Both agents really stand on the meeting node.
                        assert!(result.agents.iter().all(|agent| agent.node == meeting.node));
                    }
                }
            }
        }
    }
}

/// The trace agrees with the result: the meeting is the first round that ends
/// with both agents on one node, and between blocks each agent is back home.
#[test]
fn the_meeting_is_the_first_shared_node_and_blocks_end_at_home() {
    for seed in 0..10 {
        let g = tree(&GraphFamily::RandomTree, 15, PortAssignment::Random, seed);
        let (result, trace) = traced(&g, [3, 11], [6, 41]);
        let meeting = result.meeting.unwrap();
        let [a, b] = walks(&trace);
        let first = (0..a.len()).find(|&r| a[r] == b[r]).unwrap();
        assert_eq!(u64::try_from(first).unwrap() + 1, meeting.round);
        let block = usize::try_from(result.block_len).unwrap();
        for end in (block..a.len()).step_by(block) {
            assert_eq!(a[end - 1], 3, "seed {seed}: agent 0 away at round {end}");
            assert_eq!(b[end - 1], 11, "seed {seed}: agent 1 away at round {end}");
        }
    }
}

/// The schedule depends only on the label: a waiting agent never moves and
/// an exploring one moves every round.
#[test]
fn agents_move_exactly_in_exploring_blocks() {
    let g = tree(&GraphFamily::RandomTree, 12, PortAssignment::Random, 5);
    let (result, trace) = traced(&g, [0, 7], [10, 37]);
    let block = usize::try_from(result.block_len).unwrap();
    let [a, b] = walks(&trace);
    for (walk, label, start) in [(&a, 10, 0), (&b, 37, 7)] {
        let mut previous = start;
        for (round, &node) in walk.iter().enumerate() {
            let b = u32::try_from(round / block).unwrap();
            let exploring = b == 0 || symbol(label, b - 1) == Some(true);
            assert_eq!(
                node != previous,
                exploring,
                "label {label}, round {}",
                round + 1
            );
            previous = node;
        }
    }
}

#[test]
fn the_walker_learns_the_block_length_and_drops_its_stack() {
    // A path walked from one end: the stack grows to n - 1, then empties.
    let mut walker = Walker::new(4);
    let g = graph(&[(0, 1), (1, 2), (2, 3)], 4);
    let mut node = NodeId(0);
    while walker.stage == Stage::Tour {
        let degree = g.degree(node).unwrap();
        let port = walker.next_port(degree).unwrap();
        let edge = g.traverse(node, port).unwrap();
        node = edge.neighbor;
        walker.end_round(Some(port), Some(edge.remote_port), g.degree(node).unwrap());
    }
    assert_eq!(walker.block_len, Some(6));
    assert_eq!(walker.peak_depth, 3);
    assert!(walker.up_ports.is_empty());
    assert_eq!(node, NodeId(0));
    // 4 = 100: the string is 11 00 00 01, so the first block explores.
    assert_eq!(walker.stage, Stage::Symbol { index: 0 });
    assert_eq!(walker.activity(), Activity::Exploring);
    assert_eq!(walker.next_port(1), Some(PortId(0)));
}

/// Memory: a constant number of counters of O(log n + log L) bits, plus the
/// first tour's stack of at most n - 1 ports.
#[test]
fn memory_is_logarithmic_outside_the_first_tours_stack() {
    for nodes in [2, 10, 50, 200] {
        let g = tree(&GraphFamily::Path, nodes, PortAssignment::Canonical, 0);
        let last = u32::try_from(nodes - 1).unwrap();
        let result = run(&g, [0, last], [7, 64]);
        let log = |x: u64| u64::from(u64::BITS - x.leading_zeros());
        for agent in &result.agents {
            assert!(agent.walker.peak_depth < nodes);
            assert!(agent.walker.up_ports.is_empty() || agent.walker.stage == Stage::Tour);
            // Label, symbol index, two counters, a port, and the stack, each
            // port one bit on a path (degree at most 2).
            let fixed = log(64) + log(14) + 2 * log(block_len(nodes)) + 1;
            assert!(agent.memory_bits <= fixed + u64::try_from(nodes).unwrap());
        }
    }
}

#[test]
fn inputs_outside_the_model_are_rejected() {
    let g = six();
    let reject = |g: &PortGraph, starts: &[u32], labels: &[u32]| {
        let starts: Vec<NodeId> = starts.iter().copied().map(NodeId).collect();
        simulate(
            g,
            &starts,
            labels,
            100,
            ComplexityMetrics::default(),
            NoTrace,
        )
        .unwrap_err()
    };
    assert_eq!(reject(&g, &[0], &[1]), RendezvousError::AgentCount(1));
    assert_eq!(
        reject(&g, &[0, 1, 2], &[1, 2, 3]),
        RendezvousError::AgentCount(3)
    );
    assert_eq!(
        reject(&g, &[0, 0], &[1, 2]),
        RendezvousError::SameStart(NodeId(0))
    );
    assert_eq!(
        reject(&g, &[0, 1], &[4, 4]),
        RendezvousError::DuplicateLabel(4)
    );
    assert_eq!(reject(&g, &[0, 1], &[0, 4]), RendezvousError::ZeroLabel);
    assert!(matches!(
        reject(&g, &[0, 1], &[1]),
        RendezvousError::LabelCount { .. }
    ));
    assert_eq!(
        reject(&g, &[0, 9], &[1, 2]),
        RendezvousError::InvalidStart(NodeId(9))
    );
    let cycle = graph(&[(0, 1), (1, 2), (2, 0)], 3);
    assert_eq!(reject(&cycle, &[0, 1], &[1, 2]), RendezvousError::NotATree);
    let forest = graph(&[(0, 1), (2, 3)], 4);
    assert_eq!(reject(&forest, &[0, 1], &[1, 2]), RendezvousError::NotATree);
}

#[test]
fn the_round_limit_stops_the_run_and_reports_it() {
    let starts = [NodeId(0), NodeId(5)];
    let (result, _, _) = simulate(
        &six(),
        &starts,
        &[1, 3],
        20,
        ComplexityMetrics::default(),
        NoTrace,
    )
    .unwrap();
    assert_eq!(
        result.termination,
        Termination::RoundLimitReached { limit: 20 }
    );
    assert_eq!(result.meeting, None);
    assert_eq!(result.rounds, 20);
}

#[test]
fn metrics_count_rounds_and_moves() {
    let starts = [NodeId(0), NodeId(5)];
    let (result, metrics, _) = simulate(
        &six(),
        &starts,
        &[1, 3],
        100,
        ComplexityMetrics::default(),
        NoTrace,
    )
    .unwrap();
    assert_eq!(metrics.rounds, result.rounds);
    // Both explore in blocks 0-2 (30 rounds each), then label 3 alone for 3.
    assert_eq!(metrics.agent_moves, 30 + 30 + 3);
}
