//! Bounded execution mode for the existing grep/glob tools. No shell, background
//! worker or detached file reader is created. Each entry yields to cancellation.
use crate::tools::ToolResult;
use glob::{MatchOptions, Pattern};
use regex::{Regex, RegexBuilder};
use serde_json::{json, Value};
use std::{
    fs::Metadata,
    io::Read,
    path::{Component, Path, PathBuf},
};

const MAX_DEPTH: usize = 8;
const MAX_ENTRIES: usize = 1000;
const MAX_MATCHES: usize = 100;
const MAX_FILE: u64 = 256 * 1024;
const MAX_BYTES: u64 = 2 * 1024 * 1024;
const MAX_OUTPUT: usize = 32 * 1024;
const RESULT_BYTES: usize = MAX_OUTPUT - 1024;
const OPTIONS: MatchOptions = MatchOptions {
    case_sensitive: true,
    require_literal_separator: true,
    require_literal_leading_dot: true,
};

fn linked(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    metadata.file_type().is_symlink()
}

fn local_path(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    if !path.components().next().is_some_and(|part| {
        matches!(part, Component::Prefix(prefix)
        if matches!(prefix.kind(), std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_)))
    }) {
        return Err("资料查找仅支持本地磁盘，不支持网络共享或设备路径".into());
    }
    if path.components().any(|part| match part {
        Component::ParentDir => true,
        Component::Normal(name) => {
            let name = name.to_string_lossy();
            name.contains(':') || name.ends_with(['.', ' '])
        }
        _ => false,
    }) {
        return Err("查找路径不能包含上级目录、备用数据流或特殊名称".into());
    }
    Ok(())
}

fn lexical_within_root(root: &Path, path: &Path) -> bool {
    crate::safety::local_scope::relative(root, path).is_some()
}

fn checked_path(root: &Path, path: &Path) -> Result<PathBuf, String> {
    local_path(path)?;
    if !lexical_within_root(root, path) {
        return Err("查找路径超出委托目录".into());
    }
    // Inspect the original spelling so canonicalization cannot hide links.
    for ancestor in path.ancestors() {
        let metadata =
            std::fs::symlink_metadata(ancestor).map_err(|e| format!("查找路径无法访问：{e}"))?;
        if linked(&metadata) {
            return Err("资料查找不允许链接或重解析路径".into());
        }
    }
    let real = path
        .canonicalize()
        .map_err(|e| format!("查找路径无法访问：{e}"))?;
    if !crate::safety::is_within_root(root, &real) {
        return Err("查找路径超出委托目录".into());
    }
    Ok(real)
}

fn excluded(path: &Path) -> bool {
    path.components().any(|part| match part {
        Component::Normal(name) => {
            let name = name.to_string_lossy();
            name.starts_with('.')
                || matches!(
                    name.as_ref(),
                    "node_modules" | "target" | "dist" | "build" | "vendor" | "__pycache__"
                )
        }
        _ => false,
    })
}

fn glob_pattern(text: &str) -> Result<Pattern, String> {
    if text.is_empty()
        || text.len() > 512
        || text.starts_with(['/', '\\'])
        || text.contains(':')
        || text.split(['/', '\\']).any(|part| part == "..")
    {
        return Err("glob 模式须为不含上级目录的相对模式，最多 512 字节".into());
    }
    Pattern::new(&text.replace('\\', "/")).map_err(|e| format!("无效的 glob 模式：{e}"))
}

enum Query {
    Glob(Pattern),
    Grep {
        regex: Regex,
        filter: Option<Pattern>,
    },
}

impl Query {
    fn accepts(&self, relative: &Path) -> bool {
        let text = relative.to_string_lossy().replace('\\', "/");
        match self {
            Self::Glob(pattern) => pattern.matches_with(&text, OPTIONS),
            Self::Grep { filter, .. } => filter
                .as_ref()
                .is_none_or(|pattern| pattern.matches_with(&text, OPTIONS)),
        }
    }
}

#[derive(Default)]
struct Results {
    lines: Vec<String>,
    output_bytes: usize,
    entries: usize,
    files: usize,
    read_bytes: u64,
    excluded: usize,
    unsafe_paths: usize,
    unreadable: usize,
    non_text: usize,
    oversized: usize,
    limits: Vec<&'static str>,
}

