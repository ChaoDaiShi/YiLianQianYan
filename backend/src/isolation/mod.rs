// ============================================================
// OS-assisted process isolation — application-layer process containment.
//
// Scope: managed child processes started by the agent (bash) run with a
// sanitized environment (no inherited secrets) and are terminated as a whole
// process tree on timeout / cancel / shutdown.
//
// Honest boundary: this is NOT container-grade isolation. It does not provide
// an OS filesystem namespace or network namespace. On Windows it uses the
// process tree kill (taskkill /T); a full Restricted Token + Job Object backend
// is a separate concern and is reported honestly in [`IsolationStatus`].
// ============================================================

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde::Serialize;
use tokio::process::Command;
#[cfg(not(windows))]
use tokio::time::{timeout, Duration};

use crate::utils::process::{hide_std_command_window, hide_tokio_command_window};
#[cfg(not(windows))]
use crate::utils::text::truncate_chars;

#[cfg(windows)]
pub mod windows;

/// Environment variables passed to child processes by default. Secrets and
/// model/MCP credentials are NEVER inherited — they are stripped by building
/// from this allowlist rather than inheriting the whole parent environment.
const CHILD_ENV_ALLOWLIST: &[&str] = &[
    "PATH",
    "SYSTEMROOT",
    "SYSTEMDRIVE",
    "COMSPEC",
    "WINDIR",
    "TEMP",
    "TMP",
    "TMPDIR",
    "USERPROFILE",
    "HOMEDRIVE",
    "HOMEPATH",
    "HOME",
    "LANG",
    "LC_ALL",
    "PROGRAMFILES",
    "ProgramFiles(x86)",
    "OS",
    "PROCESSOR_ARCHITECTURE",
    "NUMBER_OF_PROCESSORS",
];

