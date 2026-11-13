use crate::{generate_graph, place_agents, GraphSpec, PlacementSpec};
use ccm_core::{
    AgentId, Algorithm, ComplexityMetrics, NodeId, PortAssignment, PortGraph, SchedulerModel,
    Termination,
};
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
    pub scheduler: SchedulerModel,
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
    /// The algorithm is defined only for a rooted start, every agent on one
    /// node, and the placement scattered them. Refused before the run rather
    /// than run under a model the algorithm was not written for.
    RequiresRootedStart {
        algorithm: Algorithm,
        placement: String,
    },
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
        let request = &prepared.request;
        if request.algorithm != Algorithm::RootedAsync
            && request.scheduler != SchedulerModel::Synchronous
        {
            return Err(RunError::Message(format!(
                "{} runs only under the synchronous scheduler",
                algorithm_label(request.algorithm)
            )));
        }
        check_placement(prepared)?;
        let limit = request
            .round_limit
            .unwrap_or_else(|| default_round_limit(request.algorithm, request.agents));
        let (termination, metrics) = match prepared.request.algorithm {
            Algorithm::DropAndFreeze => {
                let result = ccm_algorithms::drop_and_freeze::simulate(
                    &prepared.graph,
                    &prepared.starts,
                    limit,
                )
                .map_err(|error| RunError::Message(error.to_string()))?;
                (result.termination, result.metrics)
            }
            Algorithm::HelpByScouts => {
                let (result, metrics, _) = ccm_algorithms::help_by_scouts::simulate(
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
                let (result, metrics, _) = ccm_algorithms::p1tree::simulate(
                    &prepared.graph,
                    &prepared.starts,
                    limit,
                    ComplexityMetrics::default(),
                    NoTrace,
                )
                .map_err(|error| RunError::Message(error.to_string()))?;
                (result.termination, metrics)
            }
            Algorithm::RootedAsync => {
                let (result, metrics, _) = ccm_algorithms::rooted_async::simulate(
                    &prepared.graph,
                    &prepared.starts,
                    rooted_async_schedule(request.scheduler, request.seed),
                    limit,
                    ComplexityMetrics::default(),
                    NoTrace,
                )
                .map_err(|error| RunError::Message(error.to_string()))?;
                (result.termination, metrics)
            }
            Algorithm::RootedDisp => {
                let (result, metrics, _) = ccm_algorithms::rooted_disp::simulate(
                    &prepared.graph,
                    &prepared.starts,
                    limit,
                    ComplexityMetrics::default(),
                    NoTrace,
                )
                .map_err(|error| RunError::Message(error.to_string()))?;
                (result.termination, metrics)
            }
            Algorithm::GeneralDisp => {
                let (result, metrics, _) = ccm_algorithms::general_disp::simulate(
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
            algorithm_version: ccm_algorithms::ALGORITHM_VERSION,
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
            scheduler: request.scheduler,
        })
    }
}

