//! Typed algorithm adapters for the current CCM Python reference behavior.
//!
//! This crate intentionally models the dedicated `agent_drop_freeze.py`
//! transition rules.  It is not a claim that those rules are a literal port
//! of the rooted algorithm in Sudo et al.; see `docs/behavior-spec.md` and
//! `docs/complexity-metrics.md` for the distinction.

use ccm_core::{
    ComplexityMetrics, GraphError, InvariantViolation, LogicalMemory, Metrics, NodeId, PortEdge,
    PortGraph, PortId, RoundKind, Termination,
};
use core::fmt;

/// Version of the authoritative Drop-and-Freeze implementation.
pub const ALGORITHM_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The status values used by `agent_drop_freeze.py`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum DropStatus {
    Settled = 0,
    Unsettled = 1,
    SettledWaiting = 2,
}

/// Node occupancy as exposed by the Python reference.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum DropNodeStatus {
    Empty = 0,
    Occupied = 1,
}

/// Per-agent algorithm state.  The fields correspond to the dedicated Python
/// `Agent`; leader/level/home compatibility fields are deliberately omitted
/// because the Drop-and-Freeze transition functions never read them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DropAgentState {
    pub node: NodeId,
    pub status: DropStatus,
    pub probe_home: Option<NodeId>,
    pub probe_port: Option<PortId>,
    pub probe_result_empty: Option<bool>,
    pub pin: Option<PortId>,
    pub parent_port: Option<PortId>,
    pub next_port_to_try: usize,
    pub entry_pin: Option<PortId>,
}

impl DropAgentState {
    #[must_use]
    pub const fn unsettled(node: NodeId) -> Self {
        Self {
            node,
            status: DropStatus::Unsettled,
            probe_home: None,
            probe_port: None,
            probe_result_empty: None,
            pin: None,
            parent_port: None,
            next_port_to_try: 0,
            entry_pin: None,
        }
    }
}

/// Complete algorithm state at a phase barrier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SimulationState {
    pub agents: Vec<DropAgentState>,
    pub settled_agent_at: Vec<Option<ccm_core::AgentId>>,
    pub node_status: Vec<DropNodeStatus>,
}

impl SimulationState {
    #[must_use]
    pub fn positions(&self) -> Vec<NodeId> {
        self.agents.iter().map(|agent| agent.node).collect()
    }

    #[must_use]
    pub fn statuses(&self) -> Vec<DropStatus> {
        self.agents.iter().map(|agent| agent.status).collect()
    }
}

/// Metadata common to every typed result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SimulationMetadata {
    pub node_count: usize,
    pub agent_count: usize,
    pub round_limit: u64,
}

/// A trace label is kept as a compact enum until a recorder elects to format it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameLabel {
    Start,
    ProbeOut { round: u64 },
    ProbeBack { round: u64 },
    MoveOut { round: u64 },
}

impl FrameLabel {
    #[must_use]
    pub fn as_str(self) -> String {
        match self {
            Self::Start => "start".to_owned(),
            Self::ProbeOut { round } => format!("round{round}:probe_out"),
            Self::ProbeBack { round } => format!("round{round}:probe_back"),
            Self::MoveOut { round } => format!("round{round}:move_out"),
        }
    }
}

/// One full state checkpoint emitted by `FullTrace`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceFrame {
    pub label: String,
    pub state: SimulationState,
}

/// Optional semantic history.  It is separate from complexity metrics.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct SimulationTrace {
    pub frames: Vec<TraceFrame>,
}

/// Recorder interface used by the algorithm loop.  `NoTrace` does not clone
/// state or allocate labels, which keeps headless runs free of trace history.
pub trait Recorder {
    type Output;

    fn record(&mut self, label: FrameLabel, state: &SimulationState);
    fn finish(self) -> Self::Output;
}

/// Bridges Drop-and-Freeze phase checkpoints into the workspace-wide semantic
/// trace recorder. This is the production recorder used by the WASM adapter;
/// the algorithm-specific `FullTrace` remains only as a fixture convenience.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SharedRecorder<R> {
    inner: R,
}

