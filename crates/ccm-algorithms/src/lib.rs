//! The CCM dispersion algorithms, one module each.
//!
//! Every module is a self-contained, deterministic simulation over a
//! `ccm_core::PortGraph`. Tests live in `ccm-experiments/tests`.

pub mod drop_and_freeze;
pub mod help_by_scouts;
pub mod p1tree;

/// Version of the algorithm implementations, recorded in every experiment row.
pub const ALGORITHM_VERSION: &str = env!("CARGO_PKG_VERSION");
