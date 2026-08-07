// ============================================================
// Observation — targeted observation of the real system state
// after a tool execution.
//
// This is NOT global computer perception. Each observation is a
// narrow, deterministic look at one outcome (file exists? process
// running?) used by the Verifier.
// ============================================================

use serde::{Deserialize, Serialize};

/// Maximum bytes read for a file content preview.
const PREVIEW_MAX_BYTES: usize = 4096;

/// Result of observing the real state relevant to one action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub summary: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<FileObservation>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub process: Option<ProcessObservation>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

impl Observation {
    pub fn file(obs: FileObservation) -> Self {
        let summary = if obs.exists {
            format!("文件存在: {}", obs.path)
        } else {
            format!("文件不存在: {}", obs.path)
        };
        Self {
            summary,
            file: Some(obs),
            process: None,
            metadata: None,
        }
    }

    pub fn process(obs: ProcessObservation) -> Self {
        let summary = if obs.running {
            format!("进程运行中: {}", obs.process_name)
        } else {
            format!("进程未运行: {}", obs.process_name)
        };
        Self {
            summary,
            file: None,
            process: Some(obs),
            metadata: None,
        }
    }
}

/// File observation — existence, size, bounded content preview.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileObservation {
    pub path: String,
    pub exists: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_preview: Option<String>,
}

/// Observe a file at the given path. Never panics; never reads unbounded data.
pub fn observe_file(path: &std::path::Path) -> FileObservation {
    let path_str = path.display().to_string();
    if !path.exists() {
        return FileObservation {
            path: path_str,
            exists: false,
            size: None,
            content_preview: None,
        };
    }

    let size = std::fs::metadata(path).ok().map(|m| m.len());
    FileObservation {
        path: path_str,
        exists: true,
        size,
        content_preview: read_text_preview(path),
    }
}

/// Read up to PREVIEW_MAX_BYTES of a file as text.
/// Returns None for binary/unreadable content instead of panicking.
fn read_text_preview(path: &std::path::Path) -> Option<String> {
    let mut buf = Vec::with_capacity(PREVIEW_MAX_BYTES + 1);
    let mut file = std::fs::File::open(path).ok()?;
    use std::io::Read;
    let read = file
        .by_ref()
        .take(PREVIEW_MAX_BYTES as u64 + 1)
        .read_to_end(&mut buf)
        .ok()?;

    // Treat as binary if it looks like it contains a NUL byte early on.
    if buf.iter().take(256).any(|&b| b == 0) {
        return None;
    }

    let text = String::from_utf8_lossy(&buf[..read.min(buf.len())]).into_owned();
    if text.is_empty() {
        return None;
    }
    Some(text)
}

/// Process observation — is a named process running? (optional pid)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessObservation {
    pub process_name: String,
    pub running: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
}

/// Observe whether a process with the given name is currently running.
pub fn observe_process_by_name(name: &str) -> ProcessObservation {
    let mut system = sysinfo::System::new_all();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All);

    let proc = system.processes_by_name(std::ffi::OsStr::new(name)).next();
    ProcessObservation {
        process_name: name.to_string(),
        running: proc.is_some(),
        pid: proc.map(|p| p.pid().as_u32()),
    }
}

/// Observe whether the process with the given pid is currently running.
pub fn observe_process_by_pid(pid: u32) -> ProcessObservation {
    let mut system = sysinfo::System::new_all();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All);

    let proc = system.processes().get(&sysinfo::Pid::from_u32(pid));
    let running = proc.is_some();

    ProcessObservation {
        process_name: proc
            .map(|p| p.name().to_string_lossy().to_string())
            .unwrap_or_else(|| format!("pid {}", pid)),
        running,
        pid: Some(pid),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observe_missing_file() {
        let obs = observe_file(std::path::Path::new("C:/definitely/not/here.txt"));
        assert!(!obs.exists);
        assert!(obs.size.is_none());
        assert!(obs.content_preview.is_none());
    }

    #[test]
    fn observe_text_file_with_preview() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("yilian-observe-test-{}.txt", uuid::Uuid::new_v4()));
        std::fs::write(&path, "YiLian verification success").unwrap();
        let obs = observe_file(&path);
        assert!(obs.exists);
        assert!(obs.content_preview.is_some());
        assert!(obs
            .content_preview
            .as_ref()
            .unwrap()
            .contains("verification"));
        std::fs::remove_file(&path).ok();
    }
}
