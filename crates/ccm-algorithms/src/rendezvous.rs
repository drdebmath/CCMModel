//! Rendezvous of two labelled agents in a tree.
//!
//! Implements the simplified tree algorithm of Dessmark, Fraigniaud, Kowalski
//! and Pelc, *Deterministic Rendezvous in Graphs* (Algorithmica 46, 2006), for
//! two agents that start in the same round. `docs/rendezvous.md` gives the
//! time model, the decisions taken where the description leaves a choice, and
//! the hand trace the tests reproduce.
//!
//! # The algorithm
//!
//! 1. Explore the tree by depth-first search and come back to the start node.
//!    This takes `2(n − 1)` rounds, which the agent counts: that count is the
//!    length of a *block*.
//! 2. Write the label in binary, write every bit twice and append `0 1`: the
//!    label's *string* (see [`symbol`]). 5 = `101` becomes `11 00 11 01`.
//! 3. Go through the string one symbol per block: on a 1, explore the tree
//!    again and come back; on a 0, wait at the start node for the whole block.
//! 4. Two different labels give strings that differ before the shorter one
//!    ends, at a position where one string holds 1 and the other 0. In that
//!    block one agent waits at its start node while the other's tour visits
//!    every node, so they meet.
//!
//! # How it is simulated
//!
//! Every round is one call to [`apply_move_intents`]: both agents decide from
//! their own [`Walker`] state and the degree of the node they stand on, then
//! their moves are committed together. The agents meet when they stand on the
//! same node at the end of a round; crossing one edge in opposite directions
//! in the same round is not a meeting, and the simulation never looks inside
//! an edge.
//!
//! The simulator knows the node IDs and checks, at every block boundary, that
//! each agent is back on its start node. No decision reads them.
//!
//! # Exploring
//!
//! A tour is the basic walk: leave the start node by port 0, and leave every
//! node entered by port `p` by port `(p + 1) mod deg`. In a tree this is a
//! depth-first search that crosses each edge once in each direction and ends
//! on the start node after exactly `2(n − 1)` moves.
//!
//! In the first tour the agent does not know `n`, so it cannot count to the
//! end. It keeps the stack of ports leading back up the tree instead, and the
//! tour ends when the stack is empty and the agent has entered the start node
//! by its last port. From then on it knows the block length and only counts;
//! the stack is dropped.

use ccm_core::{
    apply_move_intents, AgentId, AgentState, AgentStatus, AgentStore, AgentStoreError,
    InvariantViolation, LogicalMemory, Metrics, MoveIntent, NodeId, PortGraph, PortId, RoundKind,
    SchedulerError, Termination,
};
use ccm_trace::{Checkpoint, Phase, Recorder, SimulationEvent};
use core::fmt;

/// Number of binary digits of `label`, with no leading zeros. Zero for 0.
#[must_use]
pub const fn label_bits(label: u32) -> u32 {
    u32::BITS - label.leading_zeros()
}

/// Length of the string of `label`: two symbols per bit plus the trailing
/// `0 1`.
#[must_use]
pub const fn string_len(label: u32) -> u32 {
    2 * label_bits(label) + 2
}

/// Symbol `index` (from 0) of the string of `label`, or `None` past its end.
///
/// The string is the binary form of `label`, most significant bit first, with
/// every bit written twice, followed by `0 1`. The agent computes each symbol
/// from its label when the block starts, so it never stores the string.
#[must_use]
pub const fn symbol(label: u32, index: u32) -> Option<bool> {
    let bits = label_bits(label);
    if index < 2 * bits {
        let bit = bits - 1 - index / 2;
        Some((label >> bit) & 1 == 1)
    } else if index == 2 * bits {
        Some(false)
    } else if index == 2 * bits + 1 {
        Some(true)
    } else {
        None
    }
}

/// The whole string of `label`, for display and tests.
#[must_use]
pub fn string(label: u32) -> Vec<bool> {
    (0..string_len(label))
        .map_while(|index| symbol(label, index))
        .collect()
}

/// The string of `label` written as in the paper: pairs of `0`/`1` separated
/// by spaces, so 5 gives `"11 00 11 01"`.
#[must_use]
pub fn string_text(label: u32) -> String {
    let symbols = string(label);
    let mut text = String::with_capacity(symbols.len() * 3 / 2);
    for (index, &one) in symbols.iter().enumerate() {
        if index > 0 && index % 2 == 0 {
            text.push(' ');
        }
        text.push(if one { '1' } else { '0' });
    }
    text
}

