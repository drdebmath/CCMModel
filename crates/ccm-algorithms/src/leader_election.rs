//! Leader election by agents: the ID-bit schedule.
//!
//! Implements Section 2 (Lemma 4 and Theorem 1) of Kshemkalyani, Kumar, Molla
//! and Sharma, *Agent-based Leader Election, MST, and Beyond*
//! (arXiv:2403.13716). From a dispersed start, one agent per node, the agents
//! simulate synchronous message passing by visiting their neighbours on a
//! schedule set by the bits of their IDs, and every agent learns the smallest
//! ID. `docs/leader-election.md` gives the time model, the agreed decisions
//! and the hand trace that the tests reproduce round by round.
//!
//! # How it is simulated
//!
//! Every round is one commit through [`apply_move_intents`], including the
//! rounds of the schedule in which nobody moves. Where each agent stands, its
//! incoming port and its home live in an [`AgentStore`]. A visitor is vacated
//! from its home while it is away, so the store knows who is at home. What an
//! agent remembers is its [`ElectionMemory`]. The schedule counters (simulated
//! round, block, round in block) are the same for every agent, because all of
//! them start together and count the same rounds, so the simulation keeps one
//! copy of them.
//!
//! A visitor learns who is at home only from the agents on the node it stands
//! on. The store's index finds the agent at home there in O(1); it is never
//! used to learn about a node no deciding agent stands on.
//!
//! # IDs and ports
//!
//! Agent `i` has the paper ID `i + 1`, so IDs are `1 ..= n` and the leader is
//! `AgentId(0)`. IDs are written in [`id_bits`]`(n)` bits and read from the
//! most significant bit. Ports are numbered from 0, as everywhere in this
//! repository.
//!
//! # Rooted mode
//!
//! When every agent starts on one node, [`crate::rooted_disp`] disperses them
//! first and the election starts in the round after it completes. The node an
//! agent settled on is its home.

use crate::rooted_disp::{self, RootedError};
use ccm_core::{
    apply_move_intents, AgentId, AgentOutput, AgentState, AgentStatus, AgentStore, AgentStoreError,
    InvariantViolation, LogicalMemory, Metrics, MoveIntent, NodeId, PortGraph, PortId, RoundKind,
    SchedulerError, Termination,
};
use ccm_trace::{Checkpoint, Phase, Recorder, SimulationEvent};
use core::cmp::Reverse;
use core::fmt;

/// Words per agent in the repository's word model: ID, `n`, `Δ`, `min`,
/// `received`, the three schedule counters and the leader flag. Each is
/// `O(log n)` bits; [`memory_bits_per_agent`] gives the exact count.
pub const MEMORY_WORDS_PER_AGENT: u64 = 9;

/// What an agent remembers besides the schedule counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ElectionMemory {
    /// The paper ID, `AgentId + 1`.
    pub id: u32,
    /// The smallest ID known at the start of the current simulated round: the
    /// message this agent sends in it.
    pub min: u32,
    /// The smallest of `min` and every message received in the current
    /// simulated round. It becomes `min` when the simulated round ends.
    pub received: u32,
    /// Set when the run completes: `min == id`.
    pub is_leader: bool,
}

/// How the run started.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartMode {
    /// One agent per node, each on its home.
    Dispersed,
    /// Every agent on one node; `RootedDisp` dispersed them first.
    Rooted,
}

/// One agent in the final state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ElectionAgent {
    pub node: NodeId,
    /// The node the agent started on, or in rooted mode settled on. `None`
    /// only when the round limit stopped `RootedDisp` before it settled.
    pub home: Option<NodeId>,
    pub memory: ElectionMemory,
    /// `None` until the run completes.
    pub output: Option<AgentOutput>,
}

