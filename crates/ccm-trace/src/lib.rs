//! Optional semantic tracing for CCM simulations.
//!
//! The recorder API is deliberately independent of browsers, rendering, JSON,
//! and filesystems. Algorithms emit semantic events and explicit checkpoints;
//! callers choose [`NoTrace`], [`FullTrace`], or [`BoundedTrace`] at the
//! simulation boundary.

use ccm_core::{AgentId, AgentStatus, NodeId, PortId};
use std::collections::VecDeque;
use std::fmt;

/// A named logical phase in an algorithm execution.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Phase {
    MacroRound,
    ProbeOut,
    ProbeBack,
    Movement,
    Scout,
    Vacate,
    Chase,
    Follow,
    Retrace,
    Other,
}

/// A semantic event that can be replayed without exposing implementation data
/// structures. Events intentionally use dense IDs and compact enums.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SimulationEvent {
    PhaseStarted {
        phase: Phase,
    },
    AgentMoved {
        agent: AgentId,
        from: NodeId,
        to: NodeId,
        out_port: PortId,
        in_port: Option<PortId>,
    },
    GroupMoved {
        agents: Vec<AgentId>,
        from: NodeId,
        to: NodeId,
        out_port: PortId,
    },
    AgentSettled {
        agent: AgentId,
        node: NodeId,
    },
    AgentStateChanged {
        agent: AgentId,
        from: AgentStatus,
        to: AgentStatus,
    },
    TreeEdgeAdded {
        parent: AgentId,
        child: AgentId,
        parent_node: NodeId,
        child_node: NodeId,
        parent_port: Option<PortId>,
        child_port: Option<PortId>,
    },
    NodeStateChanged {
        node: NodeId,
        occupied: bool,
    },
}

/// A semantic event annotated with the logical step at which it occurred.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordedEvent {
    pub step: u64,
    pub event: SimulationEvent,
}

/// A compact, algorithm-facing world checkpoint.
///
/// The vectors are dense and use agent-ID order. Algorithm-specific state can
/// be represented by semantic events around the checkpoint; keeping this
/// structure small avoids forcing browser-oriented state into the core.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Checkpoint {
    pub step: u64,
    pub agent_nodes: Vec<NodeId>,
    pub agent_statuses: Vec<AgentStatus>,
    pub home_nodes: Vec<Option<NodeId>>,
    pub settled_agents: Vec<Option<AgentId>>,
}

impl Checkpoint {
    /// Returns the estimated logical word cost used by [`BoundedTrace`].
    #[must_use]
    pub fn estimated_words(&self) -> usize {
        1 + self.agent_nodes.len()
            + self.agent_statuses.len()
            + self.home_nodes.len()
            + self.settled_agents.len()
    }
}

/// A trace containing retained events and checkpoints.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Trace {
    pub events: Vec<RecordedEvent>,
    pub checkpoints: Vec<Checkpoint>,
}

/// Observer interface used by algorithm transitions.
///
/// All methods are side-effect-only and object-safe. Implementations must not
/// affect algorithm state; this is what permits trace-off equivalence tests and
/// headless native runs with [`NoTrace`].
pub trait Recorder {
    /// Whether the recorder can retain anything. Algorithms may use this as a
    /// fast path, but must produce the same state transitions either way.
    fn enabled(&self) -> bool {
        true
    }

    /// Records one semantic event at a logical step.
    fn record_event(&mut self, step: u64, event: SimulationEvent);

    /// Records a complete semantic checkpoint.
    fn record_checkpoint(&mut self, checkpoint: Checkpoint);
}

/// A zero-sized recorder for native experiments and trace-off equivalence.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoTrace;

impl Recorder for NoTrace {
    fn enabled(&self) -> bool {
        false
    }

    fn record_event(&mut self, _step: u64, _event: SimulationEvent) {}

    fn record_checkpoint(&mut self, _checkpoint: Checkpoint) {}
}

/// An unbounded recorder for detailed playback of small simulations.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FullTrace {
    trace: Trace,
}

