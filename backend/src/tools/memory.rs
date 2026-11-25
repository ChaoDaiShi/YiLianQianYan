use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    agent::memory::MemoryCategory,
    db::{
        memory_governance::{CorrectMemoryRequest, WithdrawMemoryRequest},
        CreateMemoryRequest, Database, MemoryScope,
    },
};

use super::{RiskLevel, Tool, ToolExecutionContext, ToolResult};

pub(crate) struct SaveMemoryTool(pub(crate) Database);

#[derive(Deserialize)]
struct SaveMemoryArgs {
    content: String,
    category: MemoryCategory,
    #[serde(default)]
    scope: MemoryScope,
}

#[async_trait]
impl Tool for SaveMemoryTool {
    fn name(&self) -> &str {
        "save_memory"
    }

    fn description(&self) -> &str {
        "当用户明确要求记住新的事实、偏好、知识或笔记时保存记忆。scope 指定适用范围：仅本对话用 conversation，仅当前工作空间用 workspace，跨工作空间的长期偏好用 profile。每条独立记忆调用一次，成功后确认范围。纠正或撤回用 update_memory。"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "用户要记住的内容，保留原意和具体条件",
                    "maxLength": 2000
                },
                "category": {
                    "type": "string",
                    "enum": ["fact", "preference", "knowledge", "note"],
                    "description": "事实、偏好、知识或笔记"
                },
                "scope": {
                    "type": "string",
                    "enum": ["conversation", "workspace", "profile"],
                    "default": "profile",
                    "description": "适用范围：本对话、本工作空间或当前档案。未指定时保存为档案记忆"
                }
            },
            "required": ["content", "category"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Medium
    }

    async fn execute(&self, _args: Value) -> ToolResult {
        ToolResult::error("保存记忆需要通过工具执行网关")
    }

    async fn execute_with_context(
        &self,
        args: Value,
        context: &ToolExecutionContext,
    ) -> ToolResult {
        let Ok(args) = serde_json::from_value::<SaveMemoryArgs>(args) else {
            return ToolResult::error("请提供要保存的内容和有效的记忆分类");
        };
        match self.0.create_memory_with_scope(
            &CreateMemoryRequest {
                content: args.content.trim().to_string(),
                category: args.category.as_str().to_string(),
                source: "manual".into(),
                source_conversation_id: context.conversation_id().map(str::to_owned),
                metadata: None,
            },
            args.scope,
        ) {
            Ok(memory) => ToolResult::success(format!(
                "已保存为{}记忆（ID：{}）",
                args.scope.label(),
                memory.id
            )),
            Err(error) => ToolResult::error(match error.as_str() {
                "memory_conversation_required" => "仅本对话的记忆需要当前会话",
                "memory_workspace_required" => "当前对话未绑定可用的工作空间，无法保存工作空间记忆",
                "invalid_memory_content" => "记忆内容须为 1 至 2000 个字符",
                "sensitive_memory_content" => "记忆库拒绝保存含凭据等敏感信息的内容",
                _ => {
                    tracing::warn!(%error, "failed to save chat memory");
                    "记忆库写入失败"
                }
            }),
        }
    }
}

pub(crate) struct UpdateMemoryTool(pub(crate) Database);

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum UpdateMemoryArgs {
    Correct {
        memory_id: String,
        expected_revision: i64,
        content: String,
    },
    Withdraw {
        memory_id: String,
        expected_revision: i64,
    },
}

#[async_trait]
impl Tool for UpdateMemoryTool {
    fn name(&self) -> &str {
        "update_memory"
    }

    fn description(&self) -> &str {
        "用户明确要求纠正、撤回或忘掉一条已有长期记忆时调用。目标须唯一，使用本轮召回记录的 memory_id 和 revision。纠正时保留原记录标识，撤回后停止召回并保留历史。"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["correct", "withdraw"]
                },
                "memory_id": {
                    "type": "string",
                    "description": "本轮召回中目标记录的 memory_id"
                },
                "expected_revision": {
                    "type": "integer",
                    "description": "本轮召回中目标记录的 revision"
                },
                "content": {
                    "type": "string",
                    "description": "纠正时必填，提供保留未改动信息的完整新内容",
                    "maxLength": 2000
                }
            },
            "required": ["action", "memory_id", "expected_revision"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Medium
    }

    async fn execute(&self, _args: Value) -> ToolResult {
        ToolResult::error("更新记忆需要通过聊天工具执行网关")
    }

    async fn execute_with_context(
        &self,
        args: Value,
        context: &ToolExecutionContext,
    ) -> ToolResult {
        let Some(conversation_id) = context.conversation_id() else {
            return ToolResult::error("更新记忆需要当前会话");
        };
        let Ok(args) = serde_json::from_value::<UpdateMemoryArgs>(args) else {
            return ToolResult::error("请提供目标记忆的标识、版本和更新操作");
        };
        let result = match args {
            UpdateMemoryArgs::Correct {
                memory_id,
                expected_revision,
                content,
            } => self
                .0
                .correct_memory_scoped(
                    &memory_id,
                    &CorrectMemoryRequest {
                        content,
                        expected_revision,
                        reason: "用户在聊天中纠正".into(),
                        scope: None,
                        source_ref: None,
                    },
                    Some(conversation_id),
                )
                .map(|_| "已纠正原记忆"),
            UpdateMemoryArgs::Withdraw {
                memory_id,
                expected_revision,
            } => self
                .0
                .withdraw_memory_scoped(
                    &memory_id,
                    &WithdrawMemoryRequest {
                        expected_revision,
                        reason: "用户在聊天中撤回".into(),
                    },
                    Some(conversation_id),
                )
                .map(|_| "已撤回原记忆"),
        };
        match result {
            Ok(message) => ToolResult::success(message),
            Err(error) => ToolResult::error(match error.as_str() {
                "memory_not_in_scope" => "这条记忆当前已无法在本会话中更新",
                "memory_revision_conflict" => "这条记忆已被更新，请按当前版本重新确认",
                "knowledge_document_owned_chunk" => "这条内容属于资料库文档，请通过原文档更新",
                "invalid_memory_correction" => "纠正后的记忆内容须为 1 至 2000 个字符",
                "sensitive_memory_content" => "记忆库拒绝保存含凭据等敏感信息的内容",
                _ => {
                    tracing::warn!(%error, "failed to update chat memory");
                    "记忆库更新失败"
                }
            }),
        }
    }
}
