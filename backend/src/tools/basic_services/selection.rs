use crate::{
    db::workspace_service_settings::WorkspaceServiceKind,
    modules::settings::application::workspace_services::{
        resolve_conversation_service, ResolvedService,
    },
};

use super::BasicServiceTool;

impl BasicServiceTool {
    pub(super) fn selected(&self) -> Result<ResolvedService, String> {
        let kind = WorkspaceServiceKind::parse(&self.kind).map_err(|error| error.to_string())?;
        resolve_conversation_service(
            &self.db,
            kind,
            &self.conversation_id,
            self.agent_id.as_deref(),
        )
        .map_err(|error| error.to_string())
    }
}
