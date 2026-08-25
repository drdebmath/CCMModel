#![allow(clippy::cast_possible_truncation)]

use ccm_core::{DeterministicRng, GraphError, NodeId, PortAssignment, PortEdge, PortGraph, PortId};
use core::fmt;
use std::collections::BTreeSet;

/// Structured graph families used by native experiments.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GraphFamily {
    Path,
    Cycle,
    Star,
    Complete,
    Tree {
        branching: usize,
    },
    /// A rectangular grid prefix. `columns` controls the width; the final row
    /// may be partial when `nodes` is not a multiple of the width.
    Grid {
        columns: usize,
    },
    /// A seeded connected graph formed from a random spanning tree plus the
    /// requested number of distinct non-tree edges.
    RandomConnected {
        extra_edges: usize,
    },
    /// A seeded connected graph whose degree never exceeds `max_degree`.
    RandomBoundedDegree {
        max_degree: usize,
    },
}

impl GraphFamily {
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Path => "path".to_owned(),
            Self::Cycle => "cycle".to_owned(),
            Self::Star => "star".to_owned(),
            Self::Complete => "complete".to_owned(),
            Self::Tree { branching } => format!("tree:{branching}"),
            Self::Grid { columns } => format!("grid:{columns}"),
            Self::RandomConnected { extra_edges } => {
                format!("random-connected:{extra_edges}")
            }
            Self::RandomBoundedDegree { max_degree } => {
                format!("random-bounded:{max_degree}")
            }
        }
    }
}

/// A graph family plus its size and, optionally, explicit local port tables.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphSpec {
    pub family: GraphFamily,
    pub nodes: usize,
    pub explicit_ports: Option<Vec<Vec<PortEdge>>>,
}

impl GraphSpec {
    #[must_use]
    pub const fn new(family: GraphFamily, nodes: usize) -> Self {
        Self {
            family,
            nodes,
            explicit_ports: None,
        }
    }

    #[must_use]
    pub fn with_explicit_ports(mut self, ports: Vec<Vec<PortEdge>>) -> Self {
        self.explicit_ports = Some(ports);
        self
    }

