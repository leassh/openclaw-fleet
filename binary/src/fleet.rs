use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::config::{FleetConfig, NodeConfig};
use crate::ipc::IpcMessage;
use crate::ssh::probe::NodeProber;
use crate::state::{ActivityState, NodeState};
use crate::trend::TrendTracker;
use crate::triggers::DefaultTriggers;
use crate::value_gap::ValueGapTracker;

/// Heavy process threshold: if a process is using more than this % of CPU or GPU
/// on an idle machine, it counts as a value gap opportunity.
const HEAVY_PROCESS_THRESHOLD: f64 = 50.0;
/// Machine must be idle for at least this many minutes to trigger idle-process gap.
const IDLE_MINUTES_THRESHOLD: u64 = 30;

pub struct FleetManager {
    config: FleetConfig,
    states: Arc<RwLock<HashMap<String, NodeState>>>,
    prev_states: Arc<RwLock<HashMap<String, NodeState>>>,
    trend: Arc<RwLock<TrendTracker>>,
    default_triggers: DefaultTriggers,
    value_gap: Arc<RwLock<ValueGapTracker>>,
}

impl FleetManager {
    pub fn new(config: FleetConfig) -> Self {
        let mut states = HashMap::new();
        let mut prev_states = HashMap::new();
        for node in &config.nodes {
            states.insert(node.name.clone(), NodeState::new(node.name.clone()));
            prev_states.insert(node.name.clone(), NodeState::new(node.name.clone()));
        }

        let default_triggers = DefaultTriggers::new();

        Self {
            config,
            states: Arc::new(RwLock::new(states)),
            prev_states: Arc::new(RwLock::new(prev_states)),
            trend: Arc::new(RwLock::new(TrendTracker::new(30))),
            default_triggers,
            value_gap: Arc::new(RwLock::new(ValueGapTracker::new())),
        }
    }

    /// Probe all nodes in parallel and update their states.
    pub async fn probe_all(&self) {
        let probe_config = self.config.probe_config();
        let load_thresholds = self.config.load_threshold_config();

        // Save current states as previous before probing
        {
            let states = self.states.read().await;
            let mut prev = self.prev_states.write().await;
            for (name, state) in states.iter() {
                prev.insert(name.clone(), state.clone());
            }
        }

        let mut handles = Vec::new();

        for node_config in &self.config.nodes {
            let nc = node_config.clone();
            let states = Arc::clone(&self.states);
            let idle_threshold = probe_config.idle_threshold;
            let load_low = load_thresholds.low;
            let load_high = load_thresholds.high;

            let handle = tokio::spawn(async move {
                let prober = NodeProber::new(nc.clone());
                let result = prober.probe().await;

                let mut states = states.write().await;
                if let Some(state) = states.get_mut(&nc.name) {
                    state.update(&result, load_low, load_high, idle_threshold);
                    if result.reachable {
                        info!(
                            node = %nc.name,
                            cpu = ?result.cpu_percent,
                            ram_used = ?result.ram_used_mb,
                            ram_total = ?result.ram_total_mb,
                            "Probe succeeded"
                        );
                    } else {
                        warn!(node = %nc.name, "Probe: node unreachable");
                    }
                }
            });

            handles.push(handle);
        }

        // Wait for all probes to complete
        for handle in handles {
            let _ = handle.await;
        }

        // 1. Record trends from updated states
        {
            let states = self.states.read().await;
            let mut trend = self.trend.write().await;
            for (name, state) in states.iter() {
                if let Some(v) = state.disk_free_gb {
                    trend.record(name, "disk_free_gb", v);
                }
                if let (Some(used), Some(total)) = (state.ram_used_mb, state.ram_total_mb) {
                    if total > 0 {
                        let pct = (used as f64 / total as f64) * 100.0;
                        trend.record(name, "ram_percent", pct);
                    }
                }
                if let Some(v) = state.cpu_temp_c {
                    trend.record(name, "cpu_temp", v);
                }
                if let Some(v) = state.gpu_temp_c {
                    trend.record(name, "gpu_temp", v);
                }
            }
        }

        // 2. Evaluate default triggers (compare current vs previous state)
        {
            let states = self.states.read().await;
            let prev_states = self.prev_states.read().await;
            let trend = self.trend.read().await;

            for node_config in &self.config.nodes {
                if let (Some(state), Some(prev)) = (
                    states.get(&node_config.name),
                    prev_states.get(&node_config.name),
                ) {
                    let events = self.default_triggers.evaluate(state, prev, &trend);
                    for event in &events {
                        info!(
                            node = %event.node,
                            trigger = %event.name,
                            severity = %event.severity,
                            message = %event.message,
                            "Default trigger fired"
                        );
                        // Emit IPC event
                        let _ipc_msg = IpcMessage::event(
                            format!("trigger.{}", event.name),
                            json!({
                                "node": event.node,
                                "severity": event.severity,
                                "message": event.message,
                            }),
                        );

                        // Record value gap for actionable triggers
                        let mut vg = self.value_gap.write().await;
                        vg.record_from_trigger(event, state);
                    }
                }
            }
        }

        // 3. Check for idle machines with heavy processes (value gap)
        {
            let states = self.states.read().await;
            for node_config in &self.config.nodes {
                if let Some(state) = states.get(&node_config.name) {
                    // Only check machines that are idle
                    if state.activity != ActivityState::Idle {
                        continue;
                    }
                    let idle_minutes = state.idle_seconds.unwrap_or(0) / 60;
                    if idle_minutes < IDLE_MINUTES_THRESHOLD {
                        continue;
                    }

                    // Check for heavy processes
                    for proc in &state.top_processes {
                        if proc.cpu_percent > HEAVY_PROCESS_THRESHOLD {
                            let mut vg = self.value_gap.write().await;
                            vg.record_idle_heavy_process(
                                &node_config.name,
                                &proc.name,
                                proc.cpu_percent,
                                state.gpu_percent,
                                idle_minutes,
                            );
                            break; // Only record once per node per cycle
                        }
                    }
                }
            }
        }
    }

