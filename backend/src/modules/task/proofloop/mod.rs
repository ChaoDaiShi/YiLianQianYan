//! ProofLoop engine module.
//!
//! Defined in docs/superpowers/plans/2026-10-12-proofloop-parallel-development.md

pub mod acceptance;
pub mod budget;
pub mod capabilities;
pub mod compiler;
pub mod contracts;
pub mod drafts;
pub mod enhancement_contracts;
pub mod evidence;
pub mod planner;
pub mod repair;
pub mod schema_paths;

#[cfg(test)]
mod tests;

pub use acceptance::*;
pub use capabilities::*;
pub use compiler::*;
pub use contracts::*;
pub use drafts::*;
pub use enhancement_contracts::*;
pub use evidence::*;
pub use planner::*;
pub use repair::*;
pub use schema_paths::*;
