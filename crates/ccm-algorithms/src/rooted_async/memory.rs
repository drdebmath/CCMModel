//! What an agent remembers: the variables of the paper's Table 2, plus the
//! bookkeeping an asynchronous group needs to act together.
//!
//! Everything here is agent memory except [`AsyncAgent::position`] and
//! [`AsyncAgent::home`], which the simulator keeps for itself and no decision
//! ever reads.

use crate::common::{EdgeType, NodeType};
use ccm_core::{AgentId, NodeId, PortId};

/// An agent's state, from Section 4.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentState {
    Unsettled,
    Settled,
    /// Settled at a home node but travelling with the group as a scout.
    SettledScout,
}

/// Where the simulator has an agent. An agent that has left a node but not yet
/// reached the next is on the edge, visible at neither end.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Position {
    At(NodeId),
    InTransit {
        from: NodeId,
        to: NodeId,
        arrival_port: PortId,
    },
}

impl Position {
    /// The node the agent is at, or is heading for.
    #[must_use]
    pub const fn node(self) -> NodeId {
        match self {
            Self::At(node) | Self::InTransit { to: node, .. } => node,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AsyncAgent {
    /// Simulator only.
    pub position: Position,
    /// Simulator only: the node this agent settled at, for checking decisions
    /// against the truth.
    pub home: Option<NodeId>,
    pub memory: Memory,
}

/// An agent ID together with a port: the paper's `parent` and `sibling`.
pub type Link = (AgentId, PortId);

/// One agent's memory. Field names follow Table 2 where it has a name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Memory {
    pub id: AgentId,
    pub state: AgentState,
    pub arrival_port: Option<PortId>,
    /// The last group command this agent has carried out.
    pub seen_seq: u32,

    // Variables of a settled agent, describing its home node.
    pub node_type: NodeType,
    /// Agent at the parent node, and the port there that leads here.
    pub parent: Option<Link>,
    pub parent_port: Option<PortId>,
    /// Type of the parent edge as seen from here; Algorithm 2 line 21.
    pub parent_edge_type: Option<EdgeType>,
    /// Whether some tree edge at this node carries port 1; Algorithm 2 line 21.
    pub port_one_tree_edge: bool,
    pub p1_neighbor: Option<AgentId>,
    pub port_at_p1_neighbor: Option<PortId>,
    pub vacated_neighbor: bool,
    pub recent_child: Option<PortId>,
    pub sibling: Option<Link>,
    pub recent_port: Option<PortId>,
    /// The parent and sibling this node had before a reconfiguration moved it
    /// to a new parent. Retrace needs them to leave the old parent's chain of
    /// children intact; see `lead::retrace_step`.
    pub old_parent: Option<Link>,
    pub old_parent_port: Option<PortId>,
    pub old_sibling: Option<Link>,

    // Work in progress.
    pub task: Task,
    pub scout: ScoutRecord,
    /// Position among the group, by ID, from the last `Rank` command.
    pub rank: u16,
    /// Position among the unsettled agents, highest ID first.
    pub unsettled_rank: u16,
    pub scatter: ScatterTally,
    /// The outcome of this agent's last vacating errand, for the lead to read.
    pub errand: Option<ErrandResult>,
    /// Present on exactly one agent: the one coordinating the group.
    pub lead: Option<Lead>,
}

impl Memory {
    pub(super) fn unsettled(id: AgentId) -> Self {
        Self {
            id,
            state: AgentState::Unsettled,
            arrival_port: None,
            seen_seq: 0,
            node_type: NodeType::Unvisited,
            parent: None,
            parent_port: None,
            parent_edge_type: None,
            port_one_tree_edge: false,
            p1_neighbor: None,
            port_at_p1_neighbor: None,
            vacated_neighbor: false,
            recent_child: None,
            sibling: None,
            recent_port: None,
            old_parent: None,
            old_parent_port: None,
            old_sibling: None,
            task: Task::Idle,
            scout: ScoutRecord::default(),
            rank: 0,
            unsettled_rank: 0,
            scatter: ScatterTally::default(),
            errand: None,
            lead: None,
        }
    }

    /// A member of the travelling group: `A_scout` in the paper.
    #[must_use]
    pub fn in_group(&self) -> bool {
        matches!(self.state, AgentState::Unsettled | AgentState::SettledScout)
    }
}

/// What an agent is doing between cycles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Task {
    Idle,
    /// Walking out along a probe; `hop` is the number of edges taken.
    Probe {
        port: PortId,
        hop: u8,
    },
    /// Walking a probe back; `remaining` edges to go.
    Return {
        remaining: u8,
    },
    /// A settled agent checking a vacating rule at a neighbour (Algorithm 3).
    Errand {
        rule: VacateRule,
        stage: u8,
        /// The group command current when the errand began, which a vacated
        /// agent adopts so it does not replay an older command.
        seq: u32,
    },
    /// Walking an errand home; the way back is in `scout.back`.
    ErrandBack {
        remaining: u8,
        seq: u32,
    },
    /// A settled agent released by rule (V5), walking to join the group.
    Join {
        port: PortId,
    },
    /// An unsettled agent leaving to settle at an empty neighbour
    /// (Algorithm 5, lines 21-24).
    Scatter {
        port: PortId,
        parent: AgentId,
    },
    ScatterArrive {
        parent: AgentId,
        port_at_parent: PortId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VacateRule {
    V2,
    V3,
    V4,
    V5,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrandResult {
    /// The port-1 neighbour is occupied; its agent `p1` is now an anchor.
    Anchored {
        p1: AgentId,
        port_at_p1: PortId,
    },
    /// The port-1 neighbour was unoccupied but its own port-1 neighbour is
    /// occupied, and is now an anchor. Whether the middle node is vacated
    /// rather than empty is settled by the lead, from the group.
    TwoHops {
        anchor: AgentId,
        port_at_anchor: PortId,
        port_at_p1: PortId,
    },
    /// No occupied node within two port-1 hops: the node must stay.
    Refused,
    /// (V5): the parent agreed to vacate and is on its way.
    Joined,
    NotJoined,
}

/// How far a probe walked and what it saw: the scout variables of Table 2.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScoutRecord {
    /// The probe command this record answers.
    pub seq: u32,
    pub port: PortId,
    pub edge_type: EdgeType,
    pub reached: Reached,
    /// `xi(y)` and its node type, when somebody was home at `y`.
    pub found: Option<(AgentId, NodeType)>,
    /// `xi(z)` and `p_zy`, at the port-1 neighbour `z` of `y`.
    pub p1: Option<AgentId>,
    pub port_at_p1: PortId,
    /// `xi(w)` and `p_wz`, at the port-1 neighbour `w` of `z`.
    pub p1p1: Option<AgentId>,
    pub port_at_p1p1: PortId,
    /// The arrival port at each node walked, which is the way back.
    pub back: [PortId; 3],
    /// The scout reached `y` on `y`'s port 1, so `y`'s port-1 neighbour `z`
    /// is the head itself and the walk to `z` was the walk home.
    pub via_head: bool,
}

impl Default for ScoutRecord {
    fn default() -> Self {
        Self {
            seq: 0,
            port: PortId(0),
            edge_type: EdgeType::OtherOther,
            reached: Reached::R1,
            found: None,
            p1: None,
            port_at_p1: PortId(0),
            p1p1: None,
            port_at_p1p1: PortId(0),
            back: [PortId(0); 3],
            via_head: false,
        }
    }
}

/// The rule a probe stopped under (Section 4.2). The `R3a` and `R3c` cases are
/// settled later, at the head, by matching against the group.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reached {
    R1,
    R3a,
    R3b,
    R3c,
}

/// An unsettled agent's running count, during a scatter, of the empty ports
/// seen so far, and the one it will take.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ScatterTally {
    pub seen_empty: u16,
    pub port: Option<PortId>,
}

/// The coordinating agent's context: the paper's lead-scout variables
/// (`prevID`, `childPort`, `siblingDetails`, `childDetails`, `nextAgentID`)
/// and the group's command channel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Lead {
    pub phase: Phase,
    /// Increases with every command; members act on a command once.
    pub seq: u32,
    pub command: Command,
    /// The number of agents, known from the start since all begin together.
    pub k: u32,
    /// `|A_scout|`, the lead included.
    pub group_size: u32,
    pub unsettled: u32,
    /// `psi` of the node the group is at.
    pub head_owner: Option<AgentId>,
    pub prev_id: Option<AgentId>,
    pub child_port: Option<PortId>,
    pub sibling_details: Option<Link>,
    pub child_details: Option<Link>,
    /// The sibling of the child the group last backtracked from, which
    /// Retrace needs if it starts at that parent.
    pub child_sibling: Option<Link>,
    pub target: Target,
    pub skip_port: Option<PortId>,
    pub probe_limit: u16,
    pub batch: u16,
    /// Probe batches sent from the current node, for the step log.
    pub batches_here: u16,
    pub scatter_mode: bool,
    pub best: Option<Candidate>,
    pub plan: Plan,
    pub next_agent_id: Option<AgentId>,
}

/// What the group expects at the node it is moving to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    Empty,
    Owner { id: AgentId, forward: bool },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    Gather,
    Rank,
    Probe { first: u16, started: bool },
    Collect { first: u16 },
    Errand(VacateRule),
    PreMove,
    Depart,
    Scatter,
    RetraceGather,
    RetraceIssue { port: PortId },
    RetraceDepart,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    None,
    Rank,
    Probe { first: u16 },
    Collect { probe_seq: u32 },
    Move { port: PortId },
    Scatter,
}

/// An edge the head may take next.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Candidate {
    pub port: PortId,
    pub edge_type: EdgeType,
    pub owner: Option<AgentId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Plan {
    Forward(Candidate),
    Back,
}