/// Counts that belong to this algorithm and have no CSV column.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ElectionStats {
    /// `b = ⌊log₂ n⌋ + 1`, the bits of every ID.
    pub id_bits: u32,
    /// `Δ`, which every agent knows.
    pub max_degree: u16,
    /// Simulated message-passing rounds completed.
    pub simulated_rounds: u64,
    /// Blocks of `2Δ` rounds completed.
    pub blocks: u64,
    /// `RootedDisp`'s steps in rooted mode; 0 from a dispersed start.
    pub dispersion_rounds: u64,
    /// Rounds of the election's schedule, idle rounds included.
    pub election_rounds: u64,
    /// Election rounds after which every agent's `min` was the leader's ID,
    /// recorded by the simulator for checking only: no agent can see it.
    pub rounds_until_all_knew: Option<u64>,
    /// Trips out to a neighbour and back.
    pub visits: u64,
    /// Visits that met the visited node's own agent, at home with bit 0.
    pub meetings: u64,
    /// Visits that took no message: the node's own agent was away visiting,
    /// or at home with bit 1 because it had no port for that round.
    pub visits_without_message: u64,
    /// The bits an agent stores, from `docs/leader-election.md`'s table.
    pub memory_bits_per_agent: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ElectionResult {
    pub termination: Termination,
    pub mode: StartMode,
    pub agents: Vec<ElectionAgent>,
    /// All rounds: `RootedDisp`'s steps in rooted mode plus the election's.
    pub rounds: u64,
    pub stats: ElectionStats,
}

impl ElectionResult {
    /// Every agent's output in agent-ID order, once the run has completed.
    #[must_use]
    pub fn outputs(&self) -> Option<Vec<AgentOutput>> {
        self.agents.iter().map(|agent| agent.output).collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ElectionError {
    EmptyGraph,
    NoAgents,
    /// The election needs exactly one agent per node.
    AgentCountMismatch {
        agents: usize,
        nodes: usize,
    },
    InvalidStart(NodeId),
    /// The agents are neither one per node nor all on one node.
    NeitherDispersedNorRooted,
    /// The model assumes a connected graph.
    Disconnected,
    /// `RootedDisp` failed in rooted mode.
    Dispersion(RootedError),
    Store(AgentStoreError),
    Scheduler(SchedulerError),
    Invariant(InvariantViolation),
}

impl fmt::Display for ElectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyGraph => f.write_str("the graph has no nodes"),
            Self::NoAgents => f.write_str("there are no agents"),
            Self::AgentCountMismatch { agents, nodes } => write!(
                f,
                "leader election needs one agent per node: {agents} agents on {nodes} nodes"
            ),
            Self::InvalidStart(node) => write!(f, "start node {} is outside the graph", node.0),
            Self::NeitherDispersedNorRooted => f.write_str(
                "leader election starts dispersed (one agent per node) or rooted (all agents on one node)",
            ),
            Self::Disconnected => f.write_str("the graph is not connected"),
            Self::Dispersion(error) => write!(f, "RootedDisp failed: {error}"),
            Self::Store(error) => write!(f, "{error:?}"),
            Self::Scheduler(error) => write!(f, "{error:?}"),
            Self::Invariant(error) => write!(f, "{error:?}"),
        }
    }
}

impl std::error::Error for ElectionError {}

impl From<AgentStoreError> for ElectionError {
    fn from(value: AgentStoreError) -> Self {
        Self::Store(value)
    }
}

impl From<SchedulerError> for ElectionError {
    fn from(value: SchedulerError) -> Self {
        Self::Scheduler(value)
    }
}

/// `b = ⌊log₂ n⌋ + 1`: the bits of `n`, and so of every ID in `1 ..= n`.
#[must_use]
pub const fn id_bits(n: usize) -> u32 {
    usize::BITS - n.leading_zeros()
}

/// The election's length from a dispersed start: `(n − 1) · b · 2Δ` rounds.
#[must_use]
pub fn schedule_rounds(n: usize, max_degree: u16) -> u64 {
    let simulated = u64::try_from(n.saturating_sub(1)).unwrap_or(u64::MAX);
    simulated
        .saturating_mul(u64::from(id_bits(n)))
        .saturating_mul(2 * u64::from(max_degree))
}

