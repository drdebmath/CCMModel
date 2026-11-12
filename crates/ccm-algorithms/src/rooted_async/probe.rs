//! Rules (R1)-(R3c) of Section 4.2: telling an empty neighbour from a vacated
//! one using only what scouts saw and what the group remembers.

use super::memory::{AgentState, Memory, Reached, ScoutRecord};
use crate::common::NodeType;
use ccm_core::{AgentId, PortId};
use std::collections::HashMap;

/// The rule that decided a probe, named as in the paper.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProbeRule {
    /// Somebody was home.
    R1,
    /// The port-1 neighbour was occupied and a group member claims the node.
    R3aI,
    /// The port-1 neighbour was occupied and nobody claims the node: empty.
    R3aII,
    /// Nobody at the port-1 neighbour either, reached on its port 1: empty.
    R3b,
    /// Nobody two hops on: empty.
    R3cI,
    /// Two hops on was occupied, and a chain of claims leads back: vacated.
    R3cIIAlpha,
    /// Two hops on was occupied, but the chain breaks: empty.
    R3cIIBeta,
}

impl ProbeRule {
    /// A stable numeric code for traces, in declaration order:
    /// R1 = 0, R3a-i = 1, R3a-ii = 2, R3b = 3, R3c-i = 4, R3c-ii(a) = 5,
    /// R3c-ii(b) = 6.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::R1 => 0,
            Self::R3aI => 1,
            Self::R3aII => 2,
            Self::R3b => 3,
            Self::R3cI => 4,
            Self::R3cIIAlpha => 5,
            Self::R3cIIBeta => 6,
        }
    }
}

/// What a probe concluded about the neighbour behind one port.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Resolution {
    pub port: PortId,
    /// `psi(y)`: who owns the neighbour, or `None` if it is empty.
    pub owner: Option<AgentId>,
    pub node_type: NodeType,
    pub rule: ProbeRule,
}

/// The vacated members of a group, by the port-1 neighbour each remembers:
/// what Rules R3a and R3c match a scout's findings against.
///
/// Built once per conclusion, so matching every scout of a batch costs one
/// pass over the group rather than one per scout. Where two members claim the
/// same neighbour the lowest ID is kept, as a search of the group in ID order
/// would find.
pub(super) struct Claimants<'a> {
    by_p1: HashMap<(AgentId, PortId), &'a Memory>,
}

impl<'a> Claimants<'a> {
    pub fn new(pool: &'a [Memory]) -> Self {
        let mut by_p1 = HashMap::new();
        for member in pool {
            if member.state != AgentState::SettledScout {
                continue;
            }
            if let (Some(p1), Some(port)) = (member.p1_neighbor, member.port_at_p1_neighbor) {
                by_p1.entry((p1, port)).or_insert(member);
            }
        }
        Self { by_p1 }
    }

    fn find(&self, p1: AgentId, port: PortId) -> Option<&'a Memory> {
        self.by_p1.get(&(p1, port)).copied()
    }
}

/// Concludes one probe at the head, after every scout is back.
///
/// `claimants` indexes the group as it stands at the head. A vacated node's
/// agent is in it, so a vacated node is recognised by finding the group member
/// whose stored port-1 neighbour is the one the scout saw.
pub(super) fn resolve(scout: &ScoutRecord, claimants: &Claimants<'_>) -> Resolution {
    let claimant = |p1: AgentId, port: PortId| claimants.find(p1, port);
    let empty = |rule| Resolution {
        port: scout.port,
        owner: None,
        node_type: NodeType::Unvisited,
        rule,
    };
    let vacated = |member: &Memory, rule| Resolution {
        port: scout.port,
        owner: Some(member.id),
        node_type: member.node_type,
        rule,
    };

    match scout.reached {
        Reached::R1 => {
            let (owner, node_type) = scout.found.expect("R1 records who was home");
            Resolution {
                port: scout.port,
                owner: Some(owner),
                node_type,
                rule: ProbeRule::R1,
            }
        }
        Reached::R3b => empty(ProbeRule::R3b),
        Reached::R3a => {
            let z = scout.p1.expect("R3a records who was at z");
            claimant(z, scout.port_at_p1)
                .map_or_else(|| empty(ProbeRule::R3aII), |b| vacated(b, ProbeRule::R3aI))
        }
        Reached::R3c => {
            let Some(w) = scout.p1p1 else {
                return empty(ProbeRule::R3cI);
            };
            // c = psi(z), recognised by its own port-1 neighbour w; then
            // b = psi(y), recognised by having c as its port-1 neighbour.
            claimant(w, scout.port_at_p1p1)
                .and_then(|c| claimant(c.id, scout.port_at_p1))
                .map_or_else(
                    || empty(ProbeRule::R3cIIBeta),
                    |b| vacated(b, ProbeRule::R3cIIAlpha),
                )
        }
    }
}
