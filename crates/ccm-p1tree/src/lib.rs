//! `DFS_P1Tree` dispersion.
//!
//! Implements the port-one tree construction of Pattanayak, Kshemkalyani,
//! Kumar, Molla and Sharma, *Optimal Dispersion Under Asynchrony*
//! (arXiv:2507.01298): Definition 1, the movement rules (D0)-(D4), and
//! Algorithm 2 `DFS_P1Tree()`, executed by agents as described in the paper's
//! Section 4.
//!
//! # What this is, and what it is not
//!
//! One DFS head, a parallel neighbourhood search (Section 4.2), and one agent
//! settling per newly discovered node. It builds the `P1Tree` of Algorithm 2
//! and disperses the agents while doing so.
//!
//! The scouts probe the head's ports **in parallel**: each takes a distinct
//! port, they step out together, and they come back together reporting the
//! state of the neighbour they saw. With at least as many scouts as ports that
//! is two rounds regardless of degree.
//!
//! It is still not the whole of the paper's `RootedAsync()`. That algorithm
//! also **vacates** settled agents (Section 4.1) so that a node with few
//! unsettled agents nearby can still muster enough scouts, and it runs under an
//! asynchronous scheduler. This crate does not vacate and runs under the
//! repository's synchronous scheduler, so the scouts are the unsettled agents,
//! and once every agent has settled the node's own settled agent probes alone.
//! Round counts here are therefore not the paper's `O(k)` bound.
//!
//! # Port numbering
//!
//! The paper numbers ports from 1 and gives port 1 its special role. This
//! repository numbers ports from 0, so the paper's "port 1" is [`PORT_ONE`]
//! below, and every edge-type test is written against it rather than against a
//! literal.

use ccm_core::{
    AgentId, AgentStatus, ComplexityMetrics, Metrics, NodeId, PortEdge, PortGraph, PortId,
    RoundKind, Termination,
};
use ccm_trace::{Checkpoint, NoTrace, Phase, Recorder, SimulationEvent};
use core::fmt;

/// Version of the authoritative `DFS_P1Tree` implementation.
pub const ALGORITHM_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The paper's port 1, in this repository's zero-based port numbering.
pub const PORT_ONE: PortId = PortId(0);

/// Agent state. The paper's agents are `unsettled` until they settle at a node,
/// which they then never leave in this construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum P1Status {
    Unsettled,
    Settled,
    /// Settled at a home node but travelling with the DFS head as a scout.
    /// Section 4.1 calls the node it left `vacated`.
    SettledScout,
}

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

/// What a scout finds at a node, from Section 4.1.
///
/// `Vacated` is the state that makes probing interesting: the node has an owner
/// but that owner is away scouting, so nobody is home to answer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum NodeState {
    Empty,
    Occupied,
    Vacated,
}

/// One entry of a neighbourhood search: the 4-tuple of Section 4,
/// `<p_xy, type({x,y}), type(y), psi(y)>`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProbeResult {
    pub port: PortId,
    pub edge_type: EdgeType,
    pub node_type: NodeType,
    pub node_state: NodeState,
    pub owner: Option<AgentId>,
}

/// Per-agent state.
///
/// A settled agent is the node's memory: the paper uses `psi(x)` for the agent
/// settled at `x` and stores the node's type and parent pointers in it, since
/// the nodes themselves are anonymous.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct P1Agent {
    pub node: NodeId,
    pub status: P1Status,
    /// The node this agent settled at, and thereafter represents.
    pub home: Option<NodeId>,
    pub node_type: NodeType,
    /// Agent settled at the parent node, the paper's `parentID`.
    pub parent: Option<AgentId>,
    /// Port at this node leading to the parent, the paper's `parentPort`.
    pub parent_port: Option<PortId>,
    /// Port at the parent leading back here, the paper's `portAtParent`.
    pub port_at_parent: Option<PortId>,
    /// Type of the parent edge, as seen from this node.
    pub parent_edge_type: Option<EdgeType>,
    /// Whether any tree edge at this node carries port 1 at either end. This is
    /// the Definition 1 property, tracked as the tree is built.
    pub port_one_tree_edge: bool,
    /// Set when a neighbour vacated on the strength of this node being
    /// occupied, which stops this node from vacating in turn (rules V3, V5).
    pub vacated_neighbor: bool,
    pub probe_results: Vec<ProbeResult>,
}

impl P1Agent {
    fn new(node: NodeId) -> Self {
        Self {
            node,
            status: P1Status::Unsettled,
            home: None,
            node_type: NodeType::Unvisited,
            parent: None,
            parent_port: None,
            port_at_parent: None,
            parent_edge_type: None,
            port_one_tree_edge: false,
            vacated_neighbor: false,
            probe_results: Vec::new(),
        }
    }
}

/// One edge of the constructed `P1Tree`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TreeEdge {
    pub parent: NodeId,
    pub child: NodeId,
    /// Port at `child` leading to `parent`.
    pub child_port: PortId,
    /// Type of the edge as seen from `child`.
    pub edge_type: EdgeType,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct P1Result {
    pub termination: Termination,
    pub agents: Vec<P1Agent>,
    /// The `P1Tree` built so far, one entry per node that has a parent.
    pub tree: Vec<TreeEdge>,
    pub logical_steps: u64,
    /// Step at which the last agent settled, i.e. when dispersion was achieved.
    ///
    /// The construction keeps running after this to finish the tree: Section 4
    /// terminates when every node is `fullyVisited`, and it is that tail that
    /// reconfigures the remaining `partiallyVisited` nodes. Dispersion is the
    /// earlier event, so it is reported separately rather than being conflated
    /// with termination.
    pub dispersed_at_step: Option<u64>,
}

