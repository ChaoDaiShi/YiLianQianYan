use super::*;
use crate::tools::{
    search::{GlobTool, GrepTool},
    Tool,
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("yilian-scoped-search-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn root(&self) -> &str {
        self.0.to_str().unwrap()
    }
    fn write(&self, path: &str, content: impl AsRef<[u8]>) {
        let path = self.0.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn bounded_search_finds_canonical_files_and_lines_with_complete_metadata() {
    let f = Fixture::new();
    f.write("notes/alpha.md", "project alpha\nnext line");
    f.write("notes/beta.txt", "project beta");
    let glob = GlobTool::new_scoped(f.root())
        .execute(json!({"pattern":"**/*.md"}))
        .await;
    assert!(glob.ok);
    assert_eq!(glob.structured.as_ref().unwrap()["matches"], 1);
    assert!(glob.content.contains(
        std::fs::canonicalize(f.0.join("notes/alpha.md"))
            .unwrap()
            .to_str()
            .unwrap()
    ));
    let grep = GrepTool::new_scoped(f.root())
        .execute(json!({"pattern":"project", "glob_filter":"**/*.md"}))
        .await;
    assert!(grep.ok);
    assert!(grep.content.contains("alpha.md:1: project alpha"));
    assert_eq!(grep.structured.unwrap()["complete"], true);
}

#[tokio::test]
async fn bounded_search_rejects_escape_patterns_and_paths() {
    let f = Fixture::new();
    let outside = Fixture::new();
    for pattern in ["../*.txt", "/outside/*", "C:/outside/*", "a/../../*"] {
        assert!(
            !GlobTool::new_scoped(f.root())
                .execute(json!({"pattern":pattern}))
                .await
                .ok
        );
    }
    for path in [
        outside.root().to_owned(),
        f.0.join("..").to_string_lossy().into_owned(),
    ] {
        assert!(
            !GlobTool::new_scoped(f.root())
                .execute(json!({"pattern":"**/*", "path":path}))
                .await
                .ok
        );
    }
    assert!(
        !GrepTool::new_scoped(f.root())
            .execute(json!({"pattern":"a", "glob_filter":"../*"}))
            .await
            .ok
    );
    assert!(
        !GrepTool::new_scoped(f.root())
            .execute(json!({"pattern":"a".repeat(1025)}))
            .await
            .ok
    );
}

#[test]
fn bounded_search_rejects_outside_before_metadata_lookup() {
    let f = Fixture::new();
    let root = f.0.canonicalize().unwrap();
    let outside =
        f.0.with_file_name(format!("missing-outside-{}", uuid::Uuid::new_v4()));
    assert_eq!(
        checked_path(&root, &outside).unwrap_err(),
        "查找路径超出委托目录"
    );
}

#[cfg(windows)]
#[test]
fn bounded_search_lexical_windows_scope_accepts_verbatim_and_ascii_case() {
    let root = Path::new(r"\\?\C:\Users\Example\Scope");
    assert!(lexical_within_root(
        root,
        Path::new(r"c:\users\EXAMPLE\scope\notes.txt")
    ));
    assert!(lexical_within_root(
        root,
        Path::new(r"\\?\c:\users\example\SCOPE")
    ));
    assert!(!lexical_within_root(
        root,
        Path::new(r"C:\Users\Example\Scope-other\notes.txt")
    ));
    assert!(!lexical_within_root(
        root,
        Path::new(r"D:\Users\Example\Scope\notes.txt")
    ));
    assert!(!lexical_within_root(
        root,
        Path::new(r"\\server\share\Scope\notes.txt")
    ));
    assert!(!lexical_within_root(
        root,
        Path::new(r"C:root\Users\Example\Scope\notes.txt")
    ));
}

#[tokio::test]
async fn bounded_search_reports_nontext_large_files_and_excluded_directories() {
    let f = Fixture::new();
    f.write("huge.txt", vec![b'a'; MAX_FILE as usize + 1]);
    f.write("binary.txt", b"alpha\0data");
    f.write("invalid.txt", [0xff, 0xfe]);
    f.write("node_modules/private.txt", "alpha");
    f.write(".secrets/hidden.txt", "alpha");
    let result = GrepTool::new_scoped(f.root())
        .execute(json!({"pattern":"alpha"}))
        .await;
    assert!(result.ok);
    let meta = result.structured.unwrap();
    assert_eq!(meta["complete"], false);
    assert_eq!(meta["oversized"], 1);
    assert_eq!(meta["non_text"], 2);
    assert_eq!(meta["excluded"], 2);
    assert!(result.content.contains("不完整"));
    assert!(!result.content.contains("private.txt"));
    assert!(!result.content.contains("hidden.txt"));
}

#[tokio::test]
async fn bounded_search_limits_matches_output_and_total_bytes() {
    let f = Fixture::new();
    f.write("many.txt", "hit\n".repeat(110));
    let result = GrepTool::new_scoped(f.root())
        .execute(json!({"pattern":"hit"}))
        .await;
    assert_eq!(result.structured.as_ref().unwrap()["matches"], 100);
    assert_eq!(result.structured.unwrap()["complete"], false);
    assert!(result.content.len() <= MAX_OUTPUT);
    let f = Fixture::new();
    for i in 0..10 {
        f.write(&format!("{i}.txt"), vec![b'a'; MAX_FILE as usize]);
    }
    let result = GrepTool::new_scoped(f.root())
        .execute(json!({"pattern":"never matches"}))
        .await;
    assert!(
        result.structured.as_ref().unwrap()["read_bytes"]
            .as_u64()
            .unwrap()
            <= MAX_BYTES
    );
    assert_eq!(result.structured.unwrap()["complete"], false);
}

#[tokio::test]
async fn bounded_search_limits_directory_depth_and_enumeration() {
    let f = Fixture::new();
    let deep = "nested/".repeat(MAX_DEPTH + 2);
    f.write(&format!("{deep}file.txt"), "data");
    let result = GlobTool::new_scoped(f.root())
        .execute(json!({"pattern":"**/*"}))
        .await;
    assert_eq!(result.structured.unwrap()["complete"], false);
    assert!(result.content.contains("目录深度上限"));
    let f = Fixture::new();
    for i in 0..MAX_ENTRIES + 2 {
        f.write(&format!("{i:04}.txt"), "data");
    }
    let result = GlobTool::new_scoped(f.root())
        .execute(json!({"pattern":"*.missing"}))
        .await;
    assert_eq!(result.structured.as_ref().unwrap()["entries"], MAX_ENTRIES);
    assert_eq!(result.structured.unwrap()["complete"], false);
}

#[tokio::test]
async fn bounded_search_yields_so_runtime_cancellation_drops_all_work() {
    let f = Fixture::new();
    f.write("one.txt", "data");
    let tool = GlobTool::new_scoped(f.root());
    let mut future = Box::pin(tool.execute(json!({"pattern":"**/*"})));
    assert!(futures::poll!(future.as_mut()).is_pending());
    drop(future); // No detached task or reader remains after the gateway cancels.
    assert!(std::fs::remove_file(f.0.join("one.txt")).is_ok());
}

#[cfg(unix)]
#[tokio::test]
async fn bounded_search_never_follows_a_link_outside_scope() {
    let f = Fixture::new();
    let outside = Fixture::new();
    outside.write("secret.txt", "private");
    std::os::unix::fs::symlink(&outside.0, f.0.join("linked")).unwrap();
    let result = GlobTool::new_scoped(f.root())
        .execute(json!({"pattern":"**/*"}))
        .await;
    assert_eq!(result.structured.as_ref().unwrap()["unsafe_paths"], 1);
    assert_eq!(result.structured.unwrap()["complete"], false);
    assert!(!result.content.contains("secret.txt"));
}
