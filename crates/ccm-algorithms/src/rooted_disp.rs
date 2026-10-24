//! `RootedDisp` dispersion: the rooted HEO-DFS of Sudo et al.
//!
//! Implements Section 3 and Algorithm 1 of Sudo, Shibata, Nakamura, Kim and
//! Masuzawa, *Near-linear Time Dispersion of Mobile Agents*
//! (arXiv:2310.04376v3). `docs/rooted-disp.md` gives the time model, the
//! decisions taken where Algorithm 1 leaves a choice, and two worked traces
//! that the tests reproduce step for step. Line numbers below are those
//! printed in Algorithm 1.
//!
//! # How it is simulated
//!
//! Every synchronous time step is one call to [`apply_move_intents`]: all the
//! moves of the step are validated against the state at its start, then
//! committed together. Where each agent stands, its incoming port and the
//! permanent homes live in an [`AgentStore`]; what each agent remembers is its
//! [`RootedMemory`], exactly the variables Lemma 2 counts.
//!
//! Every decision reads only the agents standing on one node. The store's
//! indexes find those agents in O(1) and check invariants; they are never used
//! to learn about a node that no deciding agent is standing on.
//!
//! # Port numbering
//!
//! The paper numbers ports from 0, as this repository does, so a paper port
//! is a [`PortId`] with no offset.

use ccm_core::{
    apply_move_intents, AgentId, AgentState, AgentStatus, AgentStore, AgentStoreError,
    InvariantViolation, LogicalMemory, Metrics, MoveIntent, NodeId, PortGraph, PortId, RoundKind,
    SchedulerError, Termination,
};
use ccm_trace::{Checkpoint, Phase, Recorder, SimulationEvent};
use core::fmt;

/// What an agent remembers: the variables Lemma 2 counts.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RootedMemory {
    /// `a.settled`: false for an explorer, true for a settler, for good.
    pub settled: bool,
    /// `ψ(u).parent`: the port at this settler's home leading to its DFS
    /// parent; `None` at the root.
    pub parent: Option<PortId>,
    /// `ψ(w).next`: the port to an unsettled neighbour found by the last
    /// `Probe()` at this settler's home, or `None`.
    pub next: Option<PortId>,
    /// `ψ(w).checked`: ports `0..=checked` are known to lead to settled
    /// nodes. `None` is the paper's −1.
    pub checked: Option<PortId>,
    /// A helper's port at the probe centre leading back to its home, learnt
    /// from its incoming port on arrival. `None` unless it is helping.
    pub return_port: Option<PortId>,
}

/// Persistent words per agent in the repository's word model: the agent's
/// ID and the five fields of [`RootedMemory`]. Lemma 2 is the statement that
/// this is a constant number of `O(log(k + Δ))`-bit words.
pub const MEMORY_WORDS_PER_AGENT: u64 = 6;

/// An agent's role at the end of the run, or when the run was stopped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum RootedRole {
    /// `settled = ⊥`: travels with the leader's group.
    Explorer,
    /// `settled = ⊤` and standing on its home.
    Settler,
    /// `settled = ⊤` but away from home, helping a `Probe()`.
    Helper,
}

/// One agent in the final state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RootedAgent {
    pub node: NodeId,
    /// The node this agent settled on, which never changes once set.
    pub home: Option<NodeId>,
    pub role: RootedRole,
    pub memory: RootedMemory,
}