/// Rounds in one block on a tree of `nodes` nodes: one depth-first tour.
#[must_use]
pub const fn block_len(nodes: usize) -> u64 {
    2 * (nodes as u64).saturating_sub(1)
}

/// The exact round limit of the task: the first tour plus one block per symbol
/// of the shorter string. The agents meet at or before this round.
#[must_use]
pub fn round_bound(nodes: usize, labels: &[u32]) -> u64 {
    let shortest = labels.iter().copied().map(string_len).min().unwrap_or(0);
    block_len(nodes) * (1 + u64::from(shortest))
}

/// Where an agent is in its schedule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stage {
    /// The first tour, which measures the block length.
    Tour,
    /// Block for symbol `index` of the string (from 0).
    Symbol { index: u32 },
    /// The string has ended: the agent waits at its start node.
    Finished,
}

/// What one agent does in a round.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Activity {
    /// On a tour: the first tour, or a block whose symbol is 1.
    Exploring,
    /// At its start node through a block whose symbol is 0, or after its
    /// string has ended.
    Waiting,
}

/// One agent's memory: everything it decides from, besides the degree of the
/// node it stands on.
///
/// The fields are the agent's own. Gathering reuses this type: an agent that
/// leads a group runs exactly this schedule.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Walker {
    pub label: u32,
    pub stage: Stage,
    /// Rounds in a block, learnt at the end of the first tour.
    pub block_len: Option<u64>,
    /// Rounds spent in the current tour or block.
    pub elapsed: u64,
    /// The port by which the agent last entered a node, kept while it waits.
    pub in_port: Option<PortId>,
    /// First tour only: at each node of the current path below the start, the
    /// port leading back up. Empty means the agent stands on its start node.
    pub up_ports: Vec<PortId>,
    /// The deepest `up_ports` has been, for the memory count.
    pub peak_depth: usize,
}

impl Walker {
    #[must_use]
    pub const fn new(label: u32) -> Self {
        Self {
            label,
            stage: Stage::Tour,
            block_len: None,
            elapsed: 0,
            in_port: None,
            up_ports: Vec::new(),
            peak_depth: 0,
        }
    }

    /// What the agent does in its next round.
    #[must_use]
    pub const fn activity(&self) -> Activity {
        match self.stage {
            Stage::Tour => Activity::Exploring,
            Stage::Symbol { index } => match symbol(self.label, index) {
                Some(true) => Activity::Exploring,
                _ => Activity::Waiting,
            },
            Stage::Finished => Activity::Waiting,
        }
    }

    /// The port the agent leaves by this round, standing on a node of
    /// `degree`, or `None` if it stays. The next port of the basic walk is
    /// the one after the port it came in by; a tour starts at port 0.
    #[must_use]
    pub fn next_port(&self, degree: u16) -> Option<PortId> {
        if self.activity() == Activity::Waiting || degree == 0 {
            return None;
        }
        let port = if self.elapsed == 0 {
            0
        } else {
            self.in_port.map_or(0, |port| (port.0 + 1) % degree)
        };
        Some(PortId(port))
    }

    /// Updates the memory at the end of a round. `left_by` is the port the
    /// agent left by this round, `arrived_by` the port it entered its node by
    /// and `degree` that node's degree; both ports are `None` if it stayed.
    pub fn end_round(&mut self, left_by: Option<PortId>, arrived_by: Option<PortId>, degree: u16) {
        if let Some(port) = arrived_by {
            self.in_port = Some(port);
        }
        self.elapsed += 1;
        match self.stage {
            Stage::Tour => {
                // Going up the edge the stack records, or down a new one.
                if left_by.is_some() && left_by == self.up_ports.last().copied() {
                    self.up_ports.pop();
                } else if let Some(port) = arrived_by {
                    self.up_ports.push(port);
                    self.peak_depth = self.peak_depth.max(self.up_ports.len());
                }
                // Back on the start node, entered by its last port: every
                // port of the start node has been explored.
                let home = self.up_ports.is_empty();
                if home && arrived_by.is_some_and(|port| port.0 + 1 == degree) {
                    self.block_len = Some(self.elapsed);
                    self.up_ports = Vec::new();
                    self.begin(0);
                }
            }
            Stage::Symbol { index } => {
                if Some(self.elapsed) == self.block_len {
                    self.begin(index + 1);
                }
            }
            Stage::Finished => {}
        }
    }

    /// Starts the block of symbol `index`, or finishes past the string's end.
    fn begin(&mut self, index: u32) {
        self.elapsed = 0;
        self.in_port = None;
        self.stage = if index < string_len(self.label) {
            Stage::Symbol { index }
        } else {
            Stage::Finished
        };
    }