    #[must_use]
    pub fn label(&self) -> String {
        self.family.label()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GraphGenerationError {
    InvalidSize { family: String, nodes: usize },
    InvalidParameter { family: String, parameter: usize },
    ExplicitPortsMissing,
    ExplicitPortsSize { expected: usize, actual: usize },
    Core(GraphError),
}

impl fmt::Display for GraphGenerationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for GraphGenerationError {}

impl From<GraphError> for GraphGenerationError {
    fn from(value: GraphError) -> Self {
        Self::Core(value)
    }
}

/// Generates a deterministic port graph for a structured family.
///
/// Canonical ports are ordered by neighboring node ID.  Random and
/// adversarial assignments retain the same undirected graph while changing
/// both endpoint-local port orders.  Explicit assignments are accepted only
/// when the caller supplies reciprocal tables on `GraphSpec`.
/// Generates a deterministic graph and port assignment.
///
/// # Errors
///
/// Returns an error for an invalid family size, explicit table, degree, or
/// port-domain overflow.
///
/// # Panics
///
/// Panics only if the internally constructed undirected adjacency loses its
/// reciprocal entry, which indicates an implementation defect.
pub fn generate_graph(
    spec: &GraphSpec,
    assignment: PortAssignment,
    seed: u64,
) -> Result<PortGraph, GraphGenerationError> {
    if assignment == PortAssignment::Explicit {
        let tables = spec
            .explicit_ports
            .as_ref()
            .ok_or(GraphGenerationError::ExplicitPortsMissing)?;
        if tables.len() != spec.nodes {
            return Err(GraphGenerationError::ExplicitPortsSize {
                expected: spec.nodes,
                actual: tables.len(),
            });
        }
        return PortGraph::from_port_tables(tables.clone()).map_err(Into::into);
    }

    let edges = family_edges(&spec.family, spec.nodes, seed)?;
    let mut adjacency = vec![Vec::new(); spec.nodes];
    for &(a, b) in &edges {
        adjacency[a.index()].push(b);
        adjacency[b.index()].push(a);
    }
    for neighbors in &mut adjacency {
        neighbors.sort_unstable();
    }

    let mut rng = DeterministicRng::new(seed);
    if assignment == PortAssignment::Random {
        for neighbors in &mut adjacency {
            rng.shuffle(neighbors);
        }
    } else if assignment == PortAssignment::Adversarial {
        for neighbors in &mut adjacency {
            neighbors.reverse();
        }
    }

    let mut ports = vec![Vec::<PortEdge>::new(); spec.nodes];
    for (source, neighbors) in adjacency.iter().enumerate() {
        let source_id = NodeId(source as u32);
        for &neighbor in neighbors {
            let remote = adjacency[neighbor.index()]
                .iter()
                .position(|candidate| *candidate == source_id)
                .expect("undirected edge must have a reciprocal adjacency entry");
            ports[source].push(PortEdge {
                neighbor,
                remote_port: PortId(remote as u16),
            });
        }
    }
    PortGraph::from_port_tables(ports).map_err(Into::into)
}

fn family_edges(
    family: &GraphFamily,
    nodes: usize,
    seed: u64,
) -> Result<Vec<(NodeId, NodeId)>, GraphGenerationError> {
    let family_name = family.label();
    let mut edges = Vec::new();
    match family {
        GraphFamily::Path => {
            for node in 0..nodes.saturating_sub(1) {
                edges.push((NodeId(node as u32), NodeId((node + 1) as u32)));
            }
        }
        GraphFamily::Cycle => {
            if nodes < 3 {
                return Err(GraphGenerationError::InvalidSize {
                    family: family_name,
                    nodes,
                });
            }
            for node in 0..nodes {
                edges.push((NodeId(node as u32), NodeId(((node + 1) % nodes) as u32)));
            }
        }
        GraphFamily::Star => {
            for node in 1..nodes {
                edges.push((NodeId(0), NodeId(node as u32)));
            }
        }
        GraphFamily::Complete => {
            for left in 0..nodes {
                for right in (left + 1)..nodes {
                    edges.push((NodeId(left as u32), NodeId(right as u32)));
                }
            }
        }
        GraphFamily::Tree { branching } => {
            if *branching == 0 {
                return Err(GraphGenerationError::InvalidParameter {
                    family: family_name,
                    parameter: *branching,
                });
            }
            for child in 1..nodes {
                let parent = (child - 1) / branching;
                edges.push((NodeId(parent as u32), NodeId(child as u32)));
            }
        }
        GraphFamily::Grid { columns } => {
            if *columns == 0 {
                return Err(GraphGenerationError::InvalidParameter {
                    family: family_name,
                    parameter: *columns,
                });
            }
            for node in 0..nodes {
                let column = node % columns;
                if column + 1 < *columns && node + 1 < nodes {
                    edges.push((NodeId(node as u32), NodeId((node + 1) as u32)));
                }
                if node + columns < nodes {
                    edges.push((NodeId(node as u32), NodeId((node + columns) as u32)));
                }
            }
        }
        GraphFamily::RandomConnected { extra_edges } => {
            edges = random_connected_edges(nodes, *extra_edges, seed);
        }
        GraphFamily::RandomBoundedDegree { max_degree } => {
            if *max_degree == 0 || (*max_degree == 1 && nodes > 2) {
                return Err(GraphGenerationError::InvalidParameter {
                    family: family_name,
                    parameter: *max_degree,
                });
            }
            edges = random_bounded_edges(nodes, *max_degree, seed);
        }
    }
    Ok(edges)
}

fn random_connected_edges(nodes: usize, extra_edges: usize, seed: u64) -> Vec<(NodeId, NodeId)> {
    let mut rng = DeterministicRng::new(seed);
    let mut edges = BTreeSet::new();
    for node in 1..nodes {
        let parent = rng
            .index(node)
            .expect("a non-root node has a parent candidate");
        edges.insert((parent, node));
    }
    let maximum = nodes.saturating_mul(nodes.saturating_sub(1)) / 2;
    let target = edges.len().saturating_add(extra_edges).min(maximum);
    let mut attempts = 0_usize;
    let attempt_limit = maximum.saturating_mul(8).max(32);
    while edges.len() < target && attempts < attempt_limit {
        attempts = attempts.saturating_add(1);
        let Some(left) = rng.index(nodes) else { break };
        let Some(right) = rng.index(nodes) else { break };
        if left != right {
            edges.insert((left.min(right), left.max(right)));
        }
    }
    // A deterministic fallback guarantees the exact requested density even
    // when random sampling repeatedly hits existing edges.
    if edges.len() < target {
        'outer: for left in 0..nodes {
            for right in (left + 1)..nodes {
                edges.insert((left, right));
                if edges.len() == target {
                    break 'outer;
                }
            }
        }
    }
    edges
        .into_iter()
        .map(|(left, right)| (NodeId(left as u32), NodeId(right as u32)))
        .collect()
}

fn random_bounded_edges(nodes: usize, max_degree: usize, seed: u64) -> Vec<(NodeId, NodeId)> {
    if nodes < 2 {
        return Vec::new();
    }
    let mut rng = DeterministicRng::new(seed);
    let mut degrees = vec![0_usize; nodes];
    let mut edges = BTreeSet::new();
    for node in 1..nodes {
        let candidates: Vec<usize> = (0..node)
            .filter(|&candidate| degrees[candidate] < max_degree)
            .collect();
        let selected = rng
            .index(candidates.len())
            .map_or(node - 1, |index| candidates[index]);
        edges.insert((selected, node));
        degrees[selected] = degrees[selected].saturating_add(1);
        degrees[node] = degrees[node].saturating_add(1);
    }
    let target = nodes
        .saturating_mul(max_degree)
        .checked_div(2)
        .unwrap_or(0)
        .max(edges.len());
    let attempt_limit = nodes.saturating_mul(max_degree).saturating_mul(16).max(32);
    for _ in 0..attempt_limit {
        if edges.len() >= target {
            break;
        }
        let Some(left) = rng.index(nodes) else { break };
        let Some(right) = rng.index(nodes) else { break };
        if left == right || degrees[left] >= max_degree || degrees[right] >= max_degree {
            continue;
        }
        let edge = (left.min(right), left.max(right));
        if edges.insert(edge) {
            degrees[left] += 1;
            degrees[right] += 1;
        }
    }
    edges
        .into_iter()
        .map(|(left, right)| (NodeId(left as u32), NodeId(right as u32)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_families_have_expected_sizes() {
        let cases = [
            (GraphFamily::Path, 5, 4),
            (GraphFamily::Cycle, 5, 5),
            (GraphFamily::Star, 5, 4),
            (GraphFamily::Complete, 5, 10),
            (GraphFamily::Tree { branching: 2 }, 5, 4),
            (GraphFamily::Grid { columns: 3 }, 9, 12),
            (GraphFamily::RandomConnected { extra_edges: 3 }, 5, 7),
        ];
        for (family, nodes, edges) in cases {
            let graph = generate_graph(
                &GraphSpec::new(family, nodes),
                PortAssignment::Canonical,
                42,
            )
            .unwrap();
            assert_eq!(graph.node_count(), nodes);
            assert_eq!(graph.edge_count(), edges);
            graph.validate().unwrap();
        }
    }

    #[test]
    fn random_bounded_family_is_reproducible_connected_and_bounded() {
        let spec = GraphSpec::new(GraphFamily::RandomBoundedDegree { max_degree: 3 }, 40);
        let first = generate_graph(&spec, PortAssignment::Random, 71).unwrap();
        let second = generate_graph(&spec, PortAssignment::Random, 71).unwrap();
        assert_eq!(first, second);
        assert!((0..first.node_count()).all(|node| first
            .degree(NodeId(node as u32))
            .is_some_and(|degree| degree <= 3)));
        assert!(first.edge_count() >= first.node_count() - 1);
    }

    #[test]
    fn random_ports_are_seeded_and_reciprocal() {
        let spec = GraphSpec::new(GraphFamily::Complete, 6);
        let first = generate_graph(&spec, PortAssignment::Random, 7).unwrap();
        let second = generate_graph(&spec, PortAssignment::Random, 7).unwrap();
        assert_eq!(first, second);
        first.validate().unwrap();
    }

    #[test]
    fn cycle_rejects_duplicate_edge_size() {
        let result = generate_graph(
            &GraphSpec::new(GraphFamily::Cycle, 2),
            PortAssignment::Canonical,
            0,
        );
        assert!(matches!(
            result,
            Err(GraphGenerationError::InvalidSize { .. })
        ));
    }
}
