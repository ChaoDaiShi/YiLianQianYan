// ============================================================
// System monitoring API — CPU, memory, disk, GPU
// ============================================================

use axum::{extract::State, Json};
use std::sync::Arc;
use sysinfo::{Disks, System};

use crate::server::AppServer;

/// GET /api/system — full system snapshot
pub async fn system_info(State(_server): State<Arc<AppServer>>) -> Json<serde_json::Value> {
    let mut sys = System::new_all();
    sys.refresh_all();

    // ── CPU ──
    let cpu_count = sys.cpus().len();
    let cpu_usage: Vec<f32> = sys.cpus().iter().map(|c| c.cpu_usage()).collect();
    let cpu_avg = if cpu_usage.is_empty() {
        0.0
    } else {
        cpu_usage.iter().sum::<f32>() / cpu_usage.len() as f32
    };
    let cpu_name = sys
        .cpus()
        .first()
        .map(|c| c.brand().to_string())
        .unwrap_or_default();

    // ── Memory ──
    let total_mem = sys.total_memory(); // KB
    let used_mem = sys.used_memory(); // KB
    let mem_usage_pct = if total_mem > 0 {
        (used_mem as f64 / total_mem as f64) * 100.0
    } else {
        0.0
    };
    let total_swap = sys.total_swap();
    let used_swap = sys.used_swap();

    // ── Disks ──
    let disks = Disks::new_with_refreshed_list();
    let disk_info: Vec<serde_json::Value> = disks
        .iter()
        .map(|d| {
            let total = d.total_space();
            let available = d.available_space();
            let used = total.saturating_sub(available);
            let pct = if total > 0 {
                (used as f64 / total as f64) * 100.0
            } else {
                0.0
            };
            serde_json::json!({
                "mount": d.mount_point().to_string_lossy(),
                "name": d.name().to_string_lossy(),
                "fs_type": d.file_system().to_string_lossy(),
                "total_gb": format!("{:.1}", total as f64 / 1_073_741_824.0),
                "used_gb": format!("{:.1}", used as f64 / 1_073_741_824.0),
                "available_gb": format!("{:.1}", available as f64 / 1_073_741_824.0),
                "usage_pct": format!("{:.1}", pct),
            })
        })
        .collect();

    // ── GPU (basic via WMI on Windows, placeholder on other platforms) ──
    let gpu_info = get_gpu_info();

    // ── System ──
    let uptime = System::uptime(); // seconds
    let hostname = System::host_name().unwrap_or_default();
    let os = System::long_os_version().unwrap_or_default();
    let kernel = System::kernel_version().unwrap_or_default();

    Json(serde_json::json!({
        "hostname": hostname,
        "os": os,
        "kernel": kernel,
        "uptime_secs": uptime,
        "cpu": {
            "name": cpu_name,
            "cores": cpu_count,
            "usage_pct": format!("{:.1}", cpu_avg),
            "per_core": cpu_usage.iter().map(|u| format!("{:.1}", u)).collect::<Vec<_>>(),
        },
        "memory": {
            "total_gb": format!("{:.2}", total_mem as f64 / 1_048_576.0),
            "used_gb": format!("{:.2}", used_mem as f64 / 1_048_576.0),
            "usage_pct": format!("{:.1}", mem_usage_pct),
            "swap_total_gb": format!("{:.2}", total_swap as f64 / 1_048_576.0),
            "swap_used_gb": format!("{:.2}", used_swap as f64 / 1_048_576.0),
        },
        "disks": disk_info,
        "gpu": gpu_info,
    }))
}

/// GET /api/system/cpu — CPU only
pub async fn cpu_info(State(_server): State<Arc<AppServer>>) -> Json<serde_json::Value> {
    let mut sys = System::new_all();
    sys.refresh_cpu_all();
    // Wait briefly for accurate readings
    std::thread::sleep(std::time::Duration::from_millis(100));
    sys.refresh_cpu_all();

    let cores: Vec<serde_json::Value> = sys
        .cpus()
        .iter()
        .enumerate()
        .map(|(i, c)| {
            serde_json::json!({
                "index": i,
                "name": c.name(),
                "brand": c.brand(),
                "usage_pct": format!("{:.1}", c.cpu_usage()),
                "frequency_mhz": c.frequency(),
            })
        })
        .collect();

    let avg = if cores.is_empty() {
        0.0
    } else {
        cores
            .iter()
            .filter_map(|c| c["usage_pct"].as_str().and_then(|s| s.parse::<f32>().ok()))
            .sum::<f32>()
            / cores.len() as f32
    };

    Json(serde_json::json!({
        "cores": cores,
        "avg_usage_pct": format!("{:.1}", avg),
        "count": cores.len(),
    }))
}

/// GET /api/system/memory — Memory only
pub async fn memory_info(State(_server): State<Arc<AppServer>>) -> Json<serde_json::Value> {
    let mut sys = System::new_all();
    sys.refresh_memory();

    let total = sys.total_memory();
    let used = sys.used_memory();
    let available = sys.available_memory();
    let free = sys.free_memory();

    Json(serde_json::json!({
        "total_gb": format!("{:.2}", total as f64 / 1_048_576.0),
        "used_gb": format!("{:.2}", used as f64 / 1_048_576.0),
        "available_gb": format!("{:.2}", available as f64 / 1_048_576.0),
        "free_gb": format!("{:.2}", free as f64 / 1_048_576.0),
        "usage_pct": format!("{:.1}", if total > 0 { (used as f64 / total as f64) * 100.0 } else { 0.0 }),
    }))
}

fn get_gpu_info() -> Vec<serde_json::Value> {
    #[cfg(target_os = "windows")]
    {
        // Try PowerShell to get GPU info
        if let Ok(output) = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-Command",
                "Get-CimInstance -ClassName Win32_VideoController | Select-Object Name, AdapterRAM, DriverVersion, CurrentHorizontalResolution, CurrentVerticalResolution | ConvertTo-Json"])
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&stdout) {
                if let Some(arr) = val.as_array() {
                    return arr.iter().map(|g| {
                        let ram = g["AdapterRAM"].as_u64().unwrap_or(0);
                        serde_json::json!({
                            "name": g["Name"].as_str().unwrap_or("Unknown"),
                            "vram_gb": format!("{:.2}", ram as f64 / 1_073_741_824.0),
                            "driver": g["DriverVersion"].as_str().unwrap_or(""),
                            "resolution": format!("{}x{}",
                                g["CurrentHorizontalResolution"].as_u64().unwrap_or(0),
                                g["CurrentVerticalResolution"].as_u64().unwrap_or(0)),
                        })
                    }).collect();
                }
            }
        }
    }
    vec![
        serde_json::json!({"name": "GPU info unavailable", "vram_gb": "?", "driver": "", "resolution": ""}),
    ]
}
