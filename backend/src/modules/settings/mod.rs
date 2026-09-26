//! Settings module — model/voice provider configuration, secret lifecycle and
//! provider readiness.
//!
//! Layers: `domain` (provider policy + readiness projection), `application`
//! (use cases), `api` (HTTP handlers). Persistence is owned by `db::settings`
//! and the `AppConfig` snapshot held by `AppServer`.

pub mod api;
pub mod application;
pub mod domain;

#[cfg(test)]
mod tests;
