# Channel CI line-budget repair

> Execute in this session with `executing-plans`; no subagents. Preserve the user's uncommitted project skill and artifacts.

**Goal:** Restore GitHub Actions after the channel implementation exceeded the existing 600-line Rust file budget.

**Architecture:** Move existing code into private child modules by responsibility. Preserve public routes, tool names, protocol payloads, authorization, cancellation and delivery semantics. Keep the architecture check and its existing allowlist unchanged.

**Tech Stack:** Rust, Axum, Tokio; existing Windows GitHub Actions workflow.

## Evidence and constraints

- Failing run: `40067865415`, job `120794796108`, commit `b406a50a9a08ff570980335647af26de73228aaf`.
- `cargo test -p yilian-backend --test architecture_boundaries no_new_files_above_the_line_budget -- --exact` reproduces all five reported file violations locally.
- No dependency, schema, frontend, workflow or runtime behavior changes.
- Work on `fix/channel-ci-line-budget`; keep unrelated files out of the commit.
- Local Windows TEMP is under a Git worktree rooted at the user directory. For the existing non-repository fixture, use the existing `YILIAN_GIT_TEST_NONREPO_ROOT` setting with a verified external directory under `F:/临时测试/`. No test assertion or production code change is required for this environment issue.

## Task 1: Split existing channel code

- [x] Move `validate_input` and `validate_save` from `backend/src/api/channels.rs` to `backend/src/api/channels/validation.rs`; expose only `validate_save` to the parent.
- [x] Move `FragmentAssembler`, `PendingFragments` and `FRAGMENT_LIFETIME` to `backend/src/modules/channel/feishu_ws/fragments.rs`; expose only the assembler and its `push` method to the parent.
- [x] Move the inline `onebot_action_tests` module body unchanged to `backend/src/modules/channel/protocol/onebot_action_tests.rs` and retain its original module name.
- [x] Keep the shared robot test fixtures in `backend/src/modules/channel/robot/tests.rs`. Move its four persistence/session tests to `tests/persistence.rs`, and its five asynchronous send/execution tests to `tests/execution.rs`.
- [x] Move `QqTool::execute_trusted` to `backend/src/modules/channel/robot/tools/execution.rs`, retaining the same implementation with parent-only visibility.
- [x] Move inline QQ tool tests to `backend/src/modules/channel/robot/tools/tests.rs`, retaining their module name and assertions.
- [x] Use private `mod` declarations and imports, with no crate-root facade or new public API.

## Task 2: Verify the refactor

- [x] Run `cargo fmt --all` and `cargo fmt --all --check`.
- [x] Run `cargo check --workspace`.
- [x] Run `cargo test -p yilian-backend --test architecture_boundaries`; expect all five checks to pass.
- [x] Run `cargo test -p yilian-backend --lib modules::channel`; expect all existing channel tests to pass.
- [x] Inspect moved method bodies and test attributes against the original revision; no assertions or production statements may be removed.
- [x] Run `cargo test --workspace --all-targets`, matching the failed CI step.
- [x] Run `git diff --check` and inspect the staged file list before committing.

## Task 3: Publish and verify GitHub Actions

- [ ] Commit the scoped repair using Conventional Commits.
- [ ] Fast-forward `develop` and push it, following the user's earlier explicit instruction to merge this delivery into `develop` and push to the remote.
- [ ] Verify the actual run/job results for the pushed commit. Local test success alone does not count as remote CI success.
- [ ] Report the commit and actual CI state. Preserve the feature branch and unrelated untracked files.
