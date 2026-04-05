use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use russh::client;
use russh::keys::{load_secret_key, PrivateKeyWithHashAlg};
use russh::ChannelMsg;
use tracing::{debug, warn};

use crate::config::NodeConfig;
use crate::ssh::commands;
use crate::state::ProbeResult;

/// Minimal SSH client handler that accepts all host keys.
struct SshHandler;

impl client::Handler for SshHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

/// Parses an SSH target string like `user@host:port` or `user@host`.
fn parse_ssh_target(ssh: &str) -> (String, String, u16) {
    let (user, rest) = if let Some(at_pos) = ssh.find('@') {
        (ssh[..at_pos].to_string(), &ssh[at_pos + 1..])
    } else {
        ("root".to_string(), ssh)
    };

    let (host, port) = if let Some(colon_pos) = rest.rfind(':') {
        let port_str = &rest[colon_pos + 1..];
        if let Ok(p) = port_str.parse::<u16>() {
            (rest[..colon_pos].to_string(), p)
        } else {
            (rest.to_string(), 22)
        }
    } else {
        (rest.to_string(), 22)
    };

    (user, host, port)
}

/// Execute a single command on an established SSH session and return stdout.
async fn exec_command(
    session: &mut client::Handle<SshHandler>,
    command: &str,
) -> Result<String> {
    let mut channel = session
        .channel_open_session()
        .await
        .context("Failed to open SSH session channel")?;

    channel
        .exec(true, command)
        .await
        .context("Failed to exec command")?;

    let mut output = Vec::new();

    loop {
        let Some(msg) = channel.wait().await else {
            break;
        };

        match msg {
            ChannelMsg::Data { ref data } => {
                output.extend_from_slice(data);
            }
            ChannelMsg::ExitStatus { .. } => {}
            ChannelMsg::Eof => {}
            _ => {}
        }
    }

    Ok(String::from_utf8_lossy(&output).to_string())
}

/// Find and return available SSH key paths.
fn ssh_key_paths() -> Vec<std::path::PathBuf> {
    let ssh_dir = match dirs::home_dir() {
        Some(home) => home.join(".ssh"),
        None => return vec![],
    };

    let candidates = ["id_ed25519", "id_rsa", "id_ecdsa"];
    candidates
        .iter()
        .map(|name| ssh_dir.join(name))
        .filter(|p| p.exists())
        .collect()
}

/// Probes a node via SSH and returns system metrics.
pub struct NodeProber {
    config: NodeConfig,
}

impl NodeProber {
    pub fn new(config: NodeConfig) -> Self {
        Self { config }
    }

    /// Connect and authenticate to the remote node.
    async fn connect(&self) -> Result<client::Handle<SshHandler>> {
        let (user, host, port) = parse_ssh_target(&self.config.ssh);
        debug!(
            node = %self.config.name,
            user = %user,
            host = %host,
            port = port,
            "Connecting via SSH"
        );

        let ssh_config = client::Config {
            inactivity_timeout: Some(Duration::from_secs(15)),
            ..Default::default()
        };

        let handler = SshHandler;
        let mut session =
            client::connect(Arc::new(ssh_config), (host.as_str(), port), handler)
                .await
                .context("SSH connection failed")?;

        // Try SSH keys in order
        let key_paths = ssh_key_paths();
        let mut authenticated = false;

        for key_path in &key_paths {
            debug!(key = %key_path.display(), "Trying SSH key");
            match load_secret_key(key_path, None) {
                Ok(key_pair) => {
                    let best_hash = session
                        .best_supported_rsa_hash()
                        .await
                        .ok()
                        .flatten()
                        .flatten();

                    let auth_res = session
                        .authenticate_publickey(
                            &user,
                            PrivateKeyWithHashAlg::new(Arc::new(key_pair), best_hash),
                        )
                        .await;

                    match auth_res {
                        Ok(res) if res.success() => {
                            debug!(key = %key_path.display(), "Authentication succeeded");
                            authenticated = true;
                            break;
                        }
                        Ok(_) => {
                            debug!(key = %key_path.display(), "Key rejected by server");
                        }
                        Err(e) => {
                            debug!(key = %key_path.display(), error = %e, "Auth attempt failed");
                        }
                    }
                }
                Err(e) => {
                    debug!(key = %key_path.display(), error = %e, "Failed to load key");
                }
            }
        }

        if !authenticated {
            anyhow::bail!("No SSH key accepted for {}", self.config.ssh);
        }

        Ok(session)
    }

