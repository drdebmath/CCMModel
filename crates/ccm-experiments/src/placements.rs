#![allow(clippy::cast_possible_truncation)]

use ccm_core::{DeterministicRng, NodeId};
use core::fmt;

/// Initial-placement families.  A placement vector is always returned in
/// dense agent-ID order, so it is independent of map/set iteration order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlacementSpec {
    SingleNode { node: NodeId },
    UniformRandom { distinct_nodes: usize },
    Clustered { centers: Vec<NodeId> },
    Explicit { nodes: Vec<NodeId> },
}

impl PlacementSpec {
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::SingleNode { node } => format!("single:{}", node.0),
            Self::UniformRandom { distinct_nodes } => format!("uniform:{distinct_nodes}"),
            Self::Clustered { centers } => format!(
                "clustered:{}",
                centers
                    .iter()
                    .map(|node| node.0.to_string())
                    .collect::<Vec<_>>()
                    .join(";")
            ),
            Self::Explicit { nodes } => format!(
                "explicit:{}",
                nodes
                    .iter()
                    .map(|node| node.0.to_string())
                    .collect::<Vec<_>>()
                    .join(";")
            ),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlacementError {
    EmptyGraph,
    InvalidNode(NodeId),
    InvalidDistinctNodeCount { requested: usize, available: usize },
    EmptyCluster,
    AgentCountMismatch { expected: usize, actual: usize },
}

impl fmt::Display for PlacementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for PlacementError {}

/// Deterministically places `agents` using only the supplied seed.
/// Produces a deterministic dense starting-node vector.
///
/// # Errors
///
/// Returns an error when a requested node, distinct-node count, cluster set,
/// or explicit agent count is invalid.
///
/// # Panics
///
/// Panics only if deterministic selection is attempted from a candidate set
/// already validated as non-empty.
pub fn place_agents(
    node_count: usize,
    agents: usize,
    placement: &PlacementSpec,
    seed: u64,
) -> Result<Vec<NodeId>, PlacementError> {
    if node_count == 0 {
        return if agents == 0 {
            Ok(Vec::new())
        } else {
            Err(PlacementError::EmptyGraph)
        };
    }

    match placement {
        PlacementSpec::SingleNode { node } => {
            validate_node(*node, node_count)?;
            Ok(vec![*node; agents])
        }
        PlacementSpec::UniformRandom { distinct_nodes } => {
            if *distinct_nodes == 0 || *distinct_nodes > node_count {
                return Err(PlacementError::InvalidDistinctNodeCount {
                    requested: *distinct_nodes,
                    available: node_count,
                });
            }
            let mut candidates: Vec<NodeId> = (0..node_count).map(|n| NodeId(n as u32)).collect();
            let mut rng = DeterministicRng::new(seed);
            rng.shuffle(&mut candidates);
            candidates.truncate(*distinct_nodes);
            Ok((0..agents)
                .map(|_| {
                    candidates[rng
                        .index(candidates.len())
                        .expect("non-empty candidate set")]
                })
                .collect())
        }
        PlacementSpec::Clustered { centers } => {
            if centers.is_empty() {
                return Err(PlacementError::EmptyCluster);
            }
            for &node in centers {
                validate_node(node, node_count)?;
            }
            let mut rng = DeterministicRng::new(seed);
            Ok((0..agents)
                .map(|_| centers[rng.index(centers.len()).expect("non-empty cluster")])
                .collect())
        }
        PlacementSpec::Explicit { nodes } => {
            if nodes.len() != agents {
                return Err(PlacementError::AgentCountMismatch {
                    expected: agents,
                    actual: nodes.len(),
                });
            }
            for &node in nodes {
                validate_node(node, node_count)?;
            }
            Ok(nodes.clone())
        }
    }
}

fn validate_node(node: NodeId, node_count: usize) -> Result<(), PlacementError> {
    if node.index() >= node_count {
        Err(PlacementError::InvalidNode(node))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_and_explicit_placements_preserve_agent_order() {
        assert_eq!(
            place_agents(4, 3, &PlacementSpec::SingleNode { node: NodeId(2) }, 0).unwrap(),
            vec![NodeId(2), NodeId(2), NodeId(2)]
        );
        assert_eq!(
            place_agents(
                4,
                3,
                &PlacementSpec::Explicit {
                    nodes: vec![NodeId(3), NodeId(1), NodeId(0)]
                },
                0
            )
            .unwrap(),
            vec![NodeId(3), NodeId(1), NodeId(0)]
        );
    }

    #[test]
    fn uniform_random_is_reproducible_and_uses_requested_pool() {
        let placement = PlacementSpec::UniformRandom { distinct_nodes: 3 };
        let first = place_agents(8, 40, &placement, 91).unwrap();
        let second = place_agents(8, 40, &placement, 91).unwrap();
        assert_eq!(first, second);
        assert!(first.iter().all(|node| node.index() < 8));
        assert!(
            first
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                <= 3
        );
    }

    #[test]
    fn invalid_explicit_and_clustered_inputs_are_rejected() {
        assert!(matches!(
            place_agents(
                3,
                2,
                &PlacementSpec::Explicit {
                    nodes: vec![NodeId(0)]
                },
                0
            ),
            Err(PlacementError::AgentCountMismatch { .. })
        ));
        assert!(matches!(
            place_agents(3, 2, &PlacementSpec::Clustered { centers: vec![] }, 0),
            Err(PlacementError::EmptyCluster)
        ));
    }
}
