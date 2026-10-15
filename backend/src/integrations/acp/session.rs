use std::path::Path;
use tokio_util::sync::CancellationToken;

use super::client::{AcpClient, AcpError};
use super::model::AcpPromptOutcome;

#[derive(Clone)]
pub struct AcpSession {
    client: AcpClient,
    id: String,
}

impl AcpSession {
    pub async fn create(client: AcpClient, cwd: &Path) -> Result<Self, AcpError> {
        let id = client.new_session(cwd).await?;
        Ok(Self { client, id })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub async fn prompt(
        &self,
        text: &str,
        cancel: &CancellationToken,
    ) -> Result<AcpPromptOutcome, AcpError> {
        self.client.prompt(&self.id, text, cancel).await
    }

    pub async fn cancel(&self) -> Result<(), AcpError> {
        self.client.cancel_session(&self.id).await
    }
}