    /// Probe the node, returning metrics. On any failure, returns unreachable.
    pub async fn probe(&self) -> ProbeResult {
        match self.probe_inner().await {
            Ok(result) => result,
            Err(e) => {
                warn!(
                    node = %self.config.name,
                    error = %e,
                    "Probe failed, marking unreachable"
                );
                unreachable_result()
            }
        }
    }

    async fn probe_inner(&self) -> Result<ProbeResult> {
        let mut session = self.connect().await?;

        let os = self.config.os.as_deref().unwrap_or("linux");
        let cmds = commands::commands_for_os(os);

        // Run all probe commands
        let cpu_out = exec_command(&mut session, cmds.cpu).await.unwrap_or_default();
        let ram_out = exec_command(&mut session, cmds.ram).await.unwrap_or_default();
        let disk_out = exec_command(&mut session, cmds.disk).await.unwrap_or_default();
        let gpu_util_out = exec_command(&mut session, cmds.gpu_util).await.unwrap_or_default();
        let gpu_vram_out = exec_command(&mut session, cmds.gpu_vram).await.unwrap_or_default();
        let gpu_temp_out = exec_command(&mut session, cmds.gpu_temp).await.unwrap_or_default();
        let processes_out = exec_command(&mut session, cmds.processes).await.unwrap_or_default();
        let idle_out = exec_command(&mut session, cmds.idle_time).await.unwrap_or_default();
        let logged_in_out = exec_command(&mut session, cmds.logged_in).await.unwrap_or_default();

        // Disconnect cleanly
        let _ = session
            .disconnect(russh::Disconnect::ByApplication, "", "en")
            .await;

        // Parse based on OS
        let (cpu_percent, ram, disk_free_gb, idle_seconds, logged_in_users, top_processes) =
            match os.to_lowercase().as_str() {
                "windows" | "win" => {
                    let cpu = commands::parse_cpu_windows(&cpu_out);
                    let ram = commands::parse_ram_windows(&ram_out);
                    let disk = commands::parse_disk_windows(&disk_out);
                    let idle = commands::parse_idle_windows(&idle_out);
                    let users = parse_query_user(&logged_in_out);
                    let procs = commands::parse_processes_windows(&processes_out);
                    (cpu, ram, disk, idle, users, procs)
                }
                "macos" | "darwin" | "mac" => {
                    let cpu = parse_cpu_macos(&cpu_out);
                    let ram = parse_ram_macos(&ram_out);
                    let disk = commands::parse_disk_linux(&disk_out);
                    let idle = commands::parse_idle_macos(&idle_out);
                    let users = commands::parse_who_unix(&logged_in_out);
                    let procs = commands::parse_processes_linux(&processes_out);
                    (cpu, ram, disk, idle, users, procs)
                }
                _ => {
                    let cpu = commands::parse_cpu_linux(&cpu_out);
                    let ram = commands::parse_ram_linux(&ram_out);
                    let disk = commands::parse_disk_linux(&disk_out);
                    let idle = parse_idle_linux(&idle_out);
                    let users = commands::parse_who_unix(&logged_in_out);
                    let procs = commands::parse_processes_linux(&processes_out);
                    (cpu, ram, disk, idle, users, procs)
                }
            };

        // Parse GPU outputs (nvidia-smi format is the same across OSes)
        let gpu_percent = commands::parse_nvidia_gpu(&gpu_util_out);
        let gpu_temp_c = commands::parse_nvidia_gpu(&gpu_temp_out);
        let (vram_used_mb, vram_total_mb) = parse_nvidia_vram(&gpu_vram_out);

        let (ram_used_mb, ram_total_mb) = match ram {
            Some((used, total)) => (Some(used), Some(total)),
            None => (None, None),
        };

        Ok(ProbeResult {
            reachable: true,
            cpu_percent,
            gpu_percent,
            ram_used_mb,
            ram_total_mb,
            disk_free_gb,
            gpu_temp_c,
            cpu_temp_c: None,
            vram_used_mb,
            vram_total_mb,
            idle_seconds,
            logged_in_users,
            top_processes,
        })
    }
}