impl P1Result {
    /// Whether every settled node carries a port-1 incident tree edge, which is
    /// Definition 1.
    ///
    /// This is a property of the *finished* construction. A run that stopped at
    /// its round limit, or one with fewer agents than nodes, can still hold a
    /// node whose only tree edge is `tpq` because the port-1 neighbour that
    /// would reconfigure it was never reached.
    #[must_use]
    pub fn satisfies_definition_one(&self) -> bool {
        let settled: Vec<&P1Agent> = self
            .agents
            .iter()
            .filter(|agent| agent.status == P1Status::Settled)
            .collect();
        if settled.len() < 2 {
            return true;
        }
        settled.iter().all(|agent| agent.port_one_tree_edge)
    }

    /// Whether any node is still waiting for its port-1 neighbour. A finished
    /// construction leaves none.
    #[must_use]
    pub fn has_partially_visited(&self) -> bool {
        self.agents
            .iter()
            .any(|agent| agent.node_type == NodeType::PartiallyVisited)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum P1Error {
    EmptyGraph,
    TooManyAgents {
        agents: usize,
        nodes: usize,
    },
    InvalidStart(NodeId),
    /// The paper's construction is rooted: every agent begins at one node.
    NonRootedStart,
    InvalidPort {
        node: NodeId,
        port: PortId,
    },
    /// A parent pointer did not lead back to a real node.
    BrokenTree(NodeId),
}

impl fmt::Display for P1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for P1Error {}

/// Runs `DFS_P1Tree` with default metrics and no trace.
///
/// # Errors
///
/// Returns an error for an empty graph, more agents than nodes, a start node
/// outside the graph, or a placement that is not rooted at a single node.
pub fn run(graph: &PortGraph, starts: &[NodeId], round_limit: u64) -> Result<P1Result, P1Error> {
    let (result, _, _) = simulate(
        graph,
        starts,
        round_limit,
        ComplexityMetrics::default(),
        NoTrace,
    )?;
    Ok(result)
}

/// Runs `DFS_P1Tree` with caller-supplied metrics and recorder.
///
/// # Errors
///
/// Returns an error for an empty graph, more agents than nodes, a start node
/// outside the graph, or a placement that is not rooted at a single node.
pub fn simulate<M: Metrics, R: Recorder>(
    graph: &PortGraph,
    starts: &[NodeId],
    round_limit: u64,
    metrics: M,
    recorder: R,
) -> Result<(P1Result, M, R), P1Error> {
    Simulation::new(graph, starts, round_limit, metrics, recorder)?.run()
}

struct Simulation<'a, M: Metrics, R: Recorder> {
    graph: &'a PortGraph,
    agents: Vec<P1Agent>,
    /// `psi`: the agent settled at each node, if any.
    settled_at: Vec<Option<AgentId>>,
    /// Node type, kept beside `settled_at` for direct indexing. The settled
    /// agent holds the authoritative copy; this mirrors it.
    node_type: Vec<NodeType>,
    /// Parent of each node in the tree: the node, the port at the child leading
    /// to it, and the edge type seen from the child.
    parent: Vec<Option<(NodeId, PortId, EdgeType)>>,
    /// Whether a node already has a port-1 incident tree edge.
    port_one_tree_edge: Vec<bool>,
    head: NodeId,
    round_limit: u64,
    step: u64,
    dispersed_at_step: Option<u64>,
    metrics: M,
    recorder: R,
}

impl<'a, M: Metrics, R: Recorder> Simulation<'a, M, R> {
    fn new(
        graph: &'a PortGraph,
        starts: &[NodeId],
        round_limit: u64,
        metrics: M,
        recorder: R,
    ) -> Result<Self, P1Error> {
        if graph.node_count() == 0 {
            return Err(P1Error::EmptyGraph);
        }
        if starts.is_empty() {
            return Err(P1Error::NonRootedStart);
        }
        if starts.len() > graph.node_count() {
            return Err(P1Error::TooManyAgents {
                agents: starts.len(),
                nodes: graph.node_count(),
            });
        }
        for &node in starts {
            if node.index() >= graph.node_count() {
                return Err(P1Error::InvalidStart(node));
            }
        }
        // Section 4 places every agent at the root v0. A scattered placement is
        // the paper's general-dispersion setting (Section 6), which this
        // construction does not implement, so it is rejected rather than run
        // under a silently different model.
        let root = starts[0];
        if starts.iter().any(|&node| node != root) {
            return Err(P1Error::NonRootedStart);
        }

        let node_count = graph.node_count();
        Ok(Self {
            graph,
            agents: starts.iter().copied().map(P1Agent::new).collect(),
            settled_at: vec![None; node_count],
            node_type: vec![NodeType::Unvisited; node_count],
            parent: vec![None; node_count],
            port_one_tree_edge: vec![false; node_count],
            head: root,
            round_limit,
            step: 0,
            dispersed_at_step: None,
            metrics,
            recorder,
        })
    }

    fn run(mut self) -> Result<(P1Result, M, R), P1Error> {
        self.checkpoint(Phase::Other);

        // Initialization (Section 4): the highest-ID agent present settles at
        // the root and the root becomes visited.
        self.settle_highest_at(self.head);
        self.node_type[self.head.index()] = NodeType::Visited;
        self.sync_node_type(self.head);
        self.checkpoint(Phase::Movement);

        let termination = self.traverse()?;
        // Section 5.1: put every travelling scout back on the node it owns, so
        // the run still ends one settled agent per node.
        if termination == Termination::Completed {
            self.retrace();
        }

        let tree = self.collect_tree();
        let agents = self.agents.clone();
        let logical_steps = self.step;
        let dispersed_at_step = self.dispersed_at_step;
        Ok((
            P1Result {
                termination,
                agents,
                tree,
                logical_steps,
                dispersed_at_step,
            },
            self.metrics,
            self.recorder,
        ))
    }

