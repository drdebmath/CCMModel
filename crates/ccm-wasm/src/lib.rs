//! Typed browser adapter for the shared CCM simulation implementations.
//!
//! The adapter accepts dense typed graph/placement buffers, invokes the same
//! native algorithm functions, and exposes final state plus a compact binary
//! render stream.  It does not contain algorithm transitions or browser UI
//! code.  Binary stream records are observer data, never a second simulation.
//!
//! Render stream wire format is little-endian and versioned: an eight-byte
//! header is `[version=1, kind, flags, reserved, record_count:u32]`.
//! `kind=2` contains semantic trace events/checkpoints. `flags & 1` means bounded retention sampled or
//! evicted records. Each record begins with a one-byte record kind followed by
//! fixed-width dense IDs; group records carry an explicit agent count.

use ccm_algorithms::drop_and_freeze::{DropStatus, SharedRecorder, SimulationState as DropState};
use ccm_core::{AgentStatus, ComplexityMetrics, NodeId, PortEdge, PortGraph, Termination};
use ccm_trace::{
    BoundedTrace, Checkpoint, FullTrace, NoTrace, Phase, SimulationEvent, Trace, TraceBudget,
};
use std::fmt;
use wasm_bindgen::prelude::*;

/// Algorithm selector shared by native and browser callers.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AlgorithmSelector {
    HelpByScouts = 0,
    DropAndFreeze = 1,
    P1Tree = 2,
    RootedDisp = 3,
    GeneralDisp = 4,
}

/// Trace policy. `Bounded` uses the supplied record and sampling limits.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TraceMode {
    Off = 0,
    Full = 1,
    Bounded = 2,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum AdapterError {
    Input(String),
    Graph(String),
    Help(String),
    Drop(String),
    P1Tree(String),
    RootedDisp(String),
    GeneralDisp(String),
    Trace(String),
}

impl fmt::Display for AdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input(message)
            | Self::Graph(message)
            | Self::Help(message)
            | Self::Drop(message)
            | Self::P1Tree(message)
            | Self::RootedDisp(message)
            | Self::GeneralDisp(message)
            | Self::Trace(message) => f.write_str(message),
        }
    }
}

/// An opaque simulation instance.  The graph and initial placement are
/// immutable after construction; cancellation and trace policy are explicit.
#[wasm_bindgen]
pub struct WasmSimulation {
    graph: PortGraph,
    starts: Vec<NodeId>,
    algorithm: AlgorithmSelector,
    trace_mode: TraceMode,
    round_limit: u64,
    max_trace_records: usize,
    sample_every: u64,
    cancelled: bool,
}

#[wasm_bindgen]
impl WasmSimulation {
    /// Creates a canonical-port graph from `[source, destination, ...]` edge
    /// pairs and dense start-node IDs.
    #[allow(clippy::too_many_arguments)]
    #[wasm_bindgen(constructor)]
    pub fn new(
        node_count: u32,
        edges: &[u32],
        starts: &[u32],
        algorithm: AlgorithmSelector,
        trace_mode: TraceMode,
        round_limit: u64,
        max_trace_records: u32,
        sample_every: u32,
    ) -> Result<WasmSimulation, JsValue> {
        let graph = graph_from_edges(node_count as usize, edges).map_err(to_js)?;
        let starts = starts.iter().copied().map(NodeId).collect();
        Self::from_parts(
            graph,
            starts,
            algorithm,
            trace_mode,
            round_limit,
            max_trace_records,
            sample_every,
        )
        .map_err(to_js)
    }

    /// Creates a graph from CSR-like local port tables. `offsets` has
    /// `node_count + 1` entries; neighbors and reciprocal ports have one entry
    /// per local port. This preserves independently assigned local labels.
    #[allow(clippy::too_many_arguments)]
    pub fn from_port_tables(
        node_count: u32,
        offsets: &[u32],
        neighbors: &[u32],
        remote_ports: &[u16],
        starts: &[u32],
        algorithm: AlgorithmSelector,
        trace_mode: TraceMode,
        round_limit: u64,
        max_trace_records: u32,
        sample_every: u32,
    ) -> Result<WasmSimulation, JsValue> {
        let graph = graph_from_port_tables(node_count as usize, offsets, neighbors, remote_ports)
            .map_err(to_js)?;
        let starts = starts.iter().copied().map(NodeId).collect();
        Self::from_parts(
            graph,
            starts,
            algorithm,
            trace_mode,
            round_limit,
            max_trace_records,
            sample_every,
        )
        .map_err(to_js)
    }

    /// Runs once and returns compact typed final state plus binary render data.
    pub fn run(&mut self) -> Result<SimulationOutput, JsValue> {
        self.run_internal().map_err(to_js)
    }