/// Bits needed to write `value`, at least one.
fn width(value: u64) -> u64 {
    u64::from(u64::BITS - value.leading_zeros()).max(1)
}

/// The bits an agent stores, as tabled in `docs/leader-election.md`: ID, `n`,
/// `min` and `received` take `b` bits each; then `Δ`, the simulated-round,
/// block and round-in-block counters, and the leader flag.
#[must_use]
pub fn memory_bits_per_agent(n: usize, max_degree: u16) -> u64 {
    let n = u64::try_from(n).unwrap_or(u64::MAX);
    let delta = u64::from(max_degree);
    let b = width(n);
    4 * b
        + width(delta)
        + width(n.saturating_sub(1))
        + width(b.saturating_sub(1))
        + width((2 * delta).saturating_sub(1))
        + 1
}

/// Runs the election with caller-supplied metrics and recorder.
///
/// A dispersed start runs the election directly; a rooted start runs
/// `RootedDisp` first. `round_limit` bounds all rounds together. Reaching it
/// returns the partial state as [`Termination::RoundLimitReached`], with no
/// outputs.
///
/// # Errors
///
/// Returns an error for an empty graph, no agents, a number of agents other
/// than the number of nodes, a start outside the graph, a placement that is
/// neither dispersed nor rooted, a graph that is not connected, a failed
/// dispersion, or a broken invariant.
pub fn simulate<M: Metrics, R: Recorder>(
    graph: &PortGraph,
    starts: &[NodeId],
    round_limit: u64,
    metrics: M,
    recorder: R,
) -> Result<(ElectionResult, M, R), ElectionError> {
    let nodes = graph.node_count();
    if nodes == 0 {
        return Err(ElectionError::EmptyGraph);
    }
    if starts.is_empty() {
        return Err(ElectionError::NoAgents);
    }
    if starts.len() != nodes {
        return Err(ElectionError::AgentCountMismatch {
            agents: starts.len(),
            nodes,
        });
    }
    if let Some(&outside) = starts.iter().find(|node| node.index() >= nodes) {
        return Err(ElectionError::InvalidStart(outside));
    }
    if !connected(graph) {
        return Err(ElectionError::Disconnected);
    }
    let mut seen = vec![false; nodes];
    let dispersed = starts
        .iter()
        .all(|node| !std::mem::replace(&mut seen[node.index()], true));
    if dispersed {
        return Election::new(
            graph,
            starts,
            StartMode::Dispersed,
            0,
            round_limit,
            metrics,
            recorder,
        )?
        .run();
    }
    if starts.iter().any(|&node| node != starts[0]) {
        return Err(ElectionError::NeitherDispersedNorRooted);
    }

    // Rooted mode: RootedDisp first, then the election from its homes.
    let (dispersal, metrics, recorder) =
        rooted_disp::simulate(graph, starts, round_limit, metrics, recorder)
            .map_err(ElectionError::Dispersion)?;
    if dispersal.termination != Termination::Completed {
        let max_degree = max_degree(graph);
        let agents = dispersal
            .agents
            .iter()
            .enumerate()
            .map(|(index, agent)| {
                let id = paper_id(index);
                ElectionAgent {
                    node: agent.node,
                    home: agent.home,
                    memory: ElectionMemory {
                        id,
                        min: id,
                        received: id,
                        is_leader: false,
                    },
                    output: None,
                }
            })
            .collect();
        let result = ElectionResult {
            termination: dispersal.termination,
            mode: StartMode::Rooted,
            agents,
            rounds: dispersal.steps,
            stats: ElectionStats {
                id_bits: id_bits(nodes),
                max_degree,
                dispersion_rounds: dispersal.steps,
                memory_bits_per_agent: memory_bits_per_agent(nodes, max_degree),
                ..ElectionStats::default()
            },
        };
        return Ok((result, metrics, recorder));
    }
    let homes: Vec<NodeId> = dispersal
        .agents
        .iter()
        .map(|agent| {
            agent
                .home
                .ok_or(ElectionError::Invariant(InvariantViolation::DuplicateHome))
        })
        .collect::<Result<_, _>>()?;
    Election::new(
        graph,
        &homes,
        StartMode::Rooted,
        dispersal.steps,
        round_limit,
        metrics,
        recorder,
    )?
    .run()
}

