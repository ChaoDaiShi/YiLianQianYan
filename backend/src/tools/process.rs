// ============================================================
// Process management tool — list and manage system processes
// ============================================================

use async_trait::async_trait;
use serde::Serialize;
use std::process::Command;

use super::trait_def::{RiskLevel, Tool, ToolResult};

#[derive(Debug, Serialize)]
struct ProcessInfo {
    pid: u32,
    name: String,
    memory_mb: f64,
}

/// List running processes (cross-platform)
fn list_processes() -> Result<Vec<ProcessInfo>, String> {
    let mut processes = Vec::new();

    if cfg!(target_os = "windows") {
        // Windows: use PowerShell
        let output = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-Command",
                "Get-Process | Sort-Object -Property WS -Descending | Select-Object -First 50 Id,ProcessName,@{N='MemoryMB';E={[math]::Round($_.WorkingSet64/1MB,1)}} | ConvertTo-Json"
            ])
            .output()
            .map_err(|e| format!("Failed to list processes: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Ok(list) = serde_json::from_str::<Vec<serde_json::Value>>(&stdout) {
            for p in list {
                processes.push(ProcessInfo {
                    pid: p["Id"].as_u64().unwrap_or(0) as u32,
                    name: p["ProcessName"].as_str().unwrap_or("?").to_string(),
                    memory_mb: p["MemoryMB"].as_f64().unwrap_or(0.0),
                });
            }
        } else {
            // Fallback: use tasklist
            let output = Command::new("cmd.exe")
                .args(["/c", "tasklist /FO CSV /NH"])
                .output()
                .map_err(|e| format!("Failed: {}", e))?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines().take(50) {
                let parts: Vec<&str> = line.split(',').collect();
                if parts.len() >= 2 {
                    let name = parts[0].trim_matches('"');
                    let pid = parts[1].trim_matches('"').parse().unwrap_or(0);
                    let mem = parts
                        .get(4)
                        .map(|s| {
                            s.trim_matches('"')
                                .replace(" K", "")
                                .replace(",", "")
                                .parse::<f64>()
                                .unwrap_or(0.0)
                                / 1024.0
                        })
                        .unwrap_or(0.0);
                    processes.push(ProcessInfo {
                        pid,
                        name: name.to_string(),
                        memory_mb: mem,
                    });
                }
            }
        }
    } else {
        // Unix: use ps
        let output = Command::new("ps")
            .args(["aux", "--sort=-%mem"])
            .output()
            .map_err(|e| format!("Failed to list processes: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        for (i, line) in stdout.lines().enumerate() {
            if i == 0 {
                continue;
            } // Skip header
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 11 {
                let pid = parts[1].parse().unwrap_or(0);
                let name = parts.get(10).unwrap_or(&"?").to_string();
                let mem = parts
                    .get(3)
                    .map(|s| s.parse::<f64>().unwrap_or(0.0))
                    .unwrap_or(0.0);
                processes.push(ProcessInfo {
                    pid,
                    name,
                    memory_mb: mem,
                });
            }
            if processes.len() >= 50 {
                break;
            }
        }
    }

    Ok(processes)
}

/// Kill a process by PID
fn kill_process(pid: u32) -> Result<String, String> {
    if cfg!(target_os = "windows") {
        Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .output()
            .map_err(|e| format!("无法终止进程: {}", e))?;
    } else {
        Command::new("kill")
            .args(["-9", &pid.to_string()])
            .output()
            .map_err(|e| format!("无法终止进程: {}", e))?;
    }
    Ok(format!("进程 {} 已终止", pid))
}

pub struct ProcessTool;

#[async_trait]
impl Tool for ProcessTool {
    fn name(&self) -> &str {
        "process"
    }

    fn description(&self) -> &str {
        "管理系统进程。可以列出正在运行的进程或终止指定进程。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "kill"],
                    "description": "操作类型：list=列出进程, kill=终止进程"
                },
                "pid": {
                    "type": "number",
                    "description": "要终止的进程PID（仅action=kill时需要）"
                }
            },
            "required": ["action"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::High
    }

    async fn execute(&self, args: serde_json::Value) -> ToolResult {
        let action = args["action"].as_str().unwrap_or("list");

        match action {
            "list" => match list_processes() {
                Ok(processes) => {
                    if processes.is_empty() {
                        return ToolResult::success("未找到运行中的进程");
                    }
                    let mut output = format!("{:<8} {:<30} {:>10}\n", "PID", "进程名", "内存(MB)");
                    output.push_str(&"-".repeat(52));
                    output.push('\n');
                    for p in &processes {
                        output.push_str(&format!(
                            "{:<8} {:<30} {:>10.1}\n",
                            p.pid, p.name, p.memory_mb
                        ));
                    }
                    output.push_str(&format!("\n共 {} 个进程", processes.len()));
                    ToolResult::success(output)
                }
                Err(e) => ToolResult::error(e),
            },
            "kill" => {
                let pid = args["pid"].as_u64().unwrap_or(0) as u32;
                if pid == 0 {
                    return ToolResult::error("请提供要终止的进程PID");
                }
                match kill_process(pid) {
                    Ok(msg) => ToolResult::success(msg),
                    Err(e) => ToolResult::error(e),
                }
            }
            _ => ToolResult::error(format!("未知操作: {}，支持 list 和 kill", action)),
        }
    }
}
