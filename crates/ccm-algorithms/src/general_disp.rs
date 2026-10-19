//! `GeneralDisp`: the general HEO-DFS of Sudo et al., *Near-linear Time
//! Dispersion of Mobile Agents* (arXiv:2310.04376v3), Section 4 and
//! Algorithms 2–5. Each group of co-located agents runs its own HEO-DFS in
//! units of twelve one-step slots, and groups merge through the Zombie method.
//! The slot schedule and design decisions are in `docs/general-disp.md`.

use ccm_core::{
    apply_move_intents, AgentId, AgentState, AgentStatus, AgentStore, AgentStoreError,
    InvariantViolation, LogicalMemory, Metrics, MoveIntent, NodeId, OccupancyIndex, PortGraph,
    PortId, RoundKind, SchedulerError, Termination,
};
use ccm_trace::{Checkpoint, Phase, Recorder, SimulationEvent};
use core::fmt;

/// Time steps per unit (Section 4.2).
pub const SLOTS_PER_UNIT: u64 = 12;

/// The slot, 1–12, that time step `step` belongs to.
#[must_use]
pub fn slot_of(step: u64) -> u8 {
    u8::try_from(step % SLOTS_PER_UNIT).unwrap_or(0) + 1
}

/// A group label, ordered by level, then by leader ID.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Group {
    pub level: u32,
    pub leader: AgentId,
}

/// What an agent remembers: the variables of Algorithms 2–5.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeneralMemory {
    /// `a.settled`.
    pub settled: bool,
    /// `a.leader`: the leader of the agent's group.
    pub leader: AgentId,
    /// `a.level`.
    pub level: u32,
    /// `z.levelL`, a zombie's location level.
    pub level_location: u32,
    /// `z.levelS`, a zombie's swarm level.
    pub level_swarm: u32,
    /// `a.parent` or `ψ(u).parent`: the port back to the DFS parent.
    pub parent: Option<PortId>,
    /// `a.InitProbe`.
    pub init_probe: bool,
    /// `ψ(w).next`.
    pub next: Option<PortId>,
    /// `ψ(w).checked`; `None` is the paper's −1.
    pub checked: Option<PortId>,
    /// `s.help`: the port from home to the centre it helps.
    pub help: Option<PortId>,
    /// `ψ(w).done`.
    pub done: bool,
    /// `a.found`, set by a prober in slot 7.
    pub found: bool,
    /// A helper's port at the centre leading back home.
    pub return_port: Option<PortId>,
}

impl GeneralMemory {
    const fn initial(id: AgentId) -> Self {
        Self {
            settled: false,
            leader: id,
            level: 1,
            level_location: 1,
            level_swarm: 1,
            parent: None,
            init_probe: true,
            next: None,
            checked: None,
            help: None,
            done: false,
            found: false,
            return_port: None,
        }
    }

    /// The agent's group identifier, meaningful for leaders and settlers.
    #[must_use]
    pub const fn group(&self) -> Group {
        Group {
            level: self.level,
            leader: self.leader,
        }
    }
}

/// Persistent words per agent: its ID plus the fields of [`GeneralMemory`].
pub const MEMORY_WORDS_PER_AGENT: u64 = 14;

/// An agent's role at the end of the run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeneralRole {
    /// A leader sharing its node, running its HEO-DFS.
    ActiveLeader,
    /// A leader alone on its node.
    WaitingLeader,
    Zombie,
    /// A settler on its home.
    Settler,
    /// A settler away from home, helping a probe.
    Helper,
}

/// One agent in the final state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeneralAgent {
    pub node: NodeId,
    pub home: Option<NodeId>,
    pub role: GeneralRole,
    pub memory: GeneralMemory,
}

