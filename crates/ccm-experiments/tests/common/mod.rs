//! Helpers shared by the algorithm tests. Not every test file uses every helper.
#![allow(dead_code)]

use ccm_core::{NodeId, PortAssignment, PortGraph};
use ccm_experiments::{generate_graph, GraphFamily, GraphSpec};

/// A graph with canonical ports, built from an undirected edge list.
pub fn graph(edges: &[(u32, u32)], nodes: usize) -> PortGraph {
    let pairs: Vec<(NodeId, NodeId)> = edges.iter().map(|&(a, b)| (NodeId(a), NodeId(b))).collect();
    PortGraph::from_undirected_edges(nodes, &pairs).unwrap()
}

/// Every agent starts at node 0.
pub fn rooted(count: usize) -> Vec<NodeId> {
    vec![NodeId(0); count]
}

/// A seeded connected graph with randomly permuted port labels.
pub fn random_port_graph(nodes: usize, extra_edges: usize, seed: u64) -> PortGraph {
    generate_graph(
        &GraphSpec::new(GraphFamily::RandomConnected { extra_edges }, nodes),
        PortAssignment::Random,
        seed,
    )
    .unwrap()
}