    /// Block number of the agent's next round: 0 is the first tour, block `b`
    /// is symbol `b − 1`. `None` once the string has ended.
    #[must_use]
    pub const fn block(&self) -> Option<u32> {
        match self.stage {
            Stage::Tour => Some(0),
            Stage::Symbol { index } => Some(index + 1),
            Stage::Finished => None,
        }
    }

    /// Peak memory in bits, under the count of `docs/rendezvous.md`: the
    /// label, the symbol index, the block length and the round counter, the
    /// last entry port, and one port per level of the first tour's stack.
    #[must_use]
    pub fn memory_bits(&self, nodes: usize, max_degree: u16) -> u64 {
        let width = |value: u64| u64::from(u64::BITS - value.leading_zeros()).max(1);
        let port = width(u64::from(max_degree.saturating_sub(1)));
        let counter = width(block_len(nodes));
        let depth = u64::try_from(self.peak_depth).unwrap_or(u64::MAX);
        width(u64::from(self.label))
            + width(u64::from(string_len(self.label)))
            + 2 * counter
            + port
            + depth.saturating_mul(port)
    }
}

/// Persistent words per agent outside the first tour's stack: the label, the
/// stage, the block length, the round counter and the last entry port.
pub const MEMORY_WORDS_PER_AGENT: u64 = 5;

/// One agent in the final state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RendezvousAgent {
    pub node: NodeId,
    pub start: NodeId,
    pub walker: Walker,
    /// Peak memory in bits; see [`Walker::memory_bits`].
    pub memory_bits: u64,
}

/// Where and when the agents met.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Meeting {
    /// The round at whose end both agents stood on `node`, from 1.
    pub round: u64,
    pub node: NodeId,
    /// The block the round belongs to: 0 is the first tour.
    pub block: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RendezvousResult {
    pub termination: Termination,
    pub agents: Vec<RendezvousAgent>,
    /// `None` only if the run was stopped by the round limit.
    pub meeting: Option<Meeting>,
    /// Rounds executed.
    pub rounds: u64,
    /// Rounds per block: `2(n − 1)`.
    pub block_len: u64,
    /// [`round_bound`] for this tree and these labels.
    pub bound: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RendezvousError {
    EmptyGraph,
    /// Rendezvous is for exactly two agents.
    AgentCount(usize),
    /// One label per agent is needed.
    LabelCount {
        agents: usize,
        labels: usize,
    },
    /// Labels come from `{1, …, L}`.
    ZeroLabel,
    DuplicateLabel(u32),
    InvalidStart(NodeId),
    /// The agents must start on different nodes.
    SameStart(NodeId),
    /// The algorithm is for trees: connected, with `n − 1` edges.
    NotATree,
    /// Both strings ended and the agents never met. On a tree this cannot
    /// happen; it would mean the simulation is wrong.
    NeverMet,
    Store(AgentStoreError),
    Scheduler(SchedulerError),
    Invariant(InvariantViolation),
}

impl fmt::Display for RendezvousError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AgentCount(count) => {
                write!(f, "rendezvous needs exactly two agents, not {count}")
            }
            Self::ZeroLabel => f.write_str("labels start at 1"),
            Self::DuplicateLabel(label) => write!(f, "both agents have label {label}"),
            Self::SameStart(node) => write!(f, "both agents start on node {}", node.0),
            Self::NotATree => f.write_str("rendezvous runs on trees only"),
            other => write!(f, "{other:?}"),
        }
    }
}

impl std::error::Error for RendezvousError {}

impl From<AgentStoreError> for RendezvousError {
    fn from(value: AgentStoreError) -> Self {
        Self::Store(value)
    }
}

impl From<SchedulerError> for RendezvousError {
    fn from(value: SchedulerError) -> Self {
        Self::Scheduler(value)
    }
}

/// Default labels for `agents` agents: 1, 2, ….
#[must_use]
pub fn default_labels(agents: usize) -> Vec<u32> {
    (1..=agents)
        .map(|label| u32::try_from(label).unwrap_or(u32::MAX))
        .collect()
}

/// Whether `graph` is a tree: connected with `n − 1` edges.
#[must_use]
pub fn is_tree(graph: &PortGraph) -> bool {
    let nodes = graph.node_count();
    if nodes == 0 || graph.edge_count() + 1 != nodes {
        return false;
    }
    let mut seen = vec![false; nodes];
    let mut stack = vec![NodeId(0)];
    seen[0] = true;
    let mut reached = 1;
    while let Some(node) = stack.pop() {
        for port in 0..graph.degree(node).unwrap_or(0) {
            if let Some(edge) = graph.traverse(node, PortId(port)) {
                if !seen[edge.neighbor.index()] {
                    seen[edge.neighbor.index()] = true;
                    reached += 1;
                    stack.push(edge.neighbor);
                }
            }
        }
    }
    reached == nodes
}

