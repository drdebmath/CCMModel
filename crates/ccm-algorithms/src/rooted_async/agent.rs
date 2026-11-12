//! One Communicate-Compute-Move cycle of one agent.
//!
//! Every agent runs this same function. What it does depends only on its own
//! memory and on the agents beside it; the lead's extra duties are in
//! [`super::lead`].

use super::lead;
use super::memory::{AgentState, Command, ErrandResult, Reached, Task, VacateRule};
use super::probe::{resolve, Claimants};
use super::world::{Action, Cx, Note, World};
use crate::common::{EdgeType, PORT_ONE};
use ccm_core::{AgentId, PortId};

pub(super) fn activate(world: &World<'_>, id: AgentId) -> Action {
    let mut cx = Cx::new(world, id);
    match cx.me.task {
        Task::Idle => {
            if let Some(state) = cx.me.lead.take() {
                cx.me.lead = lead::act(&mut cx, state);
            } else if cx.me.in_group() {
                follow(&mut cx);
            }
        }
        Task::Probe { port, hop } => probe_step(&mut cx, port, hop),
        Task::Return { remaining } => {
            let port = cx.me.scout.back[usize::from(remaining) - 1];
            cx.me.task = if remaining == 1 {
                Task::Idle
            } else {
                Task::Return {
                    remaining: remaining - 1,
                }
            };
            cx.note(Note::ProbeBack);
            cx.leave(port);
        }
        Task::Errand { rule, stage, seq } => errand_step(&mut cx, rule, stage, seq),
        Task::ErrandBack { remaining, seq } => errand_back(&mut cx, remaining, seq),
        Task::Join { port } => {
            cx.me.task = Task::Idle;
            cx.leave(port);
        }
        Task::Scatter { port, parent } => {
            cx.me.task = Task::ScatterArrive {
                parent,
                port_at_parent: port,
            };
            cx.leave(port);
        }
        Task::ScatterArrive {
            parent,
            port_at_parent,
        } => {
            cx.me.state = AgentState::Settled;
            cx.me.parent = Some((parent, port_at_parent));
            cx.me.parent_port = cx.me.arrival_port;
            cx.me.node_type = crate::common::NodeType::FullyVisited;
            cx.me.task = Task::Idle;
        }
    }
    cx.finish()
}

/// A group member waiting at the head: carry out the lead's newest command.
fn follow(cx: &mut Cx<'_, '_>) {
    let Some(lead) = cx.lead_here() else {
        return;
    };
    if lead.seq <= cx.me.seen_seq {
        return;
    }
    cx.me.seen_seq = lead.seq;
    match lead.command {
        Command::None => {}
        Command::Rank => rank(cx),
        Command::Probe { first } => {
            let index = first + cx.me.rank;
            if index < lead.probe_limit {
                let port = nth_port(index, lead.skip_port);
                start_probe(cx, port, lead.seq);
            }
        }
        Command::Collect { probe_seq } => {
            let pool = cx.group_here();
            tally(&mut cx.me, &pool, probe_seq);
        }
        Command::Move { port } => cx.leave(port),
        Command::Scatter => {
            if cx.me.state == AgentState::Unsettled {
                if let (Some(port), Some(parent)) = (cx.me.scatter.port, lead.head_owner) {
                    cx.me.task = Task::Scatter { port, parent };
                }
            }
        }
    }
}

/// Records this agent's place in the group, which fixes the port it probes,
/// and among the unsettled agents, which fixes where a scatter sends it.
pub(super) fn rank(cx: &mut Cx<'_, '_>) {
    let (below, unsettled_above) = cx.group_ranks();
    cx.me.rank = below;
    cx.me.unsettled_rank = unsettled_above;
    cx.me.scatter = super::memory::ScatterTally::default();
}

/// The `index`-th port at the head in increasing order, skipping the parent
/// port, which is never probed.
pub(super) fn nth_port(index: u16, skip: Option<PortId>) -> PortId {
    match skip {
        Some(parent) if index >= parent.0 => PortId(index + 1),
        _ => PortId(index),
    }
}