    /// The `while S != {}` loop of Algorithm 2, with the stack realised by the
    /// parent pointers the settled agents hold.
    ///
    /// The loop runs until the root is popped, which is Algorithm 2's own
    /// termination. Stopping earlier, as soon as every agent had settled, left
    /// nodes `partiallyVisited` with a `tpq` parent edge and so broke
    /// Definition 1: it is exactly the walk back to the root that reconfigures
    /// them.
    fn traverse(&mut self) -> Result<Termination, P1Error> {
        let mut rounds = 0_u64;
        loop {
            self.note_dispersion();
            if rounds >= self.round_limit {
                return Ok(Termination::RoundLimitReached {
                    limit: self.round_limit,
                });
            }
            rounds += 1;
            self.metrics.macro_round();

            let results = self.neighbourhood_search()?;
            // With no unsettled agent left there is nobody to settle at an
            // unvisited node, so those neighbours are not candidates. The head
            // can still enter a partiallyVisited node to reconfigure it.
            let can_settle = self.unsettled_count() > 0;
            let next = Self::choose_next_edge(&results, can_settle);

            if let Some(chosen) = next {
                if self.must_defer(chosen) {
                    // Algorithm 2, lines 21-23: taking a tpq edge would
                    // leave this node with no port-1 incident tree edge, so
                    // it waits to be claimed by its port-1 neighbour.
                    self.node_type[self.head.index()] = NodeType::PartiallyVisited;
                    self.sync_node_type(self.head);
                    self.apply_can_vacate(self.head);
                    self.checkpoint(Phase::Movement);
                    if !self.backtrack()? {
                        return Ok(Termination::Completed);
                    }
                } else {
                    self.advance(chosen)?;
                }
            } else {
                // Algorithm 2, line 29 marks a node with no candidate edge
                // fullyVisited. That alone is not enough: a node discovered
                // through a tpq edge that happens to have no empty
                // neighbours would finish with only a tpq tree edge and
                // break Definition 1. Claim 3 of the paper is explicit that
                // such a vertex "is immediately marked partiallyVisited and
                // DFS backtracks", so that its port-1 neighbour can reach it
                // later and reconfigure it. A node therefore only becomes
                // fullyVisited once it holds a port-1 incident tree edge.
                self.node_type[self.head.index()] = if self.awaits_port_one() {
                    NodeType::PartiallyVisited
                } else {
                    NodeType::FullyVisited
                };
                self.sync_node_type(self.head);
                self.apply_can_vacate(self.head);
                self.checkpoint(Phase::Movement);
                if !self.backtrack()? {
                    return Ok(Termination::Completed);
                }
            }
        }
    }

    /// Whether this node still needs its port-1 neighbour to come and
    /// reconfigure it: it was reached through a `tpq` edge and no tree edge at
    /// it carries port 1. Observation 1 guarantees such a neighbour exists.
    fn awaits_port_one(&self) -> bool {
        let Some((_, _, parent_type)) = self.parent[self.head.index()] else {
            // The root's first tree edge is always port-1 incident, since at
            // that moment every higher-priority edge still leads somewhere
            // unvisited.
            return false;
        };
        !parent_type.is_port_one_incident() && !self.port_one_tree_edge[self.head.index()]
    }

    fn note_dispersion(&mut self) {
        if self.dispersed_at_step.is_none() && self.unsettled_count() == 0 {
            self.dispersed_at_step = Some(self.step);
        }
    }

    /// Section 4.2's parallel probe: the scouts fan out over the head's ports
    /// together and come back reporting what they saw.
    ///
    /// Ports are handed to scouts in increasing agent-ID order. When there are
    /// at least as many scouts as ports the whole search is two rounds, out and
    /// back, which is the `O(1)`-epoch neighbourhood search the paper's bound
    /// rests on; with fewer scouts the assignment repeats until every port has
    /// been probed. The parent port is skipped, as in the paper: the parent is
    /// known to be occupied and is reached by backtracking, not by probing.
    ///
    /// The paper's rules (R1)-(R3) exist to tell an *empty* neighbour from a
    /// *vacated* one, which is ambiguous because a vacated node's settled agent
    /// has left it to travel as a scout. This implementation does not vacate
    /// (Section 4.1), so a node without a settled agent present is always empty
    /// and rule (R1) decides every case on its own.
    fn neighbourhood_search(&mut self) -> Result<Vec<ProbeResult>, P1Error> {
        let head = self.head;
        let degree = self.graph.degree(head).unwrap_or(0);
        let parent_port = self.parent[head.index()].map(|(_, port, _)| port);
        let ports: Vec<PortId> = (0..degree)
            .map(PortId)
            .filter(|port| Some(*port) != parent_port)
            .collect();
        let scouts = self.probe_party();
        let mut results = Vec::with_capacity(ports.len());
        if ports.is_empty() || scouts.is_empty() {
            return Ok(results);
        }

        for batch in ports.chunks(scouts.len()) {
            let moved = batch.len() as u64;

            self.metrics.logical_rounds(RoundKind::ProbeOut, 1);
            for (&port, &scout) in batch.iter().zip(scouts.iter()) {
                let edge = self
                    .graph
                    .traverse(head, port)
                    .ok_or(P1Error::InvalidPort { node: head, port })?;
                self.metrics.port_probe();
                self.metrics.probe_out_traversal();
                self.metrics.scout_operation();
                self.agents[scout].node = edge.neighbor;
                let state = self.state_of(edge.neighbor);
                // Rules (R2) and (R3): a scout that finds nobody home cannot yet
                // tell empty from vacated, and walks on to the port-1 neighbour
                // (and sometimes one further) to settle the question. The answer
                // is Lemma 5's, which is what the ownership index already holds,
                // but the walk is real and is charged here.
                let detour = self.probe_detour(edge.neighbor, edge.remote_port, state);
                for _ in 0..detour {
                    self.metrics.probe_out_traversal();
                    self.metrics.agent_moves(1);
                }
                results.push(ProbeResult {
                    port,
                    edge_type: EdgeType::of(port, edge.remote_port),
                    node_type: self.node_type[edge.neighbor.index()],
                    node_state: state,
                    owner: self.settled_at[edge.neighbor.index()],
                });
            }
            self.metrics.agent_moves(moved);
            self.checkpoint(Phase::ProbeOut);

            self.metrics.logical_rounds(RoundKind::ProbeBack, 1);
            for (&port, &scout) in batch.iter().zip(scouts.iter()) {
                let steps = self.graph.traverse(head, port).map_or(1, |edge| {
                    1 + self.probe_detour(
                        edge.neighbor,
                        edge.remote_port,
                        self.state_of(edge.neighbor),
                    )
                });
                for _ in 0..steps {
                    self.metrics.probe_back_traversal();
                }
                self.metrics.agent_moves(steps.saturating_sub(1));
                self.agents[scout].node = head;
            }
            self.metrics.agent_moves(moved);
            self.checkpoint(Phase::ProbeBack);
        }

        if let Some(agent) = self.settled_at[head.index()] {
            self.agents[agent.index()]
                .probe_results
                .clone_from(&results);
        }
        Ok(results)
    }

