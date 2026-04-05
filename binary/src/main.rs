use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{error, info};

use openclaw_fleet::config::FleetConfig;
use openclaw_fleet::fleet::FleetManager;
use openclaw_fleet::ipc::{JsonRpcRequest, JsonRpcResponse};

#[tokio::main]
async fn main() -> Result<()> {
    // Init tracing to stderr so stdout stays clean for IPC
    tracing_subscriber::fmt().with_writer(std::io::stderr).init();

    // Read config path from argv[1], default to "fleet.yaml"
    let config_path = std::env::args().nth(1).unwrap_or_else(|| "fleet.yaml".into());

    info!(config = %config_path, "Loading fleet configuration");
    let config = FleetConfig::from_file(Path::new(&config_path))
        .context("Failed to load fleet config")?;

    let metrics_interval = config.probe_config().metrics_interval;
    let manager = std::sync::Arc::new(FleetManager::new(config));

    // Spawn probe loop
    let probe_manager = std::sync::Arc::clone(&manager);
    tokio::spawn(async move {
        loop {
            info!("Starting probe cycle");
            probe_manager.probe_all().await;
            info!("Probe cycle complete, sleeping {}s", metrics_interval);
            tokio::time::sleep(Duration::from_secs(metrics_interval)).await;
        }
    });

    // IPC loop: read JSON-RPC from stdin, write responses to stdout
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let reader = BufReader::new(stdin);
    let mut lines = reader.lines();

    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<JsonRpcRequest>(&line) {
            Ok(req) => handle_request(&manager, req).await,
            Err(e) => {
                error!(error = %e, "Failed to parse JSON-RPC request");
                JsonRpcResponse::error(0, -32700, format!("Parse error: {e}"))
            }
        };

        let json = serde_json::to_string(&response).unwrap_or_else(|_| {
            r#"{"jsonrpc":"2.0","error":{"code":-32603,"message":"Internal error"},"id":0}"#
                .to_string()
        });

        let _ = stdout.write_all(json.as_bytes()).await;
        let _ = stdout.write_all(b"\n").await;
        let _ = stdout.flush().await;
    }

    Ok(())
}

async fn handle_request(
    manager: &FleetManager,
    req: JsonRpcRequest,
) -> JsonRpcResponse {
    match req.method.as_str() {
        "ping" => JsonRpcResponse::success(req.id, json!("pong")),

        "get_fleet_status" => {
            let status = manager.fleet_status_json().await;
            JsonRpcResponse::success(req.id, status)
        }

        "get_node_detail" => {
            let node_name = req
                .params
                .as_ref()
                .and_then(|p| p.get("node"))
                .and_then(|v| v.as_str());

            match node_name {
                Some(name) => match manager.node_detail_json(name).await {
                    Some(detail) => JsonRpcResponse::success(req.id, detail),
                    None => JsonRpcResponse::error(
                        req.id,
                        -32602,
                        format!("Node '{}' not found", name),
                    ),
                },
                None => JsonRpcResponse::error(
                    req.id,
                    -32602,
                    "Missing 'node' parameter".to_string(),
                ),
            }
        }

        "get_trend" => {
            let node_name = req
                .params
                .as_ref()
                .and_then(|p| p.get("node"))
                .and_then(|v| v.as_str());

            match node_name {
                Some(name) => match manager.node_trend_json(name).await {
                    Some(detail) => JsonRpcResponse::success(req.id, detail),
                    None => JsonRpcResponse::error(
                        req.id,
                        -32602,
                        format!("Node '{}' not found", name),
                    ),
                },
                None => JsonRpcResponse::error(
                    req.id,
                    -32602,
                    "Missing 'node' parameter".to_string(),
                ),
            }
        }

        "get_value_gap" => {
            let gap = manager.value_gap_json().await;
            JsonRpcResponse::success(req.id, gap)
        }

        _ => JsonRpcResponse::error(
            req.id,
            -32601,
            format!("Method '{}' not found", req.method),
        ),
    }
}