pub(super) fn start_probe(cx: &mut Cx<'_, '_>, port: PortId, seq: u32) {
    cx.me.scout = super::memory::ScoutRecord {
        seq,
        port,
        ..Default::default()
    };
    cx.me.task = Task::Probe { port, hop: 1 };
    cx.note(Note::PortProbe);
    cx.note(Note::ProbeOut);
    cx.leave(port);
}

/// One hop of a probe walk, (R1)-(R3c). Each hop is decided at the node the
/// scout has just reached, from who is standing there and the arrival port.
fn probe_step(cx: &mut Cx<'_, '_>, port: PortId, hop: u8) {
    let arrival = cx
        .me
        .arrival_port
        .expect("a probing scout arrived somewhere");
    let here = cx.settled_here().map(|m| (m.id, m.node_type));
    cx.me.scout.back[usize::from(hop) - 1] = arrival;
    let reached = match hop {
        1 => {
            cx.me.scout.edge_type = EdgeType::of(port, arrival);
            if let Some(found) = here {
                cx.me.scout.found = Some(found);
                Some(Reached::R1)
            } else {
                // Rule (R2) concludes "empty" here when the scout arrived on
                // port 1. But then the head is y's port-1 neighbour, and (V2)
                // vacates y precisely when that neighbour is occupied: a
                // vacated child of the head would be taken for an empty node.
                // So continue as (R3) with z the head, which costs nothing,
                // since going to z is going home.
                cx.me.scout.via_head = arrival == PORT_ONE;
                None
            }
        }
        2 => {
            cx.me.scout.p1 = here.map(|(id, _)| id);
            cx.me.scout.port_at_p1 = arrival;
            if here.is_some() {
                Some(Reached::R3a)
            } else if arrival == PORT_ONE {
                Some(Reached::R3b)
            } else {
                None
            }
        }
        _ => {
            cx.me.scout.p1p1 = here.map(|(id, _)| id);
            cx.me.scout.port_at_p1p1 = arrival;
            Some(Reached::R3c)
        }
    };
    if let Some(reached) = reached {
        // Algorithm 4 has R3c-i set its result without returning; every
        // scout returns.
        cx.me.scout.reached = reached;
        let moves = if cx.me.scout.via_head { hop - 2 } else { hop };
        if moves == 0 {
            cx.me.task = Task::Idle;
            return;
        }
        cx.me.task = if moves == 1 {
            Task::Idle
        } else {
            Task::Return {
                remaining: moves - 1,
            }
        };
        cx.note(Note::ProbeBack);
        cx.leave(cx.me.scout.back[usize::from(hop) - 1]);
    } else {
        cx.me.task = Task::Probe { port, hop: hop + 1 };
        cx.note(Note::ProbeOut);
        cx.leave(PORT_ONE);
    }
}

/// Folds one probe batch into an unsettled agent's scatter target: the empty
/// ports, in port order, are dealt out highest ID first.
pub(super) fn tally(
    me: &mut super::memory::Memory,
    pool: &[super::memory::Memory],
    probe_seq: u32,
) {
    if me.state != AgentState::Unsettled {
        return;
    }
    let claimants = Claimants::new(pool);
    let mut results: Vec<_> = pool
        .iter()
        .filter(|m| m.scout.seq == probe_seq)
        .map(|m| resolve(&m.scout, &claimants))
        .collect();
    results.sort_by_key(|r| r.port);
    for result in results.into_iter().filter(|r| r.owner.is_none()) {
        if me.scatter.seen_empty == me.unsettled_rank {
            me.scatter.port = Some(result.port);
        }
        me.scatter.seen_empty += 1;
    }
}

