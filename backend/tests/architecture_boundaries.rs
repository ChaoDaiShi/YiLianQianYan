//! Lightweight architecture boundary checks (mandate R7).
//!
//! These are source-text checks, not a type system. They exist to catch the
//! regressions a reviewer would otherwise have to remember: a domain layer
//! reaching for a transport, v1 code reaching into a v2 namespace, or a new
//! file quietly crossing the line budget.
//!
//! See `docs/architecture/dependency-rules.md`.

use std::fs;
use std::path::{Path, PathBuf};

/// Files already above the 600-line budget when these checks were introduced.
/// They are recorded debt, not approval — a file may leave this list by being
/// split, and nothing may join it.
const LINE_BUDGET: usize = 600;
const KNOWN_LARGE_FILES: &[&str] = &[
    "src/agent/engine.rs",
    "src/api/approvals.rs",
    "src/api/chat.rs",
    "src/api/memories.rs",
    "src/api/voice.rs",
    "src/app/state.rs",
    "src/config/types.rs",
    "src/db/conversations.rs",
    "src/db/memories.rs",
    "src/db/migrations.rs",
    "src/db/mod.rs",
    "src/db/task.rs",
    "src/db/task_world.rs",
    "src/db/workflow_runtime.rs",
    "src/integrations/llm/client.rs",
    "src/integrations/mcp/legacy_stdio.rs",
    "src/integrations/mcp/manager.rs",
    "src/isolation/windows.rs",
    "src/modules/settings/tests.rs",
    "src/modules/task/execution.rs",
    "src/modules/task/harness.rs",
    "src/modules/task/model.rs",
    "src/modules/task/orchestrator.rs",
    "src/modules/task/planner.rs",
    "src/modules/task/projection.rs",
    "src/modules/task/runtime.rs",
    "src/modules/task/task_supervisor.rs",
    "src/modules/voice/provider.rs",
    "src/modules/voice/runtime.rs",
    "src/modules/workflow/tests.rs",
    "src/safety/descriptor.rs",
    "src/safety/execution_gateway.rs",
    "src/safety/sandbox.rs",
    "src/shared/voice.rs",
    "src/tools/gui_launch.rs",
    "src/tools/input.rs",
    "src/tools/mcp.rs",
    "src/tools/subagent.rs",
];

fn collect(directory: &Path, collected: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, collected);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            collected.push(path);
        }
    }
}

/// Every Rust source under `src/`, as `(repo-relative path, contents)`.
fn rust_sources() -> Vec<(String, String)> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut paths = Vec::new();
    collect(&manifest.join("src"), &mut paths);
    let mut sources: Vec<(String, String)> = paths
        .into_iter()
        .map(|path| {
            let relative = path
                .strip_prefix(manifest)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            (relative, fs::read_to_string(&path).unwrap_or_default())
        })
        .collect();
    sources.sort();
    sources
}

/// `modules/<name>/domain/...` and `modules/<name>/domain.rs`.
fn is_domain_layer(path: &str) -> bool {
    path.starts_with("src/modules/") && (path.contains("/domain/") || path.ends_with("/domain.rs"))
}

#[test]
fn domain_layers_do_not_import_transport() {
    let forbidden = ["axum", "crate::api", "crate::app", "tauri"];
    let mut violations = Vec::new();
    for (path, source) in rust_sources() {
        if !is_domain_layer(&path) {
            continue;
        }
        for needle in forbidden {
            if source.contains(needle) {
                violations.push(format!("{path} mentions `{needle}`"));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "a domain layer reached for transport, which the dependency rules forbid:\n{}",
        violations.join("\n")
    );
}

#[test]
fn v1_does_not_reach_into_v2() {
    // v2 lives on its own branch. These namespaces must never appear in v1
    // source, so a future merge cannot silently couple the two lines.
    let forbidden = ["modules::v2", "crate::v2", "V2TaskWorld", "v2_namespace"];
    let mut violations = Vec::new();
    for (path, source) in rust_sources() {
        for needle in forbidden {
            if source.contains(needle) {
                violations.push(format!("{path} mentions `{needle}`"));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "v1 source referenced a v2 namespace:\n{}",
        violations.join("\n")
    );
}

#[test]
fn no_new_files_above_the_line_budget() {
    let mut added = Vec::new();
    for (path, source) in rust_sources() {
        let lines = source.lines().count();
        if lines > LINE_BUDGET && !KNOWN_LARGE_FILES.contains(&path.as_str()) {
            added.push(format!("{path} ({lines} lines)"));
        }
    }
    assert!(
        added.is_empty(),
        "new files above the {LINE_BUDGET}-line budget — split them, or record them \
         deliberately and say why:\n{}",
        added.join("\n")
    );
}
