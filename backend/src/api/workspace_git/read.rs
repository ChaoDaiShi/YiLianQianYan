use std::{
    fs,
    io::Read,
    path::Path,
    process::{Command, Output, Stdio},
};

use super::{GitFileChange, WorkspaceGitDiff, WorkspaceGitStatus};

const CONTENT_LIMIT: usize = 1024 * 1024;

fn command(root: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .args([
            "--no-optional-locks",
            "--literal-pathspecs",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.quotepath=false",
            "-C",
        ])
        .arg(root)
        .env("LC_ALL", "C")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
}

fn output(root: &Path, args: &[&str]) -> Result<Output, String> {
    command(root)
        .args(args)
        .output()
        .map_err(|e| format!("无法运行 Git：{e}"))
}
fn failure(output: &Output) -> String {
    format!(
        "Git 读取失败：{}",
        String::from_utf8_lossy(&output.stderr).trim()
    )
}
fn text(root: &Path, args: &[&str]) -> Result<String, String> {
    let result = output(root, args)?;
    if !result.status.success() {
        return Err(failure(&result));
    }
    String::from_utf8(result.stdout)
        .map(|text| text.trim_end_matches(['\r', '\n']).to_string())
        .map_err(|_| "Git 输出不是 UTF-8 文本".into())
}

pub(super) fn status(root: &Path) -> Result<WorkspaceGitStatus, String> {
    let repository = output(root, &["rev-parse", "--show-toplevel"])?;
    if !repository.status.success() {
        if String::from_utf8_lossy(&repository.stderr).contains("not a git repository") {
            return Ok(WorkspaceGitStatus {
                is_repository: false,
                root: root.to_string_lossy().into_owned(),
                branch: None,
                head: None,
                ahead: None,
                behind: None,
                files: Vec::new(),
            });
        }
        return Err(failure(&repository));
    }
    let root = String::from_utf8(repository.stdout).map_err(|_| "Git 仓库路径不是 UTF-8 文本")?;
    let root = Path::new(root.trim_end_matches(['\r', '\n']));
    let branch_result = output(root, &["symbolic-ref", "--quiet", "--short", "HEAD"])?;
    let branch = if branch_result.status.success() {
        Some(
            String::from_utf8(branch_result.stdout)
                .map_err(|_| "Git 分支名不是 UTF-8 文本")?
                .trim()
                .to_string(),
        )
    } else if branch_result.status.code() == Some(1) {
        None
    } else {
        return Err(failure(&branch_result));
    };
    let head_result = output(root, &["rev-parse", "--verify", "--quiet", "HEAD"])?;
    let head = if head_result.status.success() {
        Some(
            String::from_utf8(head_result.stdout)
                .map_err(|_| "Git HEAD 不是 UTF-8 文本")?
                .trim()
                .to_string(),
        )
    } else if head_result.status.code() == Some(1) {
        None
    } else {
        return Err(failure(&head_result));
    };
    let changes = output(
        root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    if !changes.status.success() {
        return Err(failure(&changes));
    }
    let files = parse_status(&changes.stdout)?;
    let (ahead, behind) = if let (Some(branch), Some(_)) = (&branch, &head) {
        let upstream = text(
            root,
            &[
                "for-each-ref",
                "--format=%(upstream)",
                &format!("refs/heads/{branch}"),
            ],
        )?;
        if upstream.is_empty() {
            (None, None)
        } else {
            let counts = text(
                root,
                &[
                    "rev-list",
                    "--left-right",
                    "--count",
                    &format!("HEAD...{upstream}"),
                ],
            )?;
            let mut fields = counts.split_whitespace();
            let ahead = fields
                .next()
                .ok_or("Git 未返回领先提交数")?
                .parse::<u64>()
                .map_err(|e| e.to_string())?;
            let behind = fields
                .next()
                .ok_or("Git 未返回落后提交数")?
                .parse::<u64>()
                .map_err(|e| e.to_string())?;
            (Some(ahead), Some(behind))
        }
    } else {
        (None, None)
    };
    Ok(WorkspaceGitStatus {
        is_repository: true,
        root: root.to_string_lossy().into_owned(),
        branch,
        head,
        ahead,
        behind,
        files,
    })
}

fn parse_status(bytes: &[u8]) -> Result<Vec<GitFileChange>, String> {
    let mut records = bytes
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty());
    let mut files = Vec::new();
    while let Some(record) = records.next() {
        if record.len() < 4 || record[2] != b' ' {
            return Err("Git 文件状态格式无效".into());
        }
        let index = record[0] as char;
        let working = record[1] as char;
        let path = std::str::from_utf8(&record[3..])
            .map_err(|_| "Git 文件名不是 UTF-8 文本")?
            .to_string();
        let original_path = if matches!(index, 'R' | 'C') || matches!(working, 'R' | 'C') {
            Some(
                std::str::from_utf8(records.next().ok_or("Git 重命名缺少原文件名")?)
                    .map_err(|_| "Git 原文件名不是 UTF-8 文本")?
                    .to_string(),
            )
        } else {
            None
        };
        let untracked = index == '?' && working == '?';
        files.push(GitFileChange {
            path,
            original_path,
            index_status: index.to_string(),
            worktree_status: working.to_string(),
            staged: !untracked && index != ' ',
            unstaged: !untracked && working != ' ',
            untracked,
        });
    }
    Ok(files)
}

