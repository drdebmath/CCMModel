use crate::{AgentId, AgentStore, AgentStoreError, Metrics, NodeId, PortGraph, PortId};
use core::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MoveIntent {
    pub agent: AgentId,
    pub from: NodeId,
    pub port: PortId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SchedulerError {
    DuplicateIntent(AgentId),
    UnknownAgent(AgentId),
    SourceMismatch {
        agent: AgentId,
        expected: NodeId,
        actual: NodeId,
    },
    InvalidPort {
        node: NodeId,
        port: PortId,
    },
    AgentStore(AgentStoreError),
}

impl fmt::Display for SchedulerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for SchedulerError {}

impl From<AgentStoreError> for SchedulerError {
    fn from(value: AgentStoreError) -> Self {
        Self::AgentStore(value)
    }
}

/// Deterministically validates all intents against current state, then commits
/// them in agent-ID order. All validation completes before the first mutation.
///
/// # Errors
///
/// Returns an error for duplicate or unknown agents, stale source nodes,
/// invalid ports, or a failed dense-store update. No state is changed when
/// intent validation fails.
pub fn apply_move_intents<M: Metrics>(
    graph: &PortGraph,
    agents: &mut AgentStore,
    intents: &mut [MoveIntent],
    metrics: &mut M,
) -> Result<(), SchedulerError> {
    intents.sort_unstable_by_key(|intent| intent.agent);
    let mut destinations = Vec::with_capacity(intents.len());
    let mut previous = None;
    for intent in intents.iter() {
        if previous == Some(intent.agent) {
            return Err(SchedulerError::DuplicateIntent(intent.agent));
        }
        previous = Some(intent.agent);
        let state = agents
            .get(intent.agent)
            .ok_or(SchedulerError::UnknownAgent(intent.agent))?;
        if state.node != intent.from {
            return Err(SchedulerError::SourceMismatch {
                agent: intent.agent,
                expected: intent.from,
                actual: state.node,
            });
        }
        let edge = graph
            .traverse(intent.from, intent.port)
            .ok_or(SchedulerError::InvalidPort {
                node: intent.from,
                port: intent.port,
            })?;
        destinations.push((edge.neighbor, edge.remote_port));
    }
    agents.begin_step();
    for (intent, (destination, incoming_port)) in intents.iter().zip(destinations) {
        agents.arrive(intent.agent, destination, incoming_port)?;
    }
    metrics.agent_moves(intents.len() as u64);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ComplexityMetrics, NodeId};

    #[test]
    fn commit_is_agent_ordered_and_counted() {
        let graph = PortGraph::from_undirected_edges(2, &[(NodeId(0), NodeId(1))]).unwrap();
        let mut agents = AgentStore::new(2, &[NodeId(0), NodeId(0)]).unwrap();
        let mut intents = [
            MoveIntent {
                agent: AgentId(1),
                from: NodeId(0),
                port: PortId(0),
            },
            MoveIntent {
                agent: AgentId(0),
                from: NodeId(0),
                port: PortId(0),
            },
        ];
        let mut metrics = ComplexityMetrics::default();
        apply_move_intents(&graph, &mut agents, &mut intents, &mut metrics).unwrap();
        assert_eq!(intents[0].agent, AgentId(0));
        assert_eq!(agents.get(AgentId(0)).unwrap().node, NodeId(1));
        assert_eq!(
            agents.get(AgentId(0)).unwrap().arrival_port,
            Some(PortId(0))
        );
        assert_eq!(metrics.agent_moves, 2);
        assert_eq!(metrics.edge_traversals, 2);
    }
}