    /// What a scout standing on `node` sees: an owner in residence, an owner
    /// away scouting, or no owner at all.
    fn state_of(&self, node: NodeId) -> NodeState {
        match self.settled_at[node.index()] {
            None => NodeState::Empty,
            Some(agent) => {
                if self.agents[agent.index()].status == P1Status::SettledScout {
                    NodeState::Vacated
                } else {
                    NodeState::Occupied
                }
            }
        }
    }

    /// How many extra edges a scout walks past the neighbour before it can
    /// answer, following rules (R1)-(R3).
    ///
    /// (R1) somebody is home, so no detour. (R2) nobody is home and the scout
    /// arrived on port 1, so the node is empty and no detour. (R3) otherwise the
    /// scout steps to the port-1 neighbour, and once more if that one is also
    /// deserted.
    fn probe_detour(&self, neighbor: NodeId, arrival_port: PortId, state: NodeState) -> u64 {
        if state == NodeState::Occupied || arrival_port == PORT_ONE {
            return 0;
        }
        let Some(first) = self.graph.traverse(neighbor, PORT_ONE) else {
            return 0;
        };
        if self.state_of(first.neighbor) == NodeState::Occupied || first.remote_port == PORT_ONE {
            return 1;
        }
        2
    }

    /// Algorithm 2, lines 8-17: the highest-priority incident edge leading to a
    /// node the head may enter.
    ///
    /// A `PartiallyVisited` neighbour counts as empty exactly when this edge
    /// carries port 1 at that neighbour, which is rule (D4).
    fn choose_next_edge(results: &[ProbeResult], can_settle: bool) -> Option<ProbeResult> {
        let mut ordered: Vec<&ProbeResult> = results.iter().collect();
        ordered.sort_by_key(|result| (result.edge_type.priority(), result.port.0));
        ordered
            .into_iter()
            .find(|result| match result.node_type {
                NodeType::Unvisited => can_settle,
                NodeType::PartiallyVisited => {
                    matches!(result.edge_type, EdgeType::OtherOne | EdgeType::OneOne)
                }
                NodeType::Visited | NodeType::FullyVisited => false,
            })
            .copied()
    }

    /// Algorithm 2, line 21: the candidate and the parent edge are both `tpq`
    /// and this node has no port-1 incident tree edge to fall back on.
    fn must_defer(&self, chosen: ProbeResult) -> bool {
        if chosen.edge_type.is_port_one_incident() {
            return false;
        }
        let Some((_, _, parent_type)) = self.parent[self.head.index()] else {
            // The root has no parent edge, so the rule cannot apply to it.
            return false;
        };
        !parent_type.is_port_one_incident() && !self.port_one_tree_edge[self.head.index()]
    }

    /// Algorithm 2, lines 25-27: take the edge, and settle or reconfigure at the
    /// far end.
    fn advance(&mut self, chosen: ProbeResult) -> Result<(), P1Error> {
        let from = self.head;
        let edge = self
            .graph
            .traverse(from, chosen.port)
            .ok_or(P1Error::InvalidPort {
                node: from,
                port: chosen.port,
            })?;
        let target = edge.neighbor;
        let was = self.node_type[target.index()];

        self.metrics.logical_rounds(RoundKind::Movement, 1);
        self.metrics.group_move(self.unsettled_count());
        self.metrics.agent_moves(self.unsettled_count() as u64);
        self.move_unsettled_to(target);
        self.head = target;

        // The edge as seen from the target, which is the direction Definition 1
        // and the parent pointers are stated in.
        let from_target = EdgeType::of(edge.remote_port, chosen.port);

        match was {
            NodeType::Unvisited => {
                // Settle before recording the parent edge: the settled agent is
                // where the node's parent pointers live, so writing them first
                // wrote them nowhere.
                self.settle_highest_at(target);
                self.node_type[target.index()] = NodeType::Visited;
            }
            NodeType::PartiallyVisited => {
                // Reconfiguration (rule (D4)): the old tpq parent edge is
                // dropped in set_parent above and this port-1 edge takes its
                // place, which makes the node visited.
                self.node_type[target.index()] = NodeType::Visited;
            }
            NodeType::Visited | NodeType::FullyVisited => {}
        }
        self.set_parent(target, from, edge.remote_port, chosen.port, from_target);
        self.sync_node_type(target);
        // Rules V2 and V5 look at the node just left and at the new node's
        // parent, both of which are only settled now that the move is recorded.
        self.apply_can_vacate(from);
        self.apply_parent_vacate(target);
        self.checkpoint(Phase::Movement);
        Ok(())
    }