pub(super) fn diff(root: &Path, path: &str, staged: bool) -> Result<WorkspaceGitDiff, String> {
    let repository = status(root)?;
    if !repository.is_repository {
        return Err("工作空间不是 Git 仓库".into());
    }
    // Only exact paths from Git's current change list can be inspected. This also
    // prevents caller-provided absolute paths, traversal, and pathspec patterns.
    let file = repository
        .files
        .iter()
        .find(|file| file.path == path)
        .ok_or("该文件不在当前 Git 变更列表中，请刷新后重试")?;
    if (staged && !file.staged) || (!staged && !file.unstaged && !file.untracked) {
        return Err("该文件在所选暂存区或工作区中没有变更".into());
    }
    let root = Path::new(&repository.root);
    let (bytes, truncated) = if file.untracked {
        let source = root.join(&file.path);
        let bytes = if fs::symlink_metadata(&source)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            fs::read_link(source)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .as_bytes()
                .to_vec()
        } else {
            let mut bytes = Vec::new();
            fs::File::open(source)
                .map_err(|e| e.to_string())?
                .take((CONTENT_LIMIT + 1) as u64)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            bytes
        };
        let truncated = bytes.len() > CONTENT_LIMIT;
        (bytes, truncated)
    } else {
        let mut command = command(root);
        command.args([
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--find-renames",
        ]);
        if staged {
            command.arg("--cached");
        }
        command.arg("--");
        if let Some(original) = &file.original_path {
            command.arg(original);
        }
        command.arg(&file.path);
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        let mut stdout = child.stdout.take().ok_or("Git 未提供差异输出")?;
        stdout
            .by_ref()
            .take((CONTENT_LIMIT + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        let truncated = bytes.len() > CONTENT_LIMIT;
        if truncated {
            // The response limit is explicit. Stop producing a patch once the
            // visible preview is full rather than allocating an entire huge diff.
            if let Err(error) = child.kill() {
                // A short-lived Git process may already have exited after
                // writing the preview. Its real exit status is checked below.
                if child.try_wait().map_err(|e| e.to_string())?.is_none() {
                    return Err(error.to_string());
                }
            }
        }
        drop(stdout);
        let result = child.wait_with_output().map_err(|e| e.to_string())?;
        if !truncated && !result.status.success() {
            return Err(failure(&result));
        }
        (bytes, truncated)
    };
    let (content, binary) = decode_preview(bytes, truncated);
    let binary = binary
        || (!file.untracked
            && content
                .lines()
                .any(|line| line.starts_with("Binary files ") && line.ends_with(" differ")));
    Ok(WorkspaceGitDiff {
        path: file.path.clone(),
        staged,
        untracked: file.untracked,
        content,
        binary,
        truncated,
    })
}

fn decode_preview(mut bytes: Vec<u8>, truncated: bool) -> (String, bool) {
    bytes.truncate(CONTENT_LIMIT);
    if bytes.contains(&0) {
        return (String::new(), true);
    }
    match String::from_utf8(bytes) {
        Ok(text) => (text, false),
        Err(error) if truncated && error.utf8_error().error_len().is_none() => {
            let valid = error.utf8_error().valid_up_to();
            let mut bytes = error.into_bytes();
            bytes.truncate(valid);
            (
                String::from_utf8(bytes).expect("validated UTF-8 prefix"),
                false,
            )
        }
        Err(_) => (String::new(), true),
    }
}