/// Algorithm 3's excursions: the settled agent at the head walks along
/// port-1 links, looks, writes into the agent it finds, and walks back.
///
/// Rules (R1)-(R3c) can only recognise a vacated node whose port-1 neighbour,
/// or that neighbour's port-1 neighbour, is occupied, and only for as long as
/// that node stays occupied. The paper checks this for (V2) alone: (V3) and
/// (V4) vacate unconditionally, and nothing stops an occupied node that others
/// rely on from vacating later. So (V2), (V3) and (V4) all walk the chain here,
/// and the occupied node they rely on is marked with `vacated_neighbor`, which
/// then keeps it from vacating under any rule.
fn errand_step(cx: &mut Cx<'_, '_>, rule: VacateRule, stage: u8, seq: u32) {
    let here = cx
        .settled_here()
        .map(|m| (m.id, m.parent_port, m.vacated_neighbor));
    let arrival = cx.me.arrival_port;
    let next = |stage| Task::Errand { rule, stage, seq };
    match (rule, stage) {
        (VacateRule::V5, 0) => {
            cx.me.task = next(1);
            cx.leave(cx.me.parent_port.expect("V5 needs a parent"));
        }
        (_, 0) => {
            cx.me.task = next(1);
            cx.leave(PORT_ONE);
        }
        (VacateRule::V5, _) => {
            let arrival = arrival.expect("the errand arrived");
            cx.me.scout.back[0] = arrival;
            let me = cx.me.id;
            let parent_port = cx.me.parent_port;
            cx.me.errand = Some(match here {
                // (V5): the parent goes instead. Its port 1 leads here, so this
                // node is its port-1 neighbour, which Algorithm 3 forgets to
                // record and rule (R3a) needs.
                Some((z, Some(_), false)) => {
                    let parent = cx.write(z);
                    parent.state = AgentState::SettledScout;
                    parent.p1_neighbor = Some(me);
                    parent.port_at_p1_neighbor = parent_port;
                    parent.task = Task::Join { port: arrival };
                    parent.seen_seq = seq;
                    cx.me.vacated_neighbor = true;
                    ErrandResult::Joined
                }
                _ => ErrandResult::NotJoined,
            });
            go_back(cx, 1, seq);
        }
        (_, 1) => {
            let arrival = arrival.expect("the errand arrived");
            cx.me.scout.back[0] = arrival;
            if let Some((w, _, _)) = here {
                cx.write(w).vacated_neighbor = true;
                cx.me.errand = Some(ErrandResult::Anchored {
                    p1: w,
                    port_at_p1: arrival,
                });
                go_back(cx, 1, seq);
            } else if rule == VacateRule::V2 || arrival == PORT_ONE {
                // (V2) asks for an occupied port-1 neighbour, nothing further.
                // Otherwise a second hop is allowed, unless it would lead
                // straight back to this node.
                cx.me.errand = Some(ErrandResult::Refused);
                go_back(cx, 1, seq);
            } else {
                cx.me.task = next(2);
                cx.leave(PORT_ONE);
            }
        }
        _ => {
            let arrival = arrival.expect("the errand arrived");
            cx.me.scout.back[1] = arrival;
            cx.me.errand = Some(match here {
                Some((u, _, _)) => {
                    cx.write(u).vacated_neighbor = true;
                    ErrandResult::TwoHops {
                        anchor: u,
                        port_at_anchor: arrival,
                        port_at_p1: cx.me.scout.back[0],
                    }
                }
                None => ErrandResult::Refused,
            });
            go_back(cx, 2, seq);
        }
    }
}

fn go_back(cx: &mut Cx<'_, '_>, hops: u8, seq: u32) {
    let port = cx.me.scout.back[usize::from(hops) - 1];
    cx.me.task = Task::ErrandBack {
        remaining: hops - 1,
        seq,
    };
    cx.leave(port);
}

/// Home again. A one-hop anchor is enough to vacate on the spot; a two-hop one
/// waits for the lead to confirm the middle node is vacated.
fn errand_back(cx: &mut Cx<'_, '_>, remaining: u8, seq: u32) {
    if remaining > 0 {
        go_back(cx, remaining, seq);
        return;
    }
    if let Some(ErrandResult::Anchored { p1, port_at_p1 }) = cx.me.errand {
        cx.me.state = AgentState::SettledScout;
        cx.me.p1_neighbor = Some(p1);
        cx.me.port_at_p1_neighbor = Some(port_at_p1);
        cx.me.seen_seq = seq;
        cx.note(Note::Vacate);
    }
    if cx.me.errand == Some(ErrandResult::Joined) {
        cx.note(Note::Vacate);
    }
    cx.me.task = Task::Idle;
}
