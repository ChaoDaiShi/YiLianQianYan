use super::*;
use crate::{
    integrations::secret::{InMemorySecretStore, SecretRef},
    safety::ControlSession,
};
use axum::{routing::post, Router};
use std::sync::Mutex;

fn server() -> Arc<AppServer> {
    Arc::new(
        AppServer::new_with_control_session_and_store(
            std::path::Path::new(":memory:"),
            ".",
            ControlSession::new("robot-fixture-control-session-0000000000000000").unwrap(),
            Arc::new(InMemorySecretStore::new()),
        )
        .unwrap(),
    )
}
fn configured(server: &AppServer) -> Channel {
    let channel = super::super::tests::channel("onebot11");
    server.db.save_channel(&channel).unwrap();
    let mut policy = RobotPolicy::new(&channel.id);
    policy.account_id = channel.self_id.clone().unwrap();
    policy.enabled = true;
    policy.owner_ids = vec!["200".into()];
    policy.send_private_ids = vec!["300".into()];
    server.db.save_robot_policy(&policy).unwrap();
    channel
}

mod execution;
mod persistence;