    /// Records a tree edge and keeps the Definition 1 flag current at both ends.
    fn set_parent(
        &mut self,
        child: NodeId,
        parent: NodeId,
        child_port: PortId,
        parent_port: PortId,
        edge_type: EdgeType,
    ) {
        self.parent[child.index()] = Some((parent, child_port, edge_type));
        if edge_type.is_port_one_incident() {
            self.port_one_tree_edge[child.index()] = true;
            self.port_one_tree_edge[parent.index()] = true;
        }
        if let Some(agent) = self.settled_at[child.index()] {
            let parent_agent = self.settled_at[parent.index()];
            let value = &mut self.agents[agent.index()];
            value.parent = parent_agent;
            value.parent_port = Some(child_port);
            value.port_at_parent = Some(parent_port);
            value.parent_edge_type = Some(edge_type);
            value.port_one_tree_edge = self.port_one_tree_edge[child.index()];
        }
        if let Some(agent) = self.settled_at[parent.index()] {
            self.agents[agent.index()].port_one_tree_edge = self.port_one_tree_edge[parent.index()];
        }
    }

    /// Moves the DFS head to the parent of the current node. Returns false when
    /// the root is popped, which empties the stack and ends the traversal.
    fn backtrack(&mut self) -> Result<bool, P1Error> {
        let Some((parent, port, _)) = self.parent[self.head.index()] else {
            return Ok(false);
        };
        let edge = self
            .graph
            .traverse(self.head, port)
            .ok_or(P1Error::BrokenTree(self.head))?;
        if edge.neighbor != parent {
            return Err(P1Error::BrokenTree(self.head));
        }
        self.metrics.logical_rounds(RoundKind::Movement, 1);
        self.metrics.backtrack();
        self.metrics.group_move(self.unsettled_count());
        self.metrics.agent_moves(self.unsettled_count() as u64);
        self.move_unsettled_to(parent);
        self.head = parent;
        self.checkpoint(Phase::Movement);
        Ok(true)
    }

    /// Section 4: "the agent with the highest ID among the unsettled agents at
    /// v settles".
    fn settle_highest_at(&mut self, node: NodeId) {
        let candidate = self
            .agents
            .iter()
            .enumerate()
            .filter(|(_, agent)| agent.status == P1Status::Unsettled && agent.node == node)
            .map(|(index, _)| index)
            .next_back();
        let Some(index) = candidate else {
            return;
        };
        let agent = &mut self.agents[index];
        agent.status = P1Status::Settled;
        agent.home = Some(node);
        agent.node_type = NodeType::Visited;
        agent.port_one_tree_edge = self.port_one_tree_edge[node.index()];
        self.settled_at[node.index()] = Some(AgentId(
            u32::try_from(index).expect("agent count fits the dense ID domain"),
        ));
        self.metrics.settlement();
    }

    /// Algorithm 3, `Can_Vacate()`. Decides whether the agent settled at `node`
    /// may leave and travel with the head as a scout.
    ///
    /// The point of vacating is supply: a node only ever holds one settled
    /// agent, so without releasing some of them the head runs out of scouts as
    /// soon as the last agent settles, and the parallel probe has nobody to
    /// probe with. Lemma 4 is what this buys — at least a third of the tree is
    /// vacated at any moment.
    fn apply_can_vacate(&mut self, node: NodeId) {
        let Some(owner) = self.settled_at[node.index()] else {
            return;
        };
        if self.agents[owner.index()].status != P1Status::Settled {
            return;
        }
        // (V1) the root is always occupied.
        if self.parent[node.index()].is_none() {
            return;
        }
        let node_type = self.node_type[node.index()];
        let vacated_neighbor = self.agents[owner.index()].vacated_neighbor;

        match node_type {
            // (V2) a visited node vacates when its port-1 neighbour is occupied.
            // The head steps there to record that this node leaned on it.
            NodeType::Visited => {
                let Some(edge) = self.graph.traverse(node, PORT_ONE) else {
                    return;
                };
                if self.state_of(edge.neighbor) != NodeState::Occupied {
                    return;
                }
                if let Some(neighbor_owner) = self.settled_at[edge.neighbor.index()] {
                    self.agents[neighbor_owner.index()].vacated_neighbor = true;
                }
                // Algorithm 3 lines 4-8: visit the port-1 neighbour and return.
                self.metrics.agent_moves(2);
                self.metrics.vacate();
                self.agents[owner.index()].status = P1Status::SettledScout;
            }
            // (V3) a fullyVisited node vacates unless a neighbour is leaning on
            // it, and (V4) a partiallyVisited node always vacates.
            NodeType::FullyVisited if !vacated_neighbor => {
                self.metrics.vacate();
                self.agents[owner.index()].status = P1Status::SettledScout;
            }
            NodeType::PartiallyVisited => {
                self.metrics.vacate();
                self.agents[owner.index()].status = P1Status::SettledScout;
            }
            _ => {}
        }
    }

    /// (V5) Algorithm 3, lines 15-25. When this node's port at the parent is
    /// port 1, the parent may vacate instead, provided nothing already leans on
    /// it.
    fn apply_parent_vacate(&mut self, node: NodeId) {
        let Some((parent, _, _)) = self.parent[node.index()] else {
            return;
        };
        let Some(owner) = self.settled_at[node.index()] else {
            return;
        };
        if self.agents[owner.index()].port_at_parent != Some(PORT_ONE) {
            return;
        }
        // The root never vacates (V1).
        if self.parent[parent.index()].is_none() {
            return;
        }
        let Some(parent_owner) = self.settled_at[parent.index()] else {
            return;
        };
        if self.agents[parent_owner.index()].status != P1Status::Settled
            || self.agents[parent_owner.index()].vacated_neighbor
        {
            return;
        }
        // Algorithm 3 lines 16-21: visit the parent and return.
        self.metrics.agent_moves(2);
        self.metrics.vacate();
        self.agents[parent_owner.index()].status = P1Status::SettledScout;
        self.agents[owner.index()].vacated_neighbor = true;
    }

