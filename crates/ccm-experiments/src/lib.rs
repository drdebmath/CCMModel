//! Native experiment planning and execution around the pure `ccm-core` model.
//!
//! This crate owns graph-family generation, deterministic placements, sweep
//! expansion, CSV result rows, and independent-run scheduling.  Algorithm
//! implementations plug in through [`SimulationRunner`]; no algorithm is
//! duplicated here.

mod csv;
mod graphs;
mod placements;
mod runner;
mod sweep;

pub use csv::{results_to_csv, CSV_HEADER};
pub use graphs::{generate_graph, GraphFamily, GraphGenerationError, GraphSpec};
pub use placements::{place_agents, PlacementError, PlacementSpec};
pub use runner::{
    execute_many_parallel, execute_one, prepare_run, BuiltinRunner, PreparedRun, RunError,
    RunRequest, RunResult, SimulationRunner,
};
pub use sweep::{expand_sweep, run_sweep_parallel, SweepSpec};
