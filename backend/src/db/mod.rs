// ============================================================
// Database module — SQLite persistence (thread-safe, cloneable)
// ============================================================

mod conversations;
mod settings;
mod memories;
mod mcp;
mod workflows;

pub use conversations::*;
pub use memories::*;
pub use mcp::McpServer;
pub use workflows::Workflow;
// settings::* not re-exported (used internally via Database impl)

use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Thread-safe, cloneable database handle
#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    /// Open (or create) the database at the given path
    pub fn new(path: &Path) -> Result<Self, rusqlite::Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path)?;
        let db = Self { conn: Arc::new(Mutex::new(conn)) };
        db.run_migrations()?;
        // Seed after migrations so the lock is released between calls
        db.seed_builtin_workflows()?;
        Ok(db)
    }

    /// Get a clone of this database handle (cheap — Arc clone)
    pub fn clone_connection(&self) -> Self {
        self.clone()
    }

    /// Close the database connection (best-effort, called when Arc refcount drops)
    pub fn close(&self) -> Result<(), String> {
        // Connection closes when the last Arc reference drops
        Ok(())
    }

    fn run_migrations(&self) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL DEFAULT '新对话',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL DEFAULT '',
                tool_calls TEXT,
                tool_call_id TEXT,
                tool_name TEXT,
                tool_result TEXT,
                created_at INTEGER NOT NULL,
                FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_messages_conv
                ON messages(conversation_id, created_at);

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                category TEXT NOT NULL DEFAULT 'fact',
                source TEXT NOT NULL DEFAULT 'auto',
                source_conversation_id TEXT,
                embedding TEXT,
                metadata TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                FOREIGN KEY (source_conversation_id) REFERENCES conversations(id) ON DELETE SET NULL
            );

            CREATE INDEX IF NOT EXISTS idx_memories_category ON memories(category);
            CREATE INDEX IF NOT EXISTS idx_memories_source ON memories(source);
            CREATE INDEX IF NOT EXISTS idx_memories_updated ON memories(updated_at);

            CREATE TABLE IF NOT EXISTS mcp_servers (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                transport TEXT NOT NULL DEFAULT 'stdio',
                command TEXT,
                args TEXT,
                url TEXT,
                env TEXT,
                enabled INTEGER NOT NULL DEFAULT 1,
                created_at INTEGER,
                updated_at INTEGER
            );

            CREATE TABLE IF NOT EXISTS workflows (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                nodes TEXT NOT NULL DEFAULT '[]',
                tags TEXT DEFAULT '[]',
                system_prompt_extra TEXT DEFAULT '',
                is_builtin INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER,
                updated_at INTEGER
            );"
        )?;

        Ok(())
    }

    /// Seed built-in workflow templates if the workflows table is empty.
    /// Must be called OUTSIDE of run_migrations to avoid Mutex deadlock.
    fn seed_builtin_workflows(&self) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM workflows", [], |row| row.get(0))
            .unwrap_or(0);
        if count > 0 {
            return Ok(());
        }

        let now = chrono::Utc::now().timestamp_millis();
        let builtins = vec![
            ("react-default", "ReAct 对话", "标准 ReAct 循环：准备→思考↔执行工具→回答。适用于大多数对话和平台能力场景。", r#"["prepare","think","tools","respond"]"#, r#"["默认","对话"]"#, ""),
            ("code-review", "代码审查流水线", "先理解项目结构 → 逐文件静态分析 → 安全漏洞扫描 → 输出分级报告", r#"["scan","analyze","security","report"]"#, r#"["代码","安全"]"#, "请按照代码审查流程：先扫描项目结构，然后逐文件分析，关注安全漏洞，最后输出分级报告。"),
            ("doc-generate", "文档生成管道", "扫描源码 → 提取API签名 → 生成文档 → 格式化为 README/API文档", r#"["scan","extract","generate","format"]"#, r#"["文档","自动化"]"#, "请生成文档：扫描源码结构，提取所有公开API签名，生成完整的API文档。"),
            ("research", "深度研究", "多源搜索 → 交叉验证 → 信息整合 → 生成研究报告", r#"["search","verify","synthesize","report"]"#, r#"["研究","分析"]"#, "请进行深度研究：多角度搜索相关信息，交叉验证，整合出完整的研究报告。"),
            ("multi-agent", "多智能体协作", "主智能体拆解任务 → 分派给子智能体并行处理 → 汇总结果 → 质量审查", r#"["dispatch","parallel","collect","review"]"#, r#"["多智能体","并行"]"#, "请作为主协调者：将任务拆解成独立子任务，分别处理，最后汇总结果并进行质量审查。"),
            ("hitl-approval", "人机协作审批", "AI生成草稿 → 暂停等待人工审核 → 根据反馈修改 → 最终定稿", r#"["draft","interrupt","revise","finalize"]"#, r#"["HITL","审批"]"#, "请协作模式：生成初稿后暂停，等待人工审核反馈，根据反馈修改后再最终定稿。"),
        ];

        for (id, name, desc, nodes, tags, extra_prompt) in &builtins {
            conn.execute(
                "INSERT INTO workflows (id, name, description, nodes, tags, system_prompt_extra, is_builtin, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?7)",
                rusqlite::params![id, name, desc, nodes, tags, extra_prompt, now],
            )?;
        }
        Ok(())
    }

    /// Get raw connection reference for internal use
    pub(crate) fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap()
    }
}
