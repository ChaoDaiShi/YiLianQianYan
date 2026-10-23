use super::{ChannelPreset, ChannelPresetInput};
use crate::{db::Database, modules::task::AgentId};

pub const CHANNEL_PROMPT: &str = "你正在回复已配置外部渠道中的私聊文本。只根据此会话内容回答，不访问本机文件、桌面、私有知识库或其他对话。此会话没有工具权限。不要声称已执行操作。只输出发给联系人的回复正文，不描述内部生成或发送流程。尽量在2000字以内回答。";

pub fn builtin_presets() -> Vec<ChannelPreset> {
    [
        ("builtin-cyrene", "昔涟 · 温柔日常", "用自然、温柔的中文交流。先回应对方的重点，保持简洁；需要解释时给出清楚的步骤。尊重对方的真实感受，不虚构经历。"),
        ("builtin-concise", "简洁工作助手", "优先给出明确结论和可执行的下一步。表达简洁，区分已知事实和待确认信息；确有必要时使用短列表。"),
        ("builtin-study", "学习答疑", "耐心解释知识与推导过程。根据问题给出清晰步骤和适量例子，帮助对方理解；不编造资料来源。"),
    ].into_iter().map(|(id,name,instructions)|ChannelPreset{id:id.into(),name:name.into(),agent_id:None,model_profile_id:None,instructions:instructions.into(),builtin:true,created_at:0,updated_at:0}).collect()
}

pub fn validate(db: &Database, input: &ChannelPresetInput) -> Result<(), String> {
    if input.name.trim().is_empty() || input.name.chars().count() > 80 {
        return Err("预设名称必须为1–80字符".into());
    }
    if input.instructions.chars().count() > 16000 {
        return Err("回复提示词不能超过16000字符".into());
    }
    validate_references(
        db,
        input.agent_id.as_deref(),
        input.model_profile_id.as_deref(),
    )
}

pub fn validate_references(
    db: &Database,
    agent: Option<&str>,
    model: Option<&str>,
) -> Result<(), String> {
    if let Some(id) = agent {
        let agent = db
            .get_agent_definition(&AgentId::new(id).map_err(|e| e.to_string())?)?
            .ok_or("所选角色已删除")?;
        if !agent.enabled {
            return Err("所选角色已停用".into());
        }
    }
    if let Some(id) = model {
        let model = db.get_llm_model(id)?.ok_or("所选模型档案已删除")?;
        if model.api_format != "openai" {
            return Err("所选模型档案不支持对话协议".into());
        }
    }
    Ok(())
}

pub fn prompt(db: &Database, preset: Option<&ChannelPreset>) -> Result<String, String> {
    let mut parts = vec![CHANNEL_PROMPT.to_string()];
    if let Some(preset) = preset {
        validate_references(
            db,
            preset.agent_id.as_deref(),
            preset.model_profile_id.as_deref(),
        )?;
        if let Some(agent) = preset.agent_id.as_deref() {
            let profile = crate::api::agents::persona::profile_for_agent(db, agent)?;
            let persona = crate::api::agents::persona::compose_user_persona(&profile);
            if !persona.is_empty() {
                parts.push(persona);
            }
        }
        if !preset.instructions.trim().is_empty() {
            parts.push(format!(
                "当前回复预设：{}\n{}",
                preset.name,
                preset.instructions.trim()
            ));
        }
    }
    Ok(parts.join("\n\n"))
}
