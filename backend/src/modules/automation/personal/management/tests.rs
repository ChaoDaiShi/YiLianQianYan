use super::*;
use crate::{
    agent::engine::AgentEvent,
    integrations::secret::{InMemorySecretStore, SecretRef},
    modules::automation::model::{AutomationInput, Schedule},
    safety::{
        grant::{GrantEffect, GrantResource, GrantSource, SecurityGrant},
        ControlSession, PermissionId,
    },
};
use std::sync::Arc;

mod support;
use support::*;

mod projection;
mod recovery;
