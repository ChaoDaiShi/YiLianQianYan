//! Domain modules.
//!
//! Each module owns its `domain` / `application` / `api` layers plus its
//! persistence. Modules interact through application services, commands,
//! events and projections — never by reaching into each other's internals.

pub mod automation;
pub mod capability;
pub mod channel;
pub mod cloud;
pub mod code;
pub(crate) mod harness_profiles;
pub mod knowledge_build;
pub mod learning;
pub mod memory_skill;
pub mod onboarding;
pub mod resource;
pub mod settings;
pub mod task;
pub mod voice;
pub mod workflow;
