//! The lead's duties: Algorithm 5 (`RootedAsync`) and Algorithm 6 (`Retrace`)
//! as seen by the agent that coordinates the group.
//!
//! The lead is the lowest-ID member of the group (`a_min`). During the
//! construction that is always the lowest-ID unsettled agent: every settled
//! agent was the highest-ID unsettled agent when it settled, so a vacated agent
//! never outranks it, and it settles last. During Retrace the role passes to the
//! lowest-ID vacated agent and moves on each time that agent reaches home.
//!
//! Under asynchrony the group cannot simply "move together". The lead posts a
//! numbered command; members carry it out when they next see it; and the lead
//! waits at a barrier, all `group_size` members present and caught up, before
//! posting the next one. On a move it leaves last, so no member can miss the
//! command it is carrying out.

use super::agent::{nth_port, rank, start_probe, tally};
use super::memory::{
    AgentState, Candidate, Command, ErrandResult, Lead, Link, Memory, Phase, Plan, Target, Task,
    VacateRule,
};
use super::probe::{resolve, Claimants};
use super::world::{Cx, Note};
use crate::common::{EdgeType, NodeType, PORT_ONE};
use ccm_core::AgentId;

/// Runs the lead's part of one cycle. Returns the context to keep, or `None`
/// when the role has been handed on or is finished.
pub(super) fn act(cx: &mut Cx<'_, '_>, mut lead: Lead) -> Option<Lead> {
    match lead.phase {
        Phase::Gather => {
            if ready(cx, &lead) {
                return arrive(cx, lead);
            }
        }
        Phase::Rank => {
            if ready(cx, &lead) {
                plan_probe(cx, &mut lead);
            }
        }
        Phase::Probe { first, started } => {
            if !started {
                start_own_probe(cx, &mut lead, first);
            } else if ready(cx, &lead) {
                finish_batch(cx, &mut lead, first);
            }
        }
        Phase::Collect { first } => {
            if ready(cx, &lead) {
                let next = first + lead.batch;
                next_batch_or_decide(cx, &mut lead, next);
            }
        }
        Phase::Errand(rule) => errand_done(cx, &mut lead, rule),
        Phase::PreMove => {
            if ready(cx, &lead) {
                begin_move(cx, &mut lead);
            }
        }
        Phase::Depart | Phase::RetraceDepart => {
            // Leave last, once every member has gone.
            if !cx.others().any(Memory::in_group) {
                let Command::Move { port } = lead.command else {
                    unreachable!("departing without a move command");
                };
                lead.phase = if lead.phase == Phase::Depart {
                    Phase::Gather
                } else {
                    Phase::RetraceGather
                };
                cx.leave(port);
            }
        }
        Phase::Scatter => {
            // The unsettled agents have gone, and the vacated ones staying
            // behind have seen the command, so none is mid-cycle when the
            // lead role is written into one of them.
            let settled_down = cx.others().all(|m| match m.state {
                AgentState::Unsettled => false,
                AgentState::SettledScout => m.task == Task::Idle && m.seen_seq == lead.seq,
                AgentState::Settled => true,
            });
            if settled_down {
                return finish_scatter(cx, lead);
            }
        }
        Phase::RetraceGather => {
            if ready(cx, &lead) {
                return retrace_step(cx, lead);
            }
        }
        Phase::RetraceIssue { port } => {
            if ready(cx, &lead) {
                issue(cx, &mut lead, Command::Move { port });
                lead.phase = Phase::RetraceDepart;
            }
        }
    }
    Some(lead)
}

/// The barrier: every member here, idle, and caught up with the last command.
fn ready(cx: &Cx<'_, '_>, lead: &Lead) -> bool {
    let caught_up = cx
        .others()
        .filter(|m| m.in_group() && m.task == Task::Idle && m.seen_seq == lead.seq)
        .count();
    u32::try_from(caught_up + 1).is_ok_and(|present| present == lead.group_size)
}

fn issue(cx: &mut Cx<'_, '_>, lead: &mut Lead, command: Command) {
    lead.seq += 1;
    lead.command = command;
    cx.me.seen_seq = lead.seq;
}

fn owner_memory(cx: &Cx<'_, '_>, lead: &Lead) -> Memory {
    let owner = lead.head_owner.expect("the head has an owner");
    cx.find(owner)
        .expect("the head's owner is here, settled or in the group")
        .clone()
}

