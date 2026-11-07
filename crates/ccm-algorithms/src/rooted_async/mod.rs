//! `RootedAsync`: rooted dispersion under asynchrony.
//!
//! Implements Algorithm 5 of Pattanayak, Kshemkalyani, Kumar, Molla and Sharma,
//! *Optimal Dispersion Under Asynchrony* (arXiv:2507.01298), with its parts:
//! `DFS_P1Tree` (Algorithm 2), `Can_Vacate` (Algorithm 3), `Parallel_Probe`
//! (Algorithm 4) and `Retrace` (Algorithm 6).
//!
//! # How this differs from [`crate::p1tree`]
//!
//! `p1tree` builds the same kind of tree as a centralised simulation: it reads
//! a global ownership index to know what a scout would find, and moves the
//! whole group in lockstep. Here every agent is its own process. In each cycle
//! it reads only the agents at its node, writes its own memory and at most one
//! of theirs, and moves along one port (the Communicate-Compute-Move cycle of
//! Section 2). A probe's answer is worked out from rules (R1)-(R3c) and the
//! group's memory. The global state is used only by the simulator to check
//! those answers, never to make them. Any disagreement is reported as
//! [`RootedAsyncError::LocalViewDisagrees`].
//!
//! The schedule is a parameter: synchronous rounds, or asynchronous activation
//! in a seeded random order, with optionally one agent held back. Time is
//! counted in epochs, the shortest intervals in which every agent completes a
//! cycle; under the synchronous schedule an epoch is a round.
//!
//! # Where the paper's pseudocode needed repair
//!
//! Each is marked where it is handled in the code. In brief:
//!
//! - Algorithm 3's `visited` branch always returns, so rule (V5) is
//!   unreachable as printed. It runs after (V2) fails, as the prose has it.
//! - Algorithm 3 never records `P1Neighbor`, which (R3a) depends on. It is
//!   taken from the probe of port 1, or from the parent link when port 1 is
//!   the parent port, and (V5) records it for the parent it vacates.
//! - Algorithm 4 matches against the scouts' temporary fields where (R3a)
//!   names the stored `P1Neighbor`, and lets (R3c-i) end without returning.
//! - Algorithm 5 line 11 would settle a second agent at a vacated node. The
//!   group instead carries what it expects to find there.
//! - Algorithm 5 line 25 overwrites `sibling` on every arrival, erasing it on
//!   a backtrack. It is set once, when a node is settled or reconfigured.
//! - Algorithm 6 keeps its walking context in `a_min`, which changes when
//!   `a_min` reaches home. The context is handed to the next lead.
//! - A reconfigured node is still listed among its old parent's children, so
//!   Retrace would follow it the wrong way. It keeps its old parent link and
//!   is treated as a leaf when reached that way.

mod agent;
mod bits;
mod lead;
mod memory;
mod probe;
mod run;
mod world;

pub use bits::Widths;
pub use memory::{AgentState, AsyncAgent, Memory, Position};
pub use probe::ProbeRule;

use crate::p1tree::TreeEdge;
use ccm_core::{AgentId, Metrics, NodeId, PortGraph, PortId, Termination};
use ccm_trace::Recorder;
use core::fmt;

/// Who is activated when.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Schedule {
    /// Every agent completes one cycle per round, from the round's start state.
    Synchronous,
    /// One agent at a time, chosen uniformly at random from a seed.
    Random { seed: u64 },
    /// As `Random`, but `agent` is activated only every `period` steps: a slow
    /// scout, or a slow lead.
    Starve {
        seed: u64,
        agent: AgentId,
        period: u32,
    },
    /// The paper's ASYNC without atomic cycles (Section 2). Each cycle is two
    /// steps of the seeded random order: a look, which reads the node and
    /// computes from it, and a later commit, which writes and leaves. Any
    /// number of other agents' steps can fall in between, so a commit may act
    /// on a stale view. A commit that would write over memory someone else
    /// has changed since the look fails the run: under the paper's model that
    /// change would be silently lost.
    Interleaved { seed: u64 },
}