impl Results {
    fn limit(&mut self, name: &'static str) {
        if !self.limits.contains(&name) {
            self.limits.push(name);
        }
    }
    fn add(&mut self, line: String) -> bool {
        if self.lines.len() >= MAX_MATCHES {
            self.limit("匹配数上限");
            return false;
        }
        if self.output_bytes + line.len() + 1 > RESULT_BYTES {
            self.limit("输出长度上限");
            return false;
        }
        self.output_bytes += line.len() + 1;
        self.lines.push(line);
        true
    }
    fn done(self) -> ToolResult {
        let complete = self.limits.is_empty()
            && self.unsafe_paths == 0
            && self.unreadable == 0
            && self.non_text == 0
            && self.oversized == 0;
        let summary = format!(
            "\n查找完整性：{}。枚举 {} 项，检查 {} 个文件，读取 {} 字节，返回 {} 项匹配。排除隐路径/构建目录 {} 项；跳过链接/不安全路径 {}、不可读 {}、非文本 {}、超大文件 {}。限制：{}。范围排除隐路径及 node_modules/target/dist/build/vendor/__pycache__。{}",
            if complete { "在上述排除规则内完成" } else { "不完整" },
            self.entries, self.files, self.read_bytes, self.lines.len(), self.excluded,
            self.unsafe_paths, self.unreadable, self.non_text, self.oversized,
            if self.limits.is_empty() { "无".into() } else { self.limits.join("、") },
            if complete { "" } else { "不能据此断言所有资料均无匹配；请缩小范围或说明缺口。" }
        );
        let content = format!(
            "{}{}",
            if self.lines.is_empty() {
                "本次已检查范围未发现匹配".into()
            } else {
                self.lines.join("\n")
            },
            summary
        );
        debug_assert!(content.len() <= MAX_OUTPUT);
        ToolResult::success_with_structured(
            content,
            Some(json!({
                "complete": complete, "truncated": !self.limits.is_empty(), "entries": self.entries, "files": self.files,
                "read_bytes": self.read_bytes, "matches": self.lines.len(),
                "excluded": self.excluded, "unsafe_paths": self.unsafe_paths,
                "unreadable": self.unreadable, "non_text": self.non_text,
                "oversized": self.oversized,
                "skipped": { "excluded": self.excluded, "unsafe_paths": self.unsafe_paths,
                    "unreadable": self.unreadable, "non_text": self.non_text, "oversized": self.oversized },
                "limits": self.limits,
            })),
        )
    }
}

pub(super) async fn execute(root: &str, args: Value, grep: bool) -> ToolResult {
    match search(root, &args, grep).await {
        Ok(result) => result.done(),
        Err(error) => ToolResult::error(error),
    }
}