/// Algorithm 5 lines 11-19: settle at a new node, or reattach a
/// partiallyVisited one, or simply resume at a node reached by backtracking.
fn arrive(cx: &mut Cx<'_, '_>, mut lead: Lead) -> Option<Lead> {
    let arrival = cx.me.arrival_port;
    let parent_link: Option<Link> = lead.prev_id.zip(lead.child_port);
    let edge = arrival
        .zip(lead.child_port)
        .map(|(here, there)| EdgeType::of(here, there));
    match lead.target {
        Target::Empty => {
            // Line 12: the highest-ID unsettled agent settles.
            let settler = cx
                .group_here()
                .into_iter()
                .filter(|m| m.state == AgentState::Unsettled)
                .map(|m| m.id)
                .max()
                .expect("the lead itself is unsettled");
            let sibling = lead.sibling_details.take();
            let settle = |m: &mut Memory| {
                m.state = AgentState::Settled;
                m.node_type = NodeType::Visited;
                m.parent = parent_link;
                m.parent_port = arrival;
                m.parent_edge_type = edge;
                m.port_one_tree_edge = edge.is_some_and(EdgeType::is_port_one_incident);
                // Line 25 sets `sibling` on every pass, which on a backtrack
                // overwrites it with nothing; it is set once, here.
                m.sibling = sibling;
            };
            if settler == cx.me.id {
                settle(&mut cx.me);
            } else {
                settle(cx.write(settler));
            }
            lead.child_port = None;
            lead.head_owner = Some(settler);
            lead.unsettled -= 1;
            lead.group_size -= 1;
            if lead.unsettled == 0 {
                return begin_retrace(cx, lead);
            }
        }
        Target::Owner { id, forward } => {
            lead.head_owner = Some(id);
            cx.note(Note::HeadOwner(id));
            if forward {
                // Rule (D4): a partiallyVisited node reached through its port 1
                // swaps its tpq parent edge for this one.
                let sibling = lead.sibling_details.take();
                let owner = cx.write(id);
                owner.old_parent = owner.parent;
                owner.old_parent_port = owner.parent_port;
                owner.old_sibling = owner.sibling;
                owner.parent = parent_link;
                owner.parent_port = arrival;
                owner.parent_edge_type = edge;
                owner.port_one_tree_edge = true;
                owner.node_type = NodeType::Visited;
                owner.sibling = sibling;
                lead.child_port = None;
            }
        }
    }
    issue(cx, &mut lead, Command::Rank);
    rank(cx);
    lead.phase = Phase::Rank;
    Some(lead)
}

/// Fixes which ports to probe. A node of degree at least `k - 1` takes
/// Algorithm 5's shortcut (lines 21-24): probe enough ports to find an empty
/// neighbour for every unsettled agent, and scatter them there.
fn plan_probe(cx: &mut Cx<'_, '_>, lead: &mut Lead) {
    let owner = owner_memory(cx, lead);
    let degree = cx.degree();
    let skip = owner.parent_port;
    let candidates = degree - u16::from(skip.is_some());
    lead.skip_port = skip;
    lead.best = None;
    lead.batches_here = 0;
    lead.batch = u16::try_from(lead.group_size).expect("the group fits the port domain");
    lead.scatter_mode = u32::from(degree) + 1 >= lead.k;
    lead.probe_limit = if lead.scatter_mode {
        // At most this many neighbours can be occupied, so this many ports
        // hold an empty one for each unsettled agent.
        let enough = if skip.is_none() {
            lead.k - 1
        } else {
            lead.k - 2
        };
        candidates.min(u16::try_from(enough).unwrap_or(u16::MAX))
    } else {
        candidates
    };
    next_batch_or_decide(cx, lead, 0);
}

fn next_batch_or_decide(cx: &mut Cx<'_, '_>, lead: &mut Lead, first: u16) {
    if first < lead.probe_limit {
        lead.batches_here += 1;
        issue(cx, lead, Command::Probe { first });
        lead.phase = Phase::Probe {
            first,
            started: false,
        };
    } else if lead.scatter_mode {
        issue(cx, lead, Command::Scatter);
        lead.phase = Phase::Scatter;
    } else {
        decide(cx, lead);
    }
}

/// The lead probes too, but only once every member has picked up the command:
/// leaving first would take the command out of their sight.
fn start_own_probe(cx: &mut Cx<'_, '_>, lead: &mut Lead, first: u16) {
    if cx.others().any(|m| m.in_group() && m.seen_seq < lead.seq) {
        return;
    }
    lead.phase = Phase::Probe {
        first,
        started: true,
    };
    let index = first + cx.me.rank;
    if index < lead.probe_limit {
        start_probe(cx, nth_port(index, lead.skip_port), lead.seq);
    }
}