/// A bounded, safe result of a managed process run.
#[derive(Debug, Clone)]
pub struct ProcessRunResult {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

/// Honest isolation capability report.
#[derive(Debug, Clone, Serialize)]
pub struct IsolationStatus {
    pub backend: &'static str,
    pub process_containment: bool,
    /// Privilege-reduced token active. This is NOT IsTokenRestricted == TRUE
    /// (no restricting-SID list is used); it means the child token's privilege
    /// surface is verified strictly smaller than the parent's.
    pub restricted_token: bool,
    pub privilege_reduction: bool,
    /// Restricting-SID ACL sandbox — deliberately NOT used (false).
    pub restricting_sids: bool,
    pub job_object: bool,
    pub kill_tree: bool,
    pub filesystem_os_enforced: bool,
    pub network_os_enforced: bool,
}

pub fn isolation_status() -> IsolationStatus {
    #[cfg(windows)]
    {
        IsolationStatus {
            backend: "windows_restricted_privilege_job",
            process_containment: true,
            restricted_token: true,
            privilege_reduction: true,
            restricting_sids: false,
            job_object: true,
            kill_tree: true,
            filesystem_os_enforced: false,
            network_os_enforced: false,
        }
    }
    #[cfg(not(windows))]
    {
        IsolationStatus {
            backend: "process_group",
            process_containment: true,
            restricted_token: false,
            privilege_reduction: false,
            restricting_sids: false,
            job_object: false,
            kill_tree: true,
            filesystem_os_enforced: false,
            network_os_enforced: false,
        }
    }
}

/// Build a sanitized child environment: allowlist base + explicit entries.
/// Explicit entries (e.g. MCP stdio env) override the base.
pub fn build_child_env(explicit: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    for key in CHILD_ENV_ALLOWLIST {
        if let Ok(value) = std::env::var(key) {
            env.insert((*key).to_string(), value);
        }
    }
    for (k, v) in explicit {
        env.insert(k.clone(), v.clone());
    }
    env
}

/// Kill a whole process tree (parent + descendants).
pub async fn kill_process_tree(pid: u32) {
    if cfg!(windows) {
        // taskkill /T terminates the tree; best-effort.
        let mut command = Command::new("taskkill");
        hide_tokio_command_window(&mut command);
        let _ = command
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .output()
            .await;
    } else {
        // Kill the process group (negative pid) + the process itself.
        let _ = Command::new("kill")
            .args(["-TERM", &format!("-{pid}")])
            .output()
            .await;
        let _ = Command::new("kill")
            .args(["-KILL", &pid.to_string()])
            .output()
            .await;
    }
}

/// Run a child process with a sanitized env, a real async timeout, and whole
/// process-tree termination on timeout.
#[cfg(windows)]
pub async fn run_managed_process(
    program: &str,
    args: &[&str],
    current_dir: &str,
    explicit_env: &BTreeMap<String, String>,
    timeout_ms: u64,
) -> ProcessRunResult {
    let registry = Arc::new(ManagedProcessRegistry::new());
    run_managed_process_with_registry(
        program,
        args,
        current_dir,
        explicit_env,
        timeout_ms,
        &registry,
        None,
        program,
    )
    .await
}

/// Run a process while registering its live control object in the shared
/// application registry. The registry is the only supported process-control
/// authority for Gateway-routed tools.
#[cfg(windows)]
pub async fn run_managed_process_with_registry(
    program: &str,
    args: &[&str],
    current_dir: &str,
    explicit_env: &BTreeMap<String, String>,
    timeout_ms: u64,
    registry: &SharedManagedProcessRegistry,
    tool_call_id: Option<String>,
    name: &str,
) -> ProcessRunResult {
    windows::run_windows_managed_process_with_registry(
        program,
        args,
        current_dir,
        explicit_env,
        timeout_ms,
        registry,
        tool_call_id,
        name,
    )
    .await
}

/// Run a child process with a sanitized env, a real async timeout, and whole
/// process-tree termination on timeout (portable non-Windows backend).
#[cfg(not(windows))]
pub async fn run_managed_process(
    program: &str,
    args: &[&str],
    current_dir: &str,
    explicit_env: &BTreeMap<String, String>,
    timeout_ms: u64,
) -> ProcessRunResult {
    let registry = Arc::new(ManagedProcessRegistry::new());
    run_managed_process_with_registry(
        program,
        args,
        current_dir,
        explicit_env,
        timeout_ms,
        &registry,
        None,
        program,
    )
    .await
}

#[cfg(not(windows))]
pub async fn run_managed_process_with_registry(
    program: &str,
    args: &[&str],
    current_dir: &str,
    explicit_env: &BTreeMap<String, String>,
    timeout_ms: u64,
    registry: &SharedManagedProcessRegistry,
    tool_call_id: Option<String>,
    name: &str,
) -> ProcessRunResult {
    let env = build_child_env(explicit_env);

    let mut cmd = Command::new(program);
    cmd.args(args)
        .current_dir(current_dir)
        .env_clear()
        .envs(env)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            return ProcessRunResult {
                exit_code: None,
                stdout: String::new(),
                stderr: format!("spawn failed: {e}"),
                timed_out: false,
            }
        }
    };
    let pid = child.id();
    if let Some(pid) = pid {
        registry.record_with_control(
            pid,
            tool_call_id,
            name.to_string(),
            Arc::new(PortableProcessControl { pid }),
        );
    }

    let output = timeout(Duration::from_millis(timeout_ms), child.wait_with_output()).await;

    let result = match output {
        Ok(Ok(output)) => {
            let stdout = truncate_chars(&String::from_utf8_lossy(&output.stdout), 20_000);
            let stderr = truncate_chars(&String::from_utf8_lossy(&output.stderr), 20_000);
            ProcessRunResult {
                exit_code: output.status.code(),
                stdout,
                stderr,
                timed_out: false,
            }
        }
        Ok(Err(e)) => ProcessRunResult {
            exit_code: None,
            stdout: String::new(),
            stderr: format!("process error: {e}"),
            timed_out: false,
        },
        Err(_) => {
            // Timed out — kill the whole tree.
            if let Some(pid) = pid {
                kill_process_tree(pid).await;
            }
            ProcessRunResult {
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                timed_out: true,
            }
        }
    };
    if let Some(pid) = pid {
        registry.remove_pid(pid);
    }
    result
}

/// Managed process registry — tracks children started by the agent so process
/// control can distinguish managed children from arbitrary host PIDs.
pub struct ManagedProcessRegistry {
    entries: parking_lot::Mutex<Vec<ManagedProcessRecord>>,
    next_id: AtomicU64,
}

pub trait ManagedProcessControl: Send + Sync {
    fn is_alive(&self) -> bool;
    fn terminate(&self) -> Result<(), String>;
}

struct PortableProcessControl {
    pid: u32,
}