/// Whether every node can be reached from node 0. A check on the input, made
/// before the run; no agent uses it.
fn connected(graph: &PortGraph) -> bool {
    let nodes = graph.node_count();
    let mut reached = vec![false; nodes];
    let mut queue = vec![NodeId(0)];
    reached[0] = true;
    let mut count = 1;
    while let Some(node) = queue.pop() {
        for (_, edge) in graph.ports(node) {
            if !reached[edge.neighbor.index()] {
                reached[edge.neighbor.index()] = true;
                count += 1;
                queue.push(edge.neighbor);
            }
        }
    }
    count == nodes
}

fn max_degree(graph: &PortGraph) -> u16 {
    (0..graph.node_count())
        .filter_map(|node| graph.degree(node_id(node)))
        .max()
        .unwrap_or(0)
}

fn agent_id(index: usize) -> AgentId {
    AgentId(u32::try_from(index).expect("AgentStore bounds the agent count by u32"))
}

fn node_id(index: usize) -> NodeId {
    NodeId(u32::try_from(index).expect("PortGraph bounds the node count by u32"))
}

fn paper_id(index: usize) -> u32 {
    u32::try_from(index + 1).expect("AgentStore bounds the agent count by u32")
}

const fn broken(violation: InvariantViolation) -> ElectionError {
    ElectionError::Invariant(violation)
}

/// Why the run stopped early.
enum Halt {
    Limit,
    Error(ElectionError),
}

impl From<ElectionError> for Halt {
    fn from(value: ElectionError) -> Self {
        Self::Error(value)
    }
}

impl From<AgentStoreError> for Halt {
    fn from(value: AgentStoreError) -> Self {
        Self::Error(value.into())
    }
}

impl From<SchedulerError> for Halt {
    fn from(value: SchedulerError) -> Self {
        Self::Error(value.into())
    }
}

struct Election<'g, M, R> {
    graph: &'g PortGraph,
    store: AgentStore,
    memory: Vec<ElectionMemory>,
    /// The degree of each agent's home: the ports it visits through.
    degrees: Vec<u16>,
    mode: StartMode,
    bits: u32,
    delta: u16,
    round_limit: u64,
    /// All rounds so far, `RootedDisp`'s included.
    rounds: u64,
    stats: ElectionStats,
    metrics: M,
    recorder: R,
    /// Whether the last committed round moved anyone, so that some agent may
    /// still hold an incoming port.
    pins_set: bool,
    // Scratch buffers reused across rounds.
    intents: Vec<MoveIntent>,
    visitors: Vec<AgentId>,
    events: Vec<SimulationEvent>,
}

impl<'g, M: Metrics, R: Recorder> Election<'g, M, R> {
    fn new(
        graph: &'g PortGraph,
        homes: &[NodeId],
        mode: StartMode,
        rounds: u64,
        round_limit: u64,
        metrics: M,
        recorder: R,
    ) -> Result<Self, ElectionError> {
        let n = homes.len();
        let mut store = AgentStore::new(graph.node_count(), homes)?;
        for (index, &home) in homes.iter().enumerate() {
            store.mark_settled(agent_id(index), home)?;
        }
        let memory = (0..n)
            .map(|index| {
                let id = paper_id(index);
                ElectionMemory {
                    id,
                    min: id,
                    received: id,
                    is_leader: false,
                }
            })
            .collect();
        let degrees = homes
            .iter()
            .map(|&home| graph.degree(home).unwrap_or(0))
            .collect();
        let delta = max_degree(graph);
        Ok(Self {
            graph,
            store,
            memory,
            degrees,
            mode,
            bits: id_bits(n),
            delta,
            round_limit,
            rounds,
            stats: ElectionStats {
                id_bits: id_bits(n),
                max_degree: delta,
                dispersion_rounds: rounds,
                memory_bits_per_agent: memory_bits_per_agent(n, delta),
                ..ElectionStats::default()
            },
            metrics,
            recorder,
            pins_set: false,
            intents: Vec::with_capacity(n),
            visitors: Vec::with_capacity(n),
            events: Vec::new(),
        })
    }