    /// Return JSON representing the full fleet status.
    pub async fn fleet_status_json(&self) -> Value {
        let states = self.states.read().await;
        let trend = self.trend.read().await;

        let nodes: Vec<Value> = self
            .config
            .nodes
            .iter()
            .map(|nc| {
                let state = states.get(&nc.name);
                match state {
                    Some(s) => {
                        let mut v = serde_json::to_value(s).unwrap_or(json!({}));
                        if let Some(obj) = v.as_object_mut() {
                            obj.insert("host".into(), json!(nc.host));
                            obj.insert("os".into(), json!(nc.os));
                            if let Some(slope) =
                                trend.slope_per_hour(&nc.name, "disk_free_gb")
                            {
                                obj.insert(
                                    "disk_rate_gb_per_hour".into(),
                                    json!(slope),
                                );
                            }
                        }
                        v
                    }
                    None => json!({ "name": nc.name, "activity": "Offline" }),
                }
            })
            .collect();

        let total = nodes.len();
        json!({
            "nodes": nodes,
            "total": total,
        })
    }

    /// Return JSON for a single node by name.
    pub async fn node_detail_json(&self, name: &str) -> Option<Value> {
        let states = self.states.read().await;
        let state = states.get(name)?;
        let node_config = self.config.nodes.iter().find(|n| n.name == name)?;

        let trend = self.trend.read().await;

        let mut v = serde_json::to_value(state).ok()?;
        if let Some(obj) = v.as_object_mut() {
            obj.insert("host".into(), json!(node_config.host));
            obj.insert("os".into(), json!(node_config.os));
            obj.insert("ssh".into(), json!(node_config.ssh));
            obj.insert("gpu".into(), json!(node_config.gpu));
            if let Some(slope) = trend.slope_per_hour(name, "disk_free_gb") {
                obj.insert("disk_rate_gb_per_hour".into(), json!(slope));
            }
        }
        Some(v)
    }

    /// Return trend data for a node as JSON.
    pub async fn node_trend_json(&self, name: &str) -> Option<Value> {
        let trend = self.trend.read().await;
        let metrics = ["disk_free_gb", "ram_percent", "cpu_temp", "gpu_temp"];

        let mut data = serde_json::Map::new();
        data.insert("node".into(), json!(name));

        let mut has_any = false;
        for metric in &metrics {
            if let Some(slope) = trend.slope_per_hour(name, metric) {
                has_any = true;
                let mut metric_data = serde_json::Map::new();
                metric_data.insert("slope_per_hour".into(), json!(slope));
                if let Some(latest) = trend.latest_value(name, metric) {
                    metric_data.insert("latest".into(), json!(latest));
                }
                data.insert(metric.to_string(), Value::Object(metric_data));
            }
        }

        if !has_any {
            let exists = self.config.nodes.iter().any(|n| n.name == name);
            if !exists {
                return None;
            }
        }

        Some(Value::Object(data))
    }

    /// Return the value gap tracker data as JSON.
    pub async fn value_gap_json(&self) -> Value {
        let vg = self.value_gap.read().await;
        serde_json::to_value(&*vg).unwrap_or(json!({}))
    }

    /// Find a node's config by name.
    pub fn find_node_config(&self, name: &str) -> Option<&NodeConfig> {
        self.config.nodes.iter().find(|n| n.name == name)
    }
}
