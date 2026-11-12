//! How many bits an agent's memory needs: the quantity Theorem 2 bounds by
//! `O(log(k + Δ))`.
//!
//! Section 2 counts "the number of bits stored at any agent over one CCM cycle
//! to the next", so this measures [`Memory`] as it stands after a write, not
//! the temporaries of a computation. Each field is charged what it needs to
//! hold the value it holds, not the width of the Rust type that stores it:
//!
//! - an agent ID costs `⌈log₂ k⌉` bits and a port `⌈log₂ Δ⌉`;
//! - a counter costs the bits of its current value, so one that grows with the
//!   run shows up in the measurement;
//! - an enum costs the bits to tell its variants apart, plus its payload, and
//!   an `Option` one bit more than its contents.
//!
//! The simulator's own records, `position` and `home`, are not agent memory and
//! are not counted.

use super::memory::{
    Candidate, Command, ErrandResult, Lead, Memory, Phase, Plan, ScatterTally, ScoutRecord, Target,
    Task,
};
use ccm_core::AgentId;

/// Field widths for one run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Widths {
    /// Bits for an agent ID: `⌈log₂ k⌉`.
    pub id: u32,
    /// Bits for a port number: `⌈log₂ Δ⌉`.
    pub port: u32,
}

impl Widths {
    #[must_use]
    pub fn new(agents: usize, max_degree: usize) -> Self {
        Self {
            id: ceil_log2(agents),
            port: ceil_log2(max_degree),
        }
    }
}

/// `⌈log₂ n⌉`, and at least one bit.
fn ceil_log2(n: usize) -> u32 {
    let n = n.max(2) - 1;
    usize::BITS - n.leading_zeros()
}

/// The bits a counter's current value needs, and at least one.
fn value(v: u32) -> u32 {
    (u32::BITS - v.leading_zeros()).max(1)
}

/// The bits to tell `variants` cases apart.
fn tag(variants: usize) -> u32 {
    ceil_log2(variants)
}

fn optional(present: Option<u32>) -> u32 {
    1 + present.unwrap_or(0)
}

impl Widths {
    fn id_opt(self, id: Option<AgentId>) -> u32 {
        optional(id.map(|_| self.id))
    }

    fn port_opt<T>(self, port: Option<T>) -> u32 {
        optional(port.map(|_| self.port))
    }

    fn link_opt<T>(self, link: Option<T>) -> u32 {
        optional(link.map(|_| self.id + self.port))
    }

    /// The bits `memory` holds between cycles.
    #[must_use]
    pub fn memory(self, memory: &Memory) -> u32 {
        // id, state (3 cases), node_type (4) and the two flags.
        let fixed = self.id + tag(3) + tag(4) + 2;
        fixed
            + self.port_opt(memory.arrival_port)
            + value(memory.seen_seq)
            + self.link_opt(memory.parent)
            + self.port_opt(memory.parent_port)
            + optional(memory.parent_edge_type.map(|_| tag(4)))
            + self.id_opt(memory.p1_neighbor)
            + self.port_opt(memory.port_at_p1_neighbor)
            + self.port_opt(memory.recent_child)
            + self.link_opt(memory.sibling)
            + self.port_opt(memory.recent_port)
            + self.link_opt(memory.old_parent)
            + self.port_opt(memory.old_parent_port)
            + self.link_opt(memory.old_sibling)
            + self.task(memory.task)
            + self.scout(&memory.scout)
            + value(u32::from(memory.rank))
            + value(u32::from(memory.unsettled_rank))
            + self.scatter(memory.scatter)
            + optional(memory.errand.map(|e| self.errand(e)))
            + optional(memory.lead.as_ref().map(|l| self.lead(l)))
    }

