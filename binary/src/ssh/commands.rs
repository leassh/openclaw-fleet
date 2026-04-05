use crate::state::ProcessInfo;

/// Shell commands to probe a remote machine for system metrics.
pub struct ProbeCommands {
    pub cpu: &'static str,
    pub ram: &'static str,
    pub disk: &'static str,
    pub gpu_util: &'static str,
    pub gpu_vram: &'static str,
    pub gpu_temp: &'static str,
    pub processes: &'static str,
    pub idle_time: &'static str,
    pub logged_in: &'static str,
}

pub const LINUX_COMMANDS: ProbeCommands = ProbeCommands {
    cpu: "head -1 /proc/stat",
    ram: "grep -E '^(MemTotal|MemAvailable):' /proc/meminfo",
    disk: "df -BG / | tail -1",
    gpu_util: "nvidia-smi --query-gpu=utilization.gpu --format=csv,noheader,nounits 2>/dev/null",
    gpu_vram: "nvidia-smi --query-gpu=memory.used,memory.total --format=csv,noheader,nounits 2>/dev/null",
    gpu_temp: "nvidia-smi --query-gpu=temperature.gpu --format=csv,noheader,nounits 2>/dev/null",
    processes: "ps aux --no-headers",
    idle_time: "cat /proc/uptime",
    logged_in: "who",
};

pub const MACOS_COMMANDS: ProbeCommands = ProbeCommands {
    cpu: "top -l 1 -n 0 | grep 'CPU usage'",
    ram: "vm_stat | head -5",
    disk: "df -g / | tail -1",
    gpu_util: "nvidia-smi --query-gpu=utilization.gpu --format=csv,noheader,nounits 2>/dev/null",
    gpu_vram: "nvidia-smi --query-gpu=memory.used,memory.total --format=csv,noheader,nounits 2>/dev/null",
    gpu_temp: "nvidia-smi --query-gpu=temperature.gpu --format=csv,noheader,nounits 2>/dev/null",
    processes: "ps aux",
    idle_time: "ioreg -c IOHIDSystem | awk '/HIDIdleTime/ {print $NF; exit}'",
    logged_in: "who",
};

pub const WINDOWS_COMMANDS: ProbeCommands = ProbeCommands {
    cpu: "powershell -NoProfile -Command \"(Get-CimInstance Win32_Processor).LoadPercentage\"",
    ram: "powershell -NoProfile -Command \"$os=Get-CimInstance Win32_OperatingSystem; \\\"$($os.TotalVisibleMemorySize) $($os.FreePhysicalMemory)\\\"\"",
    disk: "powershell -NoProfile -Command \"Get-CimInstance Win32_LogicalDisk -Filter \\\"DeviceID='C:'\\\" | ForEach-Object { [math]::Round($_.FreeSpace/1GB) }\"",
    gpu_util: "nvidia-smi --query-gpu=utilization.gpu --format=csv,noheader,nounits 2>NUL",
    gpu_vram: "nvidia-smi --query-gpu=memory.used,memory.total --format=csv,noheader,nounits 2>NUL",
    gpu_temp: "nvidia-smi --query-gpu=temperature.gpu --format=csv,noheader,nounits 2>NUL",
    processes: "powershell -NoProfile -Command \"Get-Process | Sort-Object CPU -Descending | Select-Object -First 20 | ForEach-Object { \\\"$($_.Id) $([math]::Round($_.CPU,1)) $([math]::Round($_.WorkingSet64/1MB,1)) $($_.ProcessName)\\\" }\"",
    idle_time: concat!(
        "powershell -NoProfile -Command \"",
        "Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; ",
        "public struct LASTINPUTINFO { public uint cbSize; public uint dwTime; } ",
        "public class IdleCheck { [DllImport(\\\"user32.dll\\\")] public static extern bool GetLastInputInfo(ref LASTINPUTINFO lii); ",
        "public static uint GetIdleSecs() { var lii = new LASTINPUTINFO(); lii.cbSize = (uint)Marshal.SizeOf(lii); ",
        "GetLastInputInfo(ref lii); return (uint)(Environment.TickCount - lii.dwTime) / 1000; } }'; ",
        "[IdleCheck]::GetIdleSecs()\"",
    ),
    logged_in: "query user",
};

/// Return the appropriate command set for a given OS string.
pub fn commands_for_os(os: &str) -> &'static ProbeCommands {
    match os.to_lowercase().as_str() {
        "linux" => &LINUX_COMMANDS,
        "macos" | "darwin" | "mac" => &MACOS_COMMANDS,
        "windows" | "win" => &WINDOWS_COMMANDS,
        _ => &LINUX_COMMANDS,
    }
}

// ---------------------------------------------------------------------------
// Parsers
// ---------------------------------------------------------------------------

