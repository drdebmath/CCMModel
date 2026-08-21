#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvalidConfiguration {
    EmptyGraph,
    TooManyAgents { agents: usize, nodes: usize },
    InvalidPlacement,
    InvalidPortGraph,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvariantViolation {
    InvalidAgentNode,
    AgentCountChanged,
    DuplicateHome,
    InvalidParent,
    InvalidTraversal,
    OwnershipIndexMismatch,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Termination {
    Completed,
    RoundLimitReached { limit: u64 },
    InvalidConfiguration(InvalidConfiguration),
    Cancelled,
    InvariantViolation(InvariantViolation),
}
