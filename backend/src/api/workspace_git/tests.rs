use super::read::{diff, status};
use std::{fs, path::PathBuf, process::Command};

struct Fixture(PathBuf);
#[test]
fn git_root_requires_explicit_workspace_and_never_uses_application_directory() {
    let application = Fixture::new();
    let chosen = Fixture::new();
    let server = crate::app::state::AppServer::new_with_control_session(
        &application.0.join("workspace-test.db"),
        application.0.to_str().unwrap(),
        crate::safety::ControlSession::generate(),
    )
    .unwrap();
    assert_eq!(
        super::workspace_root(&server, None).unwrap_err().0,
        axum::http::StatusCode::BAD_REQUEST
    );
    assert_eq!(
        super::workspace_root(&server, Some(String::new()))
            .unwrap_err()
            .0,
        axum::http::StatusCode::BAD_REQUEST
    );
    let workspace = crate::workspace::Workspace::new(
        crate::workspace::WorkspaceId::generate(),
        "User-selected project".into(),
        String::new(),
        Some(chosen.0.to_string_lossy().into_owned()),
        1,
    )
    .unwrap();
    server.db.create_workspace(&workspace).unwrap();
    assert_eq!(
        super::workspace_root(&server, Some(workspace.id.to_string())).unwrap(),
        chosen.0
    );
    let relative = crate::workspace::Workspace::new(
        crate::workspace::WorkspaceId::generate(),
        "Ambiguous relative path".into(),
        String::new(),
        Some(".".into()),
        1,
    )
    .unwrap();
    server.db.create_workspace(&relative).unwrap();
    assert_eq!(
        super::workspace_root(&server, Some(relative.id.to_string()))
            .unwrap_err()
            .0,
        axum::http::StatusCode::BAD_REQUEST
    );
    drop(server);
}
impl Fixture {
    fn non_repository() -> Self {
        let parent = std::env::var_os("YILIAN_GIT_TEST_NONREPO_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let root = parent.join(format!("yilian-nonrepo-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let fixture = Self(root);
        // TEMP can be configured beneath a checkout. Check the fixture boundary
        // before status would scan that unrelated ancestor repository.
        let discovery = Command::new("git")
            .arg("-C")
            .arg(&fixture.0)
            .args(["rev-parse", "--show-toplevel"])
            .env("LC_ALL", "C")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .output()
            .unwrap();
        assert!(
            !discovery.status.success()
                && String::from_utf8_lossy(&discovery.stderr).contains("not a git repository"),
            "Non-repository fixture {} must be outside every Git worktree. Set \
             YILIAN_GIT_TEST_NONREPO_ROOT to an external temporary directory. Git stdout: {}; stderr: {}",
            fixture.0.display(),
            String::from_utf8_lossy(&discovery.stdout).trim(),
            String::from_utf8_lossy(&discovery.stderr).trim(),
        );
        fixture
    }

    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("yilian-git-inspection-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let fixture = Self(root);
        fixture.git(&["init", "--initial-branch=fixture"]);
        fixture
    }
    fn git(&self, args: &[&str]) {
        let result = Command::new("git")
            .arg("-C")
            .arg(&self.0)
            .args([
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.autocrlf=false",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    fn write(&self, name: &str, content: &str) {
        fs::write(self.0.join(name), content).unwrap();
    }
    fn seed(&self) {
        self.write("修改文件.txt", "original\n");
        self.write("old name.txt", "rename content\n");
        self.git(&["add", "."]);
        self.git(&["commit", "-m", "seed"]);
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn reads_real_staged_unstaged_rename_and_unicode_untracked_content() {
    let fixture = Fixture::new();
    fixture.seed();
    fixture.write("修改文件.txt", "staged text\n");
    fixture.git(&["add", "修改文件.txt"]);
    fixture.write("修改文件.txt", "working text\n");
    fixture.git(&["mv", "old name.txt", "新名称.txt"]);
    fixture.write("未跟踪 文件.txt", "untracked real content\n");
    let index_before = fs::read(fixture.0.join(".git/index")).unwrap();
    let result = status(&fixture.0).unwrap();
    assert!(result.is_repository);
    assert_eq!(result.branch.as_deref(), Some("fixture"));
    assert_eq!(result.head.as_ref().unwrap().len(), 40);
    assert_eq!(result.ahead, None);
    let modified = result
        .files
        .iter()
        .find(|f| f.path == "修改文件.txt")
        .unwrap();
    assert!(modified.staged && modified.unstaged);
    assert!(!modified.untracked);
    let renamed = result
        .files
        .iter()
        .find(|f| f.path == "新名称.txt")
        .unwrap();
    assert_eq!(renamed.original_path.as_deref(), Some("old name.txt"));
    assert_eq!(renamed.index_status, "R");
    let staged = diff(&fixture.0, "修改文件.txt", true).unwrap();
    assert!(staged.content.contains("+staged text"));
    assert!(!staged.content.contains("+working text"));
    let working = diff(&fixture.0, "修改文件.txt", false).unwrap();
    assert!(working.content.contains("+working text"));
    assert!(working.content.contains("-staged text"));
    let renamed_diff = diff(&fixture.0, "新名称.txt", true).unwrap();
    assert!(renamed_diff.content.contains("rename from old name.txt"));
    let untracked = diff(&fixture.0, "未跟踪 文件.txt", false).unwrap();
    assert!(untracked.untracked);
    assert_eq!(untracked.content, "untracked real content\n");
    assert!(diff(&fixture.0, "../outside.txt", false).is_err());
    assert!(diff(&fixture.0, "修改*", false).is_err());
    assert!(diff(&fixture.0, "未跟踪 文件.txt", true).is_err());
    assert_eq!(
        fs::read(fixture.0.join(".git/index")).unwrap(),
        index_before
    );
}

#[test]
fn accepts_unborn_and_non_repository_directories_and_handles_binary_and_large_files() {
    let fixture = Fixture::new();
    let unborn = status(&fixture.0).unwrap();
    assert_eq!(unborn.branch.as_deref(), Some("fixture"));
    assert!(unborn.head.is_none());
    fs::write(fixture.0.join("binary.bin"), [0, 255, 1]).unwrap();
    assert!(diff(&fixture.0, "binary.bin", false).unwrap().binary);
    fixture.write("large.txt", &"甲".repeat(400_000));
    let large = diff(&fixture.0, "large.txt", false).unwrap();
    assert!(large.truncated);
    assert!(!large.binary);
    assert!(!large.content.is_empty());
    assert!(large.content.len() <= 1024 * 1024);
    let plain = fixture.0.join("plain");
    fs::create_dir(&plain).unwrap();
    // A nested directory still belongs to the real parent repository.
    assert!(status(&plain).unwrap().is_repository);
    let outside = Fixture::non_repository();
    assert!(!status(&outside.0).unwrap().is_repository);
}

#[test]
fn reads_actual_upstream_ahead_and_behind_without_fetching_or_writing_configuration() {
    let fixture = Fixture::new();
    fixture.seed();
    fixture.git(&["branch", "upstream-fixture"]);
    fixture.git(&["branch", "--set-upstream-to=upstream-fixture"]);
    fixture.write("修改文件.txt", "second commit\n");
    fixture.git(&["add", "."]);
    fixture.git(&["commit", "-m", "second"]);
    let ahead = status(&fixture.0).unwrap();
    assert_eq!(ahead.ahead, Some(1));
    assert_eq!(ahead.behind, Some(0));
    fixture.git(&["checkout", "upstream-fixture"]);
    fixture.git(&["branch", "--set-upstream-to=fixture"]);
    let behind = status(&fixture.0).unwrap();
    assert_eq!(behind.ahead, Some(0));
    assert_eq!(behind.behind, Some(1));
}

#[test]
fn bounds_actual_tracked_diff_and_reports_detached_head() {
    let fixture = Fixture::new();
    fixture.seed();
    fixture.git(&["checkout", "--detach"]);
    assert!(status(&fixture.0).unwrap().branch.is_none());
    fixture.write("修改文件.txt", &"changed line 甲\n".repeat(100_000));
    let preview = diff(&fixture.0, "修改文件.txt", false).unwrap();
    assert!(preview.truncated);
    assert!(!preview.binary);
    assert!(preview.content.contains("+changed line 甲"));
    assert!(preview.content.len() <= 1024 * 1024);
}