    /// Section 5.1. A post-order walk of the finished tree that puts every
    /// travelling scout back on the node it owns.
    ///
    /// The scouts are all standing wherever the head finished, so the walk
    /// visits the tree bottom-up and drops each one as its home comes past.
    fn retrace(&mut self) {
        let mut travelling: Vec<usize> = self
            .agents
            .iter()
            .enumerate()
            .filter(|(_, agent)| agent.status == P1Status::SettledScout)
            .map(|(index, _)| index)
            .collect();
        if travelling.is_empty() {
            return;
        }

        // Children of each node, in the order the DFS added them, so the walk
        // below is the same post-order the construction took.
        let mut children: Vec<Vec<NodeId>> = vec![Vec::new(); self.graph.node_count()];
        for index in 0..self.graph.node_count() {
            if let Some((parent, _, _)) = self.parent[index] {
                children[parent.index()].push(NodeId(
                    u32::try_from(index).expect("node count fits the dense ID domain"),
                ));
            }
        }

        let root = self.head;
        let mut order = Vec::new();
        Self::post_order(root, &children, &mut order);

        for node in order {
            self.metrics.logical_rounds(RoundKind::Retrace, 1);
            self.metrics.retrace();
            self.metrics.agent_moves(travelling.len() as u64);
            for &index in &travelling {
                self.agents[index].node = node;
            }
            self.head = node;
            if let Some(owner) = self.settled_at[node.index()] {
                if let Some(position) = travelling.iter().position(|&i| i == owner.index()) {
                    travelling.remove(position);
                    self.agents[owner.index()].status = P1Status::Settled;
                    self.agents[owner.index()].node = node;
                }
            }
            self.checkpoint(Phase::Retrace);
            if travelling.is_empty() {
                break;
            }
        }
    }

    fn post_order(node: NodeId, children: &[Vec<NodeId>], out: &mut Vec<NodeId>) {
        for &child in &children[node.index()] {
            Self::post_order(child, children, out);
        }
        out.push(node);
    }

    /// The agents that travel to perform a neighbourhood search.
    ///
    /// Once every agent has settled there is no travelling group left, and
    /// Section 4 has the node's own settled agent do the search. The head then
    /// moves between nodes as a logical locus: the tree pointers already exist
    /// and each node's agent does its own local work, so no agent has to
    /// abandon the node it is settled at.
    fn probe_party(&self) -> Vec<usize> {
        let travelling: Vec<usize> = self
            .agents
            .iter()
            .enumerate()
            .filter(|(_, agent)| {
                matches!(agent.status, P1Status::Unsettled | P1Status::SettledScout)
            })
            .map(|(index, _)| index)
            .collect();
        if !travelling.is_empty() {
            return travelling;
        }
        // Nothing is travelling: the node's own settled agent probes for itself.
        self.settled_at[self.head.index()]
            .map(|agent| vec![agent.index()])
            .unwrap_or_default()
    }

    fn move_unsettled_to(&mut self, node: NodeId) {
        for agent in &mut self.agents {
            if agent.status == P1Status::Unsettled {
                agent.node = node;
            }
        }
    }

    fn unsettled_count(&self) -> usize {
        self.agents
            .iter()
            .filter(|agent| agent.status == P1Status::Unsettled)
            .count()
    }

    /// Mirrors a node's type into the agent settled there, which is where the
    /// paper keeps it.
    fn sync_node_type(&mut self, node: NodeId) {
        if let Some(agent) = self.settled_at[node.index()] {
            self.agents[agent.index()].node_type = self.node_type[node.index()];
        }
    }

    fn collect_tree(&self) -> Vec<TreeEdge> {
        (0..self.graph.node_count())
            .filter_map(|index| {
                let node = NodeId(u32::try_from(index).ok()?);
                let (parent, child_port, edge_type) = self.parent[index]?;
                Some(TreeEdge {
                    parent,
                    child: node,
                    child_port,
                    edge_type,
                })
            })
            .collect()
    }