    fn run(mut self) -> Result<(ElectionResult, M, R), ElectionError> {
        let n = self.memory.len();
        let agents = u64::try_from(n).unwrap_or(u64::MAX);
        self.metrics.observe_memory(LogicalMemory {
            agent_state_words: MEMORY_WORDS_PER_AGENT.saturating_mul(agents),
            ..LogicalMemory::default()
        });
        self.metrics.observe_state(0, 0, n);
        if self.everyone_knows() {
            self.stats.rounds_until_all_knew = Some(0);
        }
        let simulated_rounds = n.saturating_sub(1);
        if simulated_rounds == 0 {
            // A single agent: no neighbours and no rounds; it is the leader.
            self.decide();
        }
        if self.mode == StartMode::Dispersed || simulated_rounds == 0 {
            self.record(Phase::Other);
        }

        let termination = match self.elect(simulated_rounds) {
            Ok(()) => {
                self.validate_final()?;
                Termination::Completed
            }
            Err(Halt::Limit) => Termination::RoundLimitReached {
                limit: self.round_limit,
            },
            Err(Halt::Error(error)) => return Err(error),
        };
        let agents = (0..n)
            .map(|index| {
                let state = self.state(agent_id(index));
                let memory = self.memory[index];
                ElectionAgent {
                    node: state.node,
                    home: state.home,
                    memory,
                    output: (termination == Termination::Completed).then(|| output(memory)),
                }
            })
            .collect();
        Ok((
            ElectionResult {
                termination,
                mode: self.mode,
                agents,
                rounds: self.rounds,
                stats: self.stats,
            },
            self.metrics,
            self.recorder,
        ))
    }

    /// The whole schedule: `n − 1` simulated rounds of `b` blocks of `Δ`
    /// out-and-back pairs.
    fn elect(&mut self, simulated_rounds: usize) -> Result<(), Halt> {
        for simulated in 0..simulated_rounds {
            for block in 0..self.bits {
                // Agents whose bit is 1 visit; most ports first, so that the
                // visitors of round j are a prefix of the list.
                self.visitors.clear();
                for index in 0..self.memory.len() {
                    let agent = agent_id(index);
                    if self.bit(agent, block) {
                        self.visitors.push(agent);
                    }
                }
                let degrees = &self.degrees;
                self.visitors
                    .sort_unstable_by_key(|agent| (Reverse(degrees[agent.index()]), *agent));
                for j in 0..self.delta {
                    let count = self
                        .visitors
                        .partition_point(|agent| self.degrees[agent.index()] > j);
                    self.visit_out(PortId(j), count)?;
                    let last = block + 1 == self.bits && j + 1 == self.delta;
                    self.visit_back(block, count, last, simulated + 1 == simulated_rounds)?;
                }
                self.stats.blocks += 1;
            }
        }
        Ok(())
    }

    /// An out round: the first `count` visitors leave home through `port`.
    fn visit_out(&mut self, port: PortId, count: usize) -> Result<(), Halt> {
        self.check_limit()?;
        self.intents.clear();
        for index in 0..count {
            let agent = self.visitors[index];
            let home = self.store.vacate_home(agent)?;
            self.intents.push(MoveIntent {
                agent,
                from: home,
                port,
            });
            if self.recorder.enabled() {
                self.events.push(SimulationEvent::AgentStateChanged {
                    agent,
                    from: AgentStatus::Settled,
                    to: AgentStatus::SettledScout,
                });
            }
        }
        self.commit(RoundKind::ProbeOut)?;
        for _ in 0..count {
            self.metrics.port_probe();
            self.metrics.probe_out_traversal();
        }
        self.stats.visits += u64::try_from(count).unwrap_or(u64::MAX);
        self.record(Phase::VisitOut);
        Ok(())
    }

