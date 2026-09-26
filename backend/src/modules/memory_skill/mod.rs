//! User-reviewed evidence candidates; never an automatic long-term Memory write.
//!
//! A candidate is created from explicitly authorized completed-task evidence,
//! edited and reviewed by the user, and only then installed as a versioned
//! managed skill. Nothing here writes long-term Memory, and nothing installs
//! without an explicit confirmation flag.
//!
//! `candidate` -> `review` -> `version` is the lifecycle. `evidence`,
//! `sensitivity` and `validator` hold the checks each step applies,
//! `repository` maps rows, and `service` owns the handle and the managed-file
//! side effects.
//!
//! Candidate, evidence, review and version are the stable concepts a future
//! capability-evolution feature would build on. That feature does not exist
//! here and nothing in this module anticipates its shape.

mod candidate;
mod evidence;
mod repository;
mod review;
mod sensitivity;
mod service;
mod validator;
mod version;

pub mod store;

pub use service::MemorySkillService;

#[cfg(test)]
mod tests;