/// Parse Linux `/proc/stat` first line into CPU usage percentage.
pub fn parse_cpu_linux(line: &str) -> Option<f64> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 5 || !parts[0].starts_with("cpu") {
        return None;
    }
    let values: Vec<u64> = parts[1..].iter().filter_map(|s| s.parse().ok()).collect();
    if values.len() < 4 {
        return None;
    }
    let total: u64 = values.iter().sum();
    let idle = values[3] + values.get(4).copied().unwrap_or(0);
    if total == 0 {
        return None;
    }
    Some((total - idle) as f64 / total as f64 * 100.0)
}

/// Parse Linux `/proc/meminfo` output for MemTotal and MemAvailable.
/// Returns (used_mb, total_mb).
pub fn parse_ram_linux(output: &str) -> Option<(u64, u64)> {
    let mut total_kb: Option<u64> = None;
    let mut available_kb: Option<u64> = None;

    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            total_kb = rest.split_whitespace().next().and_then(|s| s.parse().ok());
        } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
            available_kb = rest.split_whitespace().next().and_then(|s| s.parse().ok());
        }
    }

    let total = total_kb?;
    let available = available_kb?;
    let total_mb = total / 1024;
    let used_mb = total_mb - (available / 1024);
    Some((used_mb, total_mb))
}

/// Parse `df -BG` output for free disk space in GB.
pub fn parse_disk_linux(output: &str) -> Option<f64> {
    let line = output.trim();
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 4 {
        return None;
    }
    let avail = parts[3].trim_end_matches('G');
    avail.parse().ok()
}

/// Parse nvidia-smi GPU utilization (single number).
pub fn parse_nvidia_gpu(output: &str) -> Option<f64> {
    output.trim().parse().ok()
}

/// Parse `who` output, extracting the username from each line.
pub fn parse_who_unix(output: &str) -> Vec<String> {
    output
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| l.split_whitespace().next())
        .map(|s| s.to_string())
        .collect()
}

/// Parse macOS idle time from ioreg output (nanoseconds to seconds).
pub fn parse_idle_macos(output: &str) -> Option<u64> {
    for line in output.lines() {
        if line.contains("HIDIdleTime") {
            if let Some(after_eq) = line.split('=').last() {
                let trimmed = after_eq.trim().trim_end_matches(';');
                if let Ok(ns) = trimmed.trim().parse::<u64>() {
                    return Some(ns / 1_000_000_000);
                }
            }
            if let Some(val) = line.split_whitespace().last() {
                let cleaned = val.trim_end_matches(';');
                if let Ok(ns) = cleaned.parse::<u64>() {
                    return Some(ns / 1_000_000_000);
                }
            }
        }
    }

    let trimmed = output.trim();
    if !trimmed.is_empty() {
        if let Ok(ns) = trimmed.parse::<u64>() {
            return Some(ns / 1_000_000_000);
        }
    }

    None
}

/// Parse `ps aux --no-headers` output into ProcessInfo structs.
pub fn parse_processes_linux(output: &str) -> Vec<ProcessInfo> {
    output
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 11 {
                return None;
            }
            let pid: u32 = parts[1].parse().ok()?;
            let cpu_percent: f64 = parts[2].parse().ok()?;
            let mem_kb: f64 = parts[5].parse().ok()?;
            let name = parts[10..].join(" ");
            Some(ProcessInfo {
                name,
                pid,
                cpu_percent,
                memory_mb: mem_kb / 1024.0,
            })
        })
        .collect()
}

/// Parse Windows CPU percentage from PowerShell.
pub fn parse_cpu_windows(output: &str) -> Option<f64> {
    let values: Vec<f64> = output
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .filter_map(|l| l.parse::<f64>().ok())
        .collect();
    if values.is_empty() {
        return None;
    }
    Some(values.iter().sum::<f64>() / values.len() as f64)
}

/// Parse Windows RAM output: "total_kb free_kb" -> (used_mb, total_mb).
pub fn parse_ram_windows(output: &str) -> Option<(u64, u64)> {
    let parts: Vec<&str> = output.trim().split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }
    let total_kb: u64 = parts[0].parse().ok()?;
    let free_kb: u64 = parts[1].parse().ok()?;
    let total_mb = total_kb / 1024;
    let used_mb = total_mb - (free_kb / 1024);
    Some((used_mb, total_mb))
}

/// Parse Windows disk free space in GB.
pub fn parse_disk_windows(output: &str) -> Option<f64> {
    output.trim().parse().ok()
}

/// Parse Windows idle time in seconds.
pub fn parse_idle_windows(output: &str) -> Option<u64> {
    output.trim().parse().ok()
}

/// Parse Windows process list: "pid cpu mem name" per line.
pub fn parse_processes_windows(output: &str) -> Vec<ProcessInfo> {
    output
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 4 {
                return None;
            }
            let pid: u32 = parts[0].parse().ok()?;
            let cpu_percent: f64 = parts[1].parse().ok()?;
            let memory_mb: f64 = parts[2].parse().ok()?;
            let name = parts[3..].join(" ");
            Some(ProcessInfo {
                name,
                pid,
                cpu_percent,
                memory_mb,
            })
        })
        .collect()
}