/// Role transitions and merges, recorded only when a recorder is listening.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GroupEvent {
    /// A leader met a stronger leader or settler.
    BecameZombie {
        step: u64,
        agent: AgentId,
        node: NodeId,
        stronger: Group,
    },
    /// A zombie settled into `group`.
    Settled {
        step: u64,
        agent: AgentId,
        node: NodeId,
        group: Group,
    },
    /// A weaker group's settler joined a stronger group.
    Incorporated {
        step: u64,
        settler: AgentId,
        node: NodeId,
        from: Group,
        to: Group,
    },
    /// A leader met a zombie of its own level.
    LevelUp {
        step: u64,
        leader: AgentId,
        zombie: AgentId,
        node: NodeId,
        level: u32,
    },
    /// A leader moved through the port its probe found.
    ForwardMove {
        step: u64,
        leader: AgentId,
        from: NodeId,
        to: NodeId,
    },
    /// A leader moved back to its parent.
    BackwardMove {
        step: u64,
        leader: AgentId,
        from: NodeId,
        to: NodeId,
    },
    /// A zombie's location or swarm level changed.
    ZombieLevels {
        step: u64,
        zombie: AgentId,
        location: u32,
        swarm: u32,
    },
    /// A zombie moved to chase a leader.
    Chased {
        step: u64,
        zombie: AgentId,
        from: NodeId,
        to: NodeId,
        weak: bool,
    },
}

/// Counts that belong to this algorithm and have no shared CSV column.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GeneralStats {
    pub units: u64,
    pub zombifications: u64,
    pub incorporations: u64,
    pub level_ups: u64,
    /// The highest level reached; Lemma 3 bounds it by `log2 k + 1`.
    pub max_level: u32,
    pub probe_inits: u64,
    pub probe_passes: u64,
    pub helper_recruitments: u64,
    /// Helpers of a weaker group sent home in slot 4.
    pub helpers_dismissed: u64,
    /// Recruited helpers that another group took over before they arrived.
    pub stolen_helpers: u64,
    pub forward_moves: u64,
    pub backward_moves: u64,
    pub weak_chase_moves: u64,
    pub strong_chase_moves: u64,
    /// Zombies that should have chased but had no port to follow.
    pub stuck_chases: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneralResult {
    pub termination: Termination,
    pub agents: Vec<GeneralAgent>,
    pub steps: u64,
    pub stats: GeneralStats,
    pub events: Vec<GroupEvent>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GeneralError {
    EmptyGraph,
    NoAgents,
    TooManyAgents {
        agents: usize,
        nodes: usize,
    },
    InvalidStart(NodeId),
    /// A group's component has fewer nodes than agents.
    Exhausted {
        leader: AgentId,
        node: NodeId,
    },
    Store(AgentStoreError),
    Scheduler(SchedulerError),
    /// A model invariant broke.
    Invariant {
        violation: InvariantViolation,
        step: u64,
        detail: &'static str,
    },
}

impl fmt::Display for GeneralError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for GeneralError {}

impl From<AgentStoreError> for GeneralError {
    fn from(value: AgentStoreError) -> Self {
        Self::Store(value)
    }
}

impl From<SchedulerError> for GeneralError {
    fn from(value: SchedulerError) -> Self {
        Self::Scheduler(value)
    }
}

/// Runs `GeneralDisp` with agent `i` starting on `starts[i]`, for at most
/// `round_limit` time steps.
///
/// # Errors
///
/// Returns an error for invalid input or a broken invariant.
pub fn simulate<M: Metrics, R: Recorder>(
    graph: &PortGraph,
    starts: &[NodeId],
    round_limit: u64,
    metrics: M,
    recorder: R,
) -> Result<(GeneralResult, M, R), GeneralError> {
    Simulation::new(graph, starts, round_limit, metrics, recorder)?.run()
}

fn agent_id(index: usize) -> AgentId {
    AgentId(u32::try_from(index).expect("AgentStore bounds the agent count by u32"))
}

fn node_id(index: usize) -> NodeId {
    NodeId(u32::try_from(index).expect("PortGraph bounds the node count by u32"))
}

/// A leader probing at its node this unit.
#[derive(Clone, Copy, Debug)]
struct Centre {
    leader: AgentId,
    node: NodeId,
    /// `ψ(w)`.
    settler: AgentId,
    finishing: bool,
    /// This pass probes ports `first..first + count`.
    first: u16,
    count: u16,
}

/// One prober's trip in slots 6–7.
#[derive(Clone, Copy, Debug)]
struct Trip {
    prober: AgentId,
    centre: usize,
    port: PortId,
}

struct Simulation<'g, M, R> {
    graph: &'g PortGraph,
    store: AgentStore,
    memory: Vec<GeneralMemory>,
    round_limit: u64,
    steps: u64,
    stats: GeneralStats,
    metrics: M,
    recorder: R,
    centres: Vec<Centre>,
    trips: Vec<Trip>,
    /// `(ψ(w), port)` of each helper recruited in slot 7.
    recruited: Vec<(AgentId, PortId)>,
    moved: Vec<AgentId>,
    homecoming: Vec<AgentId>,
    outgoing: Vec<AgentId>,
    weak: Vec<bool>,
    intents: Vec<MoveIntent>,
    events: Vec<SimulationEvent>,
    group_events: Vec<GroupEvent>,
}

