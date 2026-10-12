//! The CCM dispersion algorithms, one module each, plus [`common`] for the
//! port-1 vocabulary they share.
//!
//! `drop_and_freeze` and `help_by_scouts` are legacy baselines that preserve
//! historical behaviour. `p1tree` and `rooted_disp` are written from their
//! papers. Every module is a self-contained, deterministic simulation over a
//! `ccm_core::PortGraph`. Tests live in `ccm-experiments/tests`.

pub mod common;
pub mod drop_and_freeze;
pub mod help_by_scouts;
pub mod p1tree;
pub mod rooted_disp;

/// Version of the algorithm implementations, recorded in every experiment row.
pub const ALGORITHM_VERSION: &str = env!("CARGO_PKG_VERSION");
