//! `P1Tree` correctness and cost tests, moved out of the algorithm source.

mod common;

use ccm_algorithms::p1tree::{
    simulate, simulate_with_stop, EdgeType, NodeType, P1Error, P1Result, P1Status, Stop,
};
use ccm_core::{ComplexityMetrics, NodeId, PortGraph, PortId, Termination};
use ccm_trace::NoTrace;
use common::{graph, random_port_graph, rooted};

fn run_until(
    g: &PortGraph,
    starts: &[NodeId],
    round_limit: u64,
    stop: Stop,
) -> Result<P1Result, P1Error> {
    simulate_with_stop(
        g,
        starts,
        round_limit,
        stop,
        ComplexityMetrics::default(),
        NoTrace,
    )
    .map(|(result, _, _)| result)
}

/// Stops at dispersion, as the website and the experiment runner do.
fn run(g: &PortGraph, starts: &[NodeId], round_limit: u64) -> Result<P1Result, P1Error> {
    run_until(g, starts, round_limit, Stop::AtDispersion)
}

/// Runs to Algorithm 2's own termination, so the result is a full `P1Tree`.
fn run_to_full_tree(g: &PortGraph, starts: &[NodeId], round_limit: u64) -> P1Result {
    run_until(g, starts, round_limit, Stop::AtFullTree).unwrap()
}

fn metrics_after(g: &PortGraph, agents: usize, round_limit: u64) -> ComplexityMetrics {
    simulate(
        g,
        &rooted(agents),
        round_limit,
        ComplexityMetrics::default(),
        NoTrace,
    )
    .unwrap()
    .1
}

/// Definition 1: every settled node carries a port-1 incident tree edge.
fn satisfies_definition_one(result: &P1Result) -> bool {
    let settled: Vec<_> = result
        .agents
        .iter()
        .filter(|agent| agent.status == P1Status::Settled)
        .collect();
    settled.len() < 2 || settled.iter().all(|agent| agent.port_one_tree_edge)
}

fn has_partially_visited(result: &P1Result) -> bool {
    result
        .agents
        .iter()
        .any(|agent| agent.node_type == NodeType::PartiallyVisited)
}

fn sorted_homes(result: &P1Result) -> Vec<u32> {
    let mut homes: Vec<u32> = result
        .agents
        .iter()
        .map(|agent| agent.home.expect("every agent settles").0)
        .collect();
    homes.sort_unstable();
    homes
}

fn all_nodes(n: usize) -> Vec<u32> {
    (0..u32::try_from(n).unwrap()).collect()
}

#[test]
fn edge_types_follow_the_paper_definition() {
    assert_eq!(EdgeType::of(PortId(0), PortId(0)), EdgeType::OneOne);
    assert_eq!(EdgeType::of(PortId(2), PortId(0)), EdgeType::OtherOne);
    assert_eq!(EdgeType::of(PortId(0), PortId(3)), EdgeType::OneOther);
    assert_eq!(EdgeType::of(PortId(1), PortId(2)), EdgeType::OtherOther);

    // tpq is the only type without port 1 at either end, and the property is
    // symmetric across the edge.
    assert!(!EdgeType::OtherOther.is_port_one_incident());
    for edge in [EdgeType::OneOne, EdgeType::OtherOne, EdgeType::OneOther] {
        assert!(edge.is_port_one_incident());
    }
    assert_eq!(
        EdgeType::of(PortId(2), PortId(0)).is_port_one_incident(),
        EdgeType::of(PortId(0), PortId(2)).is_port_one_incident()
    );
}

#[test]
fn priority_order_is_tp1_then_port_one_then_tpq() {
    assert!(EdgeType::OtherOne.priority() < EdgeType::OneOne.priority());
    assert_eq!(EdgeType::OneOne.priority(), EdgeType::OneOther.priority());
    assert!(EdgeType::OneOther.priority() < EdgeType::OtherOther.priority());
}

#[test]
fn dispersion_is_one_agent_per_node_across_families() {
    let cases: Vec<(&str, PortGraph)> = vec![
        ("path", graph(&[(0, 1), (1, 2), (2, 3), (3, 4)], 5)),
        ("cycle", graph(&[(0, 1), (1, 2), (2, 3), (3, 0)], 4)),
        ("star", graph(&[(0, 1), (0, 2), (0, 3)], 4)),
        (
            "complete",
            graph(&[(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)], 4),
        ),
        ("tree", graph(&[(0, 1), (0, 2), (1, 3), (1, 4), (2, 5)], 6)),
        (
            "grid",
            graph(&[(0, 1), (1, 2), (3, 4), (4, 5), (0, 3), (1, 4), (2, 5)], 6),
        ),
    ];
    for (name, g) in cases {
        let n = g.node_count();
        let result = run(&g, &rooted(n), 10_000).unwrap();
        assert_eq!(result.termination, Termination::Completed, "{name}");
        assert_eq!(sorted_homes(&result), all_nodes(n), "{name}");
    }
}