/// Counts that belong to this algorithm and have no shared CSV column.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProbeStats {
    /// Calls of `Probe()` (line 3): one per iteration of the main loop.
    pub probe_calls: u64,
    /// Passes of `Probe()`'s while loop (lines 15–21), over all calls.
    pub probe_passes: u64,
    /// The most passes any single `Probe()` needed; Lemma 1 bounds it by
    /// `O(log τ)`.
    pub max_passes_in_one_probe: u64,
    /// Settlers brought to a probe centre (line 17).
    pub helper_recruitments: u64,
    /// Steps in which helpers walked home (line 22).
    pub helper_return_steps: u64,
    /// Moves of the explorer group to an unsettled node (line 7).
    pub forward_moves: u64,
    /// Moves of the explorer group back to the parent (line 11).
    pub backward_moves: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RootedResult {
    pub termination: Termination,
    pub agents: Vec<RootedAgent>,
    /// Synchronous time steps executed: the paper's measure of time.
    pub steps: u64,
    pub stats: ProbeStats,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RootedError {
    EmptyGraph,
    NoAgents,
    TooManyAgents {
        agents: usize,
        nodes: usize,
    },
    InvalidStart(NodeId),
    /// Section 3 places every agent on one node. A scattered placement is the
    /// general setting of Section 4, which this module does not implement.
    NonRootedStart,
    /// The search came back to the root with explorers left and no unsettled
    /// neighbour: the root's component has fewer nodes than there are agents.
    RootExhausted,
    Store(AgentStoreError),
    Scheduler(SchedulerError),
    Invariant(InvariantViolation),
}

impl fmt::Display for RootedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for RootedError {}

impl From<AgentStoreError> for RootedError {
    fn from(value: AgentStoreError) -> Self {
        Self::Store(value)
    }
}

impl From<SchedulerError> for RootedError {
    fn from(value: SchedulerError) -> Self {
        Self::Scheduler(value)
    }
}

/// Runs `RootedDisp` with caller-supplied metrics and recorder.
///
/// `round_limit` bounds the number of time steps. Reaching it returns the
/// partial state as [`Termination::RoundLimitReached`].
///
/// # Errors
///
/// Returns an error for an empty graph, no agents, more agents than nodes, a
/// start outside the graph, a placement that is not rooted at one node, a
/// root whose component is too small to hold every agent, or a broken
/// invariant.
pub fn simulate<M: Metrics, R: Recorder>(
    graph: &PortGraph,
    starts: &[NodeId],
    round_limit: u64,
    metrics: M,
    recorder: R,
) -> Result<(RootedResult, M, R), RootedError> {
    Simulation::new(graph, starts, round_limit, metrics, recorder)?.run()
}

/// Why the run stopped early.
enum Halt {
    Limit,
    Error(RootedError),
}

impl From<RootedError> for Halt {
    fn from(value: RootedError) -> Self {
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

/// The five kinds of time step; each takes exactly one round.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Step {
    ProbeOut,
    ProbeBack,
    HelpersHome,
    Forward,
    Backward,
}

impl Step {
    const fn round(self) -> RoundKind {
        match self {
            Self::ProbeOut => RoundKind::ProbeOut,
            Self::ProbeBack => RoundKind::ProbeBack,
            Self::HelpersHome | Self::Forward | Self::Backward => RoundKind::Movement,
        }
    }

    /// Whether the whole explorer group crosses one edge together.
    const fn is_group_move(self) -> bool {
        matches!(self, Self::Forward | Self::Backward)
    }
}

fn agent_id(index: usize) -> AgentId {
    AgentId(u32::try_from(index).expect("AgentStore bounds the agent count by u32"))
}

fn node_id(index: usize) -> NodeId {
    NodeId(u32::try_from(index).expect("PortGraph bounds the node count by u32"))
}

const fn broken(violation: InvariantViolation) -> RootedError {
    RootedError::Invariant(violation)
}

struct Simulation<'g, M, R> {
    graph: &'g PortGraph,
    store: AgentStore,
    memory: Vec<RootedMemory>,
    root: NodeId,
    /// `a_max`, the largest ID (line 2).
    leader: AgentId,
    /// The unsettled agents, ascending by ID. Between probe steps they all
    /// stand on `group_node`.
    explorers: Vec<AgentId>,
    group_node: NodeId,
    /// Settlers at the current probe centre, in the order they arrived.
    helpers: Vec<AgentId>,
    /// DFS depth of each settled node, for `maximum_tree_depth`.
    depth: Vec<usize>,
    max_depth: usize,
    settled_count: usize,
    round_limit: u64,
    steps: u64,
    stats: ProbeStats,
    metrics: M,
    recorder: R,
    // Scratch buffers reused across steps.
    intents: Vec<MoveIntent>,
    probers: Vec<AgentId>,
    assigned: Vec<(AgentId, PortId)>,
    found: Vec<(AgentId, AgentId)>,
    events: Vec<SimulationEvent>,
}

impl<'g, M: Metrics, R: Recorder> Simulation<'g, M, R> {
    fn new(
        graph: &'g PortGraph,
        starts: &[NodeId],
        round_limit: u64,
        metrics: M,
        recorder: R,
    ) -> Result<Self, RootedError> {
        let nodes = graph.node_count();
        if nodes == 0 {
            return Err(RootedError::EmptyGraph);
        }
        let Some(&root) = starts.first() else {
            return Err(RootedError::NoAgents);
        };
        if starts.len() > nodes {
            return Err(RootedError::TooManyAgents {
                agents: starts.len(),
                nodes,
            });
        }
        if let Some(&outside) = starts.iter().find(|node| node.index() >= nodes) {
            return Err(RootedError::InvalidStart(outside));
        }
        if starts.iter().any(|&node| node != root) {
            return Err(RootedError::NonRootedStart);
        }
        let store = AgentStore::new(nodes, starts)?;
        let count = starts.len();
        Ok(Self {
            graph,
            store,
            memory: vec![RootedMemory::default(); count],
            root,
            leader: agent_id(count - 1),
            explorers: (0..count).map(agent_id).collect(),
            group_node: root,
            helpers: Vec::new(),
            depth: vec![0; nodes],
            max_depth: 0,
            settled_count: 0,
            round_limit,
            steps: 0,
            stats: ProbeStats::default(),
            metrics,
            recorder,
            intents: Vec::with_capacity(count),
            probers: Vec::with_capacity(count),
            assigned: Vec::with_capacity(count),
            found: Vec::with_capacity(count),
            events: Vec::new(),
        })
    }

    fn run(mut self) -> Result<(RootedResult, M, R), RootedError> {
        // Line 1: the smallest ID settles at the root. It takes no step.
        let first = self.explorers.remove(0);
        self.settle(first, self.root, None)?;
        let agents = u64::try_from(self.memory.len()).unwrap_or(u64::MAX);
        self.metrics.observe_memory(LogicalMemory {
            agent_state_words: MEMORY_WORDS_PER_AGENT.saturating_mul(agents),
            ..LogicalMemory::default()
        });
        self.record(Phase::Other);

        let termination = match self.disperse() {
            Ok(()) => {
                self.validate_final()?;
                Termination::Completed
            }
            Err(Halt::Limit) => Termination::RoundLimitReached {
                limit: self.round_limit,
            },
            Err(Halt::Error(error)) => return Err(error),
        };
        let agents = self.agent_views();
        Ok((
            RootedResult {
                termination,
                agents,
                steps: self.steps,
                stats: self.stats,
            },
            self.metrics,
            self.recorder,
        ))
    }

    /// The main loop, lines 2–11.
    fn disperse(&mut self) -> Result<(), Halt> {
        // Line 2: until a_max settles.
        while !self.memory[self.leader.index()].settled {
            let w = self.group_node;
            // Lines 3–5.
            match self.probe(w)? {
                Some(port) => self.forward(w, port)?,
                None => self.backward(w)?,
            }
        }
        Ok(())
    }

    /// `Probe()`, lines 12–22: returns `ψ(w).next`.
    fn probe(&mut self, w: NodeId) -> Result<Option<PortId>, Halt> {
        let psi = self.settler_at(w)?;
        let degree = self.graph.degree(w).unwrap_or(0);
        // Line 14: no cursor survives from an earlier call.
        self.memory[psi.index()].next = None;
        self.memory[psi.index()].checked = None;
        self.stats.probe_calls += 1;

        let mut passes = 0;
        // Line 15: until every port of w has been checked.
        loop {
            let first = self.memory[psi.index()]
                .checked
                .map_or(0, |port| port.0 + 1);
            if first >= degree {
                break;
            }
            passes += 1;
            if let Some(port) = self.probe_pass(w, psi, first, degree)? {
                // Lines 18–19.
                self.memory[psi.index()].next = Some(port);
                break;
            }
        }
        self.stats.probe_passes += passes;
        self.stats.max_passes_in_one_probe = self.stats.max_passes_in_one_probe.max(passes);

        // Line 22.
        self.helpers_home(w)?;
        Ok(self.memory[psi.index()].next)
    }

    /// One pass of `Probe()`'s while loop, lines 16–21. Returns the port of
    /// an unsettled neighbour if a prober found one.
    fn probe_pass(
        &mut self,
        w: NodeId,
        psi: AgentId,
        first: u16,
        degree: u16,
    ) -> Result<Option<PortId>, Halt> {
        // Line 16: every agent at w but ψ(w), in ascending ID (decision 2),
        // takes one of the next Δ′ ports. Recruited helpers probe too.
        self.probers.clear();
        self.probers.extend_from_slice(&self.explorers);
        self.probers.extend_from_slice(&self.helpers);
        self.probers.sort_unstable();
        let count = self.probers.len().min(usize::from(degree - first));
        self.assigned.clear();
        self.intents.clear();
        for (offset, &agent) in self.probers[..count].iter().enumerate() {
            let port = PortId(first + u16::try_from(offset).expect("offset is below the degree"));
            self.assigned.push((agent, port));
            self.intents.push(MoveIntent {
                agent,
                from: w,
                port,
            });
        }
        self.commit(Step::ProbeOut)?;
        for _ in 0..count {
            self.metrics.port_probe();
            self.metrics.probe_out_traversal();
        }
        self.record(Phase::ProbeOut);

        // Line 17: each prober looks at the agents on the node it stands on.
        // A settler at home there comes back with it; finding nobody means
        // the node is unsettled.
        self.found.clear();
        self.intents.clear();
        let mut unsettled: Option<PortId> = None;
        for index in 0..self.assigned.len() {
            let (agent, port) = self.assigned[index];
            let state = *self.state(agent);
            let back = state
                .arrival_port
                .ok_or(broken(InvariantViolation::InvalidTraversal))?;
            self.intents.push(MoveIntent {
                agent,
                from: state.node,
                port: back,
            });
            if let Some(settler) = self.store.settled_agent_at(state.node) {
                self.found.push((agent, settler));
                self.intents.push(MoveIntent {
                    agent: settler,
                    from: state.node,
                    port: back,
                });
            } else {
                // Every settler is home when a Probe() starts, and helpers
                // come only from ports already checked, so an owner that is
                // away here would mean the model was broken.
                if self.store.home_owner(state.node).is_some() {
                    return Err(broken(InvariantViolation::OwnershipIndexMismatch).into());
                }
                // Decision 2: the smallest such port wins.
                unsettled = Some(unsettled.map_or(port, |best: PortId| best.min(port)));
            }
        }
        self.commit(Step::ProbeBack)?;
        for _ in 0..count {
            self.metrics.probe_back_traversal();
        }
        for index in 0..self.found.len() {
            let (_, settler) = self.found[index];
            self.store.vacate_home(settler)?;
            // The helper's way home is the port it just arrived through.
            self.memory[settler.index()].return_port = self.state(settler).arrival_port;
            self.helpers.push(settler);
            self.stats.helper_recruitments += 1;
            if self.recorder.enabled() {
                self.events.push(SimulationEvent::AgentStateChanged {
                    agent: settler,
                    from: AgentStatus::Settled,
                    to: AgentStatus::SettledScout,
                });
            }
        }
        self.record(Phase::ProbeBack);

        if unsettled.is_some() {
            return Ok(unsettled);
        }
        // Line 21: every prober brought a settler back.
        let last = first + u16::try_from(count - 1).expect("count is at most the degree");
        self.memory[psi.index()].checked = Some(PortId(last));
        Ok(None)
    }

    /// Line 22: every helper walks home through the port it remembered.
    /// Decision 1: the step is taken only when there is a helper.
    fn helpers_home(&mut self, w: NodeId) -> Result<(), Halt> {
        if self.helpers.is_empty() {
            return Ok(());
        }
        self.intents.clear();
        for index in 0..self.helpers.len() {
            let helper = self.helpers[index];
            let port = self.memory[helper.index()]
                .return_port
                .ok_or(broken(InvariantViolation::InvalidTraversal))?;
            self.intents.push(MoveIntent {
                agent: helper,
                from: w,
                port,
            });
        }
        self.commit(Step::HelpersHome)?;
        for index in 0..self.helpers.len() {
            let helper = self.helpers[index];
            let state = *self.state(helper);
            if state.home != Some(state.node) {
                return Err(broken(InvariantViolation::InvalidTraversal).into());
            }
            self.store.return_home(helper)?;
            self.memory[helper.index()].return_port = None;
            if self.recorder.enabled() {
                self.events.push(SimulationEvent::AgentStateChanged {
                    agent: helper,
                    from: AgentStatus::SettledScout,
                    to: AgentStatus::Settled,
                });
            }
        }
        self.helpers.clear();
        self.stats.helper_return_steps += 1;
        self.record(Phase::HelperReturn);
        Ok(())
    }

    /// Lines 6–9: the explorers move to the unsettled node `u` through
    /// `port`, and the smallest ID among them settles there with
    /// `parent ← a_max.pin`.
    fn forward(&mut self, w: NodeId, port: PortId) -> Result<(), Halt> {
        let psi_w = self.settler_at(w)?;
        self.move_group(w, port, Step::Forward)?;
        self.stats.forward_moves += 1;
        let u = self.group_node;
        let pin = self.state(self.leader).arrival_port;
        let newcomer = self.explorers.remove(0);
        self.depth[u.index()] = self.depth[w.index()] + 1;
        self.settle(newcomer, u, pin)?;
        if self.recorder.enabled() {
            self.events.push(SimulationEvent::TreeEdgeAdded {
                parent: psi_w,
                child: newcomer,
                parent_node: w,
                child_node: u,
                parent_port: Some(port),
                child_port: pin,
            });
        }
        self.record(Phase::Movement);
        Ok(())
    }

    /// Line 11: the explorers move back to `N(w, ψ(w).parent)`.
    fn backward(&mut self, w: NodeId) -> Result<(), Halt> {
        let psi = self.settler_at(w)?;
        let port = self.memory[psi.index()]
            .parent
            .ok_or(RootedError::RootExhausted)?;
        self.move_group(w, port, Step::Backward)?;
        self.metrics.backtrack();
        self.stats.backward_moves += 1;
        self.record(Phase::Movement);
        Ok(())
    }

    fn move_group(&mut self, w: NodeId, port: PortId, step: Step) -> Result<(), Halt> {
        self.intents.clear();
        for index in 0..self.explorers.len() {
            self.intents.push(MoveIntent {
                agent: self.explorers[index],
                from: w,
                port,
            });
        }
        self.commit(step)?;
        self.metrics.group_move(self.explorers.len());
        self.group_node = self.state(self.leader).node;
        Ok(())
    }

    /// Makes `agent` a settler at `node` for good. Settling is local
    /// computation and takes no step.
    fn settle(
        &mut self,
        agent: AgentId,
        node: NodeId,
        parent: Option<PortId>,
    ) -> Result<(), RootedError> {
        self.store.mark_settled(agent, node)?;
        let memory = &mut self.memory[agent.index()];
        memory.settled = true;
        memory.parent = parent;
        self.settled_count += 1;
        self.max_depth = self.max_depth.max(self.depth[node.index()]);
        self.metrics.settlement();
        if self.recorder.enabled() {
            self.events
                .push(SimulationEvent::AgentSettled { agent, node });
        }
        Ok(())
    }

    /// One synchronous time step: the moves in `self.intents` are validated
    /// and committed together, and the step is charged one round.
    fn commit(&mut self, step: Step) -> Result<(), Halt> {
        if self.steps >= self.round_limit {
            return Err(Halt::Limit);
        }
        apply_move_intents(
            self.graph,
            &mut self.store,
            &mut self.intents,
            &mut self.metrics,
        )?;
        self.steps += 1;
        self.metrics.logical_rounds(step.round(), 1);
        if self.recorder.enabled() {
            if step.is_group_move() {
                if let Some(first) = self.intents.first() {
                    self.events.push(SimulationEvent::GroupMoved {
                        agents: self.intents.iter().map(|intent| intent.agent).collect(),
                        from: first.from,
                        to: self.state(first.agent).node,
                        out_port: first.port,
                    });
                }
            } else {
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
        }
        Ok(())
    }

    /// Records the end of a step: observed maxima, then the step's semantic
    /// events and a checkpoint when a recorder is listening.
    fn record(&mut self, phase: Phase) {
        self.metrics
            .observe_state(self.explorers.len(), self.max_depth, self.settled_count);
        if !self.recorder.enabled() {
            self.events.clear();
            return;
        }
        let step = self.steps;
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
        };
        self.recorder.record_checkpoint(checkpoint);
    }

    fn state(&self, agent: AgentId) -> &AgentState {
        self.store
            .get(agent)
            .expect("every agent ID comes from the store")
    }

    /// `ψ(v)`: the settler standing on `v`, which every node the search
    /// stands on has.
    fn settler_at(&self, node: NodeId) -> Result<AgentId, RootedError> {
        self.store
            .settled_agent_at(node)
            .ok_or(broken(InvariantViolation::OwnershipIndexMismatch))
    }

    /// Definition 1: every agent settled on its own node, none away.
    fn validate_final(&self) -> Result<(), RootedError> {
        if !self.explorers.is_empty() || !self.helpers.is_empty() {
            return Err(broken(InvariantViolation::InvalidAgentNode));
        }
        for index in 0..self.store.len() {
            let agent = agent_id(index);
            let state = self.state(agent);
            if state.status != AgentStatus::Settled
                || state.home != Some(state.node)
                || self.store.settled_agent_at(state.node) != Some(agent)
            {
                return Err(broken(InvariantViolation::OwnershipIndexMismatch));
            }
            let parent = self.memory[index].parent;
            let valid = if state.node == self.root {
                parent.is_none()
            } else {
                parent.is_some_and(|port| self.graph.traverse(state.node, port).is_some())
            };
            if !valid {
                return Err(broken(InvariantViolation::InvalidParent));
            }
        }
        Ok(())
    }

    fn agent_views(&self) -> Vec<RootedAgent> {
        (0..self.store.len())
            .map(|index| {
                let state = self.state(agent_id(index));
                let memory = self.memory[index];
                let role = if !memory.settled {
                    RootedRole::Explorer
                } else if state.status == AgentStatus::SettledScout {
                    RootedRole::Helper
                } else {
                    RootedRole::Settler
                };
                RootedAgent {
                    node: state.node,
                    home: state.home,
                    role,
                    memory,
                }
            })
            .collect()
    }
}