/// Runs the rendezvous with caller-supplied metrics and recorder.
///
/// `labels[i]` is agent `i`'s label. `round_limit` bounds the rounds; reaching
/// it returns the state so far as [`Termination::RoundLimitReached`].
///
/// # Errors
///
/// Returns an error for an empty graph or a graph that is not a tree, a count
/// of agents other than two, missing, zero or equal labels, starts outside the
/// graph or on one node, or a broken invariant.
pub fn simulate<M: Metrics, R: Recorder>(
    graph: &PortGraph,
    starts: &[NodeId],
    labels: &[u32],
    round_limit: u64,
    metrics: M,
    recorder: R,
) -> Result<(RendezvousResult, M, R), RendezvousError> {
    Simulation::new(graph, starts, labels, round_limit, metrics, recorder)?.run()
}

fn agent_id(index: usize) -> AgentId {
    AgentId(u32::try_from(index).expect("AgentStore bounds the agent count by u32"))
}

const fn broken(violation: InvariantViolation) -> RendezvousError {
    RendezvousError::Invariant(violation)
}

struct Simulation<'g, M, R> {
    graph: &'g PortGraph,
    store: AgentStore,
    starts: Vec<NodeId>,
    walkers: Vec<Walker>,
    max_degree: u16,
    round_limit: u64,
    rounds: u64,
    metrics: M,
    recorder: R,
    intents: Vec<MoveIntent>,
    left_by: Vec<Option<PortId>>,
}

impl<'g, M: Metrics, R: Recorder> Simulation<'g, M, R> {
    fn new(
        graph: &'g PortGraph,
        starts: &[NodeId],
        labels: &[u32],
        round_limit: u64,
        metrics: M,
        recorder: R,
    ) -> Result<Self, RendezvousError> {
        let nodes = graph.node_count();
        if nodes == 0 {
            return Err(RendezvousError::EmptyGraph);
        }
        if starts.len() != 2 {
            return Err(RendezvousError::AgentCount(starts.len()));
        }
        if labels.len() != starts.len() {
            return Err(RendezvousError::LabelCount {
                agents: starts.len(),
                labels: labels.len(),
            });
        }
        if labels.contains(&0) {
            return Err(RendezvousError::ZeroLabel);
        }
        if labels[0] == labels[1] {
            return Err(RendezvousError::DuplicateLabel(labels[0]));
        }
        if let Some(&outside) = starts.iter().find(|node| node.index() >= nodes) {
            return Err(RendezvousError::InvalidStart(outside));
        }
        if starts[0] == starts[1] {
            return Err(RendezvousError::SameStart(starts[0]));
        }
        if !is_tree(graph) {
            return Err(RendezvousError::NotATree);
        }
        let max_degree = (0..nodes)
            .filter_map(|node| graph.degree(NodeId(u32::try_from(node).ok()?)))
            .max()
            .unwrap_or(0);
        Ok(Self {
            graph,
            store: AgentStore::new(nodes, starts)?,
            starts: starts.to_vec(),
            walkers: labels.iter().copied().map(Walker::new).collect(),
            max_degree,
            round_limit,
            rounds: 0,
            metrics,
            recorder,
            intents: Vec::with_capacity(2),
            left_by: vec![None; 2],
        })
    }

    fn run(mut self) -> Result<(RendezvousResult, M, R), RendezvousError> {
        self.observe_memory();
        self.record(Vec::new());
        let mut meeting = None;
        let termination = loop {
            if self.walkers.iter().all(|w| w.stage == Stage::Finished) {
                return Err(RendezvousError::NeverMet);
            }
            if self.rounds >= self.round_limit {
                break Termination::RoundLimitReached {
                    limit: self.round_limit,
                };
            }
            self.round()?;
            let first = self.node(0);
            if first == self.node(1) {
                let block = (self.rounds - 1) / block_len(self.graph.node_count());
                meeting = Some(Meeting {
                    round: self.rounds,
                    node: first,
                    block,
                });
                break Termination::Completed;
            }
        };
        let nodes = self.graph.node_count();
        let agents = (0..self.walkers.len())
            .map(|index| RendezvousAgent {
                node: self.node(index),
                start: self.starts[index],
                memory_bits: self.walkers[index].memory_bits(nodes, self.max_degree),
                walker: self.walkers[index].clone(),
            })
            .collect();
        let labels: Vec<u32> = self.walkers.iter().map(|w| w.label).collect();
        Ok((
            RendezvousResult {
                termination,
                agents,
                meeting,
                rounds: self.rounds,
                block_len: block_len(nodes),
                bound: round_bound(nodes, &labels),
            },
            self.metrics,
            self.recorder,
        ))
    }

