// ============================================================
// Database module — SQLite persistence (thread-safe, cloneable)
// ============================================================

mod conversations;
mod mcp;
mod memories;
mod security_audit;
mod settings;
mod task;
mod workflow_runtime;
mod workflows;
mod workspace;

pub use conversations::*;
pub use mcp::McpServer;
pub use memories::*;
pub use security_audit::*;
pub use task::*;
pub use workflow_runtime::*;
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
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
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
                env_secret_refs TEXT,
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
            );

            CREATE TABLE IF NOT EXISTS security_subjects (
                subject_id TEXT PRIMARY KEY,
                subject_type TEXT NOT NULL,
                provider TEXT NOT NULL,
                external_ref TEXT,
                display_name TEXT NOT NULL,
                status TEXT NOT NULL CHECK (status IN ('active', 'disabled')),
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS security_role_bindings (
                binding_id TEXT PRIMARY KEY,
                subject_id TEXT NOT NULL,
                role_key TEXT NOT NULL CHECK (role_key IN ('owner', 'standard', 'restricted')),
                source TEXT NOT NULL,
                effective_at INTEGER NOT NULL,
                expires_at INTEGER,
                revoked_at INTEGER,
                FOREIGN KEY (subject_id) REFERENCES security_subjects(subject_id)
            );

            CREATE UNIQUE INDEX IF NOT EXISTS idx_security_role_active_subject
                ON security_role_bindings(subject_id) WHERE revoked_at IS NULL;

            CREATE TABLE IF NOT EXISTS security_approvals (
                approval_id TEXT PRIMARY KEY,
                request_id TEXT NOT NULL,
                correlation_id TEXT NOT NULL,
                conversation_id TEXT NOT NULL,
                tool_call_id TEXT NOT NULL,
                subject_id TEXT NOT NULL,
                role_key TEXT NOT NULL,
                tool_name TEXT NOT NULL,
                risk_level TEXT NOT NULL,
                reason TEXT NOT NULL,
                arguments_view TEXT NOT NULL,
                request_digest TEXT NOT NULL,
                policy_version TEXT NOT NULL,
                grant_scope TEXT NOT NULL CHECK (grant_scope = 'once'),
                status TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                resolved_at INTEGER,
                FOREIGN KEY (subject_id) REFERENCES security_subjects(subject_id)
            );

            CREATE TABLE IF NOT EXISTS security_audit_events (
                event_id TEXT PRIMARY KEY,
                event_type TEXT NOT NULL,
                correlation_id TEXT NOT NULL,
                request_id TEXT NOT NULL,
                parent_event_id TEXT,
                created_at INTEGER NOT NULL,
                subject_id TEXT NOT NULL,
                role_key TEXT NOT NULL,
                conversation_id TEXT,
                tool_call_id TEXT,
                tool_name TEXT,
                capabilities_json TEXT NOT NULL,
                actions_json TEXT NOT NULL,
                resources_json TEXT NOT NULL,
                policy_version TEXT,
                risk_level TEXT,
                decision_status TEXT,
                request_digest TEXT,
                result_digest TEXT,
                details_json TEXT NOT NULL,
                error_category TEXT,
                previous_hash TEXT,
                event_hash TEXT,
                signature TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_security_audit_created_at
                ON security_audit_events(created_at);
            CREATE INDEX IF NOT EXISTS idx_security_audit_correlation
                ON security_audit_events(correlation_id);
            CREATE INDEX IF NOT EXISTS idx_security_audit_conversation
                ON security_audit_events(conversation_id);
            CREATE INDEX IF NOT EXISTS idx_security_audit_tool
                ON security_audit_events(tool_name);
            CREATE INDEX IF NOT EXISTS idx_security_audit_event_type
                ON security_audit_events(event_type);
            CREATE INDEX IF NOT EXISTS idx_security_audit_decision
                ON security_audit_events(decision_status);
            CREATE INDEX IF NOT EXISTS idx_security_audit_risk
                ON security_audit_events(risk_level);

            CREATE TABLE IF NOT EXISTS security_grants (
                grant_id TEXT PRIMARY KEY,
                subject_id TEXT NOT NULL,
                effect TEXT NOT NULL,
                permission_id TEXT NOT NULL,
                resource_json TEXT NOT NULL,
                source TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                expires_at INTEGER
            );

            CREATE INDEX IF NOT EXISTS idx_security_grants_subject
                ON security_grants(subject_id);
            CREATE INDEX IF NOT EXISTS idx_security_grants_permission
                ON security_grants(permission_id);
            CREATE INDEX IF NOT EXISTS idx_security_grants_expires
                ON security_grants(expires_at);

            CREATE TABLE IF NOT EXISTS workflow_graphs (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL DEFAULT '',
                description TEXT NOT NULL DEFAULT '',
                schema_version INTEGER NOT NULL,
                definition_json TEXT NOT NULL,
                created_at INTEGER,
                updated_at INTEGER
            );

            CREATE TABLE IF NOT EXISTS workflow_runs (
                run_id TEXT PRIMARY KEY,
                workflow_graph_id TEXT,
                execution_id TEXT NOT NULL,
                subject_id TEXT NOT NULL,
                agent_name TEXT NOT NULL,
                parent_execution_id TEXT,
                status TEXT NOT NULL,
                definition_snapshot_json TEXT NOT NULL,
                state_json TEXT NOT NULL,
                created_at INTEGER,
                updated_at INTEGER
            );

            CREATE INDEX IF NOT EXISTS idx_workflow_runs_graph
                ON workflow_runs(workflow_graph_id);

            -- ── v0.6 Workspace / Task / Multi-Agent Runtime ──

            CREATE TABLE IF NOT EXISTS workspaces (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                root_path TEXT,
                status TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS tasks (
                id TEXT PRIMARY KEY,
                workspace_id TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL,
                priority TEXT NOT NULL,
                workflow_graph_id TEXT,
                agent_team_id TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                completed_at INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_tasks_workspace ON tasks(workspace_id);
            CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
            CREATE INDEX IF NOT EXISTS idx_tasks_updated ON tasks(updated_at);

            CREATE TABLE IF NOT EXISTS task_executions (
                id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL,
                execution_id TEXT NOT NULL,
                subject_id TEXT NOT NULL,
                agent_name TEXT NOT NULL,
                parent_execution_id TEXT,
                workflow_run_id TEXT,
                agent_id TEXT,
                agent_team_id TEXT,
                status TEXT NOT NULL,
                attempt INTEGER NOT NULL DEFAULT 1,
                started_at INTEGER,
                finished_at INTEGER,
                error TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_task_executions_task ON task_executions(task_id);
            CREATE INDEX IF NOT EXISTS idx_task_executions_exec ON task_executions(execution_id);
            CREATE INDEX IF NOT EXISTS idx_task_executions_status ON task_executions(status);

            CREATE TABLE IF NOT EXISTS task_plans (
                id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL,
                task_execution_id TEXT NOT NULL,
                schema_version INTEGER NOT NULL,
                plan_json TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS artifacts (
                id TEXT PRIMARY KEY,
                workspace_id TEXT NOT NULL,
                task_id TEXT NOT NULL,
                task_execution_id TEXT NOT NULL,
                name TEXT NOT NULL,
                artifact_type TEXT NOT NULL,
                path TEXT,
                mime_type TEXT,
                size INTEGER,
                summary TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_artifacts_workspace ON artifacts(workspace_id);
            CREATE INDEX IF NOT EXISTS idx_artifacts_task ON artifacts(task_id);
            CREATE INDEX IF NOT EXISTS idx_artifacts_execution ON artifacts(task_execution_id);

            CREATE TABLE IF NOT EXISTS task_events (
                id TEXT PRIMARY KEY,
                workspace_id TEXT NOT NULL,
                task_id TEXT NOT NULL,
                task_execution_id TEXT,
                event_type TEXT NOT NULL,
                message TEXT NOT NULL DEFAULT '',
                metadata_json TEXT NOT NULL DEFAULT '{}',
                created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_task_events_task ON task_events(task_id, created_at);
            CREATE INDEX IF NOT EXISTS idx_task_events_execution ON task_events(task_execution_id);

            CREATE TABLE IF NOT EXISTS agent_definitions (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                instructions TEXT NOT NULL DEFAULT '',
                allowed_tools TEXT NOT NULL DEFAULT '[]',
                model TEXT,
                capabilities TEXT NOT NULL DEFAULT '[]',
                max_iterations INTEGER NOT NULL DEFAULT 10,
                enabled INTEGER NOT NULL DEFAULT 1,
                source TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS agent_teams (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                coordinator_agent_id TEXT NOT NULL,
                member_agent_ids TEXT NOT NULL DEFAULT '[]',
                delegation_policy_json TEXT NOT NULL DEFAULT '{}',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS agent_executions (
                id TEXT PRIMARY KEY,
                task_execution_id TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                parent_agent_execution_id TEXT,
                depth INTEGER NOT NULL DEFAULT 0,
                instruction TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL,
                result_summary TEXT,
                agent_state_json TEXT,
                started_at INTEGER,
                finished_at INTEGER,
                error TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_agent_executions_task_exec ON agent_executions(task_execution_id);

            CREATE TABLE IF NOT EXISTS task_decisions (
                id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL,
                task_execution_id TEXT NOT NULL,
                prompt TEXT NOT NULL,
                options_json TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                resolved_at INTEGER
            );
            ",
        )?;

        // Additive migration: older databases lack `env_secret_refs`.
        if !column_exists(&conn, "mcp_servers", "env_secret_refs")? {
            conn.execute_batch("ALTER TABLE mcp_servers ADD COLUMN env_secret_refs TEXT")?;
        }

        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "INSERT OR IGNORE INTO security_subjects (
                subject_id, subject_type, provider, external_ref, display_name,
                status, created_at, updated_at
             ) VALUES ('local-user', 'local_user', 'built_in', NULL,
                       'Local User', 'active', ?1, ?1)",
            [now],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO security_role_bindings (
                binding_id, subject_id, role_key, source, effective_at,
                expires_at, revoked_at
             ) VALUES ('local-user-owner-initial', 'local-user', 'owner',
                       'built_in', ?1, NULL, NULL)",
            [now],
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

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool, rusqlite::Error> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let names = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for name in names {
        if name? == column {
            return Ok(true);
        }
    }
    Ok(false)
}