impl<'g, M: Metrics, R: Recorder> Simulation<'g, M, R> {
    fn new(
        graph: &'g PortGraph,
        starts: &[NodeId],
        round_limit: u64,
        metrics: M,
        recorder: R,
    ) -> Result<Self, GeneralError> {
        let nodes = graph.node_count();
        if nodes == 0 {
            return Err(GeneralError::EmptyGraph);
        }
        if starts.is_empty() {
            return Err(GeneralError::NoAgents);
        }
        if starts.len() > nodes {
            return Err(GeneralError::TooManyAgents {
                agents: starts.len(),
                nodes,
            });
        }
        if let Some(&outside) = starts.iter().find(|node| node.index() >= nodes) {
            return Err(GeneralError::InvalidStart(outside));
        }
        let store = AgentStore::new(nodes, starts)?;
        let count = starts.len();
        Ok(Self {
            graph,
            store,
            memory: (0..count)
                .map(|i| GeneralMemory::initial(agent_id(i)))
                .collect(),
            round_limit,
            steps: 0,
            stats: GeneralStats {
                max_level: 1,
                ..GeneralStats::default()
            },
            metrics,
            recorder,
            centres: Vec::new(),
            trips: Vec::new(),
            recruited: Vec::new(),
            moved: Vec::new(),
            homecoming: Vec::new(),
            outgoing: Vec::new(),
            weak: vec![false; count],
            intents: Vec::with_capacity(count),
            events: Vec::new(),
            group_events: Vec::new(),
        })
    }

    fn run(mut self) -> Result<(GeneralResult, M, R), GeneralError> {
        let agents = u64::try_from(self.memory.len()).unwrap_or(u64::MAX);
        self.metrics.observe_memory(LogicalMemory {
            agent_state_words: MEMORY_WORDS_PER_AGENT.saturating_mul(agents),
            ..LogicalMemory::default()
        });
        self.record(Phase::Other);

        let termination = loop {
            if self.legitimate() {
                break Termination::Completed;
            }
            if self.steps >= self.round_limit {
                break Termination::RoundLimitReached {
                    limit: self.round_limit,
                };
            }
            self.step()?;
        };
        let agents = self.agent_views();
        Ok((
            GeneralResult {
                termination,
                agents,
                steps: self.steps,
                stats: self.stats,
                events: self.group_events,
            },
            self.metrics,
            self.recorder,
        ))
    }

    /// One time step: the slot's decisions, then its moves.
    fn step(&mut self) -> Result<(), GeneralError> {
        let slot = slot_of(self.steps);
        if slot == 1 {
            self.stats.units += 1;
            self.metrics.macro_round();
        }
        self.intents.clear();
        self.homecoming.clear();
        self.outgoing.clear();
        let occupancy = self.store.occupancy();
        let (phase, round) = match slot {
            1 => {
                self.slot1_election(&occupancy);
                (Phase::Other, RoundKind::Other)
            }
            2 => {
                self.slot2_settle(&occupancy)?;
                (Phase::Other, RoundKind::Other)
            }
            3 => {
                self.slot3_helpers_go()?;
                (Phase::Scout, RoundKind::Scout)
            }
            4 => {
                self.slot4_check(&occupancy);
                (Phase::HelperReturn, RoundKind::Movement)
            }
            5 => {
                self.slot5_finish(&occupancy);
                (Phase::HelperReturn, RoundKind::Movement)
            }
            6 => {
                self.slot6_probe_out(&occupancy);
                (Phase::ProbeOut, RoundKind::ProbeOut)
            }
            7 => {
                self.slot7_probe_back()?;
                (Phase::ProbeBack, RoundKind::ProbeBack)
            }
            8 => {
                self.slot8_tally(&occupancy);
                (Phase::HelperReturn, RoundKind::Movement)
            }
            9 | 10 => {
                self.chase(slot == 9, &occupancy);
                (Phase::Chase, RoundKind::Chase)
            }
            11 => {
                self.slot11_move(&occupancy)?;
                (Phase::Movement, RoundKind::Movement)
            }
            _ => {
                self.slot12_parent();
                (Phase::Other, RoundKind::Other)
            }
        };
        self.commit(round)?;
        self.settle_movers()?;
        self.check(slot)?;
        self.record(phase);
        Ok(())
    }

