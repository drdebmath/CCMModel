use crate::{generate_graph, place_agents, GraphSpec, PlacementSpec};
use ccm_core::{Algorithm, ComplexityMetrics, NodeId, PortAssignment, PortGraph, Termination};
use ccm_trace::NoTrace;
use core::fmt;

/// One fully specified independent simulation trial.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunRequest {
    pub algorithm: Algorithm,
    pub graph: GraphSpec,
    pub port_assignment: PortAssignment,
    pub placement: PlacementSpec,
    pub agents: usize,
    pub seed: u64,
    pub round_limit: Option<u64>,
}

impl RunRequest {
    #[must_use]
    pub fn graph_label(&self) -> String {
        self.graph.label()
    }
}

/// Generated graph and dense starting vector passed to an algorithm adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedRun {
    pub request: RunRequest,
    pub graph: PortGraph,
    pub starts: Vec<NodeId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RunError {
    InvalidConfiguration(ccm_core::InvalidConfiguration),
    Graph(crate::GraphGenerationError),
    Placement(crate::PlacementError),
    AlgorithmUnavailable(Algorithm),
    Message(String),
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for RunError {}

impl From<crate::GraphGenerationError> for RunError {
    fn from(value: crate::GraphGenerationError) -> Self {
        Self::Graph(value)
    }
}

impl From<crate::PlacementError> for RunError {
    fn from(value: crate::PlacementError) -> Self {
        Self::Placement(value)
    }
}

/// Algorithm implementations consume prepared runs through this interface.
/// The trait deliberately does not prescribe algorithm-local state or traces.
pub trait SimulationRunner: Sync {
    /// Executes exactly one trial.  Implementations must not use wall-clock
    /// time, unordered iteration, or shared mutable state to affect results.
    ///
    /// # Errors
    ///
    /// Returns a configuration, generation, placement, or algorithm error.
    fn run(&self, prepared: &PreparedRun) -> Result<RunResult, RunError>;
}

/// Executes the authoritative Rust implementations shipped in this workspace.
#[derive(Clone, Copy, Debug, Default)]
pub struct BuiltinRunner;

impl SimulationRunner for BuiltinRunner {
    fn run(&self, prepared: &PreparedRun) -> Result<RunResult, RunError> {
        let limit = prepared.request.round_limit.unwrap_or_else(|| {
            40_u64.saturating_mul(u64::try_from(prepared.request.agents.max(1)).unwrap_or(u64::MAX))
        });
        let (termination, metrics) = match prepared.request.algorithm {
            Algorithm::DropAndFreeze => {
                let result = ccm_algorithms::simulate(&prepared.graph, &prepared.starts, limit)
                    .map_err(|error| RunError::Message(error.to_string()))?;
                (result.termination, result.metrics)
            }
            Algorithm::HelpByScouts => {
                let (result, metrics, _) = ccm_help_scouts::simulate(
                    &prepared.graph,
                    &prepared.starts,
                    limit,
                    ComplexityMetrics::default(),
                    NoTrace,
                )
                .map_err(|error| RunError::Message(error.to_string()))?;
                (result.termination, metrics)
            }
            Algorithm::P1Tree => {
                let (result, metrics, _) = ccm_p1tree::simulate(
                    &prepared.graph,
                    &prepared.starts,
                    limit,
                    ComplexityMetrics::default(),
                    NoTrace,
                )
                .map_err(|error| RunError::Message(error.to_string()))?;
                (result.termination, metrics)
            }
        };
        Ok(RunResult {
            algorithm: prepared.request.algorithm,
            algorithm_version: match prepared.request.algorithm {
                Algorithm::DropAndFreeze => ccm_algorithms::ALGORITHM_VERSION,
                Algorithm::HelpByScouts => ccm_help_scouts::ALGORITHM_VERSION,
                Algorithm::P1Tree => ccm_p1tree::ALGORITHM_VERSION,
            },
            git_commit: option_env!("CCM_GIT_COMMIT").unwrap_or("unknown"),
            graph_family: prepared.request.graph.label(),
            port_assignment: port_assignment_label(prepared.request.port_assignment),
            placement: prepared.request.placement.label(),
            nodes: prepared.graph.node_count(),
            edges: prepared.graph.edge_count(),
            agents: prepared.request.agents,
            seed: prepared.request.seed,
            starts: prepared.starts.clone(),
            termination,
            metrics,
        })
    }
}

/// Canonical result row emitted by native experiments.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunResult {
    pub algorithm: Algorithm,
    pub algorithm_version: &'static str,
    pub git_commit: &'static str,
    pub graph_family: String,
    pub port_assignment: &'static str,
    pub placement: String,
    pub nodes: usize,
    pub edges: usize,
    pub agents: usize,
    pub seed: u64,
    pub starts: Vec<NodeId>,
    pub termination: Termination,
    pub metrics: ComplexityMetrics,
}