fn unreachable_result() -> ProbeResult {
    ProbeResult {
        reachable: false,
        cpu_percent: None,
        gpu_percent: None,
        ram_used_mb: None,
        ram_total_mb: None,
        disk_free_gb: None,
        gpu_temp_c: None,
        cpu_temp_c: None,
        vram_used_mb: None,
        vram_total_mb: None,
        idle_seconds: None,
        logged_in_users: vec![],
        top_processes: vec![],
    }
}

/// Parse nvidia-smi VRAM output: "used, total".
fn parse_nvidia_vram(output: &str) -> (Option<u64>, Option<u64>) {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return (None, None);
    }
    let parts: Vec<&str> = trimmed.split(',').collect();
    if parts.len() >= 2 {
        let used = parts[0].trim().parse().ok();
        let total = parts[1].trim().parse().ok();
        (used, total)
    } else {
        (None, None)
    }
}

/// Parse Linux uptime for idle.
fn parse_idle_linux(output: &str) -> Option<u64> {
    let parts: Vec<&str> = output.trim().split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }
    parts[0].parse::<f64>().ok().map(|v| v as u64)
}

/// Parse macOS `top -l 1` CPU usage line.
fn parse_cpu_macos(output: &str) -> Option<f64> {
    let line = output.lines().find(|l| l.contains("CPU usage"))?;
    let idle_part = line.split(',').find(|p| p.contains("idle"))?;
    let idle_str = idle_part.trim().split('%').next()?.trim();
    let idle: f64 = idle_str.parse().ok()?;
    Some(100.0 - idle)
}

/// Parse macOS `vm_stat` output for RAM usage.
fn parse_ram_macos(output: &str) -> Option<(u64, u64)> {
    let mut page_size: u64 = 16384;
    let mut pages_free: u64 = 0;
    let mut pages_active: u64 = 0;
    let mut pages_inactive: u64 = 0;
    let mut pages_speculative: u64 = 0;
    let mut pages_wired: u64 = 0;

    for line in output.lines() {
        if line.contains("page size of") {
            if let Some(num) = line
                .split("page size of ")
                .nth(1)
                .and_then(|s| s.split_whitespace().next())
                .and_then(|s| s.parse().ok())
            {
                page_size = num;
            }
        }
        let val = |l: &str| -> Option<u64> {
            l.split(':')
                .nth(1)?
                .trim()
                .trim_end_matches('.')
                .parse()
                .ok()
        };
        if line.starts_with("Pages free:") {
            pages_free = val(line).unwrap_or(0);
        } else if line.starts_with("Pages active:") {
            pages_active = val(line).unwrap_or(0);
        } else if line.starts_with("Pages inactive:") {
            pages_inactive = val(line).unwrap_or(0);
        } else if line.starts_with("Pages speculative:") {
            pages_speculative = val(line).unwrap_or(0);
        } else if line.starts_with("Pages wired") {
            pages_wired = val(line).unwrap_or(0);
        }
    }

    let total_pages = pages_free + pages_active + pages_inactive + pages_speculative + pages_wired;
    if total_pages == 0 {
        return None;
    }
    let total_mb = total_pages * page_size / (1024 * 1024);
    let used_pages = pages_active + pages_wired;
    let used_mb = used_pages * page_size / (1024 * 1024);
    Some((used_mb, total_mb))
}

/// Parse `query user` output (Windows) for logged-in usernames.
fn parse_query_user(output: &str) -> Vec<String> {
    output
        .lines()
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| {
            let first = l.split_whitespace().next()?;
            Some(first.trim_start_matches('>').to_string())
        })
        .collect()
}