    /// Slot 1 (lines 3–6): leaders meeting a stronger group become zombies.
    fn slot1_election(&mut self, occupancy: &OccupancyIndex) {
        let mut demoted = Vec::new();
        for agent in self.leaders() {
            let w = self.state(agent).node;
            let own = self.memory[agent.index()].group();
            let strongest = occupancy
                .agents_at(w)
                .iter()
                .filter(|&&b| self.is_leader(b) || self.memory[b.index()].settled)
                .map(|&b| self.memory[b.index()].group())
                .max();
            if let Some(stronger) = strongest.filter(|group| own < *group) {
                demoted.push((agent, w, stronger));
            }
        }
        for (agent, node, stronger) in demoted {
            let memory = &mut self.memory[agent.index()];
            memory.level_location = memory.level;
            memory.level_swarm = memory.level;
            memory.leader = stronger.leader;
            self.stats.zombifications += 1;
            self.note(GroupEvent::BecameZombie {
                step: self.steps,
                agent,
                node,
                stronger,
            });
        }
    }

    /// Slot 2 (lines 8–21): settle a zombie or incorporate the settler, level
    /// up, and set up the probe.
    fn slot2_settle(&mut self, occupancy: &OccupancyIndex) -> Result<(), GeneralError> {
        self.centres.clear();
        for leader in self.leaders() {
            let w = self.state(leader).node;
            let here = occupancy.agents_at(w);
            if here.len() == 1 {
                continue;
            }
            let mut zombies: Vec<AgentId> = here
                .iter()
                .copied()
                .filter(|&b| self.is_zombie(b))
                .collect();
            zombies.sort_unstable();
            let mut newcomer = None;
            let settler = if let Some(settler) = self.store.settled_agent_at(w) {
                let from = self.memory[settler.index()].group();
                if from < self.memory[leader.index()].group() {
                    self.memory[settler.index()].parent = self.memory[leader.index()].parent;
                    self.stats.incorporations += 1;
                    let to = self.memory[leader.index()].group();
                    self.note(GroupEvent::Incorporated {
                        step: self.steps,
                        settler,
                        node: w,
                        from,
                        to,
                    });
                }
                settler
            } else {
                if zombies.is_empty() {
                    return Err(self.broken(
                        InvariantViolation::InvalidAgentNode,
                        "an unsettled node holds a leader and no zombie",
                    ));
                }
                let zombie = zombies.remove(0);
                self.settle(zombie, w, self.memory[leader.index()].parent)?;
                newcomer = Some(zombie);
                zombie
            };
            let level = self.memory[leader.index()].level;
            if let Some(&zombie) = zombies
                .iter()
                .find(|&&z| self.memory[z.index()].level == level)
            {
                self.memory[leader.index()].level = level + 1;
                self.memory[zombie.index()].level = 0;
                self.memory[settler.index()].parent = None;
                self.memory[leader.index()].init_probe = true;
                self.stats.level_ups += 1;
                self.stats.max_level = self.stats.max_level.max(level + 1);
                self.note(GroupEvent::LevelUp {
                    step: self.steps,
                    leader,
                    zombie,
                    node: w,
                    level: level + 1,
                });
            }
            let group = self.memory[leader.index()].group();
            self.memory[settler.index()].leader = group.leader;
            self.memory[settler.index()].level = group.level;
            if let Some(agent) = newcomer {
                self.note(GroupEvent::Settled {
                    step: self.steps,
                    agent,
                    node: w,
                    group,
                });
            }
            if self.memory[leader.index()].init_probe {
                let memory = &mut self.memory[settler.index()];
                memory.next = None;
                memory.checked = None;
                memory.help = None;
                memory.done = false;
                self.memory[leader.index()].init_probe = false;
                self.stats.probe_inits += 1;
            }
            self.centres.push(Centre {
                leader,
                node: w,
                settler,
                finishing: false,
                first: 0,
                count: 0,
            });
        }
        Ok(())
    }

