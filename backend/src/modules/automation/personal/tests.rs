use super::super::{
    model::{AutomationInput, Schedule},
    occurrence,
};
use super::*;
use crate::{
    integrations::secret::{InMemorySecretStore, SecretRef},
    safety::{ControlSession, SecurityExecutionGateway, SecurityExecutionRequest, SecuritySubject},
    tools::RiskLevel,
};
use axum::{
    body::{to_bytes, Body},
    http::{Method, Request},
    routing::post,
    Router,
};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
use tower::ServiceExt;

mod support;
use support::*;

mod authority;
mod discovery;
mod file_events;
mod lifecycle;
mod management;