/// Every scout of a batch is back: conclude each probe from what the group
/// holds (Algorithm 4 lines 25-50) and keep the best edge so far.
fn finish_batch(cx: &mut Cx<'_, '_>, lead: &mut Lead, first: u16) {
    let pool = cx.group_here();
    let claimants = Claimants::new(&pool);
    let probe_seq = lead.seq;
    for scout in pool.iter().filter(|m| m.scout.seq == probe_seq) {
        let resolution = resolve(&scout.scout, &claimants);
        cx.note(Note::Probed {
            head: cx.node,
            resolution,
        });
        let eligible = match resolution.node_type {
            NodeType::Unvisited => resolution.owner.is_none(),
            // Rule (D4): enterable through its port 1 only.
            NodeType::PartiallyVisited => {
                matches!(scout.scout.edge_type, EdgeType::OtherOne | EdgeType::OneOne)
            }
            NodeType::Visited | NodeType::FullyVisited => false,
        };
        let candidate = Candidate {
            port: resolution.port,
            edge_type: scout.scout.edge_type,
            owner: resolution.owner,
        };
        let rank = |c: &Candidate| (c.edge_type.priority(), c.port.0);
        if eligible
            && lead
                .best
                .map_or(true, |best| rank(&candidate) < rank(&best))
        {
            lead.best = Some(candidate);
        }
    }
    if lead.scatter_mode {
        issue(cx, lead, Command::Collect { probe_seq });
        tally(&mut cx.me, &pool, probe_seq);
        lead.phase = Phase::Collect { first };
    } else {
        next_batch_or_decide(cx, lead, first + lead.batch);
    }
}

/// Algorithm 2 lines 18-30 and Algorithm 3: classify the head, choose the next
/// edge, and decide whether its agent vacates.
fn decide(cx: &mut Cx<'_, '_>, lead: &mut Lead) {
    cx.note(Note::MacroRound);
    let owner_id = lead.head_owner.expect("the head has an owner");
    let mut owner = owner_memory(cx, lead);

    // Line 21, and Claim 3 for a dead end: a node with only a tpq parent edge
    // and no port-1 tree edge waits for its port-1 neighbour.
    let waiting = owner
        .parent_edge_type
        .is_some_and(|t| !t.is_port_one_incident())
        && !owner.port_one_tree_edge;
    let (node_type, plan) = match lead.best {
        Some(c) if waiting && !c.edge_type.is_port_one_incident() => {
            (NodeType::PartiallyVisited, Plan::Back)
        }
        Some(c) => (NodeType::Visited, Plan::Forward(c)),
        None if waiting => (NodeType::PartiallyVisited, Plan::Back),
        None => (NodeType::FullyVisited, Plan::Back),
    };
    if plan == Plan::Back && owner.parent.is_none() {
        cx.note(Note::Stuck(
            "the root ran out of edges with agents unsettled",
        ));
        return;
    }
    owner.node_type = node_type;
    lead.plan = plan;

    // Algorithm 3, read with the prose of Section 4.1. As printed, its
    // `visited` branch always returns, so rule (V5) could never fire for the
    // child Lemma 3 applies it to; here V5 is tried whenever this node stays.
    // A node others rely on (`vacated_neighbor`) never vacates; see
    // `agent::errand_step` for why that has to cover every rule.
    let own_rule = match node_type {
        _ if owner.state != AgentState::Settled || owner.parent_port.is_none() => None,
        _ if owner.vacated_neighbor => None,
        NodeType::Visited => Some(VacateRule::V2),
        NodeType::FullyVisited => Some(VacateRule::V3),
        NodeType::PartiallyVisited => Some(VacateRule::V4),
        NodeType::Unvisited => None,
    };
    let rule = own_rule.or_else(|| v5_applies(&owner).then_some(VacateRule::V5));
    if let Some(rule) = rule {
        start_errand(&mut owner, lead, rule);
    } else {
        lead.phase = Phase::PreMove;
    }
    write_owner(cx, owner_id, owner);
}

/// (V5) needs the parent's port 1 to lead here, and this node to stay.
fn v5_applies(owner: &Memory) -> bool {
    owner.state == AgentState::Settled && owner.parent.is_some_and(|(_, port)| port == PORT_ONE)
}

fn start_errand(owner: &mut Memory, lead: &mut Lead, rule: VacateRule) {
    owner.errand = None;
    owner.task = Task::Errand {
        rule,
        stage: 0,
        seq: lead.seq,
    };
    lead.phase = Phase::Errand(rule);
}

fn write_owner(cx: &mut Cx<'_, '_>, id: AgentId, memory: Memory) {
    *cx.write(id) = memory;
}

