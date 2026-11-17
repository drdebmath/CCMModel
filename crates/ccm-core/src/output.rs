use crate::AgentId;

/// What one agent outputs at the end of a run.
///
/// Dispersion algorithms have no output: their result is where the agents
/// stand. Algorithms that compute something about the graph, such as a
/// leader, give every agent an output, and each such algorithm adds its own
/// variant here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentOutput {
    /// Leader election: whether this agent is the leader, and the agent it
    /// learnt is the leader.
    Leader { is_leader: bool, leader: AgentId },
}