#[test]
fn result_satisfies_definition_one() {
    // Definition 1 describes the finished construction, so it is checked
    // against Stop::AtFullTree. The dispersion algorithm stops earlier, at
    // the moment the last agent settles, and may leave a node parked.
    let cases = vec![
        graph(&[(0, 1), (1, 2), (2, 3), (3, 0)], 4),
        graph(&[(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)], 4),
        graph(&[(0, 1), (1, 2), (2, 3), (3, 4), (4, 0), (0, 2)], 5),
        graph(&[(0, 1), (0, 2), (1, 3), (1, 4), (2, 5), (3, 4)], 6),
    ];
    for (index, g) in cases.into_iter().enumerate() {
        let n = g.node_count();
        let result = run_to_full_tree(&g, &rooted(n), 10_000);
        assert!(
            satisfies_definition_one(&result),
            "case {index} violates Definition 1"
        );
        for edge in &result.tree {
            assert!(
                g.traverse(edge.child, edge.child_port)
                    .is_some_and(|e| e.neighbor == edge.parent),
                "case {index} has a tree edge that is not a real edge"
            );
        }
    }
}

#[test]
fn fewer_agents_than_nodes_still_disperse() {
    let g = graph(&[(0, 1), (1, 2), (2, 3), (3, 4), (4, 5)], 6);
    let result = run(&g, &rooted(3), 10_000).unwrap();
    assert_eq!(result.termination, Termination::Completed);
    let mut homes: Vec<u32> = result
        .agents
        .iter()
        .filter(|agent| agent.status == P1Status::Settled)
        .filter_map(|agent| agent.home.map(|home| home.0))
        .collect();
    homes.sort_unstable();
    homes.dedup();
    assert_eq!(homes.len(), 3, "settled agents must occupy distinct nodes");
}

#[test]
fn random_port_labellings_always_yield_a_spanning_p1tree() {
    for seed in 0..60_u64 {
        let nodes = 4 + usize::try_from(seed % 9).unwrap();
        let g = random_port_graph(nodes, usize::try_from(seed % 6).unwrap(), seed);
        let n = g.node_count();
        let result = run(&g, &rooted(n), 100_000).unwrap();

        assert_eq!(result.termination, Termination::Completed, "seed {seed}");
        assert_eq!(sorted_homes(&result), all_nodes(n), "seed {seed}");

        // Definition 1 belongs to the finished construction, so it is checked
        // on a second run that does not stop at dispersion.
        let full = run_to_full_tree(&g, &rooted(n), 100_000);
        assert!(
            !has_partially_visited(&full),
            "seed {seed} finished with a partiallyVisited node"
        );
        assert!(
            satisfies_definition_one(&full),
            "seed {seed} violates Definition 1"
        );

        // Spanning tree: n-1 edges, every node but the root named once, and
        // every parent pointer a real edge whose type matches.
        assert_eq!(result.tree.len(), n - 1, "seed {seed}");
        let mut children: Vec<u32> = result.tree.iter().map(|e| e.child.0).collect();
        children.sort_unstable();
        children.dedup();
        assert_eq!(children.len(), n - 1, "seed {seed} repeats a child");
        for edge in &result.tree {
            let real = g
                .traverse(edge.child, edge.child_port)
                .expect("tree edge is a real edge");
            assert_eq!(real.neighbor, edge.parent, "seed {seed}");
            assert_eq!(
                EdgeType::of(edge.child_port, real.remote_port),
                edge.edge_type,
                "seed {seed}"
            );
        }

        // Every node reaches the root by parent pointers, so it is acyclic
        // and rooted rather than merely edge-counted.
        let parent_of: std::collections::BTreeMap<u32, u32> = result
            .tree
            .iter()
            .map(|edge| (edge.child.0, edge.parent.0))
            .collect();
        for start in all_nodes(n) {
            let mut at = start;
            let mut hops = 0;
            while let Some(&up) = parent_of.get(&at) {
                at = up;
                hops += 1;
                assert!(hops <= n, "seed {seed} has a cycle from {start}");
            }
            assert_eq!(at, 0, "seed {seed}: {start} does not reach the root");
        }
    }
}

#[test]
fn definition_one_needs_the_walk_back_to_the_root() {
    // K4 with canonical ports is the case that separates the two stopping
    // rules. Node 3 is discovered through a tpq edge and has no empty
    // neighbours, so it is parked; only the continued walk brings its port-1
    // neighbour past to reconfigure it.
    let g = graph(&[(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)], 4);

    let dispersed = run(&g, &rooted(4), 10_000).unwrap();
    assert_eq!(dispersed.termination, Termination::Completed);
    assert_eq!(sorted_homes(&dispersed), vec![0, 1, 2, 3]);

    let full = run_to_full_tree(&g, &rooted(4), 10_000);
    assert!(satisfies_definition_one(&full));
    assert!(!has_partially_visited(&full));
    assert!(
        full.logical_steps > dispersed.logical_steps,
        "finishing the tree costs more than dispersing"
    );
}

#[test]
fn a_scattered_placement_is_rejected_rather_than_run() {
    let g = graph(&[(0, 1), (1, 2)], 3);
    assert_eq!(
        run(&g, &[NodeId(0), NodeId(2)], 100),
        Err(P1Error::NonRootedStart)
    );
}

