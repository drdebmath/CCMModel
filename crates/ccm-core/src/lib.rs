//! Pure simulation primitives shared by native experiments and browser WASM.
//!
//! This crate deliberately has no filesystem, browser, serialization, or CLI
//! dependencies. Algorithm implementations consume these types rather than
//! defining a second platform-specific model.

mod agents;
mod config;
mod graph;
mod ids;
mod metrics;
mod rng;
mod scheduler;
mod termination;

pub use agents::{AgentState, AgentStatus, AgentStore, AgentStoreError, OccupancyIndex};
pub use config::{
    Algorithm, ExperimentConfig, GraphModel, PortAssignment, SchedulerModel, StartingModel,
};
pub use graph::{GraphError, NodePorts, PortEdge, PortGraph};
pub use ids::{AgentId, NodeId, PortId};
pub use metrics::{ComplexityMetrics, LogicalMemory, Metrics, NoMetrics, RoundKind};
pub use rng::DeterministicRng;
pub use scheduler::{apply_move_intents, MoveIntent, SchedulerError};
pub use termination::{InvalidConfiguration, InvariantViolation, Termination};
