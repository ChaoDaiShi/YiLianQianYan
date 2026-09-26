//! Domain modules.
//!
//! Each module owns its `domain` / `application` / `api` layers plus its
//! persistence. Modules interact through application services, commands,
//! events and projections — never by reaching into each other's internals.

pub mod settings;
