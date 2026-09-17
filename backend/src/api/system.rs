// ============================================================
// System monitoring API — CPU, memory, disk, GPU
// ============================================================

use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;
use std::{
    panic::AssertUnwindSafe,
    sync::{Arc, OnceLock},
};
use sysinfo::{Disks, System};

use crate::server::AppServer;
use crate::utils::process::hide_std_command_window;

static GPU_INFO: OnceLock<Vec<serde_json::Value>> = OnceLock::new();
fn memory_gib(bytes: u64) -> String {
    format!("{:.2}", bytes as f64 / 1_073_741_824.0)
}

#[derive(Debug, Serialize)]
pub(super) struct HealthResponse {
    status: &'static str,
    service: &'static str,
    version: &'static str,
    database: &'static str,
    policy_version: &'static str,
}

/// GET /api/health — public runtime readiness details.
pub async fn health(State(server): State<Arc<AppServer>>) -> (StatusCode, Json<HealthResponse>) {
    let database_healthy = database_is_healthy(&server);
    let (status, response) = health_response(database_healthy);
    (status, Json(response))
}

fn database_is_healthy(server: &AppServer) -> bool {
    let check = std::panic::catch_unwind(AssertUnwindSafe(|| {
        let conn = server.db.conn();
        conn.query_row("SELECT COUNT(*) FROM sqlite_schema", [], |row| {
            row.get::<_, i64>(0)
        })
    }));

    match check {
        Ok(Ok(_)) => true,
        Ok(Err(error)) => {
            tracing::warn!(error = %error, "runtime health database check failed");
            false
        }
        Err(_) => {
            tracing::error!("runtime health database check panicked");
            false
        }
    }
}

fn health_response(database_healthy: bool) -> (StatusCode, HealthResponse) {
    let (http_status, status, database) = if database_healthy {
        (StatusCode::OK, "healthy", "healthy")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "degraded", "unavailable")
    };

    (
        http_status,
        HealthResponse {
            status,
            service: env!("CARGO_PKG_NAME"),
            version: env!("CARGO_PKG_VERSION"),
            database,
            policy_version: crate::safety::POLICY_VERSION,
        },
    )
}

/// GET /api/system — full system snapshot
pub async fn system_info(State(_server): State<Arc<AppServer>>) -> Json<serde_json::Value> {
    Json(
        tokio::task::spawn_blocking(collect_system_info)
            .await
            .expect("system metrics collector task failed"),
    )
}

fn collect_system_info() -> serde_json::Value {
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
    let total_mem = sys.total_memory(); // bytes
    let used_mem = sys.used_memory(); // bytes
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

    serde_json::json!({
        "process_count": sys.processes().len(),
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
            "total_gb": memory_gib(total_mem),
            "used_gb": memory_gib(used_mem),
            "usage_pct": format!("{:.1}", mem_usage_pct),
            "swap_total_gb": memory_gib(total_swap),
            "swap_used_gb": memory_gib(used_swap),
        },
        "disks": disk_info,
        "gpu": gpu_info,
    })
}

pub async fn get_preferences(
    State(server): State<Arc<AppServer>>,
) -> Result<
    Json<crate::capability::system_preferences::Preferences>,
    (StatusCode, Json<serde_json::Value>),
> {
    crate::capability::system_preferences::load(&server.db)
        .map(Json)
        .map_err(|message| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"message":message})),
            )
        })
}

pub async fn put_preferences(
    State(server): State<Arc<AppServer>>,
    Json(value): Json<crate::capability::system_preferences::Preferences>,
) -> Result<
    Json<crate::capability::system_preferences::Preferences>,
    (StatusCode, Json<serde_json::Value>),
> {
    crate::capability::system_preferences::save(&server.db, value)
        .map(Json)
        .map_err(|message| {
            (
                if message.starts_with("stale_") {
                    StatusCode::CONFLICT
                } else {
                    StatusCode::BAD_REQUEST
                },
                Json(serde_json::json!({"message":message})),
            )
        })
}