async fn search(root: &str, args: &Value, grep: bool) -> Result<Results, String> {
    let root_path = Path::new(root);
    local_path(root_path)?;
    let root = root_path
        .canonicalize()
        .map_err(|e| format!("委托目录无法访问：{e}"))?;
    if !root.is_dir() {
        return Err("委托目录必须是文件夹".into());
    }
    checked_path(&root, root_path)?;
    let start = match args.get("path") {
        Some(Value::String(path)) if !path.is_empty() => {
            let path = Path::new(path);
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                root.join(path)
            }
        }
        None | Some(Value::Null) => root.clone(),
        _ => return Err("查找路径必须是非空字符串".into()),
    };
    let start = checked_path(&root, &start)?;
    if !start.is_dir() {
        return Err("查找起始路径必须是目录".into());
    }
    if excluded(
        start
            .strip_prefix(&root)
            .map_err(|_| "查找路径超出委托目录")?,
    ) {
        return Err("查找范围不包含隐路径或构建依赖目录".into());
    }
    let pattern = args["pattern"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or("pattern 不能为空")?;
    let query = if grep {
        if pattern.len() > 1024 {
            return Err("正则模式最多 1024 字节".into());
        }
        let regex = RegexBuilder::new(pattern)
            .size_limit(1024 * 1024)
            .dfa_size_limit(1024 * 1024)
            .build()
            .map_err(|e| format!("无效或过于复杂的正则表达式：{e}"))?;
        let filter = match args.get("glob_filter") {
            Some(Value::String(value)) => Some(glob_pattern(value)?),
            None | Some(Value::Null) => None,
            _ => return Err("glob_filter 必须是相对 glob 字符串".into()),
        };
        Query::Grep { regex, filter }
    } else {
        Query::Glob(glob_pattern(pattern)?)
    };
    let mut result = Results::default();
    let mut pending = vec![(start.clone(), 0)];
    'walk: while let Some((directory, depth)) = pending.pop() {
        tokio::task::yield_now().await;
        let directory = match checked_path(&root, &directory) {
            Ok(path) => path,
            Err(_) => {
                result.unsafe_paths += 1;
                continue;
            }
        };
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(_) => {
                result.unreadable += 1;
                continue;
            }
        };
        let mut sorted = Vec::new();
        for entry in entries {
            tokio::task::yield_now().await;
            if result.entries >= MAX_ENTRIES {
                result.limit("枚举条目上限");
                break;
            }
            result.entries += 1;
            match entry {
                Ok(entry) => sorted.push(entry.path()),
                Err(_) => result.unreadable += 1,
            }
        }
        sorted.sort();
        for path in sorted {
            tokio::task::yield_now().await;
            let relative = path
                .strip_prefix(&start)
                .map_err(|_| "查找路径超出起始目录")?;
            if excluded(relative) {
                result.excluded += 1;
                continue;
            }
            let metadata = match std::fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(_) => {
                    result.unreadable += 1;
                    continue;
                }
            };
            if linked(&metadata) {
                result.unsafe_paths += 1;
                continue;
            }
            let real = match checked_path(&root, &path) {
                Ok(path) => path,
                Err(_) => {
                    result.unsafe_paths += 1;
                    continue;
                }
            };
            if metadata.is_dir() {
                if depth >= MAX_DEPTH {
                    result.limit("目录深度上限");
                } else {
                    pending.push((real, depth + 1));
                }
                continue;
            }
            if !metadata.is_file() {
                result.unsafe_paths += 1;
                continue;
            }
            if !query.accepts(relative) {
                continue;
            }
            result.files += 1;
            if matches!(query, Query::Glob(_)) {
                if !result.add(real.to_string_lossy().into_owned()) {
                    break 'walk;
                }
                continue;
            }
            if metadata.len() > MAX_FILE {
                result.oversized += 1;
                continue;
            }
            if result.read_bytes + metadata.len() > MAX_BYTES {
                result.limit("累计读取上限");
                break 'walk;
            }
            // Revalidate immediately before opening. No path is followed after a
            // known link substitution. Same-user concurrent replacement remains
            // outside a path-based sandbox's threat boundary.
            let real = match checked_path(&root, &path) {
                Ok(path) => path,
                Err(_) => {
                    result.unsafe_paths += 1;
                    continue;
                }
            };
            let file = match std::fs::File::open(&real) {
                Ok(file) => file,
                Err(_) => {
                    result.unreadable += 1;
                    continue;
                }
            };
            let mut bytes = Vec::new();
            let remaining = MAX_BYTES - result.read_bytes;
            let max_read = (MAX_FILE + 1).min(remaining);
            let read = file.take(max_read).read_to_end(&mut bytes);
            result.read_bytes += bytes.len() as u64;
            if read.is_err() {
                result.unreadable += 1;
                continue;
            }
            if bytes.len() as u64 > MAX_FILE {
                result.oversized += 1;
                continue;
            }
            if result.read_bytes >= MAX_BYTES {
                result.limit("累计读取上限");
                break 'walk;
            }
            if bytes.contains(&0) {
                result.non_text += 1;
                continue;
            }
            let text = match std::str::from_utf8(&bytes) {
                Ok(text) => text,
                Err(_) => {
                    result.non_text += 1;
                    continue;
                }
            };
            let Query::Grep { regex, .. } = &query else {
                unreachable!()
            };
            for (number, line) in text.lines().enumerate() {
                tokio::task::yield_now().await;
                if regex.is_match(line) {
                    let preview = crate::utils::text::truncate_chars(line, 200);
                    if !result.add(format!("{}:{}: {}", real.display(), number + 1, preview)) {
                        break 'walk;
                    }
                }
            }
        }
        if result.entries >= MAX_ENTRIES {
            if !pending.is_empty() {
                result.limit("枚举条目上限");
            }
            break;
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
