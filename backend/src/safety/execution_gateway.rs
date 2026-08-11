use serde_json::Value;

#[derive(Debug)]
pub struct SecurityExecutionRequest {
    pub conversation_id: String,
    pub tool_call_id: String,
    pub tool_name: String,
    pub arguments: Value,
}

#[derive(Debug)]
pub enum SecurityExecutionOutcome {
    Executed,
    RequiresApproval,
    Denied { reason: String },
}

pub struct SecurityExecutionGateway;

impl SecurityExecutionGateway {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::SecurityExecutionGateway;

    #[test]
    fn gateway_can_be_constructed() {
        let _gateway = SecurityExecutionGateway::new();
    }
}