    /// Requests cancellation before the next run. The current algorithm APIs
    /// are synchronous, so an in-flight call cannot be interrupted safely;
    /// callers should run in a Web Worker and cancel between calls.
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub fn clear_cancelled(&mut self) {
        self.cancelled = false;
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    #[must_use]
    pub fn algorithm(&self) -> AlgorithmSelector {
        self.algorithm
    }

    #[must_use]
    pub fn trace_mode(&self) -> TraceMode {
        self.trace_mode
    }

    #[must_use]
    pub fn node_count(&self) -> u32 {
        self.graph.node_count() as u32
    }

    #[must_use]
    pub fn agent_count(&self) -> u32 {
        self.starts.len() as u32
    }

    fn from_parts(
        graph: PortGraph,
        starts: Vec<NodeId>,
        algorithm: AlgorithmSelector,
        trace_mode: TraceMode,
        round_limit: u64,
        max_trace_records: u32,
        sample_every: u32,
    ) -> Result<Self, AdapterError> {
        if starts.iter().any(|node| node.index() >= graph.node_count()) {
            return Err(AdapterError::Input(
                "start node is outside graph".to_owned(),
            ));
        }
        if starts.len() > graph.node_count() {
            return Err(AdapterError::Input(
                "agent count cannot exceed graph node count".to_owned(),
            ));
        }
        if sample_every == 0 {
            return Err(AdapterError::Input(
                "sample_every must be nonzero".to_owned(),
            ));
        }
        Ok(Self {
            graph,
            starts,
            algorithm,
            trace_mode,
            round_limit,
            max_trace_records: max_trace_records as usize,
            sample_every: sample_every as u64,
            cancelled: false,
        })
    }

    fn run_internal(&mut self) -> Result<SimulationOutput, AdapterError> {
        if self.cancelled {
            return Ok(initial_output(
                self.algorithm,
                &self.starts,
                Termination::Cancelled,
            ));
        }
        if self.round_limit == 0 {
            return Ok(initial_output(
                self.algorithm,
                &self.starts,
                Termination::RoundLimitReached { limit: 0 },
            ));
        }
        match self.algorithm {
            AlgorithmSelector::DropAndFreeze => self.run_drop(),
            AlgorithmSelector::HelpByScouts => self.run_help(),
            AlgorithmSelector::P1Tree => self.run_p1tree(),
            AlgorithmSelector::RootedDisp => self.run_rooted_disp(),
            AlgorithmSelector::GeneralDisp => self.run_general_disp(),
        }
    }

    fn run_drop(&self) -> Result<SimulationOutput, AdapterError> {
        match self.trace_mode {
            TraceMode::Off => {
                let mut metrics = ComplexityMetrics::default();
                let run = ccm_algorithms::drop_and_freeze::simulate_with(
                    &self.graph,
                    &self.starts,
                    self.round_limit,
                    &mut metrics,
                    SharedRecorder::new(NoTrace),
                )
                .map_err(|error| AdapterError::Drop(error.to_string()))?;
                Ok(output_from_drop(
                    run.final_state,
                    run.termination,
                    metrics,
                    None,
                ))
            }
            TraceMode::Full => {
                let mut metrics = ComplexityMetrics::default();
                let run = ccm_algorithms::drop_and_freeze::simulate_with(
                    &self.graph,
                    &self.starts,
                    self.round_limit,
                    &mut metrics,
                    SharedRecorder::new(FullTrace::new()),
                )
                .map_err(|error| AdapterError::Drop(error.to_string()))?;
                Ok(output_from_drop(
                    run.final_state,
                    run.termination,
                    metrics,
                    Some(encode_trace(&run.trace.into_trace(), false)),
                ))
            }
            TraceMode::Bounded => {
                let recorder = BoundedTrace::new(self.trace_budget())
                    .map_err(|error| AdapterError::Trace(error.to_string()))?;
                let mut metrics = ComplexityMetrics::default();
                let run = ccm_algorithms::drop_and_freeze::simulate_with(
                    &self.graph,
                    &self.starts,
                    self.round_limit,
                    &mut metrics,
                    SharedRecorder::new(recorder),
                )
                .map_err(|error| AdapterError::Drop(error.to_string()))?;
                let recorder = run.trace;
                let truncated = trace_was_truncated(&recorder);
                Ok(output_from_drop(
                    run.final_state,
                    run.termination,
                    metrics,
                    Some(encode_trace(&recorder.into_trace(), truncated)),
                ))
            }
        }
    }

    fn run_help(&self) -> Result<SimulationOutput, AdapterError> {
        match self.trace_mode {
            TraceMode::Off => {
                let (result, metrics, _) = ccm_algorithms::help_by_scouts::simulate(
                    &self.graph,
                    &self.starts,
                    self.round_limit,
                    ComplexityMetrics::default(),
                    NoTrace,
                )
                .map_err(|error| AdapterError::Help(error.to_string()))?;
                Ok(output_from_help(result, metrics, None))
            }
            TraceMode::Full => {
                let (result, metrics, recorder) = ccm_algorithms::help_by_scouts::simulate(
                    &self.graph,
                    &self.starts,
                    self.round_limit,
                    ComplexityMetrics::default(),
                    FullTrace::new(),
                )
                .map_err(|error| AdapterError::Help(error.to_string()))?;
                Ok(output_from_help(
                    result,
                    metrics,
                    Some(encode_trace(&recorder.into_trace(), false)),
                ))
            }
            TraceMode::Bounded => {
                let recorder = BoundedTrace::new(self.trace_budget())
                    .map_err(|error| AdapterError::Trace(error.to_string()))?;
                let (result, metrics, recorder) = ccm_algorithms::help_by_scouts::simulate(
                    &self.graph,
                    &self.starts,
                    self.round_limit,
                    ComplexityMetrics::default(),
                    recorder,
                )
                .map_err(|error| AdapterError::Help(error.to_string()))?;
                let truncated = trace_was_truncated(&recorder);
                Ok(output_from_help(
                    result,
                    metrics,
                    Some(encode_trace(&recorder.into_trace(), truncated)),
                ))
            }
        }
    }

    fn run_p1tree(&self) -> Result<SimulationOutput, AdapterError> {
        match self.trace_mode {
            TraceMode::Off => {
                let (result, metrics, _) = ccm_algorithms::p1tree::simulate(
                    &self.graph,
                    &self.starts,
                    self.round_limit,
                    ComplexityMetrics::default(),
                    NoTrace,
                )
                .map_err(|error| AdapterError::P1Tree(error.to_string()))?;
                Ok(output_from_p1tree(result, metrics, None))
            }
            TraceMode::Full => {
                let (result, metrics, recorder) = ccm_algorithms::p1tree::simulate(
                    &self.graph,
                    &self.starts,
                    self.round_limit,
                    ComplexityMetrics::default(),
                    FullTrace::new(),
                )
                .map_err(|error| AdapterError::P1Tree(error.to_string()))?;
                Ok(output_from_p1tree(
                    result,
                    metrics,
                    Some(encode_trace(&recorder.into_trace(), false)),
                ))
            }
            TraceMode::Bounded => {
                let recorder = BoundedTrace::new(self.trace_budget())
                    .map_err(|error| AdapterError::Trace(error.to_string()))?;
                let (result, metrics, recorder) = ccm_algorithms::p1tree::simulate(
                    &self.graph,
                    &self.starts,
                    self.round_limit,
                    ComplexityMetrics::default(),
                    recorder,
                )
                .map_err(|error| AdapterError::P1Tree(error.to_string()))?;
                let truncated = trace_was_truncated(&recorder);
                Ok(output_from_p1tree(
                    result,
                    metrics,
                    Some(encode_trace(&recorder.into_trace(), truncated)),
                ))
            }
        }
    }

    fn run_rooted_disp(&self) -> Result<SimulationOutput, AdapterError> {
        match self.trace_mode {
            TraceMode::Off => {
                let (result, metrics, _) = ccm_algorithms::rooted_disp::simulate(
                    &self.graph,
                    &self.starts,
                    self.round_limit,
                    ComplexityMetrics::default(),
                    NoTrace,
                )
                .map_err(|error| AdapterError::RootedDisp(error.to_string()))?;
                Ok(output_from_rooted_disp(result, metrics, None))
            }
            TraceMode::Full => {
                let (result, metrics, recorder) = ccm_algorithms::rooted_disp::simulate(
                    &self.graph,
                    &self.starts,
                    self.round_limit,
                    ComplexityMetrics::default(),
                    FullTrace::new(),
                )
                .map_err(|error| AdapterError::RootedDisp(error.to_string()))?;
                Ok(output_from_rooted_disp(
                    result,
                    metrics,
                    Some(encode_trace(&recorder.into_trace(), false)),
                ))
            }
            TraceMode::Bounded => {
                let recorder = BoundedTrace::new(self.trace_budget())
                    .map_err(|error| AdapterError::Trace(error.to_string()))?;
                let (result, metrics, recorder) = ccm_algorithms::rooted_disp::simulate(
                    &self.graph,
                    &self.starts,
                    self.round_limit,
                    ComplexityMetrics::default(),
                    recorder,
                )
                .map_err(|error| AdapterError::RootedDisp(error.to_string()))?;
                let truncated = trace_was_truncated(&recorder);
                Ok(output_from_rooted_disp(
                    result,
                    metrics,
                    Some(encode_trace(&recorder.into_trace(), truncated)),
                ))
            }
        }
    }

    fn run_general_disp(&self) -> Result<SimulationOutput, AdapterError> {
        match self.trace_mode {
            TraceMode::Off => {
                let (result, metrics, _) = ccm_algorithms::general_disp::simulate(
                    &self.graph,
                    &self.starts,
                    self.round_limit,
                    ComplexityMetrics::default(),
                    NoTrace,
                )
                .map_err(|error| AdapterError::GeneralDisp(error.to_string()))?;
                Ok(output_from_general_disp(result, metrics, None))
            }
            TraceMode::Full => {
                let (result, metrics, recorder) = ccm_algorithms::general_disp::simulate(
                    &self.graph,
                    &self.starts,
                    self.round_limit,
                    ComplexityMetrics::default(),
                    FullTrace::new(),
                )
                .map_err(|error| AdapterError::GeneralDisp(error.to_string()))?;
                Ok(output_from_general_disp(
                    result,
                    metrics,
                    Some(encode_trace(&recorder.into_trace(), false)),
                ))
            }
            TraceMode::Bounded => {
                let recorder = BoundedTrace::new(self.trace_budget())
                    .map_err(|error| AdapterError::Trace(error.to_string()))?;
                let (result, metrics, recorder) = ccm_algorithms::general_disp::simulate(
                    &self.graph,
                    &self.starts,
                    self.round_limit,
                    ComplexityMetrics::default(),
                    recorder,
                )
                .map_err(|error| AdapterError::GeneralDisp(error.to_string()))?;
                let truncated = trace_was_truncated(&recorder);
                Ok(output_from_general_disp(
                    result,
                    metrics,
                    Some(encode_trace(&recorder.into_trace(), truncated)),
                ))
            }
        }
    }

    fn trace_budget(&self) -> TraceBudget {
        let checkpoint_words = 1_usize
            .saturating_add(self.starts.len().saturating_mul(3))
            .saturating_add(self.graph.node_count());
        TraceBudget {
            max_event_count: self.max_trace_records,
            max_checkpoint_count: self.max_trace_records,
            max_event_words: self.max_trace_records.saturating_mul(64),
            max_checkpoint_words: self
                .max_trace_records
                .saturating_mul(checkpoint_words)
                .min(4_000_000),
            event_sample_every: self.sample_every,
            checkpoint_sample_every: self.sample_every,
        }
    }
}

/// Final typed state and compact render stream returned by one run.
#[wasm_bindgen]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SimulationOutput {
    algorithm: AlgorithmSelector,
    termination_code: u8,
    positions: Vec<u32>,
    statuses: Vec<u8>,
    homes: Vec<i32>,
    rounds: u64,
    moves: u64,
    probes: u64,
    trace: RenderPayload,
    groups: Vec<u8>,
}

