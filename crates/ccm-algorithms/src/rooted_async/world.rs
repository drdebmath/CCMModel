//! The simulated world, and the local view one activation gets of it.
//!
//! An activation sees its own memory and the memories of agents at the same
//! node, and nothing else. It may rewrite its own memory and that of at most one
//! co-located agent, and may leave along one port: the paper's
//! Communicate-Compute-Move cycle (Section 2). [`Cx`] is the only way the
//! protocol touches the world, which is what keeps it local.

use super::memory::{AgentState, AsyncAgent, Memory, Position};
use super::probe::Resolution;
use ccm_core::{AgentId, NodeId, PortGraph, PortId};

pub(super) struct World<'g> {
    pub graph: &'g PortGraph,
    pub agents: Vec<AsyncAgent>,
    /// Agents standing at each node, by ascending ID.
    pub occupants: Vec<Vec<AgentId>>,
    /// Simulator only: the agent whose home each node is.
    pub home_owner: Vec<Option<AgentId>>,
    /// Simulator only: bumped whenever anything an agent at the node could
    /// see changes, so a commit can tell whether its look went stale.
    pub version: Vec<u64>,
    /// The group members (`A_scout`) and the unsettled agents standing at each
    /// node, by ascending ID. An index over `occupants` that answers "how many
    /// of the group are below me" without the simulator rescanning the node
    /// for every agent; what an agent decides is unchanged.
    pub group_at: Vec<Vec<AgentId>>,
    pub unsettled_at: Vec<Vec<AgentId>>,
    /// The agent whose memory holds the lead context. One agent holds it at
    /// a time; this lets a waiting member check that one agent rather than
    /// search its node for a lead that may be away.
    pub lead_holder: Option<AgentId>,
}

impl World<'_> {
    pub fn memory(&self, id: AgentId) -> &Memory {
        &self.agents[id.index()].memory
    }

    pub fn at(&self, node: NodeId) -> impl Iterator<Item = &Memory> + '_ {
        self.occupants[node.index()]
            .iter()
            .map(|&id| &self.agents[id.index()].memory)
    }

    pub fn land(&mut self, id: AgentId) -> bool {
        let agent = &mut self.agents[id.index()];
        let Position::InTransit {
            to, arrival_port, ..
        } = agent.position
        else {
            return false;
        };
        agent.position = Position::At(to);
        agent.memory.arrival_port = Some(arrival_port);
        self.version[to.index()] += 1;
        let list = &mut self.occupants[to.index()];
        let at = list.partition_point(|&other| other < id);
        list.insert(at, id);
        self.index_insert(id, to);
        true
    }

    /// Adds `id` to the indexes at `node` that its memory now qualifies it for.
    pub fn index_insert(&mut self, id: AgentId, node: NodeId) {
        let memory = &self.agents[id.index()].memory;
        let (in_group, unsettled) = (memory.in_group(), memory.state == AgentState::Unsettled);
        if in_group {
            insert_sorted(&mut self.group_at[node.index()], id);
        }
        if unsettled {
            insert_sorted(&mut self.unsettled_at[node.index()], id);
        }
    }

    pub fn index_remove(&mut self, id: AgentId, node: NodeId) {
        remove_sorted(&mut self.group_at[node.index()], id);
        remove_sorted(&mut self.unsettled_at[node.index()], id);
    }
}

fn insert_sorted(list: &mut Vec<AgentId>, id: AgentId) {
    if let Err(at) = list.binary_search(&id) {
        list.insert(at, id);
    }
}

fn remove_sorted(list: &mut Vec<AgentId>, id: AgentId) {
    if let Ok(at) = list.binary_search(&id) {
        list.remove(at);
    }
}

/// A side effect of an activation that the simulator records or checks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Note {
    PortProbe,
    ProbeOut,
    ProbeBack,
    Vacate,
    Backtrack,
    RetraceMove,
    GroupMove(usize),
    MacroRound,
    /// The group leaves the head: one step of the DFS.
    Step {
        forward: bool,
        batches: u16,
        scouts: u16,
    },
    /// Every agent has settled; Retrace begins.
    Constructed,
    /// The group expects this agent to own the node it has reached.
    HeadOwner(AgentId),
    /// A probe the head concluded, to be checked against the truth.
    Probed {
        head: NodeId,
        resolution: Resolution,
    },
    /// The protocol could not continue.
    Stuck(&'static str),
}

pub(super) struct Action {
    pub own: Memory,
    pub other: Option<Memory>,
    pub depart: Option<PortId>,
    pub notes: Vec<Note>,
}

/// One activation's view of the world.
pub(super) struct Cx<'w, 'g> {
    pub world: &'w World<'g>,
    pub node: NodeId,
    pub me: Memory,
    other: Option<Memory>,
    depart: Option<PortId>,
    pub notes: Vec<Note>,
}

