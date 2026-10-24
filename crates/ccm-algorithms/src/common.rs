//! Vocabulary shared by the algorithms: the paper's port 1, the edge types
//! it induces, and the DFS node types.
//!
//! Sections refer to Pattanayak et al., *Optimal Dispersion Under Asynchrony*
//! (arXiv:2507.01298), where these definitions are stated.

use ccm_core::{PortGraph, PortId};

/// The paper's port 1, in this repository's zero-based port numbering.
pub const PORT_ONE: PortId = PortId(0);

/// Node type, from Section 3.
///
/// `PartiallyVisited` is the rule that makes this a `P1Tree` rather than an
/// ordinary DFS tree: a node reached only through a `tpq` edge defers its place
/// in the tree until its port-1 neighbour comes to claim it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum NodeType {
    /// Not yet visited by the DFS head.
    Unvisited,
    /// Visited, and neither partially nor fully visited.
    Visited,
    /// Parent edge is `tpq` and every empty neighbour is reached by a `tpq`
    /// edge.
    PartiallyVisited,
    /// Visited, with no empty neighbours left.
    FullyVisited,
}

/// Edge type as seen from one endpoint, from Section 3.
///
/// The name records the pair of local ports: `OtherOne` is the paper's `tp1`,
/// meaning a port other than 1 at this end and port 1 at the far end.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum EdgeType {
    /// `tp1`: port != 1 here, port 1 at the far end.
    OtherOne,
    /// `t11`: port 1 at both ends.
    OneOne,
    /// `t1q`: port 1 here, port != 1 at the far end.
    OneOther,
    /// `tpq`: port 1 at neither end.
    OtherOther,
}

impl EdgeType {
    /// Classifies the edge leaving `local` and arriving on `remote`.
    #[must_use]
    pub fn of(local: PortId, remote: PortId) -> Self {
        match (local == PORT_ONE, remote == PORT_ONE) {
            (false, true) => Self::OtherOne,
            (true, true) => Self::OneOne,
            (true, false) => Self::OneOther,
            (false, false) => Self::OtherOther,
        }
    }

    /// Whether the edge carries port 1 at either end.
    ///
    /// This is what Definition 1 requires of at least one tree edge at every
    /// vertex, and it is symmetric: `tp1` seen from one end is `t1q` seen from
    /// the other, and only `tpq` has port 1 at neither.
    #[must_use]
    pub const fn is_port_one_incident(self) -> bool {
        !matches!(self, Self::OtherOther)
    }

    /// Rank in the priority order `tp1 > t11 ~ t1q > tpq` (Algorithm 2, line 1).
    ///
    /// `t11` and `t1q` share a rank, which is unambiguous: both require port 1
    /// at this end, and a node has only one port 1.
    #[must_use]
    pub const fn priority(self) -> u8 {
        match self {
            Self::OtherOne => 0,
            Self::OneOne | Self::OneOther => 1,
            Self::OtherOther => 2,
        }
    }
}

/// Type of the edge leaving `node` through `port`, or `None` for a port the
/// node does not have.
#[must_use]
pub fn edge_type_at(graph: &PortGraph, node: ccm_core::NodeId, port: PortId) -> Option<EdgeType> {
    graph
        .traverse(node, port)
        .map(|edge| EdgeType::of(port, edge.remote_port))
}
