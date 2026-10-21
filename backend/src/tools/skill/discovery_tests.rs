use super::*;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("yilian-skill-discovery-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn skill(&self, relative: &str, content: &str) -> PathBuf {
        let root = self.0.join(relative);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("SKILL.md"), content).unwrap();
        root
    }
    fn discover(&self, configured: &[String]) -> SkillDiscovery {
        SkillDiscovery::discover_with_roots(
            configured,
            &self.0.join("workspace"),
            Some(&self.0.join("home")),
            None,
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn finds_actual_local_agent_skills_and_keeps_configured_collision_winner() {
    let fixture = Fixture::new();
    let configured = fixture.skill("custom/report", "# Configured report\nConfigured procedure");
    fixture.skill(
        "workspace/.github/skills/review",
        "---\nname: code-review\ndescription: Review a change\n---\n# Review\nActual review body",
    );
    fixture.skill(
        "home/.codex/skills/report",
        "---\nname: report\ndescription: Codex report\n---\n# Report\nExternal actual body",
    );
    fixture.skill(
        "home/.claude/skills/claude-task",
        "---\ndescription: |\n  Claude procedure\n  with real references\n---\n# Task",
    );
    let discovery = fixture.discover(&[fixture.0.join("custom").to_string_lossy().to_string()]);
    assert_eq!(discovery.get("report").unwrap().root_dir, configured);
    let external = discovery
        .all()
        .into_iter()
        .find(|skill| skill.source_id == "user-codex" && skill.display_name == "report")
        .unwrap();
    assert_ne!(external.name, "report");
    assert_eq!(external.description, "Codex report");
    assert!(external
        .path
        .ends_with("home/.codex/skills/report/SKILL.md"));
    let review = discovery.get("review").unwrap();
    assert_eq!(review.display_name, "code-review");
    assert_eq!(review.source, "工作空间 GitHub / VS Code");
    assert!(discovery
        .get("claude-task")
        .unwrap()
        .description
        .contains("Claude procedure with real references"));
}

#[test]
fn scans_known_plugin_layout_only_and_retains_real_script_and_reference_root() {
    let fixture = Fixture::new();
    let plugin = fixture.skill(
        "home/.codex/plugins/cache/market/plugin/1.2.3/skills/tool",
        "---\nname: tool\ndescription: Plugin tool\n---\n# Body",
    );
    std::fs::create_dir(plugin.join("references")).unwrap();
    std::fs::write(plugin.join("references/guide.md"), "Actual reference").unwrap();
    std::fs::create_dir(plugin.join("scripts")).unwrap();
    std::fs::write(plugin.join("scripts/run.ps1"), "Write-Output actual").unwrap();
    fixture.skill(
        "home/random-private-folder/hidden",
        "# Must not scan unrelated home folder",
    );
    fixture.skill(
        "workspace/.github/skills/a/b/c/d/e/too-deep",
        "# Beyond bounded skill scan",
    );
    let discovery = fixture.discover(&[]);
    let mut skill = discovery.get("tool").unwrap().clone();
    assert_eq!(skill.root_dir, plugin);
    assert_eq!(
        skill.load_reference("guide").unwrap().as_deref(),
        Some("Actual reference")
    );
    assert!(skill.load_content().unwrap().contains("# Body"));
    assert!(skill.source.contains("Codex 插件"));
    assert!(plugin.join("scripts/run.ps1").is_file());
    assert!(discovery.get("hidden").is_none());
    assert!(discovery.get("too-deep").is_none());
    assert!(discovery
        .sources()
        .iter()
        .any(|source| source.kind == "plugin" && source.skill_count == 1));
}

#[test]
fn deduplicates_same_canonical_skill_and_refresh_reads_external_updates_without_writing() {
    let fixture = Fixture::new();
    let external = fixture.skill(
        "home/.agents/skills/shared",
        "---\ndescription: First version\n---\n# Shared",
    );
    let roots = vec![
        external.to_string_lossy().to_string(),
        external.to_string_lossy().to_string(),
    ];
    let first = fixture.discover(&roots);
    assert_eq!(
        first
            .all()
            .iter()
            .filter(|s| s.display_name == "shared")
            .count(),
        1
    );
    let expected = "---\ndescription: Second version\n---\n# Actual changed content";
    std::fs::write(external.join("SKILL.md"), expected).unwrap();
    let mut refreshed = fixture.discover(&roots).get("shared").unwrap().clone();
    assert_eq!(refreshed.load_content().unwrap(), expected);
    assert_eq!(
        std::fs::read_to_string(external.join("SKILL.md")).unwrap(),
        expected
    );
}

#[tokio::test]
async fn shared_runtime_loader_loads_discovered_external_skill_and_references() {
    let fixture = Fixture::new();
    let root = fixture.skill(
        "home/.claude/skills/external-only",
        "# Actual external instructions",
    );
    std::fs::create_dir(root.join("references")).unwrap();
    std::fs::write(root.join("references/steps.md"), "actual reference steps").unwrap();
    let tool = LoadSkillTool::with_discovery(std::sync::Arc::new(parking_lot::RwLock::new(
        fixture.discover(&[]),
    )));
    let full = tool
        .execute(serde_json::json!({"name":"external-only"}))
        .await;
    assert!(full.ok);
    assert!(full.content.contains("Actual external instructions"));
    let emitted_root = PathBuf::from(
        full.content
            .rsplit_once("技能根目录: ")
            .expect("full skill output must identify its actual root")
            .1
            .trim(),
    );
    // Path construction and read_dir may render different slash separators on
    // Windows. Verify the directory identity and real relative file access.
    assert_eq!(
        emitted_root.canonicalize().unwrap(),
        root.canonicalize().unwrap()
    );
    assert_eq!(
        std::fs::read_to_string(emitted_root.join("SKILL.md")).unwrap(),
        "# Actual external instructions"
    );
    let reference = tool
        .execute(serde_json::json!({"name":"external-only","part":"steps"}))
        .await;
    assert!(reference.ok);
    assert!(reference.content.contains("actual reference steps"));
    let reference_root = PathBuf::from(
        reference
            .content
            .rsplit_once("技能根目录: ")
            .expect("reference output must preserve its original skill root")
            .1
            .trim(),
    );
    assert_eq!(
        reference_root.canonicalize().unwrap(),
        root.canonicalize().unwrap()
    );
    assert_eq!(
        std::fs::read_to_string(reference_root.join("references/steps.md")).unwrap(),
        "actual reference steps"
    );
    assert!(
        !tool
            .execute(serde_json::json!({"name":"../not-discovered"}))
            .await
            .ok
    );
}

#[tokio::test]
async fn discovered_codex_procedure_loads_with_native_mapping_and_uses_real_file_tools() {
    let fixture = Fixture::new();
    let source=fixture.skill("home/.codex/skills/portable-file-procedure","# File procedure\nUse `Write` to create the requested text, `Read` to verify it, and `functions.exec_command` for a single build command.");
    let original = std::fs::read_to_string(source.join("SKILL.md")).unwrap();
    let discovery = fixture.discover(&[]);
    assert_eq!(
        discovery
            .get("portable-file-procedure")
            .unwrap()
            .compatibility
            .status,
        "adapted"
    );
    let loader =
        LoadSkillTool::with_discovery(std::sync::Arc::new(parking_lot::RwLock::new(discovery)));
    let loaded = loader
        .execute(serde_json::json!({"name":"portable-file-procedure"}))
        .await;
    assert!(loaded.ok);
    assert!(loaded.content.contains("write_file"));
    assert!(loaded.content.contains("read_file"));
    assert_eq!(
        loaded.structured.as_ref().unwrap()["compatibility"]["runtime_ready"],
        true
    );
    let registry = crate::tools::ToolRegistry::with_defaults(fixture.0.to_str().unwrap());
    let written=registry.get("write_file").unwrap().execute(serde_json::json!({"path":"actual-skill-output.txt","content":"created by a real YiLian file tool"})).await;
    assert!(written.ok);
    let read = registry
        .get("read_file")
        .unwrap()
        .execute(serde_json::json!({"path":"actual-skill-output.txt"}))
        .await;
    assert!(read.ok);
    assert!(read.content.contains("created by a real YiLian file tool"));
    assert_eq!(
        std::fs::read_to_string(source.join("SKILL.md")).unwrap(),
        original
    );
}

#[tokio::test]
async fn unsupported_host_skill_is_browseable_but_not_advertised_or_loaded_for_execution() {
    let fixture = Fixture::new();
    fixture.skill(
        "home/.codex/skills/figma-host",
        "# Host procedure\nCall `use_figma` to modify the file.",
    );
    let mut discovery = fixture.discover(&[]);
    assert!(
        !discovery
            .get("figma-host")
            .unwrap()
            .compatibility
            .runtime_ready
    );
    assert!(!discovery.render_prompt_section().contains("figma-host"));
    let mut actual = discovery.available_tools().clone();
    actual.insert("use_figma".into());
    discovery.set_available_tools(actual);
    assert!(
        discovery
            .get("figma-host")
            .unwrap()
            .compatibility
            .runtime_ready
    );
    actual = discovery.available_tools().clone();
    actual.remove("use_figma");
    discovery.set_available_tools(actual);
    let loader =
        LoadSkillTool::with_discovery(std::sync::Arc::new(parking_lot::RwLock::new(discovery)));
    let result = loader
        .execute(serde_json::json!({"name":"figma-host"}))
        .await;
    assert!(!result.ok);
    assert!(result.content.contains("use_figma"));
    assert_eq!(
        result.structured.unwrap()["compatibility"]["status"],
        "external_host"
    );
}