#[test]
fn round_limit_stops_the_run_and_reports_it() {
    let g = graph(&[(0, 1), (1, 2), (2, 3), (3, 4)], 5);
    let result = run(&g, &rooted(5), 1).unwrap();
    assert_eq!(
        result.termination,
        Termination::RoundLimitReached { limit: 1 }
    );
}

#[test]
fn scouts_probe_every_port_in_one_round_when_there_are_enough_of_them() {
    // A star root has degree 5 and no parent port. Five agents remain
    // unsettled after one settles at the root, so a single batch covers
    // every port: two rounds, out and back, not two per port.
    let g = graph(&[(0, 1), (0, 2), (0, 3), (0, 4), (0, 5)], 6);
    let metrics = metrics_after(&g, 6, 1);
    assert_eq!(metrics.port_probes, 5, "every port is probed");
    assert_eq!(
        metrics.probe_rounds, 2,
        "one batch of scouts covers all five ports"
    );
}

#[test]
fn probing_stops_at_the_first_batch_that_finds_a_target() {
    // Two scouts and five ports. The first batch already finds somewhere to
    // go, so the other three ports are never probed: the search is bounded
    // by what it is looking for, not by the degree.
    let g = graph(&[(0, 1), (0, 2), (0, 3), (0, 4), (0, 5)], 6);
    let metrics = metrics_after(&g, 3, 1);
    assert_eq!(metrics.port_probes, 2, "only the first batch is probed");
    assert_eq!(metrics.probe_rounds, 2, "one batch, out and back");
}

#[test]
fn probing_scans_on_when_a_batch_finds_nothing() {
    // With every neighbour already settled there is nothing to find, and the
    // search has to look at every port before it can call the node
    // finished. One scout, so one port per batch.
    let g = graph(&[(0, 1), (0, 2), (0, 3), (1, 2), (2, 3)], 4);
    let metrics = metrics_after(&g, 4, 10_000);
    // Reaching "no candidate anywhere" at least once means a full scan
    // happened; the cheap early exit must not have suppressed it.
    assert!(
        metrics.port_probes >= 5,
        "a node was declared finished without ruling out every port"
    );
}

#[test]
fn cost_scales_with_the_agent_count_not_the_degree() {
    // The regression this guards: a search that scanned every port made a
    // dense graph cost O(n) rounds instead of O(k). Quadrupling the degree
    // with the agent count fixed must not move the round count much.
    let mut rounds = Vec::new();
    for nodes in [60_u32, 240] {
        let edges: Vec<(u32, u32)> = (0..nodes)
            .flat_map(|a| ((a + 1)..nodes).map(move |b| (a, b)))
            .collect();
        let g = graph(&edges, nodes as usize);
        rounds.push(metrics_after(&g, 8, 1_000_000).rounds);
    }
    let (small, large) = (rounds[0], rounds[1]);
    assert!(
        large <= small * 2,
        "degree grew 4x and rounds went {small} -> {large}: the search is \
         still scanning the whole neighbourhood"
    );
}

#[test]
fn the_parent_port_is_not_probed() {
    // Section 4.2 assigns scouts to every port except the parent's: the
    // parent is known to be occupied and is reached by backtracking.
    let g = graph(&[(0, 1), (1, 2), (1, 3)], 4);
    // Round 1 probes the root's single port.
    assert_eq!(metrics_after(&g, 4, 1).port_probes, 1);
    // Round 2 is at node 1, degree 3, one of which is the parent.
    assert_eq!(metrics_after(&g, 4, 2).port_probes, 1 + 2);
}

#[test]
fn agents_vacate_so_that_scouts_remain_after_dispersion() {
    // Without vacating the head runs out of scouts the moment the last
    // agent settles, and the rest of the construction is probed one port at
    // a time. Vacating is what keeps the parallel probe fed.
    let g = graph(
        &[
            (0, 1),
            (0, 2),
            (0, 3),
            (1, 2),
            (1, 3),
            (2, 3),
            (2, 4),
            (3, 4),
        ],
        5,
    );
    let metrics = metrics_after(&g, 5, 10_000);
    assert!(metrics.vacates > 0, "no node was ever vacated");
    assert!(metrics.retraces > 0, "scouts were never walked home");
}

#[test]
fn retrace_puts_every_scout_back_on_its_own_node() {
    // The run must still end with one settled agent standing on each node it
    // owns, not merely owning it from a distance.
    for seed in 0..25_u64 {
        let nodes = 4 + usize::try_from(seed % 7).unwrap();
        let g = random_port_graph(nodes, usize::try_from(seed % 5).unwrap(), seed);
        let n = g.node_count();
        let result = run(&g, &rooted(n), 100_000).unwrap();
        assert_eq!(result.termination, Termination::Completed, "seed {seed}");
        for (index, agent) in result.agents.iter().enumerate() {
            assert_eq!(
                agent.status,
                P1Status::Settled,
                "seed {seed}: agent {index} is still travelling"
            );
            assert_eq!(
                Some(agent.node),
                agent.home,
                "seed {seed}: agent {index} is not standing on its home"
            );
        }
    }
}