fn errand_done(cx: &mut Cx<'_, '_>, lead: &mut Lead, rule: VacateRule) {
    let owner_id = lead.head_owner.expect("the head has an owner");
    let Some(owner) = cx.find(owner_id) else {
        return; // still out
    };
    let (Task::Idle, Some(result)) = (owner.task, owner.errand) else {
        return;
    };
    let mut owner = owner.clone();
    let vacated = match result {
        ErrandResult::Anchored { .. } | ErrandResult::Joined => true,
        ErrandResult::TwoHops {
            anchor,
            port_at_anchor,
            port_at_p1,
        } => {
            // The middle node is vacated, not empty, exactly when its agent is
            // in the group and names the anchor as its own port-1 neighbour:
            // the test rule (R3c) will apply to it.
            let middle = cx.group_here().into_iter().find(|m| {
                m.state == AgentState::SettledScout
                    && m.p1_neighbor == Some(anchor)
                    && m.port_at_p1_neighbor == Some(port_at_anchor)
            });
            if let Some(middle) = middle {
                owner.state = AgentState::SettledScout;
                owner.p1_neighbor = Some(middle.id);
                owner.port_at_p1_neighbor = Some(port_at_p1);
                owner.seen_seq = lead.seq;
                cx.note(Note::Vacate);
                true
            } else {
                false
            }
        }
        ErrandResult::Refused | ErrandResult::NotJoined => false,
    };
    owner.errand = None;
    if vacated {
        lead.group_size += 1;
        lead.phase = Phase::PreMove;
    } else if rule != VacateRule::V5 && v5_applies(&owner) {
        start_errand(&mut owner, lead, VacateRule::V5);
    } else {
        lead.phase = Phase::PreMove;
    }
    write_owner(cx, owner_id, owner);
}

/// Algorithm 5 lines 32-46: record the move at the head's agent and send the
/// group on.
fn begin_move(cx: &mut Cx<'_, '_>, lead: &mut Lead) {
    let owner_id = lead.head_owner.expect("the head has an owner");
    let mut owner = owner_memory(cx, lead);
    let port = match lead.plan {
        Plan::Forward(c) => {
            owner.recent_port = Some(c.port);
            if owner.recent_child.is_some() {
                lead.sibling_details = lead.child_details.take();
            }
            owner.recent_child = Some(c.port);
            if c.edge_type.is_port_one_incident() {
                owner.port_one_tree_edge = true;
            }
            lead.child_port = Some(c.port);
            lead.target = c
                .owner
                .map_or(Target::Empty, |id| Target::Owner { id, forward: true });
            c.port
        }
        Plan::Back => {
            let (parent, port_at_parent) = owner.parent.expect("checked in decide");
            lead.child_details = Some((owner_id, port_at_parent));
            lead.child_sibling = owner.sibling;
            lead.child_port = None;
            owner.recent_port = owner.parent_port;
            lead.target = Target::Owner {
                id: parent,
                forward: false,
            };
            cx.note(Note::Backtrack);
            owner
                .parent_port
                .expect("a non-root node has a parent port")
        }
    };
    lead.prev_id = Some(owner_id);
    write_owner(cx, owner_id, owner);
    cx.note(Note::GroupMove(lead.group_size as usize));
    cx.note(Note::Step {
        forward: matches!(lead.plan, Plan::Forward(_)),
        batches: lead.batches_here,
        scouts: lead.batch,
    });
    issue(cx, lead, Command::Move { port });
    lead.phase = Phase::Depart;
}

/// The unsettled agents have left for their empty neighbours. Hand the group of
/// vacated agents to its new lead for Retrace, then follow them out.
fn finish_scatter(cx: &mut Cx<'_, '_>, lead: Lead) -> Option<Lead> {
    let Some(port) = cx.me.scatter.port else {
        cx.note(Note::Stuck("the scatter found too few empty neighbours"));
        return Some(lead);
    };
    let parent = lead.head_owner.expect("the head has an owner");
    cx.note(Note::Step {
        forward: true,
        batches: lead.batches_here,
        scouts: lead.batch,
    });
    cx.note(Note::Constructed);
    let group_left = lead.group_size - lead.unsettled;
    if group_left > 0 {
        let mut handed = lead;
        handed.group_size = group_left;
        handed.unsettled = 0;
        hand_over(cx, handed, Some(parent));
    }
    cx.me.task = Task::ScatterArrive {
        parent,
        port_at_parent: port,
    };
    cx.leave(port);
    None
}

/// Construction is over. Retrace starts where the group stands.
fn begin_retrace(cx: &mut Cx<'_, '_>, lead: Lead) -> Option<Lead> {
    cx.note(Note::Constructed);
    if lead.group_size > 0 {
        let owner = lead.head_owner;
        hand_over(cx, lead, owner);
    }
    None
}