    fn settle(
        &mut self,
        agent: AgentId,
        node: NodeId,
        parent: Option<PortId>,
    ) -> Result<(), GeneralError> {
        self.store.mark_settled(agent, node)?;
        let memory = &mut self.memory[agent.index()];
        memory.settled = true;
        memory.parent = parent;
        memory.next = None;
        memory.checked = None;
        memory.help = None;
        memory.done = false;
        self.metrics.settlement();
        if self.recorder.enabled() {
            self.events
                .push(SimulationEvent::AgentSettled { agent, node });
        }
        Ok(())
    }

    /// Slot 3 (line 61): settlers with `help` set walk to the centre.
    fn slot3_helpers_go(&mut self) -> Result<(), GeneralError> {
        for index in 0..self.memory.len() {
            let memory = self.memory[index];
            let Some(port) = memory.help.filter(|_| memory.settled) else {
                continue;
            };
            let agent = agent_id(index);
            let home = self.store.vacate_home(agent)?;
            self.intents.push(MoveIntent {
                agent,
                from: home,
                port,
            });
            self.outgoing.push(agent);
            self.events.push(SimulationEvent::AgentStateChanged {
                agent,
                from: AgentStatus::Settled,
                to: AgentStatus::SettledScout,
            });
        }
        Ok(())
    }

    /// Settlers at `w` other than `ψ(w)`: the helpers standing there.
    fn helpers_at(&self, occupancy: &OccupancyIndex, w: NodeId, settler: AgentId) -> Vec<AgentId> {
        occupancy
            .agents_at(w)
            .iter()
            .copied()
            .filter(|&b| b != settler && self.memory[b.index()].settled)
            .collect()
    }

    fn send_home(&mut self, helper: AgentId, from: NodeId) {
        if let Some(port) = self.memory[helper.index()].return_port {
            self.intents.push(MoveIntent {
                agent: helper,
                from,
                port,
            });
            self.homecoming.push(helper);
        }
    }

    /// Slot 4 (lines 31–36): find missing helpers, dismiss weaker ones, and
    /// decide whether the probe is over.
    fn slot4_check(&mut self, occupancy: &OccupancyIndex) {
        for index in 0..self.centres.len() {
            let centre = self.centres[index];
            let (w, psi) = (centre.node, centre.settler);
            let leader_group = self.memory[centre.leader.index()].group();
            let helpers = self.helpers_at(occupancy, w, psi);
            let checked = self.memory[psi.index()].checked;
            let next = checked.and_then(|last| {
                (0..=last.0).map(PortId).find(|&port| {
                    !helpers
                        .iter()
                        .any(|&b| self.state(b).arrival_port == Some(port))
                })
            });
            self.memory[psi.index()].next = next;
            for &(_, port) in self.recruited.iter().filter(|&&(at, _)| at == psi) {
                if !helpers
                    .iter()
                    .any(|&b| self.state(b).arrival_port == Some(port))
                {
                    self.stats.stolen_helpers += 1;
                }
            }
            for &helper in &helpers {
                if self.memory[helper.index()].group() < leader_group {
                    self.memory[helper.index()].help = None;
                    self.stats.helpers_dismissed += 1;
                    self.send_home(helper, w);
                }
            }
            let degree = self.graph.degree(w).unwrap_or(0);
            let all_checked = degree == 0 || checked.is_some_and(|last| last.0 + 1 == degree);
            self.centres[index].finishing = next.is_some() || all_checked;
        }
        self.recruited.clear();
    }

    /// Slot 5 (lines 38–40): a finished probe sends its helpers home.
    fn slot5_finish(&mut self, occupancy: &OccupancyIndex) {
        for index in 0..self.centres.len() {
            let centre = self.centres[index];
            if !centre.finishing {
                continue;
            }
            for helper in self.helpers_at(occupancy, centre.node, centre.settler) {
                self.memory[helper.index()].help = None;
                self.send_home(helper, centre.node);
            }
            self.memory[centre.settler.index()].done = true;
        }
    }

