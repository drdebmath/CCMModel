use crate::{execute_many_parallel, GraphSpec, RunError, RunRequest, RunResult, SimulationRunner};

/// Cartesian sweep axes.  The expansion order is nodes, agents, then seeds;
/// this is part of the CSV reproducibility contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SweepSpec {
    pub base: RunRequest,
    pub node_counts: Vec<usize>,
    pub agent_counts: Vec<usize>,
    pub seeds: Vec<u64>,
}

impl SweepSpec {
    #[must_use]
    pub fn new(base: RunRequest) -> Self {
        Self {
            node_counts: vec![base.graph.nodes],
            agent_counts: vec![base.agents],
            seeds: vec![base.seed],
            base,
        }
    }

    #[must_use]
    pub fn with_node_counts(mut self, values: Vec<usize>) -> Self {
        self.node_counts = values;
        self
    }

    #[must_use]
    pub fn with_agent_counts(mut self, values: Vec<usize>) -> Self {
        self.agent_counts = values;
        self
    }

    #[must_use]
    pub fn with_seeds(mut self, values: Vec<u64>) -> Self {
        self.seeds = values;
        self
    }
}

/// Expands a sweep without generating graphs or running algorithms.
#[must_use]
pub fn expand_sweep(spec: &SweepSpec) -> Vec<RunRequest> {
    let mut requests = Vec::with_capacity(
        spec.node_counts
            .len()
            .saturating_mul(spec.agent_counts.len())
            .saturating_mul(spec.seeds.len()),
    );
    for &nodes in &spec.node_counts {
        for &agents in &spec.agent_counts {
            for &seed in &spec.seeds {
                let graph = GraphSpec {
                    family: spec.base.graph.family.clone(),
                    nodes,
                    // Explicit tables are meaningful only at their declared
                    // graph size; generated sweeps otherwise use the same
                    // family and selected port model at every size.
                    explicit_ports: (spec.base.graph.nodes == nodes)
                        .then(|| spec.base.graph.explicit_ports.clone())
                        .flatten(),
                };
                requests.push(RunRequest {
                    graph,
                    agents,
                    seed,
                    ..spec.base.clone()
                });
            }
        }
    }
    requests
}

/// Expands and executes a sweep across independent worker groups.
pub fn run_sweep_parallel<R: SimulationRunner>(
    runner: &R,
    spec: &SweepSpec,
    workers: usize,
) -> Vec<Result<RunResult, RunError>> {
    let requests = expand_sweep(spec);
    execute_many_parallel(runner, &requests, workers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GraphFamily, PlacementSpec};
    use ccm_core::{Algorithm, NodeId, PortAssignment};

    fn base() -> RunRequest {
        RunRequest {
            algorithm: Algorithm::HelpByScouts,
            graph: GraphSpec::new(GraphFamily::Path, 4),
            port_assignment: PortAssignment::Canonical,
            placement: PlacementSpec::SingleNode { node: NodeId(0) },
            agents: 2,
            seed: 10,
            round_limit: None,
        }
    }

    #[test]
    fn expansion_is_cartesian_and_stable() {
        let spec = SweepSpec::new(base())
            .with_node_counts(vec![4, 8])
            .with_agent_counts(vec![1, 2, 3])
            .with_seeds(vec![10, 11]);
        let requests = expand_sweep(&spec);
        assert_eq!(requests.len(), 12);
        assert_eq!(requests[0].graph.nodes, 4);
        assert_eq!(requests[0].agents, 1);
        assert_eq!(requests[0].seed, 10);
        assert_eq!(requests[1].seed, 11);
        assert_eq!(requests[2].agents, 2);
        assert_eq!(requests[6].graph.nodes, 8);
    }

    #[test]
    fn invalid_sweep_rows_are_retained_for_explicit_reporting() {
        let spec = SweepSpec::new(base()).with_agent_counts(vec![2, 9]);
        let requests = expand_sweep(&spec);
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[1].agents, 9);
    }
}