impl<R> SharedRecorder<R> {
    #[must_use]
    pub const fn new(inner: R) -> Self {
        Self { inner }
    }

    #[must_use]
    pub fn into_inner(self) -> R {
        self.inner
    }
}

impl<R: ccm_trace::Recorder> Recorder for SharedRecorder<R> {
    type Output = R;

    fn record(&mut self, label: FrameLabel, state: &SimulationState) {
        let (step, phase) = match label {
            FrameLabel::Start => (0, ccm_trace::Phase::Other),
            FrameLabel::ProbeOut { round } => (
                round.saturating_sub(1).saturating_mul(3).saturating_add(1),
                ccm_trace::Phase::ProbeOut,
            ),
            FrameLabel::ProbeBack { round } => (
                round.saturating_sub(1).saturating_mul(3).saturating_add(2),
                ccm_trace::Phase::ProbeBack,
            ),
            FrameLabel::MoveOut { round } => (
                round.saturating_sub(1).saturating_mul(3).saturating_add(3),
                ccm_trace::Phase::Movement,
            ),
        };
        self.inner
            .record_event(step, ccm_trace::SimulationEvent::PhaseStarted { phase });
        self.inner.record_checkpoint(ccm_trace::Checkpoint {
            step,
            agent_nodes: state.positions(),
            agent_statuses: state
                .agents
                .iter()
                .map(|agent| match agent.status {
                    DropStatus::Settled => ccm_core::AgentStatus::Settled,
                    DropStatus::Unsettled => ccm_core::AgentStatus::Unsettled,
                    DropStatus::SettledWaiting => ccm_core::AgentStatus::SettledWaiting,
                })
                .collect(),
            home_nodes: vec![None; state.agents.len()],
            settled_agents: state.settled_agent_at.clone(),
        });
    }

    fn finish(self) -> Self::Output {
        self.inner
    }
}

/// Recorder for native/headless execution.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoTrace;

impl Recorder for NoTrace {
    type Output = Option<SimulationTrace>;

    fn record(&mut self, _label: FrameLabel, _state: &SimulationState) {}

    fn finish(self) -> Self::Output {
        None
    }
}

/// Recorder retaining every phase checkpoint for small reference fixtures and
/// browser playback.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FullTrace {
    frames: Vec<TraceFrame>,
}

impl FullTrace {
    #[must_use]
    pub fn frames(&self) -> &[TraceFrame] {
        &self.frames
    }
}

impl Recorder for FullTrace {
    type Output = Option<SimulationTrace>;

    fn record(&mut self, label: FrameLabel, state: &SimulationState) {
        self.frames.push(TraceFrame {
            label: label.as_str(),
            state: state.clone(),
        });
    }

    fn finish(self) -> Self::Output {
        Some(SimulationTrace {
            frames: self.frames,
        })
    }
}

/// Result returned by the convenient `simulate` entry point.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SimulationResult {
    pub metadata: SimulationMetadata,
    pub final_state: SimulationState,
    pub termination: Termination,
    pub metrics: ComplexityMetrics,
    pub trace: Option<SimulationTrace>,
}

/// Generic result returned by `simulate_with` when a caller supplies custom
/// metrics and a recorder.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SimulationRun<T> {
    pub metadata: SimulationMetadata,
    pub final_state: SimulationState,
    pub termination: Termination,
    pub trace: T,
}

/// Input/state errors that prevent a Drop-and-Freeze run from being produced.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DropAndFreezeError {
    Graph(GraphError),
    InvalidStart(NodeId),
    TooManyAgents(usize),
    Invariant(InvariantViolation),
}

impl fmt::Display for DropAndFreezeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for DropAndFreezeError {}