#[wasm_bindgen]
impl SimulationOutput {
    /// `GeneralDisp` roles, groups, slots and merge events per frame, in the
    /// format of `encode_groups`; empty for other algorithms or without a trace.
    pub fn group_bytes(&self) -> js_sys::Uint8Array {
        js_sys::Uint8Array::from(self.groups.as_slice())
    }

    /// Dense agent-node positions as a JS Uint32Array.
    pub fn positions(&self) -> js_sys::Uint32Array {
        js_sys::Uint32Array::from(self.positions.as_slice())
    }

    /// Numeric status codes in dense agent order. Algorithms retain their
    /// native status conventions: settled=0, unsettled=1, waiting/scout=2.
    pub fn statuses(&self) -> js_sys::Uint8Array {
        js_sys::Uint8Array::from(self.statuses.as_slice())
    }

    /// Home node IDs, or -1 when an agent has no home.
    pub fn homes(&self) -> js_sys::Int32Array {
        js_sys::Int32Array::from(self.homes.as_slice())
    }

    /// Compact binary render records; this avoids one JS message per agent.
    pub fn render_bytes(&self) -> js_sys::Uint8Array {
        js_sys::Uint8Array::from(self.trace.bytes.as_slice())
    }

    #[must_use]
    pub fn algorithm(&self) -> AlgorithmSelector {
        self.algorithm
    }

    #[must_use]
    pub fn termination_code(&self) -> u8 {
        self.termination_code
    }

    #[must_use]
    pub fn rounds(&self) -> u64 {
        self.rounds
    }

    #[must_use]
    pub fn moves(&self) -> u64 {
        self.moves
    }

    #[must_use]
    pub fn probes(&self) -> u64 {
        self.probes
    }

    #[must_use]
    pub fn render_record_count(&self) -> u32 {
        self.trace.record_count
    }

    #[must_use]
    pub fn render_truncated(&self) -> bool {
        self.trace.truncated
    }

