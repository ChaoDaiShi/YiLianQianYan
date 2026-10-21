use super::Database;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceMode {
    #[default]
    Chat,
    Work,
    Create,
}
impl WorkspaceMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Work => "work",
            Self::Create => "create",
        }
    }
    pub fn prompt(self) -> &'static str {
        match self {
            Self::Chat => "当前为聊天模式。自然回答用户的问题。用户转而要求实际操作时直接使用必要工具开展工作，不要只描述计划或声称已经完成。",
            Self::Work => "当前为工作模式。围绕用户目标执行、观察并验证；需要修改文件时实际修改，需要检查项目时读取实际状态。完成后说明产物位置与验证结果。普通交流可以直接回答，无需额外调用工具。",
            Self::Create => "当前为创造模式。根据用户要求创作文稿、设计、代码或媒体产物；需要文件或图片时使用相应实际工具生成并检查，保留可继续编辑的成果。不要将创作建议当成已经交付的产物。",
        }
    }
    pub fn for_tool(self, name: &str) -> Self {
        if name == "image_generation" {
            return Self::Create;
        }
        if self == Self::Chat
            && matches!(
                name,
                "write_file"
                    | "edit_file"
                    | "bash"
                    | "process"
                    | "mouse"
                    | "keyboard"
                    | "open_application"
                    | "open_url"
            )
        {
            return Self::Work;
        }
        self
    }
}
pub(crate) fn load_mode_on(conn: &Connection, id: &str) -> Result<WorkspaceMode, String> {
    let stored: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key=?1",
            [format!("conversation_mode:{id}")],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    match stored {
        Some(value) => serde_json::from_str(&value).map_err(|e| e.to_string()),
        None => Ok(WorkspaceMode::Chat),
    }
}
impl Database {
    pub fn conversation_mode(&self, id: &str) -> Result<WorkspaceMode, String> {
        load_mode_on(&self.conn(), id)
    }
    pub fn set_conversation_mode(&self, id: &str, mode: WorkspaceMode) -> Result<bool, String> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        if !tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM conversations WHERE id=?1)",
                [id],
                |r| r.get::<_, bool>(0),
            )
            .map_err(|e| e.to_string())?
        {
            return Ok(false);
        }
        tx.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![format!("conversation_mode:{id}"),serde_json::to_string(&mode).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
        tx.execute(
            "UPDATE settings SET value=json_set(value,'$.workspace_mode',?2) WHERE key=?1",
            params![format!("chat_options:{id}"), mode.as_str()],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(true)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn modes_persist_and_actual_work_changes_mode_without_classifying_user_words() {
        let db = Database::new(std::path::Path::new(":memory:")).unwrap();
        let id = db.create_conversation("mode").unwrap().id;
        assert_eq!(db.conversation_mode(&id).unwrap(), WorkspaceMode::Chat);
        assert_eq!(
            WorkspaceMode::Chat.for_tool("read_file"),
            WorkspaceMode::Chat
        );
        assert_eq!(
            WorkspaceMode::Chat.for_tool("write_file"),
            WorkspaceMode::Work
        );
        assert_eq!(
            WorkspaceMode::Work.for_tool("image_generation"),
            WorkspaceMode::Create
        );
        assert_eq!(
            WorkspaceMode::Create.for_tool("write_file"),
            WorkspaceMode::Create
        );
        db.set_conversation_mode(&id, WorkspaceMode::Work).unwrap();
        assert_eq!(
            db.get_conversation(&id).unwrap().workspace_mode,
            WorkspaceMode::Work
        );
        assert!(!db
            .set_conversation_mode("missing", WorkspaceMode::Create)
            .unwrap());
    }
}