impl From<GraphError> for DropAndFreezeError {
    fn from(value: GraphError) -> Self {
        Self::Graph(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ProbeMove {
    agent: ccm_core::AgentId,
    destination: NodeId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MoveGroup {
    agents: Vec<ccm_core::AgentId>,
    from: NodeId,
    port: PortId,
    destination: NodeId,
    backtrack: bool,
}

/// Runs Drop-and-Freeze with the default `ComplexityMetrics` and no trace.
///
/// # Errors
///
/// Returns an error when the graph is invalid, a start node is outside the
/// graph, or an invariant fails during a transition.
pub fn simulate(
    graph: &PortGraph,
    starts: &[NodeId],
    round_limit: u64,
) -> Result<SimulationResult, DropAndFreezeError> {
    let mut metrics = ComplexityMetrics::default();
    let run = simulate_with(graph, starts, round_limit, &mut metrics, NoTrace)?;
    Ok(SimulationResult {
        metadata: run.metadata,
        final_state: run.final_state,
        termination: run.termination,
        metrics,
        trace: run.trace,
    })
}

/// Runs Drop-and-Freeze with a full trace and default metrics.
///
/// # Errors
///
/// Returns an error when the graph is invalid, a start node is outside the
/// graph, or an invariant fails during a transition.
pub fn simulate_with_trace(
    graph: &PortGraph,
    starts: &[NodeId],
    round_limit: u64,
) -> Result<SimulationResult, DropAndFreezeError> {
    let mut metrics = ComplexityMetrics::default();
    let run = simulate_with(
        graph,
        starts,
        round_limit,
        &mut metrics,
        FullTrace::default(),
    )?;
    Ok(SimulationResult {
        metadata: run.metadata,
        final_state: run.final_state,
        termination: run.termination,
        metrics,
        trace: run.trace,
    })
}

/// Runs Drop-and-Freeze while allowing callers to provide a metrics sink and
/// recorder.  The algorithm itself never depends on either observer.
///
/// # Errors
///
/// Returns an error when the graph is invalid, a start node is outside the
/// graph, or an invariant fails during a transition.
pub fn simulate_with<M: Metrics, R: Recorder>(
    graph: &PortGraph,
    starts: &[NodeId],
    round_limit: u64,
    metrics: &mut M,
    mut recorder: R,
) -> Result<SimulationRun<R::Output>, DropAndFreezeError> {
    if starts.len() > u32::MAX as usize {
        return Err(DropAndFreezeError::TooManyAgents(starts.len()));
    }
    for &node in starts {
        if node.index() >= graph.node_count() {
            return Err(DropAndFreezeError::InvalidStart(node));
        }
    }

    // Python `_init_ports` replaces any supplied local labels with canonical
    // sorted-neighbor ports.  Normalize before executing the transition rules.
    let graph = canonicalize_ports(graph)?;
    let mut state = SimulationState {
        agents: starts
            .iter()
            .copied()
            .map(DropAgentState::unsettled)
            .collect(),
        settled_agent_at: vec![None; graph.node_count()],
        node_status: vec![DropNodeStatus::Empty; graph.node_count()],
    };

    validate_state(&graph, &state)?;
    observe(metrics, &state);
    recorder.record(FrameLabel::Start, &state);

    let metadata = SimulationMetadata {
        node_count: graph.node_count(),
        agent_count: state.agents.len(),
        round_limit,
    };

    let mut completed = all_settled(&state);
    for round in 1..=round_limit {
        if completed {
            break;
        }
        metrics.macro_round();
        metrics.logical_rounds(RoundKind::ProbeOut, 1);
        probe_out(&graph, &mut state, metrics);
        validate_state(&graph, &state)?;
        observe(metrics, &state);
        recorder.record(FrameLabel::ProbeOut { round }, &state);

        metrics.logical_rounds(RoundKind::ProbeBack, 1);
        probe_back(&graph, &mut state, metrics)?;
        validate_state(&graph, &state)?;
        observe(metrics, &state);
        recorder.record(FrameLabel::ProbeBack { round }, &state);

        metrics.logical_rounds(RoundKind::Movement, 1);
        move_out(&graph, &mut state, metrics)?;
        validate_state(&graph, &state)?;
        observe(metrics, &state);
        recorder.record(FrameLabel::MoveOut { round }, &state);

        completed = all_settled(&state);
    }

    let termination = if completed {
        Termination::Completed
    } else {
        Termination::RoundLimitReached { limit: round_limit }
    };
    Ok(SimulationRun {
        metadata,
        final_state: state,
        termination,
        trace: recorder.finish(),
    })
}

fn canonicalize_ports(graph: &PortGraph) -> Result<PortGraph, GraphError> {
    let mut edges = Vec::with_capacity(graph.edge_count());
    for source_index in 0..graph.node_count() {
        let source = NodeId(u32::try_from(source_index).expect("PortGraph node IDs fit in u32"));
        for (_, edge) in graph.ports(source) {
            if source < edge.neighbor {
                edges.push((source, edge.neighbor));
            }
        }
    }
    PortGraph::from_undirected_edges(graph.node_count(), &edges)
}

fn ordered_ports(graph: &PortGraph, node: NodeId) -> Vec<PortId> {
    let mut ports: Vec<(PortId, PortEdge)> = graph.ports(node).collect();
    ports.sort_by_key(|(port, edge)| {
        if port.0 != 0 && edge.remote_port.0 == 0 {
            (0_u8, port.0)
        } else if port.0 == 0 {
            (1_u8, port.0)
        } else {
            (2_u8, port.0)
        }
    });
    ports.into_iter().map(|(port, _)| port).collect()
}

fn occupancy(state: &SimulationState) -> (Vec<Vec<ccm_core::AgentId>>, Vec<NodeId>) {
    let mut buckets = vec![Vec::new(); state.node_status.len()];
    let mut active = Vec::new();
    for (index, agent) in state.agents.iter().enumerate() {
        let id =
            ccm_core::AgentId(u32::try_from(index).expect("agent IDs fit in the dense u32 domain"));
        let bucket = &mut buckets[agent.node.index()];
        if bucket.is_empty() {
            active.push(agent.node);
        }
        bucket.push(id);
    }
    (buckets, active)
}

fn probe_out<M: Metrics>(graph: &PortGraph, state: &mut SimulationState, metrics: &mut M) {
    let (buckets, active) = occupancy(state);
    let mut moves = Vec::new();
    for node in active {
        let settled = state.settled_agent_at[node.index()];
        let mut unsettled: Vec<_> = buckets[node.index()]
            .iter()
            .copied()
            .filter(|&agent| {
                Some(agent) != settled
                    && state.agents[agent.index()].status == DropStatus::Unsettled
            })
            .collect();
        if unsettled.is_empty() {
            continue;
        }
        for &agent in &unsettled {
            let value = &mut state.agents[agent.index()];
            value.probe_home = None;
            value.probe_port = None;
            value.probe_result_empty = None;
        }
        if settled.is_none() && unsettled.len() == 1 {
            continue;
        }
        let ports = ordered_ports(graph, node);
        if ports.is_empty() {
            continue;
        }
        let start = settled.map_or(0, |agent| state.agents[agent.index()].next_port_to_try);
        if start >= ports.len() {
            continue;
        }
        unsettled.sort_unstable();
        for (agent, &port) in unsettled.iter().zip(ports[start..].iter()) {
            let Some(edge) = graph.traverse(node, port) else {
                continue;
            };
            let value = &mut state.agents[agent.index()];
            value.probe_home = Some(node);
            value.probe_port = Some(port);
            value.probe_result_empty = None;
            value.status = DropStatus::SettledWaiting;
            metrics.port_probe();
            metrics.probe_out_traversal();
            metrics.agent_moves(1);
            moves.push(ProbeMove {
                agent: *agent,
                destination: edge.neighbor,
            });
        }
    }
    for movement in moves {
        state.agents[movement.agent.index()].node = movement.destination;
    }
}

fn probe_back<M: Metrics>(
    graph: &PortGraph,
    state: &mut SimulationState,
    metrics: &mut M,
) -> Result<(), DropAndFreezeError> {
    let mut moves = Vec::new();
    for index in 0..state.agents.len() {
        let agent =
            ccm_core::AgentId(u32::try_from(index).expect("agent IDs fit in the dense u32 domain"));
        if state.agents[index].status != DropStatus::SettledWaiting {
            continue;
        }
        let source = state.agents[index].node;
        state.agents[index].probe_result_empty =
            Some(state.settled_agent_at[source.index()].is_none());
        state.agents[index].status = DropStatus::Unsettled;
        if let Some(home) = state.agents[index].probe_home {
            let Some(port) = local_port_to(graph, source, home) else {
                return Err(DropAndFreezeError::Invariant(
                    InvariantViolation::InvalidTraversal,
                ));
            };
            metrics.probe_back_traversal();
            metrics.agent_moves(1);
            moves.push((agent, home, port));
        }
    }
    for (agent, destination, _incoming_port) in moves {
        state.agents[agent.index()].node = destination;
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn move_out<M: Metrics>(
    graph: &PortGraph,
    state: &mut SimulationState,
    metrics: &mut M,
) -> Result<(), DropAndFreezeError> {
    let (buckets, active) = occupancy(state);
    let mut movers_by_node = vec![Vec::new(); state.node_status.len()];
    let mut active_with_movers = Vec::new();
    for node in active {
        let settled = state.settled_agent_at[node.index()];
        let movers: Vec<_> = buckets[node.index()]
            .iter()
            .copied()
            .filter(|&agent| {
                Some(agent) != settled
                    && state.agents[agent.index()].status == DropStatus::Unsettled
            })
            .collect();
        if !movers.is_empty() {
            active_with_movers.push(node);
            movers_by_node[node.index()] = movers;
        }
    }

    let mut newly_settled = vec![false; state.node_status.len()];
    for &node in &active_with_movers {
        if state.settled_agent_at[node.index()].is_some() {
            continue;
        }
        let agent = movers_by_node[node.index()].remove(0);
        let entry_pin = state.agents[agent.index()].entry_pin;
        let parent_port = entry_pin.filter(|&port| graph.traverse(node, port).is_some());
        state.agents[agent.index()].status = DropStatus::Settled;
        state.agents[agent.index()].parent_port = parent_port;
        state.agents[agent.index()].next_port_to_try = 0;
        state.settled_agent_at[node.index()] = Some(agent);
        state.node_status[node.index()] = DropNodeStatus::Occupied;
        newly_settled[node.index()] = true;
        metrics.settlement();
    }

    let mut planned = Vec::new();
    for &node in &active_with_movers {
        let movers = &movers_by_node[node.index()];
        if movers.is_empty() {
            continue;
        }
        let Some(settled) = state.settled_agent_at[node.index()] else {
            continue;
        };
        let ports = ordered_ports(graph, node);
        if ports.is_empty() {
            continue;
        }
        let cursor = state.agents[settled.index()].next_port_to_try;
        let mut scouts = movers.clone();
        if newly_settled[node.index()] {
            scouts.push(settled);
        }
        let mut empty_ports = Vec::new();
        for agent in scouts {
            let value = state.agents[agent.index()];
            if value.probe_home != Some(node) || value.probe_result_empty != Some(true) {
                continue;
            }
            let Some(port) = value.probe_port else {
                continue;
            };
            if graph.traverse(node, port).is_some() && !empty_ports.contains(&port) {
                empty_ports.push(port);
            }
        }
        empty_ports.sort_by_key(|port| {
            ports
                .iter()
                .position(|candidate| candidate == port)
                .unwrap_or(usize::MAX)
        });

        let chosen = empty_ports.iter().copied().find(|port| {
            ports
                .iter()
                .position(|candidate| candidate == port)
                .is_some_and(|index| index >= cursor)
        });
        if let Some(port) = chosen {
            let index = ports
                .iter()
                .position(|candidate| candidate == &port)
                .ok_or(DropAndFreezeError::Invariant(
                    InvariantViolation::InvalidTraversal,
                ))?;
            let edge = graph
                .traverse(node, port)
                .ok_or(DropAndFreezeError::Invariant(
                    InvariantViolation::InvalidTraversal,
                ))?;
            state.agents[settled.index()].next_port_to_try = index + 1;
            planned.push(MoveGroup {
                agents: movers.clone(),
                from: node,
                port,
                destination: edge.neighbor,
                backtrack: false,
            });
            continue;
        }

        let probed_count =
            scouts_for_count(state, node, movers, newly_settled[node.index()], settled);
        state.agents[settled.index()].next_port_to_try =
            core::cmp::min(ports.len(), cursor.saturating_add(probed_count));
        if state.agents[settled.index()].next_port_to_try < ports.len() {
            continue;
        }
        let Some(parent_port) = state.agents[settled.index()].parent_port else {
            continue;
        };
        let Some(edge) = graph.traverse(node, parent_port) else {
            continue;
        };
        state.agents[settled.index()].next_port_to_try = ports.len();
        planned.push(MoveGroup {
            agents: movers.clone(),
            from: node,
            port: parent_port,
            destination: edge.neighbor,
            backtrack: true,
        });
    }

    for group in planned {
        metrics.group_move(group.agents.len());
        if group.backtrack {
            metrics.backtrack();
        }
        metrics.agent_moves(group.agents.len() as u64);
        for agent in group.agents {
            let value = &mut state.agents[agent.index()];
            value.node = group.destination;
            let incoming = local_port_to(graph, group.destination, group.from).ok_or(
                DropAndFreezeError::Invariant(InvariantViolation::InvalidTraversal),
            )?;
            value.pin = Some(incoming);
            value.entry_pin = Some(incoming);
            value.probe_home = None;
            value.probe_port = None;
            value.probe_result_empty = None;
        }
    }
    Ok(())
}

fn scouts_for_count(
    state: &SimulationState,
    node: NodeId,
    movers: &[ccm_core::AgentId],
    include_settled: bool,
    settled: ccm_core::AgentId,
) -> usize {
    let mut ports = Vec::<PortId>::new();
    for &agent in movers {
        let value = state.agents[agent.index()];
        if value.probe_home == Some(node) {
            if let Some(port) = value.probe_port {
                if !ports.contains(&port) {
                    ports.push(port);
                }
            }
        }
    }
    if include_settled {
        let value = state.agents[settled.index()];
        if value.probe_home == Some(node) {
            if let Some(port) = value.probe_port {
                if !ports.contains(&port) {
                    ports.push(port);
                }
            }
        }
    }
    ports.len()
}

fn local_port_to(graph: &PortGraph, source: NodeId, destination: NodeId) -> Option<PortId> {
    graph
        .ports(source)
        .find_map(|(port, edge)| (edge.neighbor == destination).then_some(port))
}

fn all_settled(state: &SimulationState) -> bool {
    state
        .agents
        .iter()
        .all(|agent| agent.status == DropStatus::Settled)
}

fn observe<M: Metrics>(metrics: &mut M, state: &SimulationState) {
    let unsettled = state
        .agents
        .iter()
        .filter(|agent| agent.status != DropStatus::Settled)
        .count();
    let owned_nodes = state
        .settled_agent_at
        .iter()
        .filter(|owner| owner.is_some())
        .count();
    metrics.observe_state(unsettled, 0, owned_nodes);
    metrics.observe_memory(LogicalMemory {
        agent_state_words: (state.agents.len() * 9) as u64,
        ownership_words: state.settled_agent_at.len() as u64,
        ..LogicalMemory::default()
    });
}

fn validate_state(graph: &PortGraph, state: &SimulationState) -> Result<(), DropAndFreezeError> {
    if state.settled_agent_at.len() != graph.node_count()
        || state.node_status.len() != graph.node_count()
    {
        return Err(DropAndFreezeError::Invariant(
            InvariantViolation::OwnershipIndexMismatch,
        ));
    }
    for (index, agent) in state.agents.iter().enumerate() {
        if agent.node.index() >= graph.node_count() {
            return Err(DropAndFreezeError::Invariant(
                InvariantViolation::InvalidAgentNode,
            ));
        }
        if let Some(home) = agent.probe_home {
            if home.index() >= graph.node_count() {
                return Err(DropAndFreezeError::Invariant(
                    InvariantViolation::InvalidTraversal,
                ));
            }
        }
        if let Some(port) = agent.parent_port {
            if graph.traverse(agent.node, port).is_none() {
                return Err(DropAndFreezeError::Invariant(
                    InvariantViolation::InvalidParent,
                ));
            }
        }
        if let Some(owner) = state.settled_agent_at[agent.node.index()] {
            if owner
                == ccm_core::AgentId(
                    u32::try_from(index).expect("agent IDs fit in the dense u32 domain"),
                )
                && agent.status != DropStatus::Settled
            {
                return Err(DropAndFreezeError::Invariant(
                    InvariantViolation::OwnershipIndexMismatch,
                ));
            }
        }
    }
    for (node, owner) in state.settled_agent_at.iter().enumerate() {
        if let Some(agent) = owner {
            let Some(value) = state.agents.get(agent.index()) else {
                return Err(DropAndFreezeError::Invariant(
                    InvariantViolation::OwnershipIndexMismatch,
                ));
            };
            if value.node.index() != node || value.status != DropStatus::Settled {
                return Err(DropAndFreezeError::Invariant(
                    InvariantViolation::OwnershipIndexMismatch,
                ));
            }
            if state.node_status[node] != DropNodeStatus::Occupied {
                return Err(DropAndFreezeError::Invariant(
                    InvariantViolation::OwnershipIndexMismatch,
                ));
            }
        } else if state.node_status[node] != DropNodeStatus::Empty {
            return Err(DropAndFreezeError::Invariant(
                InvariantViolation::OwnershipIndexMismatch,
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccm_core::{AgentId, PortEdge};

    fn graph(edges: &[(u32, u32)], nodes: usize) -> PortGraph {
        PortGraph::from_undirected_edges(
            nodes,
            &edges
                .iter()
                .map(|&(a, b)| (NodeId(a), NodeId(b)))
                .collect::<Vec<_>>(),
        )
        .unwrap()
    }

    fn labels(result: &SimulationResult) -> Vec<String> {
        result
            .trace
            .as_ref()
            .unwrap()
            .frames
            .iter()
            .map(|frame| frame.label.clone())
            .collect()
    }

    #[test]
    fn path3_matches_python_final_state_and_round_frames() {
        let result = simulate_with_trace(
            &graph(&[(0, 1), (1, 2)], 3),
            &[NodeId(0), NodeId(0), NodeId(0)],
            3,
        )
        .unwrap();
        assert_eq!(
            result.final_state.positions(),
            vec![NodeId(0), NodeId(1), NodeId(2)]
        );
        assert_eq!(result.final_state.statuses(), vec![DropStatus::Settled; 3]);
        assert_eq!(
            labels(&result),
            vec![
                "start",
                "round1:probe_out",
                "round1:probe_back",
                "round1:move_out",
                "round2:probe_out",
                "round2:probe_back",
                "round2:move_out",
                "round3:probe_out",
                "round3:probe_back",
                "round3:move_out",
            ]
        );
        assert_eq!(result.metrics.macro_rounds, 3);
        assert_eq!(result.metrics.rounds, 9);
        assert_eq!(result.metrics.port_probes, 3);
        assert_eq!(result.metrics.agent_moves, 9);
        assert_eq!(result.metrics.settlements, 3);
        assert_eq!(result.metrics.group_moves, 2);
        assert_eq!(result.metrics.maximum_group_size, 2);
        assert_eq!(result.termination, Termination::Completed);
    }

    #[test]
    fn structured_final_positions_match_reference_cases() {
        let cases = [
            (
                graph(&[(0, 1), (1, 2)], 3),
                vec![NodeId(0), NodeId(0), NodeId(0)],
                vec![NodeId(0), NodeId(1), NodeId(2)],
            ),
            (
                graph(&[(0, 1), (1, 2), (2, 0)], 3),
                vec![NodeId(0), NodeId(0), NodeId(0)],
                vec![NodeId(0), NodeId(2), NodeId(1)],
            ),
            (
                graph(&[(0, 1), (0, 2), (0, 3)], 4),
                vec![NodeId(0); 4],
                vec![NodeId(0), NodeId(2), NodeId(3), NodeId(1)],
            ),
            (
                graph(&[(0, 1), (0, 2), (1, 3), (1, 4)], 5),
                vec![NodeId(0); 5],
                vec![NodeId(0), NodeId(2), NodeId(1), NodeId(3), NodeId(4)],
            ),
            (
                graph(&[(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)], 4),
                vec![NodeId(0); 4],
                vec![NodeId(0), NodeId(2), NodeId(1), NodeId(3)],
            ),
        ];
        for (graph, starts, expected) in cases {
            let result = simulate(&graph, &starts, 10).unwrap();
            assert_eq!(result.final_state.positions(), expected);
            assert_eq!(result.termination, Termination::Completed);
        }
    }

    #[test]
    fn single_agent_empty_node_settles_without_probe() {
        let result = simulate_with_trace(&graph(&[], 1), &[NodeId(0)], 2).unwrap();
        assert_eq!(result.final_state.statuses(), vec![DropStatus::Settled]);
        assert_eq!(result.trace.as_ref().unwrap().frames.len(), 4);
        assert_eq!(result.metrics.port_probes, 0);
    }

    #[test]
    fn round_limit_is_not_success() {
        let result = simulate(&graph(&[(0, 1), (1, 2)], 3), &[NodeId(0); 3], 1).unwrap();
        assert_eq!(
            result.final_state.statuses(),
            vec![
                DropStatus::Settled,
                DropStatus::Unsettled,
                DropStatus::Unsettled
            ]
        );
        assert_eq!(
            result.termination,
            Termination::RoundLimitReached { limit: 1 }
        );
    }

    #[test]
    fn randomized_input_ports_are_normalized_like_python_init_ports() {
        let tables = vec![
            vec![
                PortEdge {
                    neighbor: NodeId(2),
                    remote_port: PortId(0),
                },
                PortEdge {
                    neighbor: NodeId(1),
                    remote_port: PortId(0),
                },
            ],
            vec![
                PortEdge {
                    neighbor: NodeId(0),
                    remote_port: PortId(1),
                },
                PortEdge {
                    neighbor: NodeId(2),
                    remote_port: PortId(1),
                },
            ],
            vec![
                PortEdge {
                    neighbor: NodeId(0),
                    remote_port: PortId(0),
                },
                PortEdge {
                    neighbor: NodeId(1),
                    remote_port: PortId(1),
                },
            ],
        ];
        let randomized = PortGraph::from_port_tables(tables).unwrap();
        let canonical = graph(&[(0, 1), (0, 2), (1, 2)], 3);
        let a = simulate(&randomized, &[NodeId(0); 3], 5).unwrap();
        let b = simulate(&canonical, &[NodeId(0); 3], 5).unwrap();
        assert_eq!(a.final_state, b.final_state);
        assert_eq!(a.metrics, b.metrics);
    }

    #[test]
    fn no_trace_does_not_emit_frames_and_is_deterministic() {
        let graph = graph(&[(0, 1), (1, 2)], 3);
        let a = simulate(&graph, &[NodeId(0); 3], 3).unwrap();
        let b = simulate(&graph, &[NodeId(0); 3], 3).unwrap();
        assert_eq!(a, b);
        assert!(a.trace.is_none());
    }

    #[test]
    fn two_start_nodes_settle_independently() {
        let result = simulate(&graph(&[(0, 1), (1, 2)], 3), &[NodeId(0), NodeId(2)], 1).unwrap();
        assert_eq!(result.final_state.positions(), vec![NodeId(0), NodeId(2)]);
        assert_eq!(result.final_state.statuses(), vec![DropStatus::Settled; 2]);
    }

    #[test]
    fn ownership_index_is_consistent_after_every_frame() {
        let result = simulate_with_trace(&graph(&[(0, 1), (1, 2)], 3), &[NodeId(0); 3], 3).unwrap();
        for frame in &result.trace.unwrap().frames {
            assert!(validate_state(&graph(&[(0, 1), (1, 2)], 3), &frame.state).is_ok());
        }
    }

    #[test]
    fn agent_ids_are_dense_and_owner_values_are_real_agents() {
        let result = simulate(&graph(&[(0, 1)], 2), &[NodeId(0), NodeId(0)], 2).unwrap();
        assert!(result
            .final_state
            .settled_agent_at
            .iter()
            .flatten()
            .all(|agent| *agent == AgentId(0) || *agent == AgentId(1)));
    }
}