/// One probe the head concluded, with the rule that decided it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProbeEvent {
    pub epoch: u64,
    pub head: NodeId,
    pub port: PortId,
    pub rule: ProbeRule,
    /// `psi(y)`, or `None` for an empty neighbour.
    pub owner: Option<AgentId>,
}

/// One step of the DFS head: the group leaving `node`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HeadStep {
    /// Epochs completed when the group left.
    pub epoch: u64,
    pub node: NodeId,
    /// Towards a new or reattached node, rather than back to the parent.
    pub forward: bool,
    /// Probe batches sent from this node before leaving.
    pub batches: u16,
    /// Scouts available for each batch: `|A_scout|`.
    pub scouts: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RootedAsyncResult {
    pub termination: Termination,
    pub agents: Vec<AsyncAgent>,
    /// The tree the settled agents' parent links describe.
    pub tree: Vec<TreeEdge>,
    /// Epochs; rounds under [`Schedule::Synchronous`].
    pub epochs: u64,
    pub activations: u64,
    pub probes: Vec<ProbeEvent>,
    /// Every DFS step, in order.
    pub steps: Vec<HeadStep>,
    /// Epochs completed when the last agent settled and Retrace began.
    pub constructed_at: Option<u64>,
    /// The most bits any agent held between two of its cycles: the quantity
    /// Theorem 2 bounds by `O(log(k + Δ))`. See [`Widths`] for how a field is
    /// charged.
    pub memory_bits: u32,
    /// Under [`Schedule::Interleaved`], the cycles that committed after
    /// something at their node had changed since they looked: the overlap
    /// atomic cycles never have. Zero under the other schedules.
    pub stale_commits: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RootedAsyncError {
    EmptyGraph,
    TooManyAgents {
        agents: usize,
        nodes: usize,
    },
    InvalidStart(NodeId),
    /// The algorithm is rooted: every agent starts at one node.
    NonRootedStart,
    /// A probe's local conclusion differs from the true owner of the node.
    LocalViewDisagrees {
        head: NodeId,
        port: PortId,
        rule: ProbeRule,
        local: Option<AgentId>,
        actual: Option<AgentId>,
    },
    /// The group reached a node expecting a different owner.
    WrongHead {
        node: NodeId,
        expected: AgentId,
        actual: Option<AgentId>,
    },
    NodeClaimedTwice(NodeId),
    SettledAwayFromHome {
        agent: AgentId,
        node: NodeId,
    },
    /// Two writes to one agent in a synchronous round, or, under
    /// [`Schedule::Interleaved`], a write to memory another agent changed
    /// after the writer looked.
    WriteConflict(AgentId),
    /// An epoch passed with no agent changing anything.
    Deadlock {
        epoch: u64,
    },
    Protocol(&'static str),
}

impl fmt::Display for RootedAsyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for RootedAsyncError {}

/// Runs `RootedAsync` from a rooted placement under `schedule`, stopping after
/// `epoch_limit` epochs.
///
/// # Errors
///
/// Rejects an empty graph, more agents than nodes, a start outside the graph,
/// and a placement that is not rooted. Also returns an error whenever a
/// decision made from the local view is caught out by the simulator's check.
pub fn simulate<M: Metrics, R: Recorder>(
    graph: &PortGraph,
    starts: &[NodeId],
    schedule: Schedule,
    epoch_limit: u64,
    metrics: M,
    recorder: R,
) -> Result<(RootedAsyncResult, M, R), RootedAsyncError> {
    if graph.node_count() == 0 {
        return Err(RootedAsyncError::EmptyGraph);
    }
    let Some(&root) = starts.first() else {
        return Err(RootedAsyncError::NonRootedStart);
    };
    if starts.len() > graph.node_count() {
        return Err(RootedAsyncError::TooManyAgents {
            agents: starts.len(),
            nodes: graph.node_count(),
        });
    }
    if let Some(&bad) = starts.iter().find(|n| n.index() >= graph.node_count()) {
        return Err(RootedAsyncError::InvalidStart(bad));
    }
    if starts.iter().any(|&node| node != root) {
        return Err(RootedAsyncError::NonRootedStart);
    }
    run::Run::new(graph, root, starts.len(), metrics, recorder).run(schedule, epoch_limit)
}