    /// Slot 6 (lines 43–47): probers go out, one port each.
    fn slot6_probe_out(&mut self, occupancy: &OccupancyIndex) {
        self.trips.clear();
        for index in 0..self.centres.len() {
            let centre = self.centres[index];
            if centre.finishing {
                continue;
            }
            let (w, psi) = (centre.node, centre.settler);
            let degree = self.graph.degree(w).unwrap_or(0);
            let first = self.memory[psi.index()]
                .checked
                .map_or(0, |last| last.0 + 1);
            let mut probers: Vec<AgentId> = occupancy
                .agents_at(w)
                .iter()
                .copied()
                .filter(|&b| b != psi)
                .collect();
            probers.sort_unstable();
            let count = probers.len().min(usize::from(degree - first));
            let count = u16::try_from(count).expect("count is at most the degree");
            self.centres[index].first = first;
            self.centres[index].count = count;
            self.stats.probe_passes += 1;
            for (offset, &prober) in probers[..usize::from(count)].iter().enumerate() {
                let port = PortId(first + u16::try_from(offset).expect("below the degree"));
                self.intents.push(MoveIntent {
                    agent: prober,
                    from: w,
                    port,
                });
                self.trips.push(Trip {
                    prober,
                    centre: index,
                    port,
                });
                self.metrics.port_probe();
                self.metrics.probe_out_traversal();
            }
        }
    }

    /// Slot 7 (lines 49–54): recruit a same-group settler and come back.
    fn slot7_probe_back(&mut self) -> Result<(), GeneralError> {
        for index in 0..self.trips.len() {
            let trip = self.trips[index];
            let state = *self.state(trip.prober);
            let back = state.arrival_port.ok_or_else(|| {
                self.broken(
                    InvariantViolation::InvalidTraversal,
                    "a prober has no incoming port",
                )
            })?;
            let leader = self.centres[trip.centre].leader;
            let found = self
                .store
                .settled_agent_at(state.node)
                .filter(|&s| self.memory[s.index()].group() == self.memory[leader.index()].group());
            if let Some(settler) = found {
                self.memory[settler.index()].help = Some(back);
                self.stats.helper_recruitments += 1;
                self.recruited
                    .push((self.centres[trip.centre].settler, trip.port));
            }
            self.memory[trip.prober.index()].found = found.is_some();
            self.intents.push(MoveIntent {
                agent: trip.prober,
                from: state.node,
                port: back,
            });
            self.metrics.probe_back_traversal();
        }
        Ok(())
    }

    /// Slot 8 (lines 56–59): record the pass and send helpers home.
    fn slot8_tally(&mut self, occupancy: &OccupancyIndex) {
        for index in 0..self.centres.len() {
            let centre = self.centres[index];
            if centre.finishing {
                continue;
            }
            let psi = centre.settler;
            let missing = self
                .trips
                .iter()
                .filter(|trip| trip.centre == index && !self.memory[trip.prober.index()].found)
                .map(|trip| trip.port)
                .min();
            if missing.is_some() {
                self.memory[psi.index()].next = missing;
            }
            if centre.count > 0 {
                self.memory[psi.index()].checked = Some(PortId(centre.first + centre.count - 1));
            }
            for helper in self.helpers_at(occupancy, centre.node, psi) {
                self.send_home(helper, centre.node);
            }
        }
        self.trips.clear();
    }

