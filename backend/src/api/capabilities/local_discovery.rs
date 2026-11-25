use super::{import_error, ImportError, LocalDiscoveryRequest};
use axum::Json;
use std::sync::Arc;

pub async fn discover_local_services(
    Json(request): Json<LocalDiscoveryRequest>,
) -> Result<Json<serde_json::Value>, ImportError> {
    if request.directories.len() > 16 {
        return Err(import_error("too_many_discovery_directories".into()));
    }
    let mut candidates = Vec::new();
    let mut errors = Vec::new();
    for directory in request.directories {
        let path = std::path::Path::new(&directory);
        if !path.is_absolute() {
            return Err(import_error("absolute_discovery_directory_required".into()));
        }
        match std::fs::read_dir(path) {
            Ok(entries) => {
                let mut roots = vec![path.to_path_buf()];
                roots.extend(
                    entries
                        .take(256)
                        .filter_map(Result::ok)
                        .map(|e| e.path())
                        .filter(|p| p.is_dir()),
                );
                for root in roots {
                    if root.join("SKILL.md").is_file() {
                        candidates.push(serde_json::json!({"kind":"skill","name":root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),"source":directory,"path":root.to_string_lossy(),"protocol":"SKILL.md"}));
                    }
                    let config = root.join("mcp.json");
                    if config.is_file() {
                        if let Ok(raw) = std::fs::read_to_string(&config) {
                            if raw.len() <= 512 * 1024 {
                                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
                                    if let Some(servers) =
                                        value.get("mcpServers").and_then(|v| v.as_object())
                                    {
                                        for (name, server) in servers {
                                            // Imported secrets are intentionally not projected to the browser.
                                            candidates.push(serde_json::json!({"kind":"mcp","name":name,"source":config.to_string_lossy(),"path":config.to_string_lossy(),"protocol":if server.get("url").is_some(){"streamable_http"}else{"stdio"},"url":server.get("url"),"command":server.get("command"),"args":server.get("args")}));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(_) => {
                errors.push(serde_json::json!({"source":directory,"error":"directory_unavailable"}))
            }
        }
    }
    if request.include_listeners {
        #[cfg(windows)]
        {
            let output = tokio::process::Command::new("netstat")
                .args(["-ano", "-p", "tcp"])
                .output()
                .await
                .map_err(|_| import_error("listener_enumeration_failed".into()))?;
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                let fields: Vec<_> = line.split_whitespace().collect();
                if fields.len() >= 5 && fields[0] == "TCP" && fields[3] == "LISTENING" {
                    if let Some(port) = fields[1]
                        .rsplit(':')
                        .next()
                        .and_then(|v| v.parse::<u16>().ok())
                    {
                        let url = format!("http://127.0.0.1:{port}");
                        if !candidates
                            .iter()
                            .any(|c| c.get("url").and_then(|v| v.as_str()) == Some(url.as_str()))
                        {
                            candidates.push(serde_json::json!({"kind":"listener","name":format!("本机 TCP {port}"),"source":"Windows TCP listener","url":url,"protocol":"unverified","pid":fields[4]}));
                        }
                    }
                }
            }
        }
    }
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_millis(800))
        .build()
        .map_err(|_| import_error("local_probe_client_failed".into()))?;
    let semaphore = Arc::new(tokio::sync::Semaphore::new(8));
    let mut probes = tokio::task::JoinSet::new();
    for (index, candidate) in candidates
        .iter()
        .enumerate()
        .filter(|(_, c)| c["kind"] == "listener")
        .take(128)
    {
        let base = candidate["url"].as_str().unwrap_or_default().to_string();
        let client = client.clone();
        let semaphore = semaphore.clone();
        probes.spawn(async move {
            let _permit = semaphore.acquire_owned().await.ok()?;
            if let Ok(response) = client.get(format!("{base}/v1/models")).send().await {
                if response.status().is_success() && response.content_length().unwrap_or(0) <= 512 * 1024 {
                    if let Ok(value) = response.json::<serde_json::Value>().await {
                        if let Some(models) = value.get("data").and_then(|v|v.as_array()) {
                            let ids: Vec<String> = models.iter().filter_map(|m|m["id"].as_str().map(String::from)).take(100).collect();
                            return Some((index,"model",format!("{base}/v1"),ids));
                        }
                    }
                }
            }
            if let Ok(response) = client.post(format!("{base}/mcp")).header("Accept","application/json, text/event-stream").json(&serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"YiLian-local-discovery","version":"1.0"}}})).send().await {
                if response.status().is_success() && response.content_length().unwrap_or(0) <= 512 * 1024 {
                    if let Ok(value) = response.json::<serde_json::Value>().await {
                        if value["result"]["protocolVersion"].is_string() { return Some((index,"mcp",format!("{base}/mcp"),Vec::new())); }
                    }
                }
            }
            None
        });
    }
    while let Some(result) = probes.join_next().await {
        if let Ok(Some((index, kind, url, models))) = result {
            candidates[index]["kind"] = kind.into();
            candidates[index]["url"] = url.into();
            candidates[index]["protocol"] = if kind == "mcp" {
                "streamable_http"
            } else {
                "openai"
            }
            .into();
            candidates[index]["models"] = serde_json::json!(models);
        }
    }
    Ok(Json(
        serde_json::json!({"candidates":candidates,"errors":errors}),
    ))
}