impl<'w, 'g> Cx<'w, 'g> {
    pub fn new(world: &'w World<'g>, id: AgentId) -> Self {
        let agent = &world.agents[id.index()];
        let Position::At(node) = agent.position else {
            unreachable!("an agent is landed before it is activated");
        };
        Self {
            world,
            node,
            me: agent.memory.clone(),
            other: None,
            depart: None,
            notes: Vec::new(),
        }
    }

    pub fn finish(self) -> Action {
        Action {
            own: self.me,
            other: self.other,
            depart: self.depart,
            notes: self.notes,
        }
    }

    pub fn degree(&self) -> u16 {
        self.world.graph.degree(self.node).unwrap_or(0)
    }

    /// The other agents here, as this activation sees them.
    pub fn others(&self) -> impl Iterator<Item = &Memory> + '_ {
        let me = self.me.id;
        self.world
            .at(self.node)
            .filter(move |m| m.id != me)
            .map(|m| match &self.other {
                Some(written) if written.id == m.id => written,
                _ => m,
            })
    }

    /// The lead context of the lead standing here, if it is here: what a
    /// search of [`Self::others`] for a memory holding one would find.
    pub fn lead_here(&self) -> Option<super::memory::Lead> {
        let me = self.me.id;
        let mut found: Option<&Memory> = None;
        if let Some(holder) = self.world.lead_holder {
            let agent = &self.world.agents[holder.index()];
            if holder != me && agent.position == Position::At(self.node) {
                let memory = match &self.other {
                    Some(written) if written.id == holder => written,
                    _ => &agent.memory,
                };
                if memory.lead.is_some() {
                    found = Some(memory);
                }
            }
        }
        // This activation's own write may have handed the role on.
        if let Some(written) = &self.other {
            if written.id != me
                && written.lead.is_some()
                && found.map_or(true, |m| written.id < m.id)
            {
                found = Some(written);
            }
        }
        let lead = found.and_then(|m| m.lead);
        debug_assert_eq!(lead, self.others().find_map(|m| m.lead), "lead index");
        lead
    }

    /// `xi`: the agent settled and standing here, if any.
    pub fn settled_here(&self) -> Option<&Memory> {
        self.others().find(|m| m.state == AgentState::Settled)
    }

    /// This agent's place in the group here: how many members have a lower ID,
    /// and how many unsettled members a higher one. The same counts as taken
    /// over [`Self::group_here`], read from the world's index, with this
    /// activation's own write to another agent taken into account.
    pub fn group_ranks(&self) -> (u16, u16) {
        let me = self.me.id;
        let group = &self.world.group_at[self.node.index()];
        let unsettled = &self.world.unsettled_at[self.node.index()];
        let mut below = group.partition_point(|&id| id < me);
        let mut above = unsettled.len() - unsettled.partition_point(|&id| id <= me);
        if let Some(written) = &self.other {
            let before = self.world.memory(written.id);
            if written.id < me && before.in_group() != written.in_group() {
                if written.in_group() {
                    below += 1;
                } else {
                    below -= 1;
                }
            }
            let was = before.state == AgentState::Unsettled;
            let is = written.state == AgentState::Unsettled;
            if written.id > me && was != is {
                if is {
                    above += 1;
                } else {
                    above -= 1;
                }
            }
        }
        #[cfg(debug_assertions)]
        {
            let group = self.group_here();
            let naive_below = group.iter().filter(|m| m.id < me).count();
            let naive_above = group
                .iter()
                .filter(|m| m.state == AgentState::Unsettled && m.id > me)
                .count();
            assert_eq!((below, above), (naive_below, naive_above), "group index");
        }
        let fit = |n: usize| u16::try_from(n).expect("the group fits the port domain");
        (fit(below), fit(above))
    }

    /// The group as it stands here, this agent included if it belongs.
    pub fn group_here(&self) -> Vec<Memory> {
        let mut group: Vec<Memory> = self.others().filter(|m| m.in_group()).cloned().collect();
        if self.me.in_group() {
            group.push(self.me.clone());
        }
        group.sort_by_key(|m| m.id);
        group
    }

    pub fn find(&self, id: AgentId) -> Option<&Memory> {
        if id == self.me.id {
            return Some(&self.me);
        }
        self.others().find(|m| m.id == id)
    }

    /// Opens the one other agent this activation may write. Asking for a second
    /// agent is a protocol bug, not a modelling choice, so it panics.
    pub fn write(&mut self, id: AgentId) -> &mut Memory {
        assert_ne!(id, self.me.id, "an agent writes its own memory directly");
        if let Some(written) = &self.other {
            assert_eq!(written.id, id, "one other write per cycle");
        } else {
            let current = self
                .others()
                .find(|m| m.id == id)
                .expect("an agent only writes a co-located agent")
                .clone();
            self.other = Some(current);
        }
        self.other.as_mut().expect("just opened")
    }

    pub fn leave(&mut self, port: PortId) {
        debug_assert!(self.depart.is_none(), "one move per cycle");
        self.depart = Some(port);
    }

    pub fn note(&mut self, note: Note) {
        self.notes.push(note);
    }
}