/// Passes the lead role to the lowest-ID vacated agent here.
fn hand_over(cx: &mut Cx<'_, '_>, mut lead: Lead, next_agent: Option<AgentId>) {
    let Some(next) = cx
        .others()
        .filter(|m| m.state == AgentState::SettledScout)
        .map(|m| m.id)
        .min()
    else {
        return;
    };
    // If the group last arrived here from a child, Retrace resumes as though
    // returning from that child, and needs that child's sibling.
    let from_child = matches!(lead.target, Target::Owner { forward: false, .. });
    lead.sibling_details = if from_child { lead.child_sibling } else { None };
    lead.next_agent_id = next_agent;
    lead.phase = Phase::RetraceGather;
    cx.write(next).lead = Some(lead);
}

/// Algorithm 6 at one node: drop off whichever vacated agent lives here, then
/// pick the next node of the post-order walk.
fn retrace_step(cx: &mut Cx<'_, '_>, mut lead: Lead) -> Option<Lead> {
    let arrival = cx.me.arrival_port;
    let owner_id = if let Some(settled) = cx.settled_here() {
        settled.id
    } else {
        let Some(next) = lead.next_agent_id else {
            cx.note(Note::Stuck(
                "retrace reached an empty node with no one expected",
            ));
            return Some(lead);
        };
        let Some(found) = cx
            .find(next)
            .filter(|m| m.state == AgentState::SettledScout)
        else {
            cx.note(Note::Stuck(
                "retrace expected an agent the group does not have",
            ));
            return Some(lead);
        };
        let id = found.id;
        if id == cx.me.id {
            cx.me.state = AgentState::Settled;
        } else {
            cx.write(id).state = AgentState::Settled;
        }
        lead.group_size -= 1;
        id
    };
    if lead.group_size == 0 {
        return None;
    }

    let mut owner = cx.find(owner_id).expect("just found").clone();
    let mut sibling = lead.sibling_details;
    let to_parent = |owner: &Memory| {
        (
            owner.parent.map(|(id, _)| id),
            owner.parent_port,
            owner.sibling,
        )
    };
    let reconfigured_back = owner.old_parent_port.is_some()
        && arrival == owner.old_parent_port
        && arrival != owner.parent_port;
    let (next_agent, next_port) = if reconfigured_back {
        // Reached through the old parent's chain of children, which still
        // lists this node from before its reconfiguration. Treat it as the
        // leaf it was there and go back the way the group came.
        sibling = owner.old_sibling;
        (owner.old_parent.map(|(id, _)| id), arrival)
    } else if let Some(child) = owner.recent_child {
        if Some(child) == arrival {
            if let Some((id, port)) = lead.sibling_details {
                // Lines 19-21: across to the previous sibling.
                sibling = None;
                owner.recent_child = Some(port);
                (Some(id), Some(port))
            } else {
                // Lines 15-17: no more children, up to the parent.
                owner.recent_child = None;
                let (id, port, next_sibling) = to_parent(&owner);
                sibling = next_sibling;
                (id, port)
            }
        } else {
            // Lines 23-27: down to the most recent child, expecting its agent
            // if the group holds it. A reconfigured child is matched by its
            // old parent link too.
            let me = owner.id;
            let expected = cx
                .group_here()
                .into_iter()
                .find(|m| m.parent == Some((me, child)) || m.old_parent == Some((me, child)))
                .map(|m| m.id);
            (expected, Some(child))
        }
    } else {
        // Lines 29-32: a leaf, up to the parent.
        let (id, port, next_sibling) = to_parent(&owner);
        sibling = next_sibling;
        (id, port)
    };
    let Some(port) = next_port else {
        cx.note(Note::Stuck(
            "retrace ran past the root with agents still away",
        ));
        return Some(lead);
    };
    lead.next_agent_id = next_agent;
    lead.sibling_details = sibling;
    cx.note(Note::RetraceMove);

    if owner_id == cx.me.id {
        // The lead is home. Its context goes to the next lowest ID, which
        // issues the move this cycle chose.
        cx.me = Memory {
            lead: None,
            ..owner
        };
        lead.phase = Phase::RetraceIssue { port };
        let next = cx
            .others()
            .filter(|m| m.state == AgentState::SettledScout)
            .map(|m| m.id)
            .min()
            .expect("the group is not empty");
        cx.write(next).lead = Some(lead);
        return None;
    }
    write_owner(cx, owner_id, owner);
    issue(cx, &mut lead, Command::Move { port });
    lead.phase = Phase::RetraceDepart;
    Some(lead)
}
