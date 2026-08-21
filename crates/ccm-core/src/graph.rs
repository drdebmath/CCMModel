use crate::{NodeId, PortId};
use core::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PortEdge {
    pub neighbor: NodeId,
    pub remote_port: PortId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NodePorts {
    pub start: u32,
    pub len: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortGraph {
    nodes: Vec<NodePorts>,
    ports: Vec<PortEdge>,
    edge_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GraphError {
    TooManyNodes(usize),
    TooManyPorts(usize),
    DegreeTooLarge {
        node: NodeId,
        degree: usize,
    },
    InvalidNode {
        source: NodeId,
        neighbor: NodeId,
    },
    InvalidRemotePort {
        source: NodeId,
        local_port: PortId,
        edge: PortEdge,
    },
    NonReciprocal {
        source: NodeId,
        local_port: PortId,
        edge: PortEdge,
    },
    SelfLoop(NodeId),
    ParallelEdge {
        a: NodeId,
        b: NodeId,
    },
}

impl fmt::Display for GraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for GraphError {}

impl PortGraph {
    /// Builds a graph from node-indexed local-port tables. The index in each
    /// inner vector is the local port and each value names the neighbor and its
    /// reciprocal port.
    ///
    /// # Errors
    ///
    /// Returns an error when IDs cannot represent the graph or any local port
    /// lacks the exact reciprocal mapping.
    pub fn from_port_tables(tables: Vec<Vec<PortEdge>>) -> Result<Self, GraphError> {
        if tables.len() > u32::MAX as usize {
            return Err(GraphError::TooManyNodes(tables.len()));
        }

        let mut nodes = Vec::with_capacity(tables.len());
        let total_ports = tables.iter().map(Vec::len).sum();
        if total_ports > u32::MAX as usize {
            return Err(GraphError::TooManyPorts(total_ports));
        }
        let mut ports = Vec::with_capacity(total_ports);
        for (index, table) in tables.into_iter().enumerate() {
            if table.len() > u16::MAX as usize {
                return Err(GraphError::DegreeTooLarge {
                    node: NodeId(
                        u32::try_from(index).map_err(|_| GraphError::TooManyNodes(index + 1))?,
                    ),
                    degree: table.len(),
                });
            }
            nodes.push(NodePorts {
                start: u32::try_from(ports.len())
                    .map_err(|_| GraphError::TooManyPorts(total_ports))?,
                len: u16::try_from(table.len()).map_err(|_| GraphError::DegreeTooLarge {
                    node: NodeId(u32::try_from(index).unwrap_or(u32::MAX)),
                    degree: table.len(),
                })?,
            });
            ports.extend(table);
        }

        let graph = Self {
            nodes,
            ports,
            edge_count: total_ports / 2,
        };
        graph.validate()?;
        Ok(graph)
    }

    /// Builds a simple undirected graph with canonical ports ordered by
    /// neighboring node ID.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid node IDs, self-loops, parallel edges, or a
    /// graph too large for the dense ID and port domains.
    pub fn from_undirected_edges(
        node_count: usize,
        edges: &[(NodeId, NodeId)],
    ) -> Result<Self, GraphError> {
        if node_count > u32::MAX as usize {
            return Err(GraphError::TooManyNodes(node_count));
        }
        let mut neighbors = vec![Vec::<NodeId>::new(); node_count];
        for &(a, b) in edges {
            if a.index() >= node_count {
                return Err(GraphError::InvalidNode {
                    source: a,
                    neighbor: a,
                });
            }
            if b.index() >= node_count {
                return Err(GraphError::InvalidNode {
                    source: a,
                    neighbor: b,
                });
            }
            if a == b {
                return Err(GraphError::SelfLoop(a));
            }
            if neighbors[a.index()].contains(&b) {
                return Err(GraphError::ParallelEdge { a, b });
            }
            neighbors[a.index()].push(b);
            neighbors[b.index()].push(a);
        }
        for list in &mut neighbors {
            list.sort_unstable();
        }

        let mut tables = Vec::with_capacity(node_count);
        for (source, list) in neighbors.iter().enumerate() {
            if list.len() > u16::MAX as usize {
                return Err(GraphError::DegreeTooLarge {
                    node: NodeId(
                        u32::try_from(source).map_err(|_| GraphError::TooManyNodes(node_count))?,
                    ),
                    degree: list.len(),
                });
            }
            let source_id =
                NodeId(u32::try_from(source).map_err(|_| GraphError::TooManyNodes(node_count))?);
            let mut table = Vec::with_capacity(list.len());
            for &neighbor in list {
                let remote = neighbors[neighbor.index()]
                    .binary_search(&source_id)
                    .map_err(|_| GraphError::NonReciprocal {
                        source: source_id,
                        local_port: PortId(0),
                        edge: PortEdge {
                            neighbor,
                            remote_port: PortId(0),
                        },
                    })?;
                let remote_port =
                    u16::try_from(remote).map_err(|_| GraphError::DegreeTooLarge {
                        node: neighbor,
                        degree: neighbors[neighbor.index()].len(),
                    })?;
                table.push(PortEdge {
                    neighbor,
                    remote_port: PortId(remote_port),
                });
            }
            tables.push(table);
        }
        Self::from_port_tables(tables)
    }

    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    #[must_use]
    pub const fn edge_count(&self) -> usize {
        self.edge_count
    }

    #[must_use]
    pub fn degree(&self, node: NodeId) -> Option<u16> {
        self.nodes.get(node.index()).map(|ports| ports.len)
    }

    #[must_use]
    pub fn traverse(&self, node: NodeId, port: PortId) -> Option<PortEdge> {
        let node_ports = self.nodes.get(node.index())?;
        if port.0 >= node_ports.len {
            return None;
        }
        self.ports
            .get(node_ports.start as usize + port.index())
            .copied()
    }

    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn ports(&self, node: NodeId) -> impl ExactSizeIterator<Item = (PortId, PortEdge)> + '_ {
        let range = self.nodes.get(node.index()).map_or(0..0, |entry| {
            entry.start as usize..entry.start as usize + entry.len as usize
        });
        self.ports[range]
            .iter()
            .copied()
            .enumerate()
            .map(|(port, edge)| (PortId(port as u16), edge))
    }

    /// Checks ID validity and exact reciprocal traversal for every local port.
    ///
    /// # Errors
    ///
    /// Returns the first invalid node, remote port, or reciprocal mapping.
    #[allow(clippy::cast_possible_truncation)]
    pub fn validate(&self) -> Result<(), GraphError> {
        for source_index in 0..self.nodes.len() {
            let source = NodeId(source_index as u32);
            for (local_port, edge) in self.ports(source) {
                if edge.neighbor.index() >= self.nodes.len() {
                    return Err(GraphError::InvalidNode {
                        source,
                        neighbor: edge.neighbor,
                    });
                }
                let Some(reverse) = self.traverse(edge.neighbor, edge.remote_port) else {
                    return Err(GraphError::InvalidRemotePort {
                        source,
                        local_port,
                        edge,
                    });
                };
                if reverse.neighbor != source || reverse.remote_port != local_port {
                    return Err(GraphError::NonReciprocal {
                        source,
                        local_port,
                        edge,
                    });
                }
            }
        }
        if self.ports.len() % 2 != 0 {
            let edge = self.ports.last().copied().unwrap_or(PortEdge {
                neighbor: NodeId(0),
                remote_port: PortId(0),
            });
            return Err(GraphError::NonReciprocal {
                source: NodeId(0),
                local_port: PortId(0),
                edge,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_path_has_reciprocal_ports() {
        let graph =
            PortGraph::from_undirected_edges(3, &[(NodeId(0), NodeId(1)), (NodeId(1), NodeId(2))])
                .unwrap();
        assert_eq!(graph.edge_count(), 2);
        assert_eq!(
            graph.traverse(NodeId(0), PortId(0)).unwrap().neighbor,
            NodeId(1)
        );
        let middle_to_end = graph.traverse(NodeId(1), PortId(1)).unwrap();
        assert_eq!(
            middle_to_end,
            PortEdge {
                neighbor: NodeId(2),
                remote_port: PortId(0)
            }
        );
        graph.validate().unwrap();
    }

    #[test]
    fn rejects_nonreciprocal_table() {
        let error = PortGraph::from_port_tables(vec![
            vec![PortEdge {
                neighbor: NodeId(1),
                remote_port: PortId(0),
            }],
            vec![PortEdge {
                neighbor: NodeId(1),
                remote_port: PortId(0),
            }],
        ])
        .unwrap_err();
        assert!(matches!(error, GraphError::NonReciprocal { .. }));
    }
}