/// The schedule a request asks for, seeded from the run's own seed so a run is
/// reproducible from its CSV row.
#[must_use]
pub fn rooted_async_schedule(
    scheduler: SchedulerModel,
    seed: u64,
) -> ccm_algorithms::rooted_async::Schedule {
    use ccm_algorithms::rooted_async::Schedule;
    match scheduler {
        SchedulerModel::Synchronous => Schedule::Synchronous,
        SchedulerModel::Asynchronous => Schedule::Random { seed },
        SchedulerModel::Interleaved => Schedule::Interleaved { seed },
        SchedulerModel::Starved { agent, period } => Schedule::Starve {
            seed,
            agent: AgentId(agent),
            period,
        },
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
    pub scheduler: SchedulerModel,
}

impl RunResult {
    #[must_use]
    pub fn empty(prepared: &PreparedRun, termination: Termination) -> Self {
        Self {
            algorithm: prepared.request.algorithm,
            algorithm_version: ccm_algorithms::ALGORITHM_VERSION,
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
            scheduler: prepared.request.scheduler,
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

/// Whether an algorithm is defined only for a rooted start. Help-by-Scouts is
/// a legacy baseline that supports rooted placements only (see its module
/// documentation); `P1Tree` and `RootedDisp` implement rooted constructions.
#[must_use]
pub const fn requires_rooted_start(algorithm: Algorithm) -> bool {
    matches!(
        algorithm,
        Algorithm::HelpByScouts
            | Algorithm::P1Tree
            | Algorithm::RootedAsync
            | Algorithm::RootedDisp
    )
}

/// Refuses a placement the algorithm is not defined for.
///
/// # Errors
///
/// Returns [`RunError::RequiresRootedStart`] when a rooted-only algorithm is
/// given agents on more than one node.
pub fn check_placement(prepared: &PreparedRun) -> Result<(), RunError> {
    let algorithm = prepared.request.algorithm;
    let rooted = prepared.starts.windows(2).all(|pair| pair[0] == pair[1]);
    if requires_rooted_start(algorithm) && !rooted {
        return Err(RunError::RequiresRootedStart {
            algorithm,
            placement: prepared.request.placement.label(),
        });
    }
    Ok(())
}

/// The round limit used when a request sets none.
///
/// Most algorithms get `40 (k + 1)` rounds. `RootedAsync` counts epochs and
/// needs roughly 40 per agent on grids and random graphs, so it gets
/// `100 (k + 1)`. `GeneralDisp` spends twelve steps per unit and Theorem 2
/// bounds its units by `O(k log² k)`, so it gets `480 k (⌈log2 k⌉ + 1)²`
/// steps: far above any run in its tests, and still finite.
#[must_use]
pub fn default_round_limit(algorithm: Algorithm, agents: usize) -> u64 {
    let k = u64::try_from(agents.max(1)).unwrap_or(u64::MAX);
    match algorithm {
        Algorithm::GeneralDisp => {
            let log = u64::from(k.next_power_of_two().trailing_zeros()) + 1;
            480_u64
                .saturating_mul(k)
                .saturating_mul(log)
                .saturating_mul(log)
        }
        Algorithm::RootedAsync => 100_u64.saturating_mul(k.saturating_add(1)),
        _ => 40_u64.saturating_mul(k.saturating_add(1)),
    }
}

#[must_use]
pub fn algorithm_label(algorithm: Algorithm) -> &'static str {
    match algorithm {
        Algorithm::DropAndFreeze => "drop_and_freeze",
        Algorithm::HelpByScouts => "help_by_scouts",
        Algorithm::P1Tree => "p1tree",
        Algorithm::RootedAsync => "rooted_async",
        Algorithm::RootedDisp => "rooted_disp",
        Algorithm::GeneralDisp => "general_disp",
    }
}

/// The scheduler, as a CSV value: `synchronous`, `asynchronous`,
/// `interleaved`, or `starved:<agent>:<period>`.
#[must_use]
pub fn scheduler_label(scheduler: SchedulerModel) -> String {
    match scheduler {
        SchedulerModel::Synchronous => "synchronous".to_owned(),
        SchedulerModel::Asynchronous => "asynchronous".to_owned(),
        SchedulerModel::Interleaved => "interleaved".to_owned(),
        SchedulerModel::Starved { agent, period } => format!("starved:{agent}:{period}"),
    }
}

/// What the `rounds` column counts: synchronous rounds, or asynchronous
/// epochs. The two are not comparable, so every row says which it holds.
#[must_use]
pub fn time_unit(scheduler: SchedulerModel) -> &'static str {
    match scheduler {
        SchedulerModel::Synchronous => "round",
        SchedulerModel::Asynchronous
        | SchedulerModel::Interleaved
        | SchedulerModel::Starved { .. } => "epoch",
    }
}

/// Whether an algorithm is written from a paper or preserves the behaviour of
/// this repository's historical code. Kept apart from [`algorithm_label`] so
/// that the label stays a stable key across result files.
#[must_use]
pub fn algorithm_kind(algorithm: Algorithm) -> &'static str {
    match algorithm {
        Algorithm::DropAndFreeze | Algorithm::HelpByScouts => "legacy_baseline",
        Algorithm::P1Tree
        | Algorithm::RootedAsync
        | Algorithm::RootedDisp
        | Algorithm::GeneralDisp => "paper",
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
            scheduler: SchedulerModel::Synchronous,
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