impl FullTrace {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            trace: Trace {
                events: Vec::new(),
                checkpoints: Vec::new(),
            },
        }
    }

    #[must_use]
    pub const fn trace(&self) -> &Trace {
        &self.trace
    }

    #[must_use]
    pub fn into_trace(self) -> Trace {
        self.trace
    }
}

impl Recorder for FullTrace {
    fn record_event(&mut self, step: u64, event: SimulationEvent) {
        self.trace.events.push(RecordedEvent { step, event });
    }

    fn record_checkpoint(&mut self, checkpoint: Checkpoint) {
        self.trace.checkpoints.push(checkpoint);
    }
}

/// Explicit budgets and sampling intervals for [`BoundedTrace`].
///
/// A zero maximum disables retention for that category. Sampling is based on
/// the ordinal of submitted events/checkpoints, not wall-clock time or hash
/// iteration order. Retention eviction is oldest-first and deterministic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TraceBudget {
    pub max_event_count: usize,
    pub max_checkpoint_count: usize,
    pub max_event_words: usize,
    pub max_checkpoint_words: usize,
    pub event_sample_every: u64,
    pub checkpoint_sample_every: u64,
}

impl Default for TraceBudget {
    fn default() -> Self {
        Self {
            max_event_count: 10_000,
            max_checkpoint_count: 128,
            max_event_words: 100_000,
            max_checkpoint_words: 1_000_000,
            event_sample_every: 1,
            checkpoint_sample_every: 1,
        }
    }
}

/// Invalid bounded-trace configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BudgetError {
    ZeroEventSampleInterval,
    ZeroCheckpointSampleInterval,
}

impl fmt::Display for BudgetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroEventSampleInterval => f.write_str("event sample interval must be nonzero"),
            Self::ZeroCheckpointSampleInterval => {
                f.write_str("checkpoint sample interval must be nonzero")
            }
        }
    }
}

impl std::error::Error for BudgetError {}

/// Retention statistics useful for experiment metadata and tests.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TraceStats {
    pub submitted_events: u64,
    pub submitted_checkpoints: u64,
    pub sampled_events: u64,
    pub sampled_checkpoints: u64,
    pub retained_event_evictions: u64,
    pub retained_checkpoint_evictions: u64,
    pub dropped_events: u64,
    pub dropped_checkpoints: u64,
}

/// A deterministic, bounded recorder.
///
/// Events and checkpoints have independent count and estimated-word budgets.
/// New entries are retained only when selected by the configured ordinal
/// sampler. If a selected entry would exceed a budget, oldest retained entries
/// are evicted first. An entry larger than the complete category budget is
/// dropped. No random number generator or clock is used.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundedTrace {
    budget: TraceBudget,
    events: VecDeque<RecordedEvent>,
    checkpoints: VecDeque<Checkpoint>,
    event_words: usize,
    checkpoint_words: usize,
    next_event_ordinal: u64,
    next_checkpoint_ordinal: u64,
    stats: TraceStats,
}

impl BoundedTrace {
    /// Creates an empty bounded recorder.
    ///
    /// # Errors
    ///
    /// Returns [`BudgetError`] when a sampling interval is zero.
    pub const fn new(budget: TraceBudget) -> Result<Self, BudgetError> {
        if budget.event_sample_every == 0 {
            return Err(BudgetError::ZeroEventSampleInterval);
        }
        if budget.checkpoint_sample_every == 0 {
            return Err(BudgetError::ZeroCheckpointSampleInterval);
        }
        Ok(Self {
            budget,
            events: VecDeque::new(),
            checkpoints: VecDeque::new(),
            event_words: 0,
            checkpoint_words: 0,
            next_event_ordinal: 0,
            next_checkpoint_ordinal: 0,
            stats: TraceStats {
                submitted_events: 0,
                submitted_checkpoints: 0,
                sampled_events: 0,
                sampled_checkpoints: 0,
                retained_event_evictions: 0,
                retained_checkpoint_evictions: 0,
                dropped_events: 0,
                dropped_checkpoints: 0,
            },
        })
    }

    #[must_use]
    pub const fn budget(&self) -> TraceBudget {
        self.budget
    }

    #[must_use]
    pub const fn stats(&self) -> TraceStats {
        self.stats
    }

