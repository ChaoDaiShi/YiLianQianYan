use crate::{
    db::Database,
    interaction::chat_understanding::Role,
    modules::task::{AgentDefinition, AgentId, AgentSource},
};

pub(crate) fn is_preset(id: &str) -> bool {
    Role::ALL.into_iter().any(|role| role.agent_id() == id)
}

pub(crate) fn defaults(role: Role) -> AgentDefinition {
    let allowed: &[&str] = match role {
        Role::Daily => &["ask_user", "save_memory", "update_memory"],
        Role::General => &[],
        Role::Research => &[
            "ask_user",
            "ai_search",
            "read_file",
            "grep",
            "glob",
            "vision_analysis",
            "write_todos",
            "visible_browser_tabs",
            "visible_browser_observe",
        ],
        Role::Creative => &[
            "ask_user",
            "ai_search",
            "vision_analysis",
            "image_generation",
            "read_file",
            "write_file",
            "edit_file",
            "write_todos",
            "load_skill",
        ],
        Role::Development => &[
            "ask_user",
            "read_file",
            "write_file",
            "edit_file",
            "grep",
            "glob",
            "bash",
            "process",
            "load_skill",
            "write_todos",
        ],
    };
    AgentDefinition {
        id: AgentId::new(role.agent_id()).expect("built-in ID"),
        name: role.name().into(),
        description: role.guidance().into(),
        instructions: role.guidance().into(),
        allowed_tools: allowed.iter().map(|name| name.to_string()).collect(),
        model: None,
        capabilities: Vec::new(),
        max_iterations: 20,
        enabled: true,
        source: AgentSource::Builtin,
    }
}

pub(crate) fn get(db: &Database, id: &AgentId) -> Result<Option<AgentDefinition>, String> {
    if let Some(role) = Role::ALL
        .into_iter()
        .find(|role| role.agent_id() == id.as_str())
    {
        // Materialize in the existing store so model and other resource bindings
        // can reference the same stable agent ID.
        let definition = defaults(role);
        db.insert_builtin_agent_definition(&definition)?;
    }
    db.get_agent_definition(id)
}

pub(crate) fn ensure(db: &Database) -> Result<(), String> {
    for role in Role::ALL {
        db.insert_builtin_agent_definition(&defaults(role))?;
    }
    Ok(())
}