impl RunResult {
    #[must_use]
    pub fn empty(prepared: &PreparedRun, termination: Termination) -> Self {
        Self {
            algorithm: prepared.request.algorithm,
            algorithm_version: match prepared.request.algorithm {
                Algorithm::DropAndFreeze => ccm_algorithms::ALGORITHM_VERSION,
                Algorithm::HelpByScouts => ccm_help_scouts::ALGORITHM_VERSION,
                Algorithm::P1Tree => ccm_p1tree::ALGORITHM_VERSION,
            },
            git_commit: option_env!("CCM_GIT_COMMIT").unwrap_or("unknown"),
            graph_family: prepared.request.graph.label(),
            port_assignment: port_assignment_label(prepared.request.port_assignment),
            placement: prepared.request.placement.label(),
            nodes: prepared.graph.node_count(),
            edges: prepared.graph.edge_count(),
            agents: prepared.request.agents,
            seed: prepared.request.seed,
            starts: prepared.starts.clone(),
            termination,
            metrics: ComplexityMetrics::default(),
        }
    }

    #[must_use]
    pub fn completed(&self) -> bool {
        matches!(self.termination, Termination::Completed)
    }
}

#[must_use]
pub const fn port_assignment_label(assignment: PortAssignment) -> &'static str {
    match assignment {
        PortAssignment::Canonical => "canonical",
        PortAssignment::Random => "random",
        PortAssignment::Adversarial => "adversarial",
        PortAssignment::Explicit => "explicit",
    }
}

/// Generates the graph and starts once, before invoking an algorithm.
///
/// # Errors
///
/// Returns an invalid-configuration, graph-generation, or placement error.
pub fn prepare_run(request: RunRequest) -> Result<PreparedRun, RunError> {
    if request.graph.nodes == 0 {
        return Err(RunError::InvalidConfiguration(
            ccm_core::InvalidConfiguration::EmptyGraph,
        ));
    }
    if request.agents > request.graph.nodes {
        return Err(RunError::InvalidConfiguration(
            ccm_core::InvalidConfiguration::TooManyAgents {
                agents: request.agents,
                nodes: request.graph.nodes,
            },
        ));
    }
    let graph = generate_graph(&request.graph, request.port_assignment, request.seed)?;
    let starts = place_agents(
        graph.node_count(),
        request.agents,
        &request.placement,
        request.seed,
    )?;
    Ok(PreparedRun {
        request,
        graph,
        starts,
    })
}

/// Prepares and executes one deterministic trial.
///
/// # Errors
///
/// Returns a preparation or algorithm error.
pub fn execute_one<R: SimulationRunner>(
    runner: &R,
    request: RunRequest,
) -> Result<RunResult, RunError> {
    let prepared = prepare_run(request)?;
    runner.run(&prepared)
}

/// Executes independent trials in bounded worker groups while preserving input
/// order.  The worker count affects throughput only, never result ordering or
/// per-trial seeds.
///
/// # Panics
///
/// Panics if an algorithm runner panics inside a worker thread. Algorithm
/// errors are returned as row results and do not panic.
pub fn execute_many_parallel<R: SimulationRunner>(
    runner: &R,
    requests: &[RunRequest],
    workers: usize,
) -> Vec<Result<RunResult, RunError>> {
    if requests.is_empty() {
        return Vec::new();
    }
    let worker_count = workers.max(1).min(requests.len());
    let chunk_size = requests.len().div_ceil(worker_count);
    std::thread::scope(|scope| {
        let handles: Vec<_> = requests
            .chunks(chunk_size)
            .map(|chunk| {
                scope.spawn(move || {
                    chunk
                        .iter()
                        .cloned()
                        .map(|request| execute_one(runner, request))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().expect("experiment worker panicked"))
            .collect()
    })
}

#[must_use]
pub fn algorithm_label(algorithm: Algorithm) -> &'static str {
    match algorithm {
        Algorithm::DropAndFreeze => "drop_and_freeze",
        Algorithm::HelpByScouts => "help_by_scouts",
        Algorithm::P1Tree => "p1tree",
    }
}

#[must_use]
pub fn termination_label(termination: &Termination) -> &'static str {
    match termination {
        Termination::Completed => "completed",
        Termination::RoundLimitReached { .. } => "round_limit_reached",
        Termination::InvalidConfiguration(_) => "invalid_configuration",
        Termination::Cancelled => "cancelled",
        Termination::InvariantViolation(_) => "invariant_violation",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GraphFamily;

    struct DeterministicStub;

    impl SimulationRunner for DeterministicStub {
        fn run(&self, prepared: &PreparedRun) -> Result<RunResult, RunError> {
            Ok(RunResult::empty(prepared, Termination::Completed))
        }
    }

    fn request(seed: u64) -> RunRequest {
        RunRequest {
            algorithm: Algorithm::DropAndFreeze,
            graph: GraphSpec::new(GraphFamily::Path, 5),
            port_assignment: PortAssignment::Canonical,
            placement: PlacementSpec::SingleNode { node: NodeId(0) },
            agents: 3,
            seed,
            round_limit: Some(20),
        }
    }

    #[test]
    fn preparation_is_reproducible() {
        let first = prepare_run(request(7)).unwrap();
        let second = prepare_run(request(7)).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn parallel_execution_preserves_single_run_results_and_order() {
        let requests: Vec<_> = (0..12).map(request).collect();
        let runner = DeterministicStub;
        let sequential: Vec<_> = requests
            .iter()
            .cloned()
            .map(|request| execute_one(&runner, request))
            .collect();
        let parallel = execute_many_parallel(&runner, &requests, 3);
        assert_eq!(parallel, sequential);
    }
}