    fn checkpoint(&mut self, phase: Phase) {
        if !self.recorder.enabled() {
            self.step = self.step.saturating_add(1);
            return;
        }
        let step = self.step;
        self.recorder
            .record_event(step, SimulationEvent::PhaseStarted { phase });
        self.recorder.record_checkpoint(Checkpoint {
            step,
            agent_nodes: self.agents.iter().map(|agent| agent.node).collect(),
            agent_statuses: self
                .agents
                .iter()
                .map(|agent| match agent.status {
                    P1Status::Settled => AgentStatus::Settled,
                    P1Status::Unsettled => AgentStatus::Unsettled,
                    P1Status::SettledScout => AgentStatus::SettledScout,
                })
                .collect(),
            home_nodes: self.agents.iter().map(|agent| agent.home).collect(),
            settled_agents: self.settled_at.clone(),
        });
        self.step = self.step.saturating_add(1);
    }
}

/// Classifies an incident edge without running a simulation, for callers that
/// want to reason about a graph's port structure directly.
#[must_use]
pub fn edge_type_at(graph: &PortGraph, node: NodeId, port: PortId) -> Option<EdgeType> {
    let PortEdge { remote_port, .. } = graph.traverse(node, port)?;
    Some(EdgeType::of(port, remote_port))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(edges: &[(u32, u32)], nodes: usize) -> PortGraph {
        let pairs: Vec<(NodeId, NodeId)> =
            edges.iter().map(|&(a, b)| (NodeId(a), NodeId(b))).collect();
        PortGraph::from_undirected_edges(nodes, &pairs).unwrap()
    }

    fn rooted(count: usize) -> Vec<NodeId> {
        vec![NodeId(0); count]
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
    fn every_agent_settles_on_a_path() {
        let g = graph(&[(0, 1), (1, 2), (2, 3), (3, 4)], 5);
        let result = run(&g, &rooted(5), 1000).unwrap();
        assert_eq!(result.termination, Termination::Completed);
        let mut homes: Vec<u32> = result
            .agents
            .iter()
            .map(|agent| agent.home.expect("every agent settles").0)
            .collect();
        homes.sort_unstable();
        assert_eq!(homes, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn dispersion_is_one_agent_per_node_across_families() {
        let cases: Vec<(&str, PortGraph, usize)> = vec![
            ("path", graph(&[(0, 1), (1, 2), (2, 3)], 4), 4),
            ("cycle", graph(&[(0, 1), (1, 2), (2, 3), (3, 0)], 4), 4),
            ("star", graph(&[(0, 1), (0, 2), (0, 3)], 4), 4),
            (
                "complete",
                graph(&[(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)], 4),
                4,
            ),
            (
                "tree",
                graph(&[(0, 1), (0, 2), (1, 3), (1, 4), (2, 5)], 6),
                6,
            ),
            (
                "grid",
                graph(&[(0, 1), (1, 2), (3, 4), (4, 5), (0, 3), (1, 4), (2, 5)], 6),
                6,
            ),
        ];
        for (name, g, n) in cases {
            let result = run(&g, &rooted(n), 10_000).unwrap();
            assert_eq!(result.termination, Termination::Completed, "{name}");
            let mut homes: Vec<u32> = result
                .agents
                .iter()
                .map(|agent| agent.home.unwrap_or(NodeId(u32::MAX)).0)
                .collect();
            homes.sort_unstable();
            let expected: Vec<u32> = (0..u32::try_from(n).unwrap()).collect();
            assert_eq!(homes, expected, "{name} did not disperse one per node");
        }
    }

    #[test]
    fn result_satisfies_definition_one() {
        // Definition 1: every vertex of the tree carries an incident tree edge
        // of type tp1, t11 or t1q.
        let cases = vec![
            graph(&[(0, 1), (1, 2), (2, 3), (3, 0)], 4),
            graph(&[(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)], 4),
            graph(&[(0, 1), (1, 2), (2, 3), (3, 4), (4, 0), (0, 2)], 5),
            graph(&[(0, 1), (0, 2), (1, 3), (1, 4), (2, 5), (3, 4)], 6),
        ];
        for (index, g) in cases.into_iter().enumerate() {
            let n = g.node_count();
            let result = run(&g, &rooted(n), 10_000).unwrap();
            assert!(
                result.satisfies_definition_one(),
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
    fn the_tree_spans_the_settled_nodes_without_cycles() {
        let g = graph(&[(0, 1), (1, 2), (2, 3), (3, 0), (0, 2), (1, 3)], 4);
        let result = run(&g, &rooted(4), 10_000).unwrap();
        // n settled nodes and n-1 parent pointers, each child named once, is a
        // spanning tree.
        assert_eq!(result.tree.len(), 3);
        let mut children: Vec<u32> = result.tree.iter().map(|edge| edge.child.0).collect();
        children.sort_unstable();
        children.dedup();
        assert_eq!(children.len(), 3);
        assert!(!children.contains(&0), "the root has no parent");
    }

    #[test]
    fn fewer_agents_than_nodes_still_disperse() {
        let g = graph(&[(0, 1), (1, 2), (2, 3), (3, 4), (4, 5)], 6);
        let result = run(&g, &rooted(3), 10_000).unwrap();
        assert_eq!(result.termination, Termination::Completed);
        let settled = result
            .agents
            .iter()
            .filter(|agent| agent.status == P1Status::Settled)
            .count();
        assert_eq!(settled, 3);
        let mut homes: Vec<u32> = result
            .agents
            .iter()
            .filter_map(|agent| agent.home.map(|home| home.0))
            .collect();
        homes.sort_unstable();
        homes.dedup();
        assert_eq!(homes.len(), 3, "settled agents must occupy distinct nodes");
    }

    #[test]
    fn single_node_and_single_agent_are_immediate() {
        let g = PortGraph::from_undirected_edges(1, &[]).unwrap();
        let result = run(&g, &[NodeId(0)], 10).unwrap();
        assert_eq!(result.termination, Termination::Completed);
        assert_eq!(result.agents[0].home, Some(NodeId(0)));
        assert!(result.tree.is_empty());
    }

    #[test]
    fn the_run_is_deterministic() {
        let g = graph(&[(0, 1), (1, 2), (2, 3), (3, 0), (0, 2)], 4);
        let first = run(&g, &rooted(4), 10_000).unwrap();
        let second = run(&g, &rooted(4), 10_000).unwrap();
        assert_eq!(first, second);
    }

    /// Builds a connected graph with randomly permuted port labels.
    ///
    /// Port labels are the whole subject of this algorithm, so the properties
    /// below are only convincing when tested against many labellings rather
    /// than the canonical sorted-neighbour one.
    fn random_port_graph(nodes: usize, extra_edges: usize, seed: u64) -> PortGraph {
        use ccm_core::{DeterministicRng, PortEdge};
        let mut rng = DeterministicRng::new(seed);
        let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); nodes];
        let connect = |a: usize, b: usize, adjacency: &mut Vec<Vec<usize>>| {
            if a != b && !adjacency[a].contains(&b) {
                adjacency[a].push(b);
                adjacency[b].push(a);
            }
        };
        // A random spanning tree keeps the graph connected.
        for child in 1..nodes {
            let parent = rng.index(child).unwrap_or(0);
            connect(child, parent, &mut adjacency);
        }
        for _ in 0..extra_edges {
            let (Some(a), Some(b)) = (rng.index(nodes), rng.index(nodes)) else {
                continue;
            };
            connect(a, b, &mut adjacency);
        }
        // Permuting each node's neighbour order permutes its port labels.
        for list in &mut adjacency {
            rng.shuffle(list);
        }
        let tables: Vec<Vec<PortEdge>> = (0..nodes)
            .map(|node| {
                adjacency[node]
                    .iter()
                    .map(|&neighbor| {
                        let remote = adjacency[neighbor]
                            .iter()
                            .position(|&back| back == node)
                            .expect("adjacency is symmetric");
                        PortEdge {
                            neighbor: NodeId(u32::try_from(neighbor).unwrap()),
                            remote_port: PortId(u16::try_from(remote).unwrap()),
                        }
                    })
                    .collect()
            })
            .collect();
        PortGraph::from_port_tables(tables).expect("reciprocal tables")
    }

    #[test]
    fn random_port_labellings_always_yield_a_spanning_p1tree() {
        for seed in 0..60_u64 {
            let nodes = 4 + usize::try_from(seed % 9).unwrap();
            let g = random_port_graph(nodes, usize::try_from(seed % 6).unwrap(), seed);
            let n = g.node_count();
            let result = run(&g, &vec![NodeId(0); n], 100_000).unwrap();

            assert_eq!(result.termination, Termination::Completed, "seed {seed}");

            // Dispersion: one agent per node, every node covered.
            let mut homes: Vec<u32> = result
                .agents
                .iter()
                .map(|agent| agent.home.expect("settled").0)
                .collect();
            homes.sort_unstable();
            let expected: Vec<u32> = (0..u32::try_from(n).unwrap()).collect();
            assert_eq!(homes, expected, "seed {seed}");

            // Definition 1, and nothing left waiting.
            assert!(
                !result.has_partially_visited(),
                "seed {seed} finished with a partiallyVisited node"
            );
            assert!(
                result.satisfies_definition_one(),
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
            for start in 0..u32::try_from(n).unwrap() {
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
        // K4 with canonical ports: node 3 is discovered through a tpq edge and
        // has no empty neighbours, so stopping at that point would leave it with
        // only a tpq tree edge. It is the tail of the traversal that repairs it.
        let g = graph(&[(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)], 4);
        let full = run(&g, &rooted(4), 10_000).unwrap();
        assert!(full.satisfies_definition_one());
        assert!(full.dispersed_at_step.is_some());
        assert!(
            full.dispersed_at_step.unwrap() < full.logical_steps,
            "the construction continues after the last agent settles"
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
    fn tracing_does_not_change_the_result() {
        let g = graph(&[(0, 1), (1, 2), (2, 3), (3, 0), (0, 2)], 4);
        let plain = run(&g, &rooted(4), 10_000).unwrap();
        let (traced, _, recorder) = simulate(
            &g,
            &rooted(4),
            10_000,
            ComplexityMetrics::default(),
            ccm_trace::FullTrace::new(),
        )
        .unwrap();
        assert_eq!(plain.agents, traced.agents);
        assert_eq!(plain.tree, traced.tree);
        assert!(!recorder.into_trace().checkpoints.is_empty());
    }

    #[test]
    fn scouts_probe_every_port_in_one_round_when_there_are_enough_of_them() {
        // A star root has degree 5 and no parent port. Five agents remain
        // unsettled after one settles at the root, so a single batch covers
        // every port: two rounds, out and back, not two per port.
        let g = graph(&[(0, 1), (0, 2), (0, 3), (0, 4), (0, 5)], 6);
        let (_, metrics, _) =
            simulate(&g, &rooted(6), 1, ComplexityMetrics::default(), NoTrace).unwrap();
        assert_eq!(metrics.port_probes, 5, "every port is probed");
        assert_eq!(
            metrics.probe_rounds, 2,
            "one batch of scouts covers all five ports"
        );
    }

    #[test]
    fn fewer_scouts_than_ports_repeat_the_assignment() {
        // Same star, but only two scouts remain after one settles at the root,
        // so the five ports take three batches.
        let g = graph(&[(0, 1), (0, 2), (0, 3), (0, 4), (0, 5)], 6);
        let (_, metrics, _) =
            simulate(&g, &rooted(3), 1, ComplexityMetrics::default(), NoTrace).unwrap();
        assert_eq!(metrics.port_probes, 5);
        assert_eq!(
            metrics.probe_rounds, 6,
            "ceil(5 / 2) batches, two rounds each"
        );
    }

    #[test]
    fn the_parent_port_is_not_probed() {
        // Section 4.2 assigns scouts to every port except the parent's: the
        // parent is known to be occupied and is reached by backtracking.
        let g = graph(&[(0, 1), (1, 2), (1, 3)], 4);
        let (_, first, _) =
            simulate(&g, &rooted(4), 1, ComplexityMetrics::default(), NoTrace).unwrap();
        // Round 1 probes the root's single port.
        assert_eq!(first.port_probes, 1);
        let (_, second, _) =
            simulate(&g, &rooted(4), 2, ComplexityMetrics::default(), NoTrace).unwrap();
        // Round 2 is at node 1, degree 3, one of which is the parent.
        assert_eq!(second.port_probes, 1 + 2);
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
        let (result, metrics, _) = simulate(
            &g,
            &rooted(5),
            10_000,
            ComplexityMetrics::default(),
            NoTrace,
        )
        .unwrap();
        assert_eq!(result.termination, Termination::Completed);
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
            let result = run(&g, &vec![NodeId(0); n], 100_000).unwrap();
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
            let mut homes: Vec<u32> = result.agents.iter().map(|a| a.node.0).collect();
            homes.sort_unstable();
            let expected: Vec<u32> = (0..u32::try_from(n).unwrap()).collect();
            assert_eq!(homes, expected, "seed {seed}");
        }
    }

    #[test]
    fn metrics_count_probes_and_settlements() {
        let g = graph(&[(0, 1), (1, 2)], 3);
        let (result, metrics, _) = simulate(
            &g,
            &rooted(3),
            10_000,
            ComplexityMetrics::default(),
            NoTrace,
        )
        .unwrap();
        assert_eq!(result.termination, Termination::Completed);
        assert_eq!(metrics.settlements, 3, "one settlement per node reached");
        assert!(
            metrics.port_probes > 0,
            "the neighbourhood search probes ports"
        );
        assert!(metrics.agent_moves > 0);
    }
}