    /// Slots 9–10 (lines 63–68): weak zombies chase in both slots, strong
    /// ones only in slot 10.
    fn chase(&mut self, first_slot: bool, occupancy: &OccupancyIndex) {
        let zombies: Vec<AgentId> = (0..self.memory.len())
            .map(agent_id)
            .filter(|&z| self.is_zombie(z))
            .collect();
        if first_slot {
            let mut updates = Vec::with_capacity(zombies.len());
            for &zombie in &zombies {
                let v = self.state(zombie).node;
                let memory = self.memory[zombie.index()];
                let location = self
                    .store
                    .settled_agent_at(v)
                    .map_or(memory.level_location, |s| self.memory[s.index()].level);
                let swarm = occupancy
                    .agents_at(v)
                    .iter()
                    .filter(|&&b| self.is_zombie(b))
                    .map(|&b| self.memory[b.index()].level)
                    .max()
                    .unwrap_or(memory.level);
                updates.push((zombie, location, swarm));
            }
            for (zombie, location, swarm) in updates {
                let memory = &mut self.memory[zombie.index()];
                if (memory.level_location, memory.level_swarm) != (location, swarm) {
                    memory.level_location = location;
                    memory.level_swarm = swarm;
                    self.note(GroupEvent::ZombieLevels {
                        step: self.steps,
                        zombie,
                        location,
                        swarm,
                    });
                }
                self.weak[zombie.index()] = swarm < location;
            }
        }
        for &zombie in &zombies {
            let weak = self.weak[zombie.index()];
            if first_slot && !weak {
                continue;
            }
            let v = self.state(zombie).node;
            let leader_here = occupancy.agents_at(v).iter().any(|&b| self.is_leader(b));
            if leader_here {
                continue;
            }
            let port = self
                .store
                .settled_agent_at(v)
                .and_then(|s| self.memory[s.index()].next);
            let Some(port) = port else {
                self.stats.stuck_chases += 1;
                continue;
            };
            self.intents.push(MoveIntent {
                agent: zombie,
                from: v,
                port,
            });
            if weak {
                self.stats.weak_chase_moves += 1;
            } else {
                self.stats.strong_chase_moves += 1;
            }
            self.metrics.chase();
            if self.recorder.enabled() {
                let to = self.graph.traverse(v, port).map_or(v, |edge| edge.neighbor);
                self.group_events.push(GroupEvent::Chased {
                    step: self.steps,
                    zombie,
                    from: v,
                    to,
                    weak,
                });
            }
        }
    }

    /// Slot 11 (lines 23–26): move forward through `next`, or back.
    fn slot11_move(&mut self, occupancy: &OccupancyIndex) -> Result<(), GeneralError> {
        self.moved.clear();
        for index in 0..self.centres.len() {
            let centre = self.centres[index];
            let psi = centre.settler;
            if !self.memory[psi.index()].done {
                continue;
            }
            let forward = self.memory[psi.index()].next.is_some();
            if !forward {
                self.memory[psi.index()].next = self.memory[psi.index()].parent;
            }
            let Some(port) = self.memory[psi.index()].next else {
                return Err(GeneralError::Exhausted {
                    leader: centre.leader,
                    node: centre.node,
                });
            };
            let group: Vec<AgentId> = occupancy
                .agents_at(centre.node)
                .iter()
                .copied()
                .filter(|&b| b != psi)
                .collect();
            for &agent in &group {
                if self.memory[agent.index()].settled {
                    return Err(self.broken(
                        InvariantViolation::InvalidAgentNode,
                        "a helper is still at the centre in slot 11",
                    ));
                }
                self.intents.push(MoveIntent {
                    agent,
                    from: centre.node,
                    port,
                });
            }
            self.metrics.group_move(group.len());
            let to = self
                .graph
                .traverse(centre.node, port)
                .map_or(centre.node, |edge| edge.neighbor);
            if forward {
                self.stats.forward_moves += 1;
                self.note(GroupEvent::ForwardMove {
                    step: self.steps,
                    leader: centre.leader,
                    from: centre.node,
                    to,
                });
            } else {
                self.stats.backward_moves += 1;
                self.metrics.backtrack();
                self.note(GroupEvent::BackwardMove {
                    step: self.steps,
                    leader: centre.leader,
                    from: centre.node,
                    to,
                });
            }
            self.moved.push(centre.leader);
        }
        Ok(())
    }

    /// Slot 12 (lines 27–29): leaders that moved record `parent ← pin`.
    fn slot12_parent(&mut self) {
        for index in 0..self.moved.len() {
            let leader = self.moved[index];
            let pin = self.state(leader).arrival_port;
            let memory = &mut self.memory[leader.index()];
            memory.parent = pin;
            memory.init_probe = true;
        }
        self.moved.clear();
    }