    /// One synchronous round: each agent decides from its own memory and the
    /// degree of its node, then the moves are committed together.
    fn round(&mut self) -> Result<(), RendezvousError> {
        self.intents.clear();
        for index in 0..self.walkers.len() {
            let node = self.node(index);
            let degree = self.graph.degree(node).unwrap_or(0);
            let port = self.walkers[index].next_port(degree);
            self.left_by[index] = port;
            if let Some(port) = port {
                self.intents.push(MoveIntent {
                    agent: agent_id(index),
                    from: node,
                    port,
                });
            }
        }
        let mut moves = Vec::new();
        if self.recorder.enabled() {
            moves.extend(self.intents.iter().copied());
        }
        apply_move_intents(
            self.graph,
            &mut self.store,
            &mut self.intents,
            &mut self.metrics,
        )?;
        self.rounds += 1;
        self.metrics.logical_rounds(RoundKind::Movement, 1);
        for index in 0..self.walkers.len() {
            let state = self.state(index);
            let (node, arrived_by) = (state.node, state.arrival_port);
            let degree = self.graph.degree(node).unwrap_or(0);
            let left_by = self.left_by[index];
            let was = self.walkers[index].stage;
            self.walkers[index].end_round(left_by, arrived_by, degree);
            // A block ends with the agent on its start node: checked with the
            // simulator's node IDs, never used by the agent.
            if was != self.walkers[index].stage && node != self.starts[index] {
                return Err(broken(InvariantViolation::InvalidTraversal));
            }
        }
        self.observe_memory();
        self.record(moves);
        Ok(())
    }

    fn observe_memory(&mut self) {
        let agents = u64::try_from(self.walkers.len()).unwrap_or(u64::MAX);
        let stack: usize = self.walkers.iter().map(|w| w.up_ports.len()).sum();
        self.metrics.observe_memory(LogicalMemory {
            agent_state_words: MEMORY_WORDS_PER_AGENT * agents,
            tree_words: u64::try_from(stack).unwrap_or(u64::MAX),
            ..LogicalMemory::default()
        });
        let depth = self.walkers.iter().map(|w| w.up_ports.len()).max();
        self.metrics.observe_state(0, depth.unwrap_or(0), 0);
    }

    /// Records the round's moves and a checkpoint when a recorder listens.
    fn record(&mut self, moves: Vec<MoveIntent>) {
        if !self.recorder.enabled() {
            return;
        }
        let step = self.rounds;
        self.recorder.record_event(
            step,
            SimulationEvent::PhaseStarted {
                phase: Phase::Movement,
            },
        );
        for intent in moves {
            let state = *self.state(intent.agent.index());
            self.recorder.record_event(
                step,
                SimulationEvent::AgentMoved {
                    agent: intent.agent,
                    from: intent.from,
                    to: state.node,
                    out_port: intent.port,
                    in_port: state.arrival_port,
                },
            );
        }
        let count = self.store.len();
        self.recorder.record_checkpoint(Checkpoint {
            step,
            agent_nodes: (0..count).map(|index| self.node(index)).collect(),
            // What each agent did this round; at step 0, what it is about to
            // do.
            agent_statuses: (0..count)
                .map(|index| {
                    let exploring = if step == 0 {
                        self.walkers[index].activity() == Activity::Exploring
                    } else {
                        self.left_by[index].is_some()
                    };
                    if exploring {
                        AgentStatus::Unsettled
                    } else {
                        AgentStatus::SettledWaiting
                    }
                })
                .collect(),
            home_nodes: self.starts.iter().copied().map(Some).collect(),
            settled_agents: Vec::new(),
            transit_from: Vec::new(),
        });
    }

    fn state(&self, index: usize) -> &AgentState {
        self.store
            .get(agent_id(index))
            .expect("every agent index comes from the store")
    }

    fn node(&self, index: usize) -> NodeId {
        self.state(index).node
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_double_every_bit_and_end_with_zero_one() {
        assert_eq!(string_text(5), "11 00 11 01");
        assert_eq!(string_text(1), "11 01");
        assert_eq!(string_text(3), "11 11 01");
        assert_eq!(string_len(5), 8);
        assert_eq!(symbol(1, 4), None);
    }
}