    /// A back round: each visitor looks at the agents on the node it stands
    /// on, exchanges messages with the one at home whose bit is 0 if there is
    /// one, and goes back through the port it arrived by.
    fn visit_back(
        &mut self,
        block: u32,
        count: usize,
        last_of_simulated_round: bool,
        last_of_run: bool,
    ) -> Result<(), Halt> {
        self.check_limit()?;
        self.intents.clear();
        for index in 0..count {
            let visitor = self.visitors[index];
            let state = *self.state(visitor);
            let node = state.node;
            match self.store.settled_agent_at(node) {
                Some(host) if !self.bit(host, block) => {
                    // The agent whose bit is 0 stays home all block, so it is
                    // the node's own agent. Checked, never assumed.
                    let host_state = self.state(host);
                    if host_state.node != node || host_state.home != Some(node) {
                        return Err(broken(InvariantViolation::InvalidAgentNode).into());
                    }
                    self.exchange(visitor, host);
                    self.stats.meetings += 1;
                    if self.recorder.enabled() {
                        self.events.push(SimulationEvent::MessageExchanged {
                            visitor,
                            host,
                            node,
                        });
                    }
                }
                // Nobody at home, or an agent at home whose bit is 1 because
                // it has no port for this round: no message.
                _ => self.stats.visits_without_message += 1,
            }
            let back = state
                .arrival_port
                .ok_or(broken(InvariantViolation::InvalidTraversal))?;
            self.intents.push(MoveIntent {
                agent: visitor,
                from: node,
                port: back,
            });
        }
        self.commit(RoundKind::ProbeBack)?;
        for index in 0..count {
            let visitor = self.visitors[index];
            let state = *self.state(visitor);
            if state.home != Some(state.node) {
                return Err(broken(InvariantViolation::InvalidTraversal).into());
            }
            self.store.return_home(visitor)?;
            self.metrics.probe_back_traversal();
            if self.recorder.enabled() {
                self.events.push(SimulationEvent::AgentStateChanged {
                    agent: visitor,
                    from: AgentStatus::SettledScout,
                    to: AgentStatus::Settled,
                });
            }
        }
        if last_of_simulated_round {
            self.end_simulated_round();
            if last_of_run {
                self.decide();
            }
        }
        self.record(Phase::VisitBack);
        Ok(())
    }

    /// Both agents send the `min` they held at the start of the simulated
    /// round; what they receive waits in `received` (decision 2).
    fn exchange(&mut self, visitor: AgentId, host: AgentId) {
        let sent_by_visitor = self.memory[visitor.index()].min;
        let sent_by_host = self.memory[host.index()].min;
        let visitor_memory = &mut self.memory[visitor.index()];
        visitor_memory.received = visitor_memory.received.min(sent_by_host);
        let host_memory = &mut self.memory[host.index()];
        host_memory.received = host_memory.received.min(sent_by_visitor);
    }

    /// The end of a simulated round: every agent's `min` takes what it
    /// received. Local computation, no round.
    fn end_simulated_round(&mut self) {
        for index in 0..self.memory.len() {
            let memory = &mut self.memory[index];
            if memory.received < memory.min {
                memory.min = memory.received;
                if self.recorder.enabled() {
                    self.events.push(SimulationEvent::MinimumChanged {
                        agent: agent_id(index),
                        min: AgentId(memory.min - 1),
                    });
                }
            }
        }
        self.stats.simulated_rounds += 1;
        self.metrics.macro_round();
        if self.stats.rounds_until_all_knew.is_none() && self.everyone_knows() {
            self.stats.rounds_until_all_knew = Some(self.stats.election_rounds);
        }
    }

