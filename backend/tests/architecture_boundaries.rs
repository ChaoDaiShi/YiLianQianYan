//! Lightweight architecture boundary checks (mandate R7).
//!
//! These are source-text checks, not a type system. They exist to catch the
//! regressions a reviewer would otherwise have to remember: a domain layer
//! reaching for a transport, v1 code reaching into a v2 namespace, a file
//! quietly crossing the line budget, or a compatibility shim growing back.
//!
//! Deliberately no AST crate: `syn` would be a large dependency for checks that
//! only need to read text, and these run in the same build as the crate they
//! police. The cost is that they can be fooled by a string literal — the
//! `no_removed_facade_paths` check in particular would false-positive on a
//! literal containing `crate::task::`. No such literal exists; if one is ever
//! needed, exempt it by path rather than loosening the check.
//!
//! See `docs/architecture/dependency-rules.md` and
//! `docs/architecture/compatibility-facades.md`.

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
    "src/integrations/mcp/runtime/manager.rs",
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

/// The twelve crate-root compatibility shims R2 removed. They are *internal
/// implementation* by `docs/architecture/public-api-policy.md`: re-adding one
/// would create a second name for an item another module already owns, and the
/// two would drift. The `pub mod` half of this list matters most — a shim can be
/// re-added as a module declaration before any call site reaches for it.
const REMOVED_FACADES: &[&str] = &[
    "capability",
    "llm",
    "mcp",
    "mcp_runtime",
    "memory_skill",
    "resource_input",
    "secret",
    "server",
    "skill_management",
    "task",
    "voice",
    "workflow",
];

#[test]
fn no_removed_facade_paths() {
    let mut violations = Vec::new();

    for (path, source) in rust_sources() {
        for facade in REMOVED_FACADES {
            // Anchored by the leading `crate::` / `yilian_backend::` so a nested
            // path such as `crate::modules::task::` or `crate::integrations::mcp::`
            // cannot match.
            for prefix in ["crate::", "yilian_backend::"] {
                let needle = format!("{prefix}{facade}::");
                if let Some(line) = source.lines().position(|l| l.contains(&needle)) {
                    violations.push(format!("{path}:{} mentions `{needle}`", line + 1));
                }
            }
        }
    }

    // A shim is a module declaration plus a file; catch the declaration in
    // `lib.rs`, because a dangling `pub mod task;` with no call sites yet is
    // exactly how the twelve came back the first time.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lib = fs::read_to_string(manifest.join("src/lib.rs")).unwrap_or_default();
    for facade in REMOVED_FACADES {
        let declaration = format!("pub mod {facade};");
        if lib.lines().any(|l| l.trim() == declaration) {
            violations.push(format!("src/lib.rs declares `{declaration}`"));
        }
    }

    assert!(
        violations.is_empty(),
        "a removed compatibility facade is referenced again. The owning module is \
         listed in docs/architecture/compatibility-facades.md — point the call site \
         there instead of restoring the shim:\n{}",
        violations.join("\n")
    );
}

/// Cross-module glob re-exports that are deliberate.
///
/// The house pattern is a module's `mod.rs` re-exporting its own children
/// (`pub use model::*;`) — that is a module publishing its surface, not a
/// boundary crossing, and it is not what this check reads.
///
/// What it reads is a glob whose path is *absolute* (`pub use crate::…::*`),
/// which reaches into another module's internals. Those are allowed only when
/// the path is listed here, and every entry needs a reason.
const ALLOWED_CROSS_MODULE_GLOBS: &[&str] = &[
    // Compatibility facades for the pre-R1 `crate::api::task_world` and
    // `crate::api::workflow_runtime` paths, consumed by `app/router.rs`. Both
    // re-export another module's own `api` surface, which that module designs
    // as public — so this is a name-preserving move, not an internals grab.
    "crate::modules::task::api::*",
    "crate::modules::workflow::api::*",
];

#[test]
fn cross_module_globs_are_allowlisted() {
    let mut violations = Vec::new();

    for (path, source) in rust_sources() {
        for line in source.lines() {
            let trimmed = line.trim();
            let Some(rest) = trimmed.strip_prefix("pub use ") else {
                continue;
            };
            // The line is `pub use crate::…::*;` — strip the terminator first,
            // then the glob. Stripping `*` first never matches, because the `;`
            // is in the way.
            let rest = rest.trim_end_matches(';').trim();
            let Some(target) = rest.strip_suffix("::*") else {
                continue;
            };
            let target = target.trim();
            let absolute = target.starts_with("crate::") || target.starts_with("yilian_backend::");
            if !absolute {
                continue;
            }
            let glob = format!("{target}::*");
            if !ALLOWED_CROSS_MODULE_GLOBS.contains(&glob.as_str()) {
                violations.push(format!("{path}: `pub use {glob};`"));
            }
        }
    }

    // `lib.rs` is the crate's documented surface: it re-exports by name, never
    // by glob, so `public-api-policy.md` layer B stays explicit.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lib = fs::read_to_string(manifest.join("src/lib.rs")).unwrap_or_default();
    for line in lib.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("pub use ") && trimmed.ends_with("::*;") {
            violations.push(format!("src/lib.rs: `{trimmed}`"));
        }
    }

    assert!(
        violations.is_empty(),
        "cross-module glob re-export, which re-exports another module's internals. \
         Either re-export the named items, or add the path to \
         ALLOWED_CROSS_MODULE_GLOBS with a reason:\n{}",
        violations.join("\n")
    );
}