    pub fn events(&self) -> impl Iterator<Item = &RecordedEvent> {
        self.events.iter()
    }

    pub fn checkpoints(&self) -> impl Iterator<Item = &Checkpoint> {
        self.checkpoints.iter()
    }

    #[must_use]
    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    #[must_use]
    pub fn checkpoint_count(&self) -> usize {
        self.checkpoints.len()
    }

    #[must_use]
    pub const fn retained_event_words(&self) -> usize {
        self.event_words
    }

    #[must_use]
    pub const fn retained_checkpoint_words(&self) -> usize {
        self.checkpoint_words
    }

    #[must_use]
    pub fn into_trace(self) -> Trace {
        Trace {
            events: self.events.into_iter().collect(),
            checkpoints: self.checkpoints.into_iter().collect(),
        }
    }

    fn retain_event(&mut self, event: RecordedEvent) {
        let words = event.estimated_words();
        if self.budget.max_event_count == 0 || words > self.budget.max_event_words {
            self.stats.dropped_events += 1;
            return;
        }
        while self.events.len() >= self.budget.max_event_count
            || self.event_words.saturating_add(words) > self.budget.max_event_words
        {
            if let Some(old) = self.events.pop_front() {
                self.event_words -= old.estimated_words();
                self.stats.retained_event_evictions += 1;
            } else {
                self.stats.dropped_events += 1;
                return;
            }
        }
        self.event_words += words;
        self.events.push_back(event);
    }

    fn retain_checkpoint(&mut self, checkpoint: Checkpoint) {
        let words = checkpoint.estimated_words();
        if self.budget.max_checkpoint_count == 0 || words > self.budget.max_checkpoint_words {
            self.stats.dropped_checkpoints += 1;
            return;
        }
        while self.checkpoints.len() >= self.budget.max_checkpoint_count
            || self.checkpoint_words.saturating_add(words) > self.budget.max_checkpoint_words
        {
            if let Some(old) = self.checkpoints.pop_front() {
                self.checkpoint_words -= old.estimated_words();
                self.stats.retained_checkpoint_evictions += 1;
            } else {
                self.stats.dropped_checkpoints += 1;
                return;
            }
        }
        self.checkpoint_words += words;
        self.checkpoints.push_back(checkpoint);
    }
}

impl Default for BoundedTrace {
    fn default() -> Self {
        Self::new(TraceBudget::default()).expect("default intervals are nonzero")
    }
}

impl Recorder for BoundedTrace {
    fn record_event(&mut self, step: u64, event: SimulationEvent) {
        let ordinal = self.next_event_ordinal;
        self.next_event_ordinal = self.next_event_ordinal.saturating_add(1);
        self.stats.submitted_events = self.stats.submitted_events.saturating_add(1);
        if ordinal % self.budget.event_sample_every != 0 {
            self.stats.sampled_events = self.stats.sampled_events.saturating_add(1);
            return;
        }
        self.retain_event(RecordedEvent { step, event });
    }

    fn record_checkpoint(&mut self, checkpoint: Checkpoint) {
        let ordinal = self.next_checkpoint_ordinal;
        self.next_checkpoint_ordinal = self.next_checkpoint_ordinal.saturating_add(1);
        self.stats.submitted_checkpoints = self.stats.submitted_checkpoints.saturating_add(1);
        if ordinal % self.budget.checkpoint_sample_every != 0 {
            self.stats.sampled_checkpoints = self.stats.sampled_checkpoints.saturating_add(1);
            return;
        }
        self.retain_checkpoint(checkpoint);
    }
}

fn event_words(event: &SimulationEvent) -> usize {
    match event {
        SimulationEvent::PhaseStarted { .. } => 1,
        SimulationEvent::AgentMoved { .. } => 5,
        SimulationEvent::GroupMoved { agents, .. } => 4 + agents.len(),
        SimulationEvent::AgentSettled { .. } | SimulationEvent::NodeStateChanged { .. } => 2,
        SimulationEvent::AgentStateChanged { .. } => 3,
        SimulationEvent::TreeEdgeAdded { .. } => 6,
    }
}

