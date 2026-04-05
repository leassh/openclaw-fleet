use serde::Serialize;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ActivityState {
    Offline,
    Active,
    Away,
    Idle,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum LoadState {
    Unknown,
    None,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcessInfo {
    pub name: String,
    pub pid: u32,
    pub cpu_percent: f64,
    pub memory_mb: f64,
}

#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub reachable: bool,
    pub cpu_percent: Option<f64>,
    pub gpu_percent: Option<f64>,
    pub ram_used_mb: Option<u64>,
    pub ram_total_mb: Option<u64>,
    pub disk_free_gb: Option<f64>,
    pub gpu_temp_c: Option<f64>,
    pub cpu_temp_c: Option<f64>,
    pub vram_used_mb: Option<u64>,
    pub vram_total_mb: Option<u64>,
    pub idle_seconds: Option<u64>,
    pub logged_in_users: Vec<String>,
    pub top_processes: Vec<ProcessInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeState {
    pub name: String,
    pub activity: ActivityState,
    pub load: LoadState,
    pub cpu_percent: Option<f64>,
    pub gpu_percent: Option<f64>,
    pub ram_used_mb: Option<u64>,
    pub ram_total_mb: Option<u64>,
    pub disk_free_gb: Option<f64>,
    pub gpu_temp_c: Option<f64>,
    pub cpu_temp_c: Option<f64>,
    pub vram_used_mb: Option<u64>,
    pub vram_total_mb: Option<u64>,
    pub idle_seconds: Option<u64>,
    pub logged_in_users: Vec<String>,
    pub top_processes: Vec<ProcessInfo>,
    #[serde(skip_serializing)]
    pub last_probe: Option<Instant>,
    pub consecutive_failures: u32,
    pub custom_states: Vec<String>,
}

impl NodeState {
    pub fn new(name: String) -> Self {
        Self {
            name,
            activity: ActivityState::Offline,
            load: LoadState::Unknown,
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
            last_probe: None,
            consecutive_failures: 0,
            custom_states: vec![],
        }
    }

    pub fn update(
        &mut self,
        probe: &ProbeResult,
        load_low: u8,
        load_high: u8,
        idle_threshold_secs: u64,
    ) {
        self.last_probe = Some(Instant::now());

        if !probe.reachable {
            self.consecutive_failures += 1;
            if self.consecutive_failures >= 2 {
                self.activity = ActivityState::Offline;
                self.load = LoadState::Unknown;
            }
            return;
        }

        // Reachable -- reset failures and copy metrics
        self.consecutive_failures = 0;
        self.cpu_percent = probe.cpu_percent;
        self.gpu_percent = probe.gpu_percent;
        self.ram_used_mb = probe.ram_used_mb;
        self.ram_total_mb = probe.ram_total_mb;
        self.disk_free_gb = probe.disk_free_gb;
        self.gpu_temp_c = probe.gpu_temp_c;
        self.cpu_temp_c = probe.cpu_temp_c;
        self.vram_used_mb = probe.vram_used_mb;
        self.vram_total_mb = probe.vram_total_mb;
        self.idle_seconds = probe.idle_seconds;
        self.logged_in_users = probe.logged_in_users.clone();
        self.top_processes = probe.top_processes.clone();

        // Compute activity state based on idle_seconds
        self.activity = match probe.idle_seconds {
            Some(idle) if idle < 300 => ActivityState::Active,
            Some(idle) if idle >= idle_threshold_secs => ActivityState::Idle,
            Some(_) => ActivityState::Away,
            None => ActivityState::Away,
        };

        // Compute load state: max of cpu and gpu
        let max_load = match (probe.cpu_percent, probe.gpu_percent) {
            (Some(cpu), Some(gpu)) => Some(cpu.max(gpu)),
            (Some(cpu), None) => Some(cpu),
            (None, Some(gpu)) => Some(gpu),
            (None, None) => None,
        };

        self.load = match max_load {
            Some(v) if v < 5.0 => LoadState::None,
            Some(v) if v < load_low as f64 => LoadState::Low,
            Some(v) if v > load_high as f64 => LoadState::High,
            Some(_) => LoadState::Medium,
            None => LoadState::Unknown,
        };
    }
}
