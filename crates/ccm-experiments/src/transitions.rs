//! Role-transition traces for `GeneralDisp`, prepared like a CSV run.

use crate::runner::{check_placement, default_round_limit, prepare_run, RunError, RunRequest};
use ccm_algorithms::general_disp::{self, slot_of, Group, GroupEvent};
use ccm_core::{Algorithm, ComplexityMetrics, Termination};
use ccm_trace::{Checkpoint, Recorder, SimulationEvent};
use std::fmt::Write as _;

/// One row of a role-transition trace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoleTransition {
    pub step: u64,
    pub slot: u8,
    pub kind: &'static str,
    pub agent: u32,
    pub node: u32,
    /// `name=value` pairs separated by spaces.
    pub detail: String,
}

/// A role-transition trace and how the run ended.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitionTrace {
    pub termination: Termination,
    pub steps: u64,
    pub transitions: Vec<RoleTransition>,
}

/// Turns the event log on without storing checkpoints.
struct Listening;

impl Recorder for Listening {
    fn record_event(&mut self, _step: u64, _event: SimulationEvent) {}
    fn record_checkpoint(&mut self, _checkpoint: Checkpoint) {}
}

fn group(group: Group) -> String {
    format!("{}:{}", group.level, group.leader.0)
}

fn transition(event: GroupEvent) -> RoleTransition {
    let (step, kind, agent, node, detail) = match event {
        GroupEvent::BecameZombie {
            step,
            agent,
            node,
            stronger,
        } => (
            step,
            "became_zombie",
            agent,
            node,
            format!("stronger={}", group(stronger)),
        ),
        GroupEvent::Settled {
            step,
            agent,
            node,
            group: label,
        } => (
            step,
            "settled",
            agent,
            node,
            format!("group={}", group(label)),
        ),
        GroupEvent::Incorporated {
            step,
            settler,
            node,
            from,
            to,
        } => (
            step,
            "incorporated",
            settler,
            node,
            format!("from={} to={}", group(from), group(to)),
        ),
        GroupEvent::LevelUp {
            step,
            leader,
            zombie,
            node,
            level,
        } => (
            step,
            "level_up",
            leader,
            node,
            format!("level={level} zombie={}", zombie.0),
        ),
        GroupEvent::ForwardMove {
            step,
            leader,
            from,
            to,
        } => (step, "forward", leader, from, format!("to={}", to.0)),
        GroupEvent::BackwardMove {
            step,
            leader,
            from,
            to,
        } => (step, "backward", leader, from, format!("to={}", to.0)),
        GroupEvent::ZombieLevels {
            step,
            zombie,
            location,
            swarm,
        } => {
            return RoleTransition {
                step,
                slot: slot_of(step),
                kind: "zombie_levels",
                agent: zombie.0,
                node: u32::MAX,
                detail: format!("location={location} swarm={swarm}"),
            };
        }
        GroupEvent::Chased {
            step,
            zombie,
            from,
            to,
            weak,
        } => (
            step,
            "chase",
            zombie,
            from,
            format!("to={} weak={weak}", to.0),
        ),
    };
    RoleTransition {
        step,
        slot: slot_of(step),
        kind,
        agent: agent.0,
        node: node.0,
        detail,
    }
}

/// Prepares `request` and runs `GeneralDisp` with its event log on.
///
/// # Errors
///
/// Returns a preparation error, [`RunError::AlgorithmUnavailable`] when the
/// request names another algorithm (only `GeneralDisp` has role
/// transitions), or the algorithm's error as [`RunError::Message`].
pub fn general_disp_transitions(request: RunRequest) -> Result<TransitionTrace, RunError> {
    if request.algorithm != Algorithm::GeneralDisp {
        return Err(RunError::AlgorithmUnavailable(request.algorithm));
    }
    let prepared = prepare_run(request)?;
    check_placement(&prepared)?;
    let limit = prepared
        .request
        .round_limit
        .unwrap_or_else(|| default_round_limit(Algorithm::GeneralDisp, prepared.request.agents));
    let (result, _, _) = general_disp::simulate(
        &prepared.graph,
        &prepared.starts,
        limit,
        ComplexityMetrics::default(),
        Listening,
    )
    .map_err(|error| RunError::Message(error.to_string()))?;
    Ok(TransitionTrace {
        termination: result.termination,
        steps: result.steps,
        transitions: result.events.into_iter().map(transition).collect(),
    })
}

/// Header of [`transitions_to_csv`]. `node` is empty for `zombie_levels`.
pub const TRANSITIONS_HEADER: &str = "step,slot,event,agent,node,detail\n";

/// One CSV row per transition, in the order they happened.
#[must_use]
pub fn transitions_to_csv(trace: &TransitionTrace) -> String {
    let mut output = String::from(TRANSITIONS_HEADER);
    for row in &trace.transitions {
        let node = if row.node == u32::MAX {
            String::new()
        } else {
            row.node.to_string()
        };
        let _ = writeln!(
            output,
            "{},{},{},{},{},{}",
            row.step, row.slot, row.kind, row.agent, node, row.detail
        );
    }
    output
}