/// GET /api/system/cpu — CPU only
pub async fn cpu_info(State(_server): State<Arc<AppServer>>) -> Json<serde_json::Value> {
    Json(
        tokio::task::spawn_blocking(collect_cpu_info)
            .await
            .expect("CPU metrics collector task failed"),
    )
}

fn collect_cpu_info() -> serde_json::Value {
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

    serde_json::json!({
        "cores": cores,
        "avg_usage_pct": format!("{:.1}", avg),
        "count": cores.len(),
    })
}

/// GET /api/system/memory — Memory only
pub async fn memory_info(State(_server): State<Arc<AppServer>>) -> Json<serde_json::Value> {
    Json(
        tokio::task::spawn_blocking(collect_memory_info)
            .await
            .expect("memory metrics collector task failed"),
    )
}

fn collect_memory_info() -> serde_json::Value {
    let mut sys = System::new_all();
    sys.refresh_memory();

    let total = sys.total_memory();
    let used = sys.used_memory();
    let available = sys.available_memory();
    let free = sys.free_memory();

    serde_json::json!({
        "total_gb": memory_gib(total),
        "used_gb": memory_gib(used),
        "available_gb": memory_gib(available),
        "free_gb": memory_gib(free),
        "usage_pct": format!("{:.1}", if total > 0 { (used as f64 / total as f64) * 100.0 } else { 0.0 }),
    })
}

fn get_gpu_info() -> Vec<serde_json::Value> {
    cached_gpu_info_with(&GPU_INFO, query_gpu_info)
}

fn cached_gpu_info_with(
    cache: &OnceLock<Vec<serde_json::Value>>,
    collect: impl FnOnce() -> Vec<serde_json::Value>,
) -> Vec<serde_json::Value> {
    cache.get_or_init(collect).clone()
}

fn query_gpu_info() -> Vec<serde_json::Value> {
    #[cfg(target_os = "windows")]
    {
        // Try PowerShell to get GPU info
        let mut command = std::process::Command::new("powershell.exe");
        hide_std_command_window(&mut command);
        if let Ok(output) = command
            .args([
                "-NoProfile",
                "-Command",
                "Get-CimInstance -ClassName Win32_VideoController | Select-Object Name, AdapterRAM, DriverVersion, CurrentHorizontalResolution, CurrentVerticalResolution | ConvertTo-Json",
            ])
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

#[cfg(test)]
mod health_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[test]
    fn memory_bytes_are_reported_in_gibibytes() {
        assert_eq!(memory_gib(1_073_741_824), "1.00");
        assert_eq!(memory_gib(16 * 1_073_741_824), "16.00");
    }

    #[test]
    fn system_collector_keeps_resource_sections() {
        let value = collect_system_info();

        assert!(value.get("cpu").is_some());
        assert!(value.get("memory").is_some());
        assert!(value.get("disks").is_some());
        assert!(value.get("gpu").is_some());
    }

    #[test]
    fn focused_collectors_keep_existing_shapes() {
        let cpu = collect_cpu_info();
        let memory = collect_memory_info();

        assert!(cpu
            .get("cores")
            .and_then(serde_json::Value::as_array)
            .is_some());
        assert!(cpu.get("avg_usage_pct").is_some());
        assert!(memory.get("total_gb").is_some());
        assert!(memory.get("usage_pct").is_some());
    }

    #[test]
    fn unavailable_database_degrades_health() {
        let (status, response) = health_response(false);

        assert_eq!(status, axum::http::StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.status, "degraded");
        assert_eq!(response.database, "unavailable");
        assert_eq!(response.policy_version, crate::safety::POLICY_VERSION);
    }

    #[test]
    fn gpu_metadata_cache_collects_once() {
        let cache = std::sync::OnceLock::new();
        let calls = AtomicUsize::new(0);

        let first = cached_gpu_info_with(&cache, || {
            calls.fetch_add(1, Ordering::SeqCst);
            vec![serde_json::json!({"name": "First GPU"})]
        });
        let second = cached_gpu_info_with(&cache, || {
            calls.fetch_add(1, Ordering::SeqCst);
            vec![serde_json::json!({"name": "Unexpected GPU"})]
        });

        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(first, second);
        assert_eq!(second[0]["name"], "First GPU");
    }
}