impl ManagedProcessControl for PortableProcessControl {
    fn is_alive(&self) -> bool {
        #[cfg(windows)]
        {
            let mut command = std::process::Command::new("tasklist");
            hide_std_command_window(&mut command);
            return command
                .args(["/FI", &format!("PID eq {}", self.pid), "/NH"])
                .output()
                .map(|output| {
                    output.status.success()
                        && String::from_utf8_lossy(&output.stdout).contains(&self.pid.to_string())
                })
                .unwrap_or(false);
        }
        #[cfg(not(windows))]
        std::process::Command::new("kill")
            .args(["-0", &self.pid.to_string()])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    fn terminate(&self) -> Result<(), String> {
        #[cfg(windows)]
        let status = {
            let mut command = std::process::Command::new("taskkill");
            hide_std_command_window(&mut command);
            command
                .args(["/PID", &self.pid.to_string(), "/T", "/F"])
                .status()
        };
        #[cfg(not(windows))]
        let status = std::process::Command::new("kill")
            .args(["-TERM", &format!("-{}", self.pid)])
            .status();
        status
            .map_err(|error| format!("managed process termination failed: {error}"))
            .and_then(|status| {
                if status.success() {
                    Ok(())
                } else {
                    Err(format!("managed process termination exited with {status}"))
                }
            })
    }
}

#[derive(Clone)]
pub struct ManagedProcessRecord {
    pub pid: u32,
    pub tool_call_id: Option<String>,
    pub started_at: i64,
    pub name: String,
    control: Arc<dyn ManagedProcessControl>,
}

impl ManagedProcessRegistry {
    pub fn new() -> Self {
        Self {
            entries: parking_lot::Mutex::new(Vec::new()),
            next_id: AtomicU64::new(1),
        }
    }

    pub fn record(&self, pid: u32, tool_call_id: Option<String>, name: String) {
        self.record_with_control(
            pid,
            tool_call_id,
            name,
            Arc::new(PortableProcessControl { pid }),
        );
    }

    pub fn record_with_control(
        &self,
        pid: u32,
        tool_call_id: Option<String>,
        name: String,
        control: Arc<dyn ManagedProcessControl>,
    ) {
        self.entries.lock().push(ManagedProcessRecord {
            pid,
            tool_call_id,
            started_at: chrono::Utc::now().timestamp_millis(),
            name,
            control,
        });
    }

    pub fn contains_pid(&self, pid: u32) -> bool {
        self.contains_active_pid(pid)
    }

    pub fn contains_active_pid(&self, pid: u32) -> bool {
        let mut entries = self.entries.lock();
        entries.retain(|entry| entry.control.is_alive());
        entries.iter().any(|entry| entry.pid == pid)
    }

    pub fn terminate_pid(&self, pid: u32) -> Result<(), String> {
        let control = {
            let mut entries = self.entries.lock();
            entries.retain(|entry| entry.control.is_alive());
            entries
                .iter()
                .find(|entry| entry.pid == pid)
                .map(|entry| Arc::clone(&entry.control))
                .ok_or_else(|| format!("managed process {pid} is not active"))?
        };
        control.terminate()?;
        self.remove_pid(pid);
        Ok(())
    }

    pub fn shutdown_all(&self) {
        let controls = {
            let mut entries = self.entries.lock();
            let controls = entries
                .iter()
                .map(|entry| (entry.pid, Arc::clone(&entry.control)))
                .collect::<Vec<_>>();
            entries.clear();
            controls
        };
        for (pid, control) in controls {
            if let Err(error) = control.terminate() {
                tracing::warn!(pid, %error, "failed to terminate managed process during shutdown");
            }
        }
    }

    pub fn remove_pid(&self, pid: u32) {
        self.entries.lock().retain(|e| e.pid != pid);
    }

    pub fn list(&self) -> Vec<ManagedProcessRecord> {
        self.entries.lock().clone()
    }

    #[allow(dead_code)]
    fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }
}

pub type SharedManagedProcessRegistry = Arc<ManagedProcessRegistry>;

#[cfg(test)]
mod registry_tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct TestControl {
        alive: AtomicBool,
        terminated: AtomicBool,
    }

    impl ManagedProcessControl for TestControl {
        fn is_alive(&self) -> bool {
            self.alive.load(Ordering::SeqCst)
        }

        fn terminate(&self) -> Result<(), String> {
            self.terminated.store(true, Ordering::SeqCst);
            self.alive.store(false, Ordering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn registry_only_controls_active_process_and_removes_after_termination() {
        let registry = ManagedProcessRegistry::new();
        let control = Arc::new(TestControl {
            alive: AtomicBool::new(true),
            terminated: AtomicBool::new(false),
        });
        registry.record_with_control(
            4242,
            Some("call-1".to_string()),
            "test-child".to_string(),
            control.clone(),
        );

        assert!(registry.contains_active_pid(4242));
        registry.terminate_pid(4242).unwrap();
        assert!(control.terminated.load(Ordering::SeqCst));
        assert!(!registry.contains_active_pid(4242));
    }

    #[test]
    fn registry_prunes_dead_processes_and_unknown_pid_is_side_effect_free() {
        let registry = ManagedProcessRegistry::new();
        let control = Arc::new(TestControl {
            alive: AtomicBool::new(false),
            terminated: AtomicBool::new(false),
        });
        registry.record_with_control(4343, None, "dead".to_string(), control.clone());

        assert!(!registry.contains_active_pid(4343));
        assert!(registry.terminate_pid(9999).is_err());
        assert!(!control.terminated.load(Ordering::SeqCst));
    }
}