    /// After the last simulated round every agent decides: it is the leader
    /// exactly when its `min` is its own ID.
    fn decide(&mut self) {
        for index in 0..self.memory.len() {
            let memory = &mut self.memory[index];
            memory.is_leader = memory.min == memory.id;
            let decided = output(*memory);
            if self.recorder.enabled() {
                self.events.push(SimulationEvent::OutputDecided {
                    agent: agent_id(index),
                    output: decided,
                });
            }
        }
    }

    /// Bit `block` of `agent`'s ID, counting from the most significant.
    fn bit(&self, agent: AgentId, block: u32) -> bool {
        (self.memory[agent.index()].id >> (self.bits - 1 - block)) & 1 == 1
    }

    /// For the record only: whether every agent's `min` is the smallest ID.
    fn everyone_knows(&self) -> bool {
        self.memory.iter().all(|memory| memory.min == 1)
    }

    fn check_limit(&self) -> Result<(), Halt> {
        if self.rounds >= self.round_limit {
            Err(Halt::Limit)
        } else {
            Ok(())
        }
    }

    /// One synchronous round: the moves in `self.intents`, possibly none, are
    /// validated and committed together, and the round is counted.
    fn commit(&mut self, kind: RoundKind) -> Result<(), Halt> {
        if self.intents.is_empty() {
            // Nobody moves. Incoming ports still become ⊥, as in every round.
            if self.pins_set {
                self.store.begin_step();
                self.pins_set = false;
            }
        } else {
            apply_move_intents(
                self.graph,
                &mut self.store,
                &mut self.intents,
                &mut self.metrics,
            )?;
            self.pins_set = true;
        }
        self.rounds += 1;
        self.stats.election_rounds += 1;
        self.metrics.logical_rounds(kind, 1);
        if self.recorder.enabled() {
            for index in 0..self.intents.len() {
                let intent = self.intents[index];
                let state = *self.state(intent.agent);
                self.events.push(SimulationEvent::AgentMoved {
                    agent: intent.agent,
                    from: intent.from,
                    to: state.node,
                    out_port: intent.port,
                    in_port: state.arrival_port,
                });
            }
        }
        Ok(())
    }

    /// Records the end of a round: its phase and events, then a checkpoint,
    /// when a recorder is listening.
    fn record(&mut self, phase: Phase) {
        if !self.recorder.enabled() {
            self.events.clear();
            return;
        }
        let step = self.rounds;
        self.recorder
            .record_event(step, SimulationEvent::PhaseStarted { phase });
        for event in self.events.drain(..) {
            self.recorder.record_event(step, event);
        }
        let agents = self.store.len();
        let checkpoint = Checkpoint {
            step,
            agent_nodes: (0..agents).map(|i| self.state(agent_id(i)).node).collect(),
            agent_statuses: (0..agents)
                .map(|i| self.state(agent_id(i)).status)
                .collect(),
            home_nodes: (0..agents).map(|i| self.state(agent_id(i)).home).collect(),
            settled_agents: (0..self.graph.node_count())
                .map(|n| self.store.settled_agent_at(node_id(n)))
                .collect(),
            transit_from: Vec::new(),
        };
        self.recorder.record_checkpoint(checkpoint);
    }

    fn state(&self, agent: AgentId) -> &AgentState {
        self.store
            .get(agent)
            .expect("every agent ID comes from the store")
    }

    /// The end of the schedule: every agent is at home, settled.
    fn validate_final(&self) -> Result<(), ElectionError> {
        for index in 0..self.store.len() {
            let agent = agent_id(index);
            let state = self.state(agent);
            if state.status != AgentStatus::Settled
                || state.home != Some(state.node)
                || self.store.settled_agent_at(state.node) != Some(agent)
            {
                return Err(broken(InvariantViolation::OwnershipIndexMismatch));
            }
        }
        Ok(())
    }
}

fn output(memory: ElectionMemory) -> AgentOutput {
    AgentOutput::Leader {
        is_leader: memory.is_leader,
        leader: AgentId(memory.min - 1),
    }
}
