use crate::{AgentId, NodeId, PortId};
use core::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum AgentStatus {
    Unsettled,
    Settled,
    SettledWaiting,
    SettledScout,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AgentState {
    pub node: NodeId,
    pub status: AgentStatus,
    pub home: Option<NodeId>,
    pub parent: Option<AgentId>,
    pub arrival_port: Option<PortId>,
    pub flags: u8,
}

impl AgentState {
    #[must_use]
    pub const fn unsettled(node: NodeId) -> Self {
        Self {
            node,
            status: AgentStatus::Unsettled,
            home: None,
            parent: None,
            arrival_port: None,
            flags: 0,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentStore {
    states: Vec<AgentState>,
    node_count: usize,
    home_owner: Vec<Option<AgentId>>,
    settled_agent_at: Vec<Option<AgentId>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentStoreError {
    TooManyAgents(usize),
    InvalidAgent(AgentId),
    InvalidNode(NodeId),
    HomeCannotChange {
        agent: AgentId,
        current: NodeId,
        requested: NodeId,
    },
    HomeAlreadyOwned {
        node: NodeId,
        owner: AgentId,
    },
    NodeAlreadySettled {
        node: NodeId,
        agent: AgentId,
    },
}

impl fmt::Display for AgentStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for AgentStoreError {}

impl AgentStore {
    /// Creates dense agent state and empty ownership indexes.
    ///
    /// # Errors
    ///
    /// Returns an error when the number of agents exceeds the ID domain or an
    /// initial node is outside `0..node_count`.
    pub fn new(node_count: usize, initial_nodes: &[NodeId]) -> Result<Self, AgentStoreError> {
        if initial_nodes.len() > u32::MAX as usize {
            return Err(AgentStoreError::TooManyAgents(initial_nodes.len()));
        }
        let mut states = Vec::with_capacity(initial_nodes.len());
        for &node in initial_nodes {
            if node.index() >= node_count {
                return Err(AgentStoreError::InvalidNode(node));
            }
            states.push(AgentState::unsettled(node));
        }
        Ok(Self {
            states,
            node_count,
            home_owner: vec![None; node_count],
            settled_agent_at: vec![None; node_count],
        })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.states.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }

    #[must_use]
    pub fn get(&self, agent: AgentId) -> Option<&AgentState> {
        self.states.get(agent.index())
    }

    pub fn get_mut(&mut self, agent: AgentId) -> Option<&mut AgentState> {
        self.states.get_mut(agent.index())
    }

    /// Moves an agent without assigning an incoming port.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown agent or invalid destination node.
    pub fn move_to(&mut self, agent: AgentId, node: NodeId) -> Result<(), AgentStoreError> {
        if node.index() >= self.node_count {
            return Err(AgentStoreError::InvalidNode(node));
        }
        let state = self
            .states
            .get_mut(agent.index())
            .ok_or(AgentStoreError::InvalidAgent(agent))?;
        state.node = node;
        Ok(())
    }

    /// Starts one synchronous model step. Per the paper's model, an agent's
    /// incoming port is bottom whenever it did not move in the preceding step.
    pub fn begin_step(&mut self) {
        for state in &mut self.states {
            state.arrival_port = None;
        }
    }

    /// Commits a traversal and records the reciprocal arrival port.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown agent or invalid destination node.
    pub fn arrive(
        &mut self,
        agent: AgentId,
        node: NodeId,
        incoming_port: PortId,
    ) -> Result<(), AgentStoreError> {
        self.move_to(agent, node)?;
        self.states[agent.index()].arrival_port = Some(incoming_port);
        Ok(())
    }

    /// Permanently associates an agent with a unique home node.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid IDs or when another agent owns the node.
    pub fn claim_home(&mut self, agent: AgentId, node: NodeId) -> Result<(), AgentStoreError> {
        self.require_agent_and_node(agent, node)?;
        if let Some(current) = self.states[agent.index()].home {
            if current != node {
                return Err(AgentStoreError::HomeCannotChange {
                    agent,
                    current,
                    requested: node,
                });
            }
        }
        if let Some(owner) = self.home_owner[node.index()] {
            if owner != agent {
                return Err(AgentStoreError::HomeAlreadyOwned { node, owner });
            }
        }
        self.home_owner[node.index()] = Some(agent);
        self.states[agent.index()].home = Some(node);
        Ok(())
    }

    /// Marks an agent as the unique physically present settler at a node.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid IDs, duplicate homes, or duplicate
    /// physically settled agents.
    pub fn mark_settled(&mut self, agent: AgentId, node: NodeId) -> Result<(), AgentStoreError> {
        self.require_agent_and_node(agent, node)?;
        if let Some(current) = self.states[agent.index()].home {
            if current != node {
                return Err(AgentStoreError::HomeCannotChange {
                    agent,
                    current,
                    requested: node,
                });
            }
        }
        if let Some(existing) = self.settled_agent_at[node.index()] {
            if existing != agent {
                return Err(AgentStoreError::NodeAlreadySettled {
                    node,
                    agent: existing,
                });
            }
        }
        if let Some(owner) = self.home_owner[node.index()] {
            if owner != agent {
                return Err(AgentStoreError::HomeAlreadyOwned { node, owner });
            }
        }
        let state = &mut self.states[agent.index()];
        state.node = node;
        state.home = Some(node);
        state.status = AgentStatus::Settled;
        self.home_owner[node.index()] = Some(agent);
        self.settled_agent_at[node.index()] = Some(agent);
        Ok(())
    }

    /// Temporarily removes a settler from its home while retaining ownership.
    ///
    /// # Errors
    ///
    /// Returns an error unless the agent exists, has a home, and is the
    /// physically settled agent recorded at that home.
    pub fn vacate_home(&mut self, agent: AgentId) -> Result<NodeId, AgentStoreError> {
        let state = self
            .states
            .get(agent.index())
            .ok_or(AgentStoreError::InvalidAgent(agent))?;
        let home = state.home.ok_or(AgentStoreError::InvalidAgent(agent))?;
        if self.settled_agent_at[home.index()] != Some(agent) {
            return Err(AgentStoreError::NodeAlreadySettled {
                node: home,
                agent: self.settled_agent_at[home.index()].unwrap_or(agent),
            });
        }
        self.settled_agent_at[home.index()] = None;
        self.states[agent.index()].status = AgentStatus::SettledScout;
        Ok(home)
    }

    /// Returns a traveling settler to its immutable home.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown/non-owner agent or an occupied home.
    pub fn return_home(&mut self, agent: AgentId) -> Result<NodeId, AgentStoreError> {
        let state = self
            .states
            .get(agent.index())
            .ok_or(AgentStoreError::InvalidAgent(agent))?;
        let home = state.home.ok_or(AgentStoreError::InvalidAgent(agent))?;
        if let Some(existing) = self.settled_agent_at[home.index()] {
            if existing != agent {
                return Err(AgentStoreError::NodeAlreadySettled {
                    node: home,
                    agent: existing,
                });
            }
        }
        self.states[agent.index()].node = home;
        self.states[agent.index()].status = AgentStatus::Settled;
        self.settled_agent_at[home.index()] = Some(agent);
        Ok(home)
    }

    #[must_use]
    pub fn home_owner(&self, node: NodeId) -> Option<AgentId> {
        self.home_owner.get(node.index()).copied().flatten()
    }

    #[must_use]
    pub fn settled_agent_at(&self, node: NodeId) -> Option<AgentId> {
        self.settled_agent_at.get(node.index()).copied().flatten()
    }

    #[must_use]
    pub fn occupancy(&self) -> OccupancyIndex {
        OccupancyIndex::build(self.node_count, &self.states)
    }

    fn require_agent_and_node(&self, agent: AgentId, node: NodeId) -> Result<(), AgentStoreError> {
        if agent.index() >= self.states.len() {
            return Err(AgentStoreError::InvalidAgent(agent));
        }
        if node.index() >= self.node_count {
            return Err(AgentStoreError::InvalidNode(node));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OccupancyIndex {
    offsets: Vec<usize>,
    agents: Vec<AgentId>,
}

impl OccupancyIndex {
    fn build(node_count: usize, states: &[AgentState]) -> Self {
        let mut offsets = vec![0; node_count + 1];
        for state in states {
            offsets[state.node.index() + 1] += 1;
        }
        for node in 0..node_count {
            offsets[node + 1] += offsets[node];
        }
        let mut next = offsets[..node_count].to_vec();
        let mut agents = vec![AgentId(0); states.len()];
        for (id, state) in states.iter().enumerate() {
            let slot = next[state.node.index()];
            agents[slot] = AgentId(u32::try_from(id).expect("agent count was validated"));
            next[state.node.index()] += 1;
        }
        Self { offsets, agents }
    }

    #[must_use]
    pub fn agents_at(&self, node: NodeId) -> &[AgentId] {
        let Some(&start) = self.offsets.get(node.index()) else {
            return &[];
        };
        let Some(&end) = self.offsets.get(node.index() + 1) else {
            return &[];
        };
        &self.agents[start..end]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn occupancy_is_dense_and_stable_by_agent_id() {
        let store = AgentStore::new(3, &[NodeId(1), NodeId(0), NodeId(1)]).unwrap();
        let occupancy = store.occupancy();
        assert_eq!(occupancy.agents_at(NodeId(0)), &[AgentId(1)]);
        assert_eq!(occupancy.agents_at(NodeId(1)), &[AgentId(0), AgentId(2)]);
    }

    #[test]
    fn ownership_is_unique() {
        let mut store = AgentStore::new(2, &[NodeId(0), NodeId(0)]).unwrap();
        store.mark_settled(AgentId(0), NodeId(0)).unwrap();
        assert!(matches!(
            store.mark_settled(AgentId(1), NodeId(0)),
            Err(AgentStoreError::NodeAlreadySettled { .. })
        ));
    }

    #[test]
    fn home_is_immutable_across_vacate_and_return() {
        let mut store = AgentStore::new(2, &[NodeId(0)]).unwrap();
        store.mark_settled(AgentId(0), NodeId(0)).unwrap();
        store.vacate_home(AgentId(0)).unwrap();
        store.move_to(AgentId(0), NodeId(1)).unwrap();
        assert_eq!(store.home_owner(NodeId(0)), Some(AgentId(0)));
        assert_eq!(store.settled_agent_at(NodeId(0)), None);
        store.return_home(AgentId(0)).unwrap();
        assert_eq!(store.get(AgentId(0)).unwrap().node, NodeId(0));
        assert_eq!(store.settled_agent_at(NodeId(0)), Some(AgentId(0)));
        assert!(matches!(
            store.claim_home(AgentId(0), NodeId(1)),
            Err(AgentStoreError::HomeCannotChange { .. })
        ));
    }
}
