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
//! Settled agents **vacate** (Section 4.1, Algorithm 3) so the head keeps
//! enough scouts after dispersion, and Retrace (Section 5.1) walks them home.
//! It is still not the whole of the paper's `RootedAsync()`: the run uses the
//! repository's synchronous scheduler, not an asynchronous one, so round counts
//! here are not the paper's `O(k)` epoch bound.
//!
//! # Port numbering
//!
//! The paper numbers ports from 1 and gives port 1 its special role. This
//! repository numbers ports from 0, so the paper's "port 1" is [`PORT_ONE`]
//! below, and every edge-type test is written against it rather than against a
//! literal.

use ccm_core::{AgentId, AgentStatus, Metrics, NodeId, PortGraph, PortId, RoundKind, Termination};
use ccm_trace::{Checkpoint, Phase, Recorder, SimulationEvent};
use core::fmt;

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

/// When the construction stops.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stop {
    /// Section 5: "The process continues until no unsettled agents remain."
    /// This is the dispersion algorithm, and the default.
    ///
    /// The tree that exists at that moment spans the settled nodes but is not
    /// promised to be a `P1Tree`: a node parked as `partiallyVisited` may still
    /// be holding a `tpq` parent edge, waiting for a port-1 neighbour that the
    /// run no longer needs to visit.
    AtDispersion,
    /// Keep going until the root is popped, so every node is `fullyVisited` and
    /// Definition 1 holds.
    ///
    /// This is Algorithm 2's own termination, not the dispersion algorithm's.
    /// It exists so the `P1Tree` property can be tested and studied; it costs
    /// more, and on a graph whose degree far exceeds the agent count it costs a
    /// great deal more, because the walk continues with no unsettled agents
    /// left to make a neighbourhood search cheap.
    AtFullTree,
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
    simulate_with_stop(
        graph,
        starts,
        round_limit,
        Stop::AtDispersion,
        metrics,
        recorder,
    )
}

/// As [`simulate`], with an explicit stopping rule.
///
/// # Errors
///
/// As [`simulate`].
pub fn simulate_with_stop<M: Metrics, R: Recorder>(
    graph: &PortGraph,
    starts: &[NodeId],
    round_limit: u64,
    stop: Stop,
    metrics: M,
    recorder: R,
) -> Result<(P1Result, M, R), P1Error> {
    Simulation::new(graph, starts, round_limit, stop, metrics, recorder)?.run()
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
    /// The node the run started from. Retrace walks the tree from here.
    root: NodeId,
    stop: Stop,
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
        stop: Stop,
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
            root,
            stop,
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
            // Section 5: "The process continues until no unsettled agents
            // remain." Once the tree has k vertices the construction is done,
            // and Retrace follows. Walking on past this point to finish the
            // whole spanning tree is not what the algorithm does, and on a graph
            // whose degree far exceeds the agent count that walk is what made
            // the cost scale with n instead of k.
            if self.stop == Stop::AtDispersion && self.dispersed_at_step.is_some() {
                return Ok(Termination::Completed);
            }
            if rounds >= self.round_limit {
                return Ok(Termination::RoundLimitReached {
                    limit: self.round_limit,
                });
            }
            rounds += 1;
            self.metrics.macro_round();

            // With no unsettled agent left there is nobody to settle at an
            // unvisited node, so those neighbours are not candidates. The head
            // can still enter a partiallyVisited node to reconfigure it.
            let can_settle = self.unsettled_count() > 0;
            let results = self.neighbourhood_search(can_settle)?;
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
    /// has left it to travel as a scout. Here the state is read directly, and
    /// the rules only charge the detour walks they require; see
    /// [`Self::probe_detour`].
    fn neighbourhood_search(&mut self, can_settle: bool) -> Result<Vec<ProbeResult>, P1Error> {
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

            // Stop as soon as a batch has turned up somewhere to go. Remark 1
            // of the paper bounds the search at "k-1 ports at the root node and
            // k-2 ports at a non-root node", not at the degree, and the reason
            // is pigeonhole: at most k nodes are ever occupied, so while any
            // agent is still unsettled some probed neighbour must be empty.
            //
            // Scanning every port instead made the cost scale with the degree
            // rather than with the agent count: 40 agents on a 4000-node
            // complete graph took 32357 rounds, where the DFS itself only made
            // 155 visits. The remaining ports are only worth probing when
            // nothing has been found, which is exactly the case that has to
            // rule out an empty neighbour before a node can be declared
            // finished.
            if Self::choose_next_edge(&results, can_settle).is_some() {
                break;
            }
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

        let mut order = Vec::new();
        Self::post_order(self.root, &children, &mut order);

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
