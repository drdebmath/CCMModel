//! Indexed, deterministic port of the repository's Help-by-Scouts behavior.

use ccm_core::{
    AgentId, AgentStatus, ComplexityMetrics, Metrics, NodeId, PortGraph, PortId, RoundKind,
    Termination,
};
use ccm_trace::{NoTrace, Phase, Recorder, SimulationEvent};
use core::fmt;

/// Version of the authoritative Help-by-Scouts implementation.
pub const ALGORITHM_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HelpStatus {
    Unsettled,
    Settled,
    SettledScout,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodeType {
    Unvisited,
    Visited,
    PartiallyVisited,
    FullyVisited,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EdgeType {
    OneOne,
    OtherOne,
    OneOther,
    OtherOther,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProbeResult {
    pub port: PortId,
    pub edge_type: EdgeType,
    pub node_type: NodeType,
    pub owner: Option<AgentId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelpAgent {
    pub node: NodeId,
    pub status: HelpStatus,
    pub home: Option<NodeId>,
    pub node_type: NodeType,
    pub parent: Option<AgentId>,
    pub parent_port: Option<PortId>,
    pub port_at_parent: Option<PortId>,
    pub arrival_port: Option<PortId>,
    pub vacated_neighbor: bool,
    pub previous: Option<AgentId>,
    pub child_port: Option<PortId>,
    pub recent_port: Option<PortId>,
    pub probe_results: Vec<ProbeResult>,
    pub depth: usize,
}

impl HelpAgent {
    fn new(node: NodeId) -> Self {
        Self {
            node,
            status: HelpStatus::Unsettled,
            home: None,
            node_type: NodeType::Unvisited,
            parent: None,
            parent_port: None,
            port_at_parent: None,
            arrival_port: None,
            vacated_neighbor: false,
            previous: None,
            child_port: None,
            recent_port: None,
            probe_results: Vec::new(),
            depth: 0,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelpResult {
    pub termination: Termination,
    pub agents: Vec<HelpAgent>,
    pub logical_steps: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HelpError {
    EmptyGraph,
    TooManyAgents {
        agents: usize,
        nodes: usize,
    },
    InvalidStart(NodeId),
    InvalidPort {
        node: NodeId,
        port: PortId,
    },
    MissingAgentAtNode {
        agent: AgentId,
        expected: NodeId,
        actual: NodeId,
    },
    MissingSettler(NodeId),
    MissingParent(NodeId),
    DuplicateHome(NodeId),
    RoundLimit(u64),
    Invariant(&'static str),
}

impl fmt::Display for HelpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for HelpError {}

struct Simulation<'g, M, R> {
    graph: &'g PortGraph,
    agents: Vec<HelpAgent>,
    occupants: Vec<Vec<AgentId>>,
    home_owner: Vec<Option<AgentId>>,
    unsettled: Vec<bool>,
    vacated: Vec<bool>,
    steps: u64,
    limit: u64,
    metrics: M,
    recorder: R,
}

impl<'g, M: Metrics, R: Recorder> Simulation<'g, M, R> {
    fn new(
        graph: &'g PortGraph,
        starts: &[NodeId],
        limit: u64,
        metrics: M,
        recorder: R,
    ) -> Result<Self, HelpError> {
        if graph.node_count() == 0 {
            return Err(HelpError::EmptyGraph);
        }
        if starts.len() > graph.node_count() {
            return Err(HelpError::TooManyAgents {
                agents: starts.len(),
                nodes: graph.node_count(),
            });
        }
        let mut agents = Vec::with_capacity(starts.len());
        let mut occupants = vec![Vec::new(); graph.node_count()];
        for (index, &node) in starts.iter().enumerate() {
            if node.index() >= graph.node_count() {
                return Err(HelpError::InvalidStart(node));
            }
            let id = AgentId(
                u32::try_from(index).map_err(|_| HelpError::Invariant("agent id overflow"))?,
            );
            agents.push(HelpAgent::new(node));
            occupants[node.index()].push(id);
        }
        Ok(Self {
            graph,
            agents,
            occupants,
            home_owner: vec![None; graph.node_count()],
            unsettled: vec![true; starts.len()],
            vacated: vec![false; starts.len()],
            steps: 0,
            limit,
            metrics,
            recorder,
        })
    }

    fn active_ids(&self) -> Vec<AgentId> {
        (0..self.agents.len())
            .filter(|&id| self.unsettled[id] || self.vacated[id])
            .map(|id| AgentId(u32::try_from(id).expect("agent IDs validated")))
            .collect()
    }

    fn tick(&mut self, count: u64) -> Result<(), HelpError> {
        self.steps = self.steps.saturating_add(count);
        if self.steps > self.limit {
            return Err(HelpError::RoundLimit(self.limit));
        }
        Ok(())
    }

    fn physical_settler(&self, node: NodeId, exclude: Option<AgentId>) -> Option<AgentId> {
        self.occupants[node.index()].iter().copied().find(|&id| {
            Some(id) != exclude && matches!(self.agents[id.index()].status, HelpStatus::Settled)
                || (Some(id) != exclude
                    && self.agents[id.index()].status == HelpStatus::SettledScout
                    && self.agents[id.index()].home == Some(node))
        })
    }

    fn move_agent(
        &mut self,
        agent: AgentId,
        from: NodeId,
        port: PortId,
    ) -> Result<NodeId, HelpError> {
        let actual = self.agents[agent.index()].node;
        if actual != from {
            return Err(HelpError::MissingAgentAtNode {
                agent,
                expected: from,
                actual,
            });
        }
        let edge = self
            .graph
            .traverse(from, port)
            .ok_or(HelpError::InvalidPort { node: from, port })?;
        let position = self.occupants[from.index()]
            .iter()
            .position(|&id| id == agent)
            .ok_or(HelpError::MissingAgentAtNode {
                agent,
                expected: from,
                actual,
            })?;
        self.occupants[from.index()].remove(position);
        self.occupants[edge.neighbor.index()].push(agent);
        self.occupants[edge.neighbor.index()].sort_unstable();
        self.agents[agent.index()].node = edge.neighbor;
        self.agents[agent.index()].arrival_port = Some(edge.remote_port);
        self.metrics.agent_moves(1);
        self.recorder.record_event(
            self.steps,
            SimulationEvent::AgentMoved {
                agent,
                from,
                to: edge.neighbor,
                out_port: port,
                in_port: Some(edge.remote_port),
            },
        );
        Ok(edge.neighbor)
    }

    fn move_group(
        &mut self,
        ids: &[AgentId],
        from: NodeId,
        port: PortId,
    ) -> Result<NodeId, HelpError> {
        let to = self
            .graph
            .traverse(from, port)
            .ok_or(HelpError::InvalidPort { node: from, port })?
            .neighbor;
        for &id in ids {
            self.move_agent(id, from, port)?;
        }
        self.metrics.group_move(ids.len());
        self.recorder.record_event(
            self.steps,
            SimulationEvent::GroupMoved {
                agents: ids.to_vec(),
                from,
                to,
                out_port: port,
            },
        );
        Ok(to)
    }

    fn edge_type(&self, node: NodeId, port: PortId) -> Result<EdgeType, HelpError> {
        let edge = self
            .graph
            .traverse(node, port)
            .ok_or(HelpError::InvalidPort { node, port })?;
        Ok(match (port == PortId(0), edge.remote_port == PortId(0)) {
            (true, true) => EdgeType::OneOne,
            (false, true) => EdgeType::OtherOne,
            (true, false) => EdgeType::OneOther,
            (false, false) => EdgeType::OtherOther,
        })
    }

    fn settle(&mut self, agent: AgentId, node: NodeId, head: AgentId) -> Result<(), HelpError> {
        if self.home_owner[node.index()].is_some() {
            return Err(HelpError::DuplicateHome(node));
        }
        let previous = self.agents[head.index()].previous;
        let (parent, parent_port, port_at_parent) = if let Some(parent) = previous {
            let parent_port = self.agents[head.index()]
                .arrival_port
                .ok_or(HelpError::MissingParent(node))?;
            let port_at_parent = self.agents[head.index()]
                .child_port
                .or(self.agents[parent.index()].recent_port)
                .ok_or(HelpError::MissingParent(node))?;
            (Some(parent), Some(parent_port), Some(port_at_parent))
        } else {
            (None, None, None)
        };
        let depth = parent.map_or(0, |id| self.agents[id.index()].depth.saturating_add(1));
        let state = &mut self.agents[agent.index()];
        state.status = HelpStatus::Settled;
        state.home = Some(node);
        state.parent = parent;
        state.parent_port = parent_port;
        state.port_at_parent = port_at_parent;
        state.depth = depth;
        self.home_owner[node.index()] = Some(agent);
        self.unsettled[agent.index()] = false;
        self.agents[head.index()].child_port = None;
        self.metrics.settlement();
        self.recorder
            .record_event(self.steps, SimulationEvent::AgentSettled { agent, node });
        self.observe();
        Ok(())
    }

    fn candidate_rank(result: ProbeResult) -> (u8, u8, u16) {
        let node_rank = match result.node_type {
            NodeType::Unvisited => 0,
            NodeType::PartiallyVisited
                if matches!(result.edge_type, EdgeType::OtherOne | EdgeType::OneOne) =>
            {
                1
            }
            _ => 99,
        };
        let edge_rank = match result.edge_type {
            EdgeType::OtherOne => 0,
            EdgeType::OneOne | EdgeType::OneOther => 1,
            EdgeType::OtherOther => 2,
        };
        (node_rank, edge_rank, result.port.0)
    }

    fn parallel_probe(
        &mut self,
        node: NodeId,
        owner: AgentId,
        scouts: &[AgentId],
    ) -> Result<Option<PortId>, HelpError> {
        let parent_port = self.agents[owner.index()].parent_port;
        let ports: Vec<PortId> = self
            .graph
            .ports(node)
            .map(|(port, _)| port)
            .filter(|&port| Some(port) != parent_port)
            .collect();
        self.agents[owner.index()].probe_results.clear();
        if !ports.is_empty() && scouts.is_empty() {
            return Err(HelpError::Invariant("no scouts available for probe"));
        }
        for batch in ports.chunks(scouts.len().max(1)) {
            for (index, &port) in batch.iter().enumerate() {
                let scout = scouts[index];
                let edge_type = self.edge_type(node, port)?;
                let destination = self.move_agent(scout, node, port)?;
                self.metrics.port_probe();
                self.metrics.scout_operation();
                self.metrics.probe_out_traversal();
                let owner_at_destination = self.home_owner[destination.index()];
                let node_type = owner_at_destination
                    .map_or(NodeType::Unvisited, |id| self.agents[id.index()].node_type);
                let return_port =
                    self.agents[scout.index()]
                        .arrival_port
                        .ok_or(HelpError::InvalidPort {
                            node: destination,
                            port,
                        })?;
                self.move_agent(scout, destination, return_port)?;
                self.metrics.probe_back_traversal();
                self.agents[owner.index()].probe_results.push(ProbeResult {
                    port,
                    edge_type,
                    node_type,
                    owner: owner_at_destination,
                });
            }
            self.metrics.logical_rounds(RoundKind::Scout, 2);
            self.tick(2)?;
        }
        self.agents[owner.index()]
            .probe_results
            .sort_unstable_by_key(|result| result.port);
        self.recorder.record_event(
            self.steps,
            SimulationEvent::PhaseStarted {
                phase: Phase::Scout,
            },
        );
        Ok(self.agents[owner.index()]
            .probe_results
            .iter()
            .copied()
            .min_by_key(|result| Self::candidate_rank(*result))
            .filter(|result| Self::candidate_rank(*result).0 != 99)
            .map(|result| result.port))
    }

    fn update_node_type(&mut self, node: NodeId, owner: AgentId) -> Result<(), HelpError> {
        let empty: Vec<ProbeResult> = self.agents[owner.index()]
            .probe_results
            .iter()
            .copied()
            .filter(|result| result.node_type == NodeType::Unvisited)
            .collect();
        if empty.is_empty() {
            self.agents[owner.index()].node_type = NodeType::FullyVisited;
        } else if let Some(parent_port) = self.agents[owner.index()].parent_port {
            if self.edge_type(node, parent_port)? == EdgeType::OtherOther
                && empty
                    .iter()
                    .all(|result| result.edge_type == EdgeType::OtherOther)
            {
                self.agents[owner.index()].node_type = NodeType::PartiallyVisited;
            } else {
                self.agents[owner.index()].node_type = NodeType::Visited;
            }
        } else {
            self.agents[owner.index()].node_type = NodeType::Visited;
        }
        Ok(())
    }

    fn can_vacate(&mut self, node: NodeId, owner: AgentId) -> Result<HelpStatus, HelpError> {
        self.metrics.vacate();
        self.metrics.logical_rounds(RoundKind::Vacate, 2);
        self.tick(2)?;
        let state = self.agents[owner.index()].clone();
        if state.parent_port.is_none() {
            return Ok(HelpStatus::Settled);
        }
        if state.node_type == NodeType::Visited
            || (state.node_type == NodeType::FullyVisited && !state.vacated_neighbor)
        {
            let destination = self.move_agent(owner, node, PortId(0))?;
            let other = self.physical_settler(destination, Some(owner));
            if let Some(other) = other {
                self.agents[other.index()].vacated_neighbor = true;
            }
            let return_port =
                self.agents[owner.index()]
                    .arrival_port
                    .ok_or(HelpError::InvalidPort {
                        node: destination,
                        port: PortId(0),
                    })?;
            self.move_agent(owner, destination, return_port)?;
            self.metrics.logical_rounds(RoundKind::Vacate, 2);
            self.tick(2)?;
            return Ok(if other.is_some() {
                HelpStatus::SettledScout
            } else {
                HelpStatus::Settled
            });
        }
        if state.node_type == NodeType::PartiallyVisited {
            return Ok(HelpStatus::SettledScout);
        }
        if state.port_at_parent == Some(PortId(0)) {
            let parent_port = state.parent_port.ok_or(HelpError::MissingParent(node))?;
            let parent_node = self.move_agent(owner, node, parent_port)?;
            let parent_owner = self.physical_settler(parent_node, Some(owner));
            if let Some(parent_owner) = parent_owner {
                if self.agents[parent_owner.index()].vacated_neighbor {
                    self.move_agent(owner, parent_node, PortId(0))?;
                } else {
                    self.agents[parent_owner.index()].status = HelpStatus::SettledScout;
                    self.vacated[parent_owner.index()] = true;
                    let ids = [owner, parent_owner];
                    self.move_group(&ids, parent_node, PortId(0))?;
                    self.agents[owner.index()].vacated_neighbor = true;
                }
            } else {
                self.move_agent(owner, parent_node, PortId(0))?;
            }
            self.metrics.logical_rounds(RoundKind::Vacate, 2);
            self.tick(2)?;
        }
        Ok(HelpStatus::Settled)
    }

    fn run(mut self) -> Result<(HelpResult, M, R), HelpError> {
        let termination = match self.run_transitions() {
            Ok(()) => {
                self.validate_final()?;
                Termination::Completed
            }
            Err(HelpError::RoundLimit(limit)) => Termination::RoundLimitReached { limit },
            Err(error) => return Err(error),
        };
        Ok((
            HelpResult {
                termination,
                agents: self.agents,
                logical_steps: self.steps,
            },
            self.metrics,
            self.recorder,
        ))
    }

    fn run_transitions(&mut self) -> Result<(), HelpError> {
        self.observe();
        while self.unsettled.iter().any(|&value| value) {
            self.metrics.macro_round();
            let active = self.active_ids();
            let head = *active
                .first()
                .ok_or(HelpError::Invariant("empty active set"))?;
            let node = self.agents[head.index()].node;
            let owner = if let Some(owner) = self.physical_settler(node, None) {
                owner
            } else {
                let to_settle = self.occupants[node.index()]
                    .iter()
                    .copied()
                    .filter(|id| self.unsettled[id.index()])
                    .max()
                    .ok_or(HelpError::MissingSettler(node))?;
                self.settle(to_settle, node, head)?;
                self.tick(1)?;
                to_settle
            };
            if !self.unsettled.iter().any(|&value| value) {
                break;
            }
            self.agents[head.index()].previous = Some(owner);
            let scouts = self.active_ids();
            let next_port = self.parallel_probe(node, owner, &scouts)?;
            self.observe();
            self.update_node_type(node, owner)?;
            let status = self.can_vacate(node, owner)?;
            self.agents[owner.index()].status = status;
            match status {
                HelpStatus::Settled => self.vacated[owner.index()] = false,
                HelpStatus::SettledScout => self.vacated[owner.index()] = true,
                HelpStatus::Unsettled => {}
            }
            let moving = self.active_ids();
            if let Some(port) = next_port {
                self.agents[owner.index()].recent_port = Some(port);
                self.agents[head.index()].child_port = Some(port);
                let destination = self.move_group(&moving, node, port)?;
                self.metrics.logical_rounds(RoundKind::Movement, 1);
                self.tick(1)?;
                if let Some(destination_owner) = self.physical_settler(destination, None) {
                    let arrival = self.agents[head.index()].arrival_port;
                    if self.agents[destination_owner.index()].node_type
                        == NodeType::PartiallyVisited
                        && arrival == Some(PortId(0))
                    {
                        self.agents[destination_owner.index()].parent = Some(owner);
                        self.agents[destination_owner.index()].port_at_parent = Some(port);
                        self.agents[destination_owner.index()].parent_port = arrival;
                        self.agents[destination_owner.index()].node_type = NodeType::Visited;
                        self.agents[destination_owner.index()].depth =
                            self.agents[owner.index()].depth.saturating_add(1);
                    }
                }
            } else {
                let parent_port = self.agents[owner.index()]
                    .parent_port
                    .ok_or(HelpError::MissingParent(node))?;
                self.agents[head.index()].child_port = None;
                self.agents[owner.index()].recent_port = Some(parent_port);
                self.move_group(&moving, node, parent_port)?;
                self.metrics.backtrack();
                self.metrics.logical_rounds(RoundKind::Movement, 1);
                self.tick(1)?;
            }
        }
        self.retrace()?;
        Ok(())
    }

    fn retrace(&mut self) -> Result<(), HelpError> {
        self.metrics.retrace();
        let mut vacated = self.active_ids();
        vacated.retain(|id| self.vacated[id.index()]);
        if vacated.is_empty() {
            return Ok(());
        }
        let mut tree = vec![Vec::<(NodeId, PortId)>::new(); self.graph.node_count()];
        for agent in &self.agents {
            if let (Some(home), Some(parent), Some(parent_port), Some(port_at_parent)) = (
                agent.home,
                agent.parent,
                agent.parent_port,
                agent.port_at_parent,
            ) {
                let parent_home = self.agents[parent.index()]
                    .home
                    .ok_or(HelpError::MissingParent(home))?;
                tree[home.index()].push((parent_home, parent_port));
                tree[parent_home.index()].push((home, port_at_parent));
            }
        }
        let start = self.agents[vacated[0].index()].node;
        let mut visited = vec![false; self.graph.node_count()];
        let mut stack = vec![(start, 0usize)];
        visited[start.index()] = true;
        self.settle_vacated_at(start);
        while !stack.is_empty() && self.vacated.iter().any(|&value| value) {
            let (node, index) = *stack.last().expect("stack non-empty");
            let next = tree[node.index()]
                .iter()
                .copied()
                .enumerate()
                .skip(index)
                .find(|(_, (neighbor, _))| !visited[neighbor.index()]);
            if let Some((edge_index, (neighbor, port))) = next {
                stack.last_mut().expect("stack non-empty").1 = edge_index + 1;
                let group = self.vacated_ids();
                self.move_group(&group, node, port)?;
                self.metrics.logical_rounds(RoundKind::Retrace, 1);
                self.tick(1)?;
                visited[neighbor.index()] = true;
                self.settle_vacated_at(neighbor);
                stack.push((neighbor, 0));
            } else {
                stack.pop();
                if let Some(&(parent, _)) = stack.last() {
                    let port = tree[node.index()]
                        .iter()
                        .find(|(neighbor, _)| *neighbor == parent)
                        .map(|(_, port)| *port)
                        .ok_or(HelpError::MissingParent(node))?;
                    let group = self.vacated_ids();
                    self.move_group(&group, node, port)?;
                    self.metrics.logical_rounds(RoundKind::Retrace, 1);
                    self.tick(1)?;
                }
            }
        }
        if self.vacated.iter().any(|&value| value) {
            return Err(HelpError::Invariant("retrace left vacated settlers"));
        }
        Ok(())
    }

    fn vacated_ids(&self) -> Vec<AgentId> {
        self.vacated
            .iter()
            .enumerate()
            .filter(|(_, value)| **value)
            .map(|(id, _)| AgentId(u32::try_from(id).expect("agent IDs validated")))
            .collect()
    }

    fn settle_vacated_at(&mut self, node: NodeId) {
        if let Some(owner) = self.home_owner[node.index()] {
            if self.vacated[owner.index()] {
                self.vacated[owner.index()] = false;
                self.agents[owner.index()].status = HelpStatus::Settled;
                self.recorder.record_event(
                    self.steps,
                    SimulationEvent::AgentStateChanged {
                        agent: owner,
                        from: AgentStatus::SettledScout,
                        to: AgentStatus::Settled,
                    },
                );
            }
        }
    }

    fn observe(&mut self) {
        let unsettled = self.unsettled.iter().filter(|value| **value).count();
        let owned = self
            .home_owner
            .iter()
            .filter(|owner| owner.is_some())
            .count();
        let depth = self
            .agents
            .iter()
            .map(|agent| agent.depth)
            .max()
            .unwrap_or(0);
        let probe_words = self
            .agents
            .iter()
            .map(|agent| agent.probe_results.len().saturating_mul(4))
            .sum::<usize>();
        self.metrics.observe_state(unsettled, depth, owned);
        self.metrics.observe_memory(ccm_core::LogicalMemory {
            agent_state_words: u64::try_from(self.agents.len().saturating_mul(25))
                .unwrap_or(u64::MAX),
            ownership_words: u64::try_from(self.home_owner.len()).unwrap_or(u64::MAX),
            tree_words: u64::try_from(self.agents.len().saturating_mul(2)).unwrap_or(u64::MAX),
            probe_words: u64::try_from(probe_words).unwrap_or(u64::MAX),
            auxiliary_words: 0,
        });
    }

    fn validate_final(&self) -> Result<(), HelpError> {
        let mut occupied = vec![false; self.graph.node_count()];
        for agent in &self.agents {
            if agent.status != HelpStatus::Settled || agent.home != Some(agent.node) {
                return Err(HelpError::Invariant("agent not settled at home"));
            }
            if occupied[agent.node.index()] {
                return Err(HelpError::DuplicateHome(agent.node));
            }
            occupied[agent.node.index()] = true;
        }
        Ok(())
    }
}

/// Runs Help-by-Scouts with explicit instrumentation and recording policies.
///
/// # Errors
///
/// Returns configuration, traversal, round-limit, or invariant errors.
pub fn simulate<M: Metrics, R: Recorder>(
    graph: &PortGraph,
    starts: &[NodeId],
    round_limit: u64,
    metrics: M,
    recorder: R,
) -> Result<(HelpResult, M, R), HelpError> {
    Simulation::new(graph, starts, round_limit, metrics, recorder)?.run()
}

/// Convenience entry point with complexity metrics and tracing disabled.
///
/// # Errors
///
/// Returns configuration, traversal, round-limit, or invariant errors.
pub fn run(
    graph: &PortGraph,
    starts: &[NodeId],
    round_limit: u64,
) -> Result<HelpResult, HelpError> {
    simulate(
        graph,
        starts,
        round_limit,
        ComplexityMetrics::default(),
        NoTrace,
    )
    .map(|(result, _, _)| result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccm_core::PortEdge;
    use ccm_trace::FullTrace;

    #[test]
    fn single_agent_settles_without_moving() {
        let graph = PortGraph::from_undirected_edges(1, &[]).unwrap();
        let result = run(&graph, &[NodeId(0)], 40).unwrap();
        assert_eq!(result.termination, Termination::Completed);
        assert_eq!(result.agents[0].status, HelpStatus::Settled);
        assert_eq!(result.agents[0].home, Some(NodeId(0)));
    }

    #[test]
    fn rooted_path_disperses() {
        let graph =
            PortGraph::from_undirected_edges(3, &[(NodeId(0), NodeId(1)), (NodeId(1), NodeId(2))])
                .unwrap();
        let result = run(&graph, &[NodeId(0), NodeId(0), NodeId(0)], 200).unwrap();
        let mut nodes: Vec<NodeId> = result.agents.iter().map(|agent| agent.node).collect();
        nodes.sort_unstable();
        assert_eq!(nodes, vec![NodeId(0), NodeId(1), NodeId(2)]);
        assert_eq!(
            result
                .agents
                .iter()
                .map(|agent| agent.home)
                .collect::<Vec<_>>(),
            vec![Some(NodeId(2)), Some(NodeId(1)), Some(NodeId(0))]
        );
    }

    #[test]
    fn same_input_is_deterministic() {
        let graph = PortGraph::from_undirected_edges(
            4,
            &[
                (NodeId(0), NodeId(1)),
                (NodeId(1), NodeId(2)),
                (NodeId(2), NodeId(3)),
            ],
        )
        .unwrap();
        let first = run(&graph, &[NodeId(0); 4], 400).unwrap();
        let second = run(&graph, &[NodeId(0); 4], 400).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn explicit_cycle_matches_python_final_fixture() {
        let graph = PortGraph::from_port_tables(vec![
            vec![
                PortEdge {
                    neighbor: NodeId(1),
                    remote_port: PortId(0),
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
                    neighbor: NodeId(2),
                    remote_port: PortId(0),
                },
            ],
            vec![
                PortEdge {
                    neighbor: NodeId(1),
                    remote_port: PortId(1),
                },
                PortEdge {
                    neighbor: NodeId(0),
                    remote_port: PortId(1),
                },
            ],
        ])
        .unwrap();
        let result = run(&graph, &[NodeId(0); 3], 200).unwrap();
        assert_eq!(
            result
                .agents
                .iter()
                .map(|agent| agent.home)
                .collect::<Vec<_>>(),
            vec![Some(NodeId(2)), Some(NodeId(1)), Some(NodeId(0))]
        );
    }

    #[test]
    fn canonical_star_matches_python_final_fixture() {
        let graph = PortGraph::from_undirected_edges(
            4,
            &[
                (NodeId(0), NodeId(1)),
                (NodeId(0), NodeId(2)),
                (NodeId(0), NodeId(3)),
            ],
        )
        .unwrap();
        let result = run(&graph, &[NodeId(0); 4], 400).unwrap();
        assert_eq!(
            result
                .agents
                .iter()
                .map(|agent| agent.home)
                .collect::<Vec<_>>(),
            vec![
                Some(NodeId(1)),
                Some(NodeId(3)),
                Some(NodeId(2)),
                Some(NodeId(0))
            ]
        );
    }

    #[test]
    fn canonical_tree_matches_python_final_fixture() {
        let graph = PortGraph::from_undirected_edges(
            5,
            &[
                (NodeId(0), NodeId(1)),
                (NodeId(0), NodeId(2)),
                (NodeId(1), NodeId(3)),
                (NodeId(1), NodeId(4)),
            ],
        )
        .unwrap();
        let result = run(&graph, &[NodeId(0); 5], 600).unwrap();
        assert_eq!(
            result
                .agents
                .iter()
                .map(|agent| agent.home)
                .collect::<Vec<_>>(),
            vec![
                Some(NodeId(4)),
                Some(NodeId(3)),
                Some(NodeId(1)),
                Some(NodeId(2)),
                Some(NodeId(0)),
            ]
        );
    }

    #[test]
    fn canonical_complete_four_matches_python_final_fixture() {
        let graph = PortGraph::from_undirected_edges(
            4,
            &[
                (NodeId(0), NodeId(1)),
                (NodeId(0), NodeId(2)),
                (NodeId(0), NodeId(3)),
                (NodeId(1), NodeId(2)),
                (NodeId(1), NodeId(3)),
                (NodeId(2), NodeId(3)),
            ],
        )
        .unwrap();
        let result = run(&graph, &[NodeId(0); 4], 600).unwrap();
        assert_eq!(
            result
                .agents
                .iter()
                .map(|agent| agent.home)
                .collect::<Vec<_>>(),
            vec![
                Some(NodeId(3)),
                Some(NodeId(1)),
                Some(NodeId(2)),
                Some(NodeId(0))
            ]
        );
    }

    #[test]
    fn documents_python_multi_start_failure() {
        let graph =
            PortGraph::from_undirected_edges(3, &[(NodeId(0), NodeId(1)), (NodeId(1), NodeId(2))])
                .unwrap();
        assert!(matches!(
            run(&graph, &[NodeId(0), NodeId(2)], 200),
            Err(HelpError::MissingAgentAtNode { .. })
        ));
    }

    #[test]
    fn tracing_does_not_change_results_or_metrics() {
        let graph =
            PortGraph::from_undirected_edges(3, &[(NodeId(0), NodeId(1)), (NodeId(1), NodeId(2))])
                .unwrap();
        let (plain, plain_metrics, _) = simulate(
            &graph,
            &[NodeId(0); 3],
            200,
            ComplexityMetrics::default(),
            NoTrace,
        )
        .unwrap();
        let (traced, traced_metrics, trace) = simulate(
            &graph,
            &[NodeId(0); 3],
            200,
            ComplexityMetrics::default(),
            FullTrace::new(),
        )
        .unwrap();
        assert_eq!(plain, traced);
        assert_eq!(plain_metrics, traced_metrics);
        assert!(!trace.trace().events.is_empty());
    }

    #[test]
    fn round_limit_returns_partial_state_and_metrics() {
        let graph =
            PortGraph::from_undirected_edges(3, &[(NodeId(0), NodeId(1)), (NodeId(1), NodeId(2))])
                .unwrap();
        let (result, metrics, _) = simulate(
            &graph,
            &[NodeId(0); 3],
            1,
            ComplexityMetrics::default(),
            NoTrace,
        )
        .unwrap();
        assert_eq!(
            result.termination,
            Termination::RoundLimitReached { limit: 1 }
        );
        assert_eq!(metrics.macro_rounds, 1);
        assert_eq!(metrics.settlements, 1);
    }
}