    fn task(self, task: Task) -> u32 {
        tag(9)
            + match task {
                Task::Idle => 0,
                Task::Probe { hop, .. } => self.port + value(u32::from(hop)),
                Task::Return { remaining } => value(u32::from(remaining)),
                Task::Errand { stage, seq, .. } => tag(4) + value(u32::from(stage)) + value(seq),
                Task::ErrandBack { remaining, seq } => value(u32::from(remaining)) + value(seq),
                Task::Join { .. } => self.port,
                Task::Scatter { .. } | Task::ScatterArrive { .. } => self.port + self.id,
            }
    }

    fn scout(self, scout: &ScoutRecord) -> u32 {
        value(scout.seq)
            + self.port
            + tag(4)
            + tag(4)
            + optional(scout.found.map(|_| self.id + tag(4)))
            + self.id_opt(scout.p1)
            + self.port
            + self.id_opt(scout.p1p1)
            + self.port
            + 3 * self.port
            + 1
    }

    fn scatter(self, scatter: ScatterTally) -> u32 {
        value(u32::from(scatter.seen_empty)) + self.port_opt(scatter.port)
    }

    fn errand(self, errand: ErrandResult) -> u32 {
        tag(5)
            + match errand {
                ErrandResult::Anchored { .. } => self.id + self.port,
                ErrandResult::TwoHops { .. } => self.id + 2 * self.port,
                ErrandResult::Refused | ErrandResult::Joined | ErrandResult::NotJoined => 0,
            }
    }

    fn candidate(self, candidate: Candidate) -> u32 {
        self.port + tag(4) + self.id_opt(candidate.owner)
    }

    fn lead(self, lead: &Lead) -> u32 {
        let phase = tag(11)
            + match lead.phase {
                Phase::Probe { first, .. } => value(u32::from(first)) + 1,
                Phase::Collect { first } => value(u32::from(first)),
                Phase::Errand(_) => tag(4),
                Phase::RetraceIssue { .. } => self.port,
                _ => 0,
            };
        let command = tag(6)
            + match lead.command {
                Command::Probe { first } => value(u32::from(first)),
                Command::Collect { probe_seq } => value(probe_seq),
                Command::Move { .. } => self.port,
                Command::None | Command::Rank | Command::Scatter => 0,
            };
        let target = 1 + match lead.target {
            Target::Empty => 0,
            Target::Owner { .. } => self.id + 1,
        };
        let plan = 1 + match lead.plan {
            Plan::Forward(candidate) => self.candidate(candidate),
            Plan::Back => 0,
        };
        phase
            + value(lead.seq)
            + command
            + value(lead.k)
            + value(lead.group_size)
            + value(lead.unsettled)
            + self.id_opt(lead.head_owner)
            + self.id_opt(lead.prev_id)
            + self.port_opt(lead.child_port)
            + self.link_opt(lead.sibling_details)
            + self.link_opt(lead.child_details)
            + self.link_opt(lead.child_sibling)
            + target
            + self.port_opt(lead.skip_port)
            + value(u32::from(lead.probe_limit))
            + value(u32::from(lead.batch))
            + value(u32::from(lead.batches_here))
            + 1
            + optional(lead.best.map(|c| self.candidate(c)))
            + plan
            + self.id_opt(lead.next_agent_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_are_ceiling_logs() {
        assert_eq!(ceil_log2(1), 1);
        assert_eq!(ceil_log2(2), 1);
        assert_eq!(ceil_log2(3), 2);
        assert_eq!(ceil_log2(4), 2);
        assert_eq!(ceil_log2(5), 3);
        assert_eq!(ceil_log2(256), 8);
        assert_eq!(value(0), 1);
        assert_eq!(value(255), 8);
        assert_eq!(value(256), 9);
    }

    #[test]
    fn a_fresh_agent_costs_more_with_more_agents_but_only_logarithmically() {
        let fresh = Memory::unsettled(AgentId(0));
        let small = Widths::new(4, 4).memory(&fresh);
        let large = Widths::new(1 << 16, 4).memory(&fresh);
        assert!(large > small);
        assert!(large < 8 * small);
    }
}
