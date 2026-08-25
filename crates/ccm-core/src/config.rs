use crate::InvalidConfiguration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Algorithm {
    DropAndFreeze,
    HelpByScouts,
    /// `DFS_P1Tree` dispersion, which builds a port-one tree.
    P1Tree,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GraphModel {
    Path,
    Cycle,
    Star,
    Complete,
    RandomConnected { edges: usize },
    Explicit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PortAssignment {
    Canonical,
    Random,
    Adversarial,
    Explicit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StartingModel {
    SingleNode,
    UniformRandom { distinct_nodes: usize },
    Clustered { clusters: usize },
    Explicit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SchedulerModel {
    Synchronous,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExperimentConfig {
    pub algorithm: Algorithm,
    pub graph_model: GraphModel,
    pub port_assignment: PortAssignment,
    pub starting_model: StartingModel,
    pub scheduler_model: SchedulerModel,
    pub nodes: usize,
    pub agents: usize,
    pub seed: u64,
    pub round_limit: Option<u64>,
}

impl ExperimentConfig {
    /// Validates constraints shared by every algorithm adapter.
    ///
    /// # Errors
    ///
    /// Returns the first invalid graph size, agent count, or placement model.
    pub fn validate(&self) -> Result<(), InvalidConfiguration> {
        if self.nodes == 0 {
            return Err(InvalidConfiguration::EmptyGraph);
        }
        if self.agents > self.nodes {
            return Err(InvalidConfiguration::TooManyAgents {
                agents: self.agents,
                nodes: self.nodes,
            });
        }
        match self.starting_model {
            StartingModel::UniformRandom { distinct_nodes }
                if distinct_nodes == 0 || distinct_nodes > self.nodes =>
            {
                Err(InvalidConfiguration::InvalidPlacement)
            }
            StartingModel::Clustered { clusters } if clusters == 0 || clusters > self.nodes => {
                Err(InvalidConfiguration::InvalidPlacement)
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> ExperimentConfig {
        ExperimentConfig {
            algorithm: Algorithm::DropAndFreeze,
            graph_model: GraphModel::Path,
            port_assignment: PortAssignment::Canonical,
            starting_model: StartingModel::SingleNode,
            scheduler_model: SchedulerModel::Synchronous,
            nodes: 4,
            agents: 3,
            seed: 42,
            round_limit: None,
        }
    }

    #[test]
    fn rejects_more_agents_than_nodes() {
        let mut value = config();
        value.agents = 5;
        assert!(matches!(
            value.validate(),
            Err(InvalidConfiguration::TooManyAgents { .. })
        ));
    }
}