    #[must_use]
    pub fn render_byte_len(&self) -> u32 {
        self.trace.bytes.len() as u32
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RenderPayload {
    bytes: Vec<u8>,
    record_count: u32,
    truncated: bool,
}

fn graph_from_edges(node_count: usize, edges: &[u32]) -> Result<PortGraph, AdapterError> {
    if edges.len() % 2 != 0 {
        return Err(AdapterError::Input(
            "edges must contain source/destination pairs".to_owned(),
        ));
    }
    let pairs = edges
        .chunks_exact(2)
        .map(|pair| (NodeId(pair[0]), NodeId(pair[1])))
        .collect::<Vec<_>>();
    PortGraph::from_undirected_edges(node_count, &pairs)
        .map_err(|error| AdapterError::Graph(error.to_string()))
}

fn graph_from_port_tables(
    node_count: usize,
    offsets: &[u32],
    neighbors: &[u32],
    remote_ports: &[u16],
) -> Result<PortGraph, AdapterError> {
    if offsets.len() != node_count.saturating_add(1) {
        return Err(AdapterError::Input(
            "offsets must have node_count + 1 entries".to_owned(),
        ));
    }
    if neighbors.len() != remote_ports.len() {
        return Err(AdapterError::Input(
            "neighbors and remote_ports must have equal lengths".to_owned(),
        ));
    }
    let mut tables = Vec::with_capacity(node_count);
    for node in 0..node_count {
        let start = offsets[node] as usize;
        let end = offsets[node + 1] as usize;
        if start > end || end > neighbors.len() {
            return Err(AdapterError::Input("invalid CSR offsets".to_owned()));
        }
        tables.push(
            (start..end)
                .map(|index| PortEdge {
                    neighbor: NodeId(neighbors[index]),
                    remote_port: ccm_core::PortId(remote_ports[index]),
                })
                .collect(),
        );
    }
    PortGraph::from_port_tables(tables).map_err(|error| AdapterError::Graph(error.to_string()))
}

fn initial_output(
    algorithm: AlgorithmSelector,
    starts: &[NodeId],
    termination: Termination,
) -> SimulationOutput {
    SimulationOutput {
        algorithm,
        termination_code: termination_code(&termination),
        positions: starts.iter().map(|node| node.0).collect(),
        statuses: vec![1; starts.len()],
        homes: vec![-1; starts.len()],
        rounds: 0,
        moves: 0,
        probes: 0,
        trace: RenderPayload {
            bytes: Vec::new(),
            record_count: 0,
            truncated: false,
        },
        groups: Vec::new(),
    }
}

fn output_from_drop(
    state: DropState,
    termination: Termination,
    metrics: ComplexityMetrics,
    trace: Option<RenderPayload>,
) -> SimulationOutput {
    SimulationOutput {
        algorithm: AlgorithmSelector::DropAndFreeze,
        termination_code: termination_code(&termination),
        positions: state.agents.iter().map(|agent| agent.node.0).collect(),
        statuses: state
            .agents
            .iter()
            .map(|agent| drop_status(agent.status))
            .collect(),
        homes: vec![-1; state.agents.len()],
        rounds: metrics.rounds,
        moves: metrics.agent_moves,
        probes: metrics.port_probes,
        trace: trace.unwrap_or_else(empty_payload),
        groups: Vec::new(),
    }
}

fn output_from_help(
    result: ccm_algorithms::help_by_scouts::HelpResult,
    metrics: ComplexityMetrics,
    trace: Option<RenderPayload>,
) -> SimulationOutput {
    SimulationOutput {
        algorithm: AlgorithmSelector::HelpByScouts,
        termination_code: termination_code(&result.termination),
        positions: result.agents.iter().map(|agent| agent.node.0).collect(),
        statuses: result
            .agents
            .iter()
            .map(|agent| match agent.status {
                ccm_algorithms::help_by_scouts::HelpStatus::Settled => 0,
                ccm_algorithms::help_by_scouts::HelpStatus::Unsettled => 1,
                ccm_algorithms::help_by_scouts::HelpStatus::SettledScout => 2,
            })
            .collect(),
        homes: result
            .agents
            .iter()
            .map(|agent| agent.home.map_or(-1, |node| node.0 as i32))
            .collect(),
        rounds: metrics.rounds,
        moves: metrics.agent_moves,
        probes: metrics.port_probes,
        trace: trace.unwrap_or_else(empty_payload),
        groups: Vec::new(),
    }
}

fn output_from_p1tree(
    result: ccm_algorithms::p1tree::P1Result,
    metrics: ComplexityMetrics,
    trace: Option<RenderPayload>,
) -> SimulationOutput {
    SimulationOutput {
        algorithm: AlgorithmSelector::P1Tree,
        termination_code: termination_code(&result.termination),
        positions: result.agents.iter().map(|agent| agent.node.0).collect(),
        statuses: result
            .agents
            .iter()
            .map(|agent| match agent.status {
                ccm_algorithms::p1tree::P1Status::Settled => 0,
                ccm_algorithms::p1tree::P1Status::Unsettled => 1,
                // The renderer's third status is "settled but away", which is
                // what a travelling scout is.
                ccm_algorithms::p1tree::P1Status::SettledScout => 2,
            })
            .collect(),
        homes: result
            .agents
            .iter()
            .map(|agent| agent.home.map_or(-1, |node| node.0 as i32))
            .collect(),
        rounds: metrics.rounds,
        moves: metrics.agent_moves,
        probes: metrics.port_probes,
        trace: trace.unwrap_or_else(empty_payload),
        groups: Vec::new(),
    }
}

fn output_from_rooted_disp(
    result: ccm_algorithms::rooted_disp::RootedResult,
    metrics: ComplexityMetrics,
    trace: Option<RenderPayload>,
) -> SimulationOutput {
    SimulationOutput {
        algorithm: AlgorithmSelector::RootedDisp,
        termination_code: termination_code(&result.termination),
        positions: result.agents.iter().map(|agent| agent.node.0).collect(),
        statuses: result
            .agents
            .iter()
            .map(|agent| match agent.role {
                ccm_algorithms::rooted_disp::RootedRole::Settler => 0,
                ccm_algorithms::rooted_disp::RootedRole::Explorer => 1,
                // A helper is a settler away from home, the renderer's third
                // status.
                ccm_algorithms::rooted_disp::RootedRole::Helper => 2,
            })
            .collect(),
        homes: result
            .agents
            .iter()
            .map(|agent| agent.home.map_or(-1, |node| node.0 as i32))
            .collect(),
        rounds: metrics.rounds,
        moves: metrics.agent_moves,
        probes: metrics.port_probes,
        trace: trace.unwrap_or_else(empty_payload),
        groups: Vec::new(),
    }
}

fn output_from_general_disp(
    result: ccm_algorithms::general_disp::GeneralResult,
    metrics: ComplexityMetrics,
    trace: Option<RenderPayload>,
) -> SimulationOutput {
    use ccm_algorithms::general_disp::GeneralRole;
    let groups = if trace.is_some() {
        encode_groups(&result)
    } else {
        Vec::new()
    };
    SimulationOutput {
        algorithm: AlgorithmSelector::GeneralDisp,
        termination_code: termination_code(&result.termination),
        positions: result.agents.iter().map(|agent| agent.node.0).collect(),
        statuses: result
            .agents
            .iter()
            .map(|agent| match agent.role {
                GeneralRole::Settler => 0,
                GeneralRole::ActiveLeader | GeneralRole::WaitingLeader | GeneralRole::Zombie => 1,
                GeneralRole::Helper => 2,
            })
            .collect(),
        homes: result
            .agents
            .iter()
            .map(|agent| agent.home.map_or(-1, |node| node.0 as i32))
            .collect(),
        rounds: metrics.rounds,
        moves: metrics.agent_moves,
        probes: metrics.port_probes,
        trace: trace.unwrap_or_else(empty_payload),
        groups,
    }
}

/// The slot, 1–12, of `GeneralDisp` time step `step`.
#[wasm_bindgen]
#[must_use]
pub fn general_disp_slot(step: u64) -> u8 {
    ccm_algorithms::general_disp::slot_of(step)
}

/// The `GeneralDisp` group stream, little-endian.
///
/// Header `[version=1, kind=4, truncated, reserved, record_count:u32]`, then
/// records sorted by frame. Each starts `[record kind, frame:u64, slot:u8]`:
/// `frame` is the checkpoint step it shows in, `slot` the slot that produced
/// that frame (0 for frame 0).
/// - Kind 1, role change: `agent:u32, role:u8, level:u32, leader:u32`. Roles:
///   settler 0, helper 1, active leader 2, waiting leader 3, weak zombie 4,
///   strong zombie 5.
/// - Kind 2, group event: `code:u8, agent:u32, node:u32, a, b, c, d:u32`.
///   Codes and fields: became zombie 0 (stronger level, leader), settled 1
///   (group level, leader), incorporated 2 (from level, leader, to level,
///   leader), level up 3 (level, zombie), forward 4 (to), backward 5 (to),
///   zombie levels 6 (location, swarm; node is `u32::MAX`), chase 7 (to, weak).
fn encode_groups(result: &ccm_algorithms::general_disp::GeneralResult) -> Vec<u8> {
    use ccm_algorithms::general_disp::{slot_of, ShownRole, MAX_ROLE_CHANGES};
    let slot = |frame: u64| if frame == 0 { 0 } else { slot_of(frame - 1) };
    let mut records: Vec<(u64, Vec<u8>)> = Vec::new();
    for event in &result.events {
        let (step, code, agent, node, fields) = general_event_fields(event);
        let frame = step + 1;
        let mut bytes = vec![2];
        bytes.extend_from_slice(&frame.to_le_bytes());
        bytes.push(slot(frame));
        bytes.push(code);
        put_u32(&mut bytes, agent);
        put_u32(&mut bytes, node);
        for field in fields {
            put_u32(&mut bytes, field);
        }
        records.push((frame, bytes));
    }
    for change in &result.roles {
        let role = match change.role {
            ShownRole::Settler => 0,
            ShownRole::Helper => 1,
            ShownRole::ActiveLeader => 2,
            ShownRole::WaitingLeader => 3,
            ShownRole::WeakZombie => 4,
            ShownRole::StrongZombie => 5,
        };
        let mut bytes = vec![1];
        bytes.extend_from_slice(&change.step.to_le_bytes());
        bytes.push(slot(change.step));
        put_u32(&mut bytes, change.agent.0);
        bytes.push(role);
        put_u32(&mut bytes, change.group.level);
        put_u32(&mut bytes, change.group.leader.0);
        records.push((change.step, bytes));
    }
    records.sort_by_key(|(frame, _)| *frame);
    let truncated = result.roles.len() >= MAX_ROLE_CHANGES;
    let mut bytes = stream_header(4, records.len() as u32, truncated);
    for (_, record) in records {
        bytes.extend_from_slice(&record);
    }
    bytes
}

fn general_event_fields(
    event: &ccm_algorithms::general_disp::GroupEvent,
) -> (u64, u8, u32, u32, [u32; 4]) {
    use ccm_algorithms::general_disp::GroupEvent as E;
    match *event {
        E::BecameZombie {
            step,
            agent,
            node,
            stronger,
        } => (
            step,
            0,
            agent.0,
            node.0,
            [stronger.level, stronger.leader.0, 0, 0],
        ),
        E::Settled {
            step,
            agent,
            node,
            group,
        } => (
            step,
            1,
            agent.0,
            node.0,
            [group.level, group.leader.0, 0, 0],
        ),
        E::Incorporated {
            step,
            settler,
            node,
            from,
            to,
        } => (
            step,
            2,
            settler.0,
            node.0,
            [from.level, from.leader.0, to.level, to.leader.0],
        ),
        E::LevelUp {
            step,
            leader,
            zombie,
            node,
            level,
        } => (step, 3, leader.0, node.0, [level, zombie.0, 0, 0]),
        E::ForwardMove {
            step,
            leader,
            from,
            to,
        } => (step, 4, leader.0, from.0, [to.0, 0, 0, 0]),
        E::BackwardMove {
            step,
            leader,
            from,
            to,
        } => (step, 5, leader.0, from.0, [to.0, 0, 0, 0]),
        E::ZombieLevels {
            step,
            zombie,
            location,
            swarm,
        } => (step, 6, zombie.0, u32::MAX, [location, swarm, 0, 0]),
        E::Chased {
            step,
            zombie,
            from,
            to,
            weak,
        } => (step, 7, zombie.0, from.0, [to.0, u32::from(weak), 0, 0]),
    }
}

fn empty_payload() -> RenderPayload {
    RenderPayload {
        bytes: Vec::new(),
        record_count: 0,
        truncated: false,
    }
}

fn trace_was_truncated(recorder: &BoundedTrace) -> bool {
    let stats = recorder.stats();
    stats.sampled_events > 0
        || stats.sampled_checkpoints > 0
        || stats.retained_event_evictions > 0
        || stats.retained_checkpoint_evictions > 0
        || stats.dropped_events > 0
        || stats.dropped_checkpoints > 0
}

fn drop_status(status: DropStatus) -> u8 {
    match status {
        DropStatus::Settled => 0,
        DropStatus::Unsettled => 1,
        DropStatus::SettledWaiting => 2,
    }
}

fn termination_code(termination: &Termination) -> u8 {
    match termination {
        Termination::Completed => 0,
        Termination::RoundLimitReached { .. } => 1,
        Termination::Cancelled => 2,
        Termination::InvalidConfiguration(_) | Termination::InvariantViolation(_) => 3,
    }
}

fn encode_trace(trace: &Trace, truncated: bool) -> RenderPayload {
    let record_count = trace.events.len().saturating_add(trace.checkpoints.len());
    let mut bytes = stream_header(2, record_count as u32, truncated);
    let mut event_index = 0;
    let mut checkpoint_index = 0;
    while event_index < trace.events.len() || checkpoint_index < trace.checkpoints.len() {
        let next_event_step = trace.events.get(event_index).map(|event| event.step);
        let next_checkpoint_step = trace
            .checkpoints
            .get(checkpoint_index)
            .map(|checkpoint| checkpoint.step);
        if next_event_step.is_some()
            && (next_checkpoint_step.is_none() || next_event_step <= next_checkpoint_step)
        {
            let event = &trace.events[event_index];
            encode_event(&mut bytes, event.step, &event.event);
            event_index += 1;
        } else {
            encode_checkpoint(&mut bytes, &trace.checkpoints[checkpoint_index]);
            checkpoint_index += 1;
        }
    }
    RenderPayload {
        bytes,
        record_count: record_count as u32,
        truncated,
    }
}

fn stream_header(kind: u8, record_count: u32, truncated: bool) -> Vec<u8> {
    let mut bytes = vec![1, kind, u8::from(truncated), 0];
    put_u32(&mut bytes, record_count);
    bytes
}

fn encode_event(bytes: &mut Vec<u8>, step: u64, event: &SimulationEvent) {
    bytes.push(2);
    put_u64(bytes, step);
    match event {
        SimulationEvent::PhaseStarted { phase } => {
            bytes.push(0);
            bytes.push(phase_code(*phase));
        }
        SimulationEvent::AgentMoved {
            agent,
            from,
            to,
            out_port,
            in_port,
        } => {
            bytes.push(1);
            put_u32(bytes, agent.0);
            put_u32(bytes, from.0);
            put_u32(bytes, to.0);
            put_u16(bytes, out_port.0);
            put_optional_u16(bytes, in_port.map(|port| port.0));
        }
        SimulationEvent::GroupMoved {
            agents,
            from,
            to,
            out_port,
        } => {
            bytes.push(2);
            put_u32(bytes, agents.len() as u32);
            for agent in agents {
                put_u32(bytes, agent.0);
            }
            put_u32(bytes, from.0);
            put_u32(bytes, to.0);
            put_u16(bytes, out_port.0);
        }
        SimulationEvent::AgentSettled { agent, node } => {
            bytes.push(3);
            put_u32(bytes, agent.0);
            put_u32(bytes, node.0);
        }
        SimulationEvent::AgentStateChanged { agent, from, to } => {
            bytes.push(4);
            put_u32(bytes, agent.0);
            bytes.push(status_code(*from));
            bytes.push(status_code(*to));
        }
        SimulationEvent::TreeEdgeAdded {
            parent,
            child,
            parent_node,
            child_node,
            parent_port,
            child_port,
        } => {
            bytes.push(5);
            put_u32(bytes, parent.0);
            put_u32(bytes, child.0);
            put_u32(bytes, parent_node.0);
            put_u32(bytes, child_node.0);
            put_optional_u16(bytes, parent_port.map(|port| port.0));
            put_optional_u16(bytes, child_port.map(|port| port.0));
        }
        SimulationEvent::NodeStateChanged { node, occupied } => {
            bytes.push(6);
            put_u32(bytes, node.0);
            bytes.push(u8::from(*occupied));
        }
    }
}

fn encode_checkpoint(bytes: &mut Vec<u8>, checkpoint: &Checkpoint) {
    bytes.push(3);
    put_u64(bytes, checkpoint.step);
    put_u32(bytes, checkpoint.agent_nodes.len() as u32);
    for (index, node) in checkpoint.agent_nodes.iter().enumerate() {
        put_u32(bytes, node.0);
        bytes.push(
            checkpoint
                .agent_statuses
                .get(index)
                .map_or(0, |status| status_code(*status)),
        );
        put_optional_u32(
            bytes,
            checkpoint
                .home_nodes
                .get(index)
                .and_then(|node| node.map(|value| value.0)),
        );
    }
}

fn phase_code(phase: Phase) -> u8 {
    match phase {
        Phase::MacroRound => 0,
        Phase::ProbeOut => 1,
        Phase::ProbeBack => 2,
        Phase::Movement => 3,
        Phase::Scout => 4,
        Phase::Vacate => 5,
        Phase::Chase => 6,
        Phase::Follow => 7,
        Phase::Retrace => 8,
        Phase::Other => 9,
        Phase::HelperReturn => 10,
    }
}

fn status_code(status: AgentStatus) -> u8 {
    match status {
        AgentStatus::Unsettled => 1,
        AgentStatus::Settled => 0,
        AgentStatus::SettledWaiting | AgentStatus::SettledScout => 2,
    }
}

fn put_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn put_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn put_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn put_optional_u16(bytes: &mut Vec<u8>, value: Option<u16>) {
    match value {
        Some(value) => {
            bytes.push(1);
            put_u16(bytes, value);
        }
        None => bytes.push(0),
    }
}

fn put_optional_u32(bytes: &mut Vec<u8>, value: Option<u32>) {
    match value {
        Some(value) => {
            bytes.push(1);
            put_u32(bytes, value);
        }
        None => bytes.push(0),
    }
}

fn to_js(error: AdapterError) -> JsValue {
    JsValue::from_str(&error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path_simulation(
        algorithm: AlgorithmSelector,
        mode: TraceMode,
        limit: u64,
        max_records: u32,
    ) -> WasmSimulation {
        WasmSimulation::new(
            3,
            &[0, 1, 1, 2],
            &[0, 0, 0],
            algorithm,
            mode,
            limit,
            max_records,
            1,
        )
        .unwrap()
    }

    #[test]
    fn native_protocol_runs_both_selectors_without_json() {
        let mut drop = path_simulation(AlgorithmSelector::DropAndFreeze, TraceMode::Full, 3, 0);
        let drop_output = drop.run_internal().unwrap();
        assert_eq!(drop_output.termination_code, 0);
        assert_eq!(drop_output.positions, vec![0, 1, 2]);
        assert!(drop_output.trace.record_count > 0);

        let mut help = path_simulation(AlgorithmSelector::HelpByScouts, TraceMode::Full, 200, 0);
        let help_output = help.run_internal().unwrap();
        assert_eq!(help_output.termination_code, 0);
        assert_eq!(help_output.positions, vec![2, 1, 0]);
        assert!(help_output.trace.record_count > 0);
    }

    #[test]
    fn off_trace_has_no_render_bytes_and_bounded_trace_is_deterministic() {
        let mut off = path_simulation(AlgorithmSelector::DropAndFreeze, TraceMode::Off, 3, 0);
        let off_output = off.run_internal().unwrap();
        assert_eq!(off_output.trace, empty_payload());

        let mut left = path_simulation(AlgorithmSelector::HelpByScouts, TraceMode::Bounded, 200, 2);
        let mut right =
            path_simulation(AlgorithmSelector::HelpByScouts, TraceMode::Bounded, 200, 2);
        let left_output = left.run_internal().unwrap();
        let right_output = right.run_internal().unwrap();
        assert_eq!(left_output, right_output);
        assert!(left_output.trace.record_count <= 2);
    }

    #[test]
    fn semantic_stream_interleaves_events_and_checkpoints_by_step() {
        let trace = Trace {
            events: vec![
                ccm_trace::RecordedEvent {
                    step: 1,
                    event: SimulationEvent::PhaseStarted {
                        phase: Phase::Scout,
                    },
                },
                ccm_trace::RecordedEvent {
                    step: 3,
                    event: SimulationEvent::PhaseStarted {
                        phase: Phase::Retrace,
                    },
                },
            ],
            checkpoints: vec![Checkpoint {
                step: 2,
                agent_nodes: Vec::new(),
                agent_statuses: Vec::new(),
                home_nodes: Vec::new(),
                settled_agents: Vec::new(),
            }],
        };
        let payload = encode_trace(&trace, false);
        assert_eq!(payload.bytes[8], 2);
        assert_eq!(
            u64::from_le_bytes(payload.bytes[9..17].try_into().unwrap()),
            1
        );
        assert_eq!(payload.bytes[19], 3);
        assert_eq!(
            u64::from_le_bytes(payload.bytes[20..28].try_into().unwrap()),
            2
        );
        assert_eq!(payload.bytes[32], 2);
        assert_eq!(
            u64::from_le_bytes(payload.bytes[33..41].try_into().unwrap()),
            3
        );
    }

    #[test]
    fn cancellation_and_zero_limit_are_explicit_terminations() {
        let mut cancelled =
            path_simulation(AlgorithmSelector::DropAndFreeze, TraceMode::Full, 3, 0);
        cancelled.cancel();
        assert_eq!(cancelled.run_internal().unwrap().termination_code, 2);

        let mut limited = path_simulation(AlgorithmSelector::DropAndFreeze, TraceMode::Full, 0, 0);
        assert_eq!(limited.run_internal().unwrap().termination_code, 1);
    }

    /// Walks a version-1 semantic stream and returns the phase code of every
    /// `PhaseStarted` record, failing if any record is malformed.
    fn phase_codes(bytes: &[u8]) -> Vec<u8> {
        let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        let optional = |at: usize, width: usize| if bytes[at] == 0 { 1 } else { 1 + width };
        assert_eq!((bytes[0], bytes[1]), (1, 2), "version 1, semantic stream");
        let records = u32_at(4);
        let mut at = 8;
        let mut phases = Vec::new();
        for _ in 0..records {
            let kind = bytes[at];
            at += 9; // record kind + u64 step
            if kind == 3 {
                let agents = u32_at(at);
                at += 4;
                for _ in 0..agents {
                    at += 5; // node + status
                    at += optional(at, 4);
                }
                continue;
            }
            assert_eq!(kind, 2, "event or checkpoint record");
            let event = bytes[at];
            at += 1;
            match event {
                0 => {
                    phases.push(bytes[at]);
                    at += 1;
                }
                1 => {
                    at += 14;
                    at += optional(at, 2);
                }
                2 => at += 4 + 4 * u32_at(at) + 10,
                3 => at += 8,
                4 => at += 6,
                5 => {
                    at += 16;
                    at += optional(at, 2);
                    at += optional(at, 2);
                }
                6 => at += 5,
                other => panic!("unknown event kind {other}"),
            }
        }
        assert_eq!(at, bytes.len(), "the stream ends exactly after its records");
        phases
    }

    #[test]
    fn rooted_disp_runs_and_streams_its_helper_return_phase() {
        let mut rooted = path_simulation(AlgorithmSelector::RootedDisp, TraceMode::Full, 100, 0);
        let output = rooted.run_internal().unwrap();
        assert_eq!(output.termination_code, 0);
        assert_eq!(output.positions, vec![0, 1, 2]);
        assert_eq!(output.homes, vec![0, 1, 2]);
        assert_eq!(output.statuses, vec![0, 0, 0]);
        assert!(output.trace.record_count > 0);
        assert!(phase_codes(&output.trace.bytes).contains(&phase_code(Phase::HelperReturn)));
    }

    #[test]
    fn rooted_disp_reproduces_the_star_hand_trace_with_tracing_off() {
        let edges: Vec<u32> = (1..9).flat_map(|leaf| [0, leaf]).collect();
        let mut star = WasmSimulation::new(
            9,
            &edges,
            &[0; 9],
            AlgorithmSelector::RootedDisp,
            TraceMode::Off,
            1_000,
            0,
            1,
        )
        .unwrap();
        let output = star.run_internal().unwrap();
        assert_eq!(output.termination_code, 0);
        assert_eq!((output.rounds, output.probes, output.moves), (73, 65, 264));
        assert_eq!(output.trace, empty_payload());
    }

    #[test]
    fn rooted_disp_refuses_a_scattered_start() {
        let mut scattered = WasmSimulation::new(
            3,
            &[0, 1, 1, 2],
            &[0, 2],
            AlgorithmSelector::RootedDisp,
            TraceMode::Off,
            100,
            0,
            1,
        )
        .unwrap();
        assert!(matches!(
            scattered.run_internal(),
            Err(AdapterError::RootedDisp(message)) if message.contains("NonRootedStart")
        ));
    }

    /// Path of 6 nodes with agents on 0, 0, 0, 0, 3, 3.
    fn two_groups(mode: TraceMode) -> WasmSimulation {
        WasmSimulation::new(
            6,
            &[0, 1, 1, 2, 2, 3, 3, 4, 4, 5],
            &[0, 0, 0, 0, 3, 3],
            AlgorithmSelector::GeneralDisp,
            mode,
            100_000,
            0,
            1,
        )
        .unwrap()
    }

    #[test]
    fn general_disp_runs_a_scattered_start_like_the_native_call() {
        let mut simulation = two_groups(TraceMode::Off);
        let output = simulation.run_internal().unwrap();
        let (native, metrics, _) = ccm_algorithms::general_disp::simulate(
            &simulation.graph,
            &simulation.starts,
            100_000,
            ComplexityMetrics::default(),
            NoTrace,
        )
        .unwrap();
        assert_eq!(output.termination_code, 0);
        assert_eq!(output.rounds, native.steps);
        assert_eq!(output.moves, metrics.agent_moves);
        assert_eq!(output.positions, vec![0, 1, 2, 5, 3, 4]);
        assert_eq!(output.statuses, vec![0, 0, 0, 1, 0, 0]);
        assert_eq!(output.homes, vec![0, 1, 2, -1, 3, 4]);
        assert_eq!(output.trace, empty_payload());
    }

    #[test]
    fn general_disp_streams_a_well_formed_trace() {
        let output = two_groups(TraceMode::Full).run_internal().unwrap();
        assert_eq!(output.termination_code, 0);
        assert!(output.trace.record_count > 0);
        assert!(!phase_codes(&output.trace.bytes).is_empty());
        let bounded = two_groups(TraceMode::Bounded);
        let mut bounded = WasmSimulation {
            max_trace_records: 8,
            ..bounded
        };
        let output = bounded.run_internal().unwrap();
        assert!(output.trace.record_count <= 16);
        assert!(output.trace.truncated);
    }

    /// Decodes a group stream into `(kind, frame, slot, fields)` records.
    fn group_records(bytes: &[u8]) -> Vec<(u8, u64, u8, Vec<u32>)> {
        let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        assert_eq!((bytes[0], bytes[1]), (1, 4), "version 1, group stream");
        let mut at = 8;
        let mut records = Vec::new();
        for _ in 0..u32_at(4) {
            let kind = bytes[at];
            let frame = u64::from_le_bytes(bytes[at + 1..at + 9].try_into().unwrap());
            let slot = bytes[at + 9];
            at += 10;
            let fields = match kind {
                1 => {
                    let fields = vec![
                        u32_at(at),
                        u32::from(bytes[at + 4]),
                        u32_at(at + 5),
                        u32_at(at + 9),
                    ];
                    at += 13;
                    fields
                }
                2 => {
                    let mut fields = vec![u32::from(bytes[at])];
                    fields.extend((0..6).map(|i| u32_at(at + 1 + 4 * i)));
                    at += 25;
                    fields
                }
                other => panic!("unknown group record kind {other}"),
            };
            records.push((kind, frame, slot, fields));
        }
        assert_eq!(at, bytes.len(), "the stream ends exactly after its records");
        records
    }

    #[test]
    fn general_disp_streams_roles_slots_and_merges() {
        let output = two_groups(TraceMode::Full).run_internal().unwrap();
        let records = group_records(&output.groups);
        let first: Vec<&Vec<u32>> = records
            .iter()
            .filter(|record| record.0 == 1 && record.1 == 0)
            .map(|record| &record.3)
            .collect();
        assert_eq!(first.len(), 6);
        assert!(first
            .iter()
            .all(|fields| fields[1] == 2 && fields[2] == 1 && fields[3] == fields[0]));
        assert!(records.contains(&(2, 74, 2, vec![2, 4, 3, 1, 5, 2, 3])));
        let mut roles = vec![u32::MAX; 6];
        for record in records.iter().filter(|record| record.0 == 1) {
            roles[record.3[0] as usize] = record.3[1];
        }
        assert_eq!(roles, vec![0, 0, 0, 3, 0, 0]);
        assert!(records.windows(2).all(|pair| pair[0].1 <= pair[1].1));
        for record in &records {
            let slot = if record.1 == 0 {
                0
            } else {
                general_disp_slot(record.1 - 1)
            };
            assert_eq!(record.2, slot);
        }
        assert!(two_groups(TraceMode::Off)
            .run_internal()
            .unwrap()
            .groups
            .is_empty());
        assert!(
            path_simulation(AlgorithmSelector::RootedDisp, TraceMode::Full, 100, 0)
                .run_internal()
                .unwrap()
                .groups
                .is_empty()
        );
    }

    #[test]
    fn port_table_input_preserves_reciprocal_labels() {
        let simulation = WasmSimulation::from_port_tables(
            2,
            &[0, 1, 2],
            &[1, 0],
            &[0, 0],
            &[0],
            AlgorithmSelector::HelpByScouts,
            TraceMode::Off,
            10,
            0,
            1,
        )
        .unwrap();
        assert_eq!(
            simulation
                .graph
                .traverse(NodeId(0), ccm_core::PortId(0))
                .unwrap()
                .neighbor,
            NodeId(1)
        );
    }
}
