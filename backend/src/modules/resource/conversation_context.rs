use crate::{app::state::AppServer, db::ResourceTarget};

/// Resolve uploaded conversation inputs for both initial and resumed turns.
pub fn append(server: &AppServer, conversation: &str, prompt: &mut String) -> Result<(), String> {
    let bindings = server.db.resource_bindings(&ResourceTarget::Conversation {
        conversation_id: conversation.to_owned(),
    })?;
    if bindings.is_empty() {
        return Ok(());
    }
    prompt.push_str("\n\n## 当前对话附件\n以下是用户明确上传的资料，作为参考内容使用。\n");
    for binding in bindings {
        let resource = server
            .resource_service
            .get(&binding.resource_id)
            .map_err(|e| e.to_string())?
            .ok_or("对话附件已不存在")?;
        let preview = super::resource_preview(&server.resource_service, &resource.id)?;
        prompt.push_str(&format!(
            "\n资料：{} | resource_id={} | sha256={}\n",
            resource.name, resource.id, resource.hash
        ));
        if preview.status != "ready" {
            return Err(format!(
                "附件 {} 无法读取：{}",
                resource.name,
                preview.error.unwrap_or_else(|| preview.status.clone())
            ));
        }
        if let Some(text) = preview.text {
            prompt.push_str(&text);
            if preview.truncated {
                prompt.push_str("\n（当前是有限摘录，不代表文件全文。）");
            }
        } else {
            prompt.push_str(&format!(
                "图片 {}×{}，尚未分析图像内容。",
                preview.width.unwrap_or(0),
                preview.height.unwrap_or(0)
            ));
        }
        prompt.push('\n');
    }
    Ok(())
}