    /// Commits the step's moves together and charges one round.
    fn commit(&mut self, round: RoundKind) -> Result<(), GeneralError> {
        apply_move_intents(
            self.graph,
            &mut self.store,
            &mut self.intents,
            &mut self.metrics,
        )?;
        self.steps += 1;
        self.metrics.logical_rounds(round, 1);
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

    /// Puts returning helpers back home and gives arriving ones a return port.
    fn settle_movers(&mut self) -> Result<(), GeneralError> {
        for index in 0..self.homecoming.len() {
            let helper = self.homecoming[index];
            let state = *self.state(helper);
            if state.home != Some(state.node) {
                return Err(self.broken(
                    InvariantViolation::InvalidTraversal,
                    "a helper's way home did not lead home",
                ));
            }
            self.store.return_home(helper)?;
            self.memory[helper.index()].return_port = None;
            self.events.push(SimulationEvent::AgentStateChanged {
                agent: helper,
                from: AgentStatus::SettledScout,
                to: AgentStatus::Settled,
            });
        }
        for index in 0..self.outgoing.len() {
            let helper = self.outgoing[index];
            self.memory[helper.index()].return_port = self.state(helper).arrival_port;
        }
        Ok(())
    }

    /// Invariants after the step in slot `slot`.
    fn check(&self, slot: u8) -> Result<(), GeneralError> {
        if matches!(slot, 8..=12 | 1 | 2) {
            for index in 0..self.memory.len() {
                let state = self.state(agent_id(index));
                if self.memory[index].settled && state.home != Some(state.node) {
                    return Err(self.broken(
                        InvariantViolation::InvalidAgentNode,
                        "a settler is away from home outside slots 3-8",
                    ));
                }
            }
        }
        if slot == 1 {
            let mut seen = vec![false; self.graph.node_count()];
            for leader in self.leaders() {
                let node = self.state(leader).node.index();
                if seen[node] {
                    return Err(self.broken(
                        InvariantViolation::InvalidAgentNode,
                        "two leaders share a node after slot 1",
                    ));
                }
                seen[node] = true;
            }
        }
        Ok(())
    }

    /// Definition 1: every agent on its own node, no zombie, every settler
    /// home and not asked to help, and no prober out.
    fn legitimate(&self) -> bool {
        if !self.trips.is_empty() {
            return false;
        }
        let mut occupied = vec![false; self.graph.node_count()];
        for index in 0..self.memory.len() {
            let agent = agent_id(index);
            let memory = &self.memory[index];
            let state = self.state(agent);
            if self.is_zombie(agent) {
                return false;
            }
            if memory.settled && (state.home != Some(state.node) || memory.help.is_some()) {
                return false;
            }
            if occupied[state.node.index()] {
                return false;
            }
            occupied[state.node.index()] = true;
        }
        true
    }

    fn record(&mut self, phase: Phase) {
        let unsettled = self.memory.iter().filter(|m| !m.settled).count();
        let settled = self.memory.len() - unsettled;
        self.metrics.observe_state(unsettled, 0, settled);
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

    fn note(&mut self, event: GroupEvent) {
        if self.recorder.enabled() {
            self.group_events.push(event);
        }
    }

    fn state(&self, agent: AgentId) -> &AgentState {
        self.store
            .get(agent)
            .expect("every agent ID comes from the store")
    }

    fn is_leader(&self, agent: AgentId) -> bool {
        let memory = &self.memory[agent.index()];
        !memory.settled && memory.leader == agent
    }

    fn is_zombie(&self, agent: AgentId) -> bool {
        let memory = &self.memory[agent.index()];
        !memory.settled && memory.leader != agent
    }

    fn leaders(&self) -> Vec<AgentId> {
        (0..self.memory.len())
            .map(agent_id)
            .filter(|&a| self.is_leader(a))
            .collect()
    }

    const fn broken(&self, violation: InvariantViolation, detail: &'static str) -> GeneralError {
        GeneralError::Invariant {
            violation,
            step: self.steps,
            detail,
        }
    }

    fn agent_views(&self) -> Vec<GeneralAgent> {
        let occupancy = self.store.occupancy();
        (0..self.memory.len())
            .map(|index| {
                let agent = agent_id(index);
                let state = self.state(agent);
                let memory = self.memory[index];
                let role = if memory.settled {
                    if state.home == Some(state.node) {
                        GeneralRole::Settler
                    } else {
                        GeneralRole::Helper
                    }
                } else if memory.leader == agent {
                    if occupancy.agents_at(state.node).len() == 1 {
                        GeneralRole::WaitingLeader
                    } else {
                        GeneralRole::ActiveLeader
                    }
                } else {
                    GeneralRole::Zombie
                };
                GeneralAgent {
                    node: state.node,
                    home: state.home,
                    role,
                    memory,
                }
            })
            .collect()
    }
}