impl RecordedEvent {
    #[must_use]
    pub fn estimated_words(&self) -> usize {
        1 + event_words(&self.event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checkpoint(step: u64) -> Checkpoint {
        Checkpoint {
            step,
            agent_nodes: vec![NodeId(0), NodeId(1)],
            agent_statuses: vec![AgentStatus::Unsettled, AgentStatus::Settled],
            home_nodes: vec![None, Some(NodeId(1))],
            settled_agents: vec![None, Some(AgentId(1))],
        }
    }

    fn run_traceable<R: Recorder>(recorder: &mut R) -> u64 {
        let mut digest = 0_u64;
        let from = NodeId(0);
        let to = NodeId(1);
        recorder.record_event(
            0,
            SimulationEvent::PhaseStarted {
                phase: Phase::Movement,
            },
        );
        digest = digest.wrapping_add(1);
        recorder.record_event(
            1,
            SimulationEvent::AgentMoved {
                agent: AgentId(0),
                from,
                to,
                out_port: PortId(0),
                in_port: Some(PortId(0)),
            },
        );
        digest = digest.wrapping_add(2);
        recorder.record_checkpoint(checkpoint(1));
        digest
    }

    #[test]
    fn no_trace_is_zero_sized_and_does_not_change_execution_digest() {
        assert_eq!(core::mem::size_of::<NoTrace>(), 0);
        let mut no_trace = NoTrace;
        let mut full = FullTrace::new();
        assert_eq!(run_traceable(&mut no_trace), run_traceable(&mut full));
        assert!(!no_trace.enabled());
        assert_eq!(full.trace().events.len(), 2);
        assert_eq!(full.trace().checkpoints.len(), 1);
    }

    #[test]
    fn bounded_sampling_and_eviction_are_deterministic() {
        let budget = TraceBudget {
            max_event_count: 2,
            max_checkpoint_count: 1,
            max_event_words: 20,
            max_checkpoint_words: 9,
            event_sample_every: 2,
            checkpoint_sample_every: 1,
        };
        let mut left = BoundedTrace::new(budget).unwrap();
        let mut right = BoundedTrace::new(budget).unwrap();
        for step in 0..6 {
            let event = SimulationEvent::NodeStateChanged {
                node: NodeId(u32::try_from(step).expect("test step fits in NodeId")),
                occupied: step % 2 == 0,
            };
            left.record_event(step, event.clone());
            right.record_event(step, event);
            left.record_checkpoint(checkpoint(step));
            right.record_checkpoint(checkpoint(step));
        }
        assert_eq!(left, right);
        assert_eq!(
            left.events().map(|event| event.step).collect::<Vec<_>>(),
            vec![2, 4]
        );
        assert_eq!(
            left.checkpoints().map(|item| item.step).collect::<Vec<_>>(),
            vec![5]
        );
        assert_eq!(left.stats().sampled_events, 3);
        assert_eq!(left.stats().retained_event_evictions, 1);
        assert_eq!(left.stats().retained_checkpoint_evictions, 5);
    }

    #[test]
    fn oversized_entries_are_dropped_without_eviction_loop() {
        let budget = TraceBudget {
            max_event_count: 2,
            max_checkpoint_count: 1,
            max_event_words: 2,
            max_checkpoint_words: 2,
            ..TraceBudget::default()
        };
        let mut recorder = BoundedTrace::new(budget).unwrap();
        recorder.record_event(
            0,
            SimulationEvent::GroupMoved {
                agents: vec![AgentId(0), AgentId(1)],
                from: NodeId(0),
                to: NodeId(1),
                out_port: PortId(0),
            },
        );
        recorder.record_checkpoint(checkpoint(0));
        assert_eq!(recorder.event_count(), 0);
        assert_eq!(recorder.checkpoint_count(), 0);
        assert_eq!(recorder.stats().dropped_events, 1);
        assert_eq!(recorder.stats().dropped_checkpoints, 1);
    }

    #[test]
    fn zero_sampling_interval_is_rejected() {
        let budget = TraceBudget {
            event_sample_every: 0,
            ..TraceBudget::default()
        };
        assert_eq!(
            BoundedTrace::new(budget),
            Err(BudgetError::ZeroEventSampleInterval)
        );
    }
}
