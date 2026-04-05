use std::collections::HashMap;

use serde::Serialize;

use crate::state::NodeState;
use crate::triggers::TriggerEvent;

/// Tracks situations where Leassh Pro would have taken automated action
/// but the free edition can only observe and report.
#[derive(Debug, Clone, Serialize, Default)]
pub struct ValueGapTracker {
    pub total_missed_actions: u64,
    pub missed_by_category: HashMap<String, u64>,
    pub recent_examples: Vec<ValueGapExample>,
}

/// A specific example of a missed automation opportunity.
#[derive(Debug, Clone, Serialize)]
pub struct ValueGapExample {
    pub timestamp: u64,
    pub node: String,
    pub what_happened: String,
    pub what_pro_would_do: String,
}

const MAX_RECENT_EXAMPLES: usize = 10;

impl ValueGapTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a value gap from a trigger event. Determines what Pro would have
    /// done for each trigger type and logs it.
    pub fn record_from_trigger(&mut self, event: &TriggerEvent, node_state: &NodeState) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let (category, what_happened, what_pro_would_do) = match event.name.as_str() {
            "node_offline" => (
                "service_restart".to_string(),
                format!("{}: node went offline", event.node),
                "Leassh Pro would fire a webhook to your alerting system and attempt automatic recovery".to_string(),
            ),
            "disk_critically_low" => (
                "disk_cleanup".to_string(),
                format!(
                    "{}: disk critically low ({:.1} GB free)",
                    event.node,
                    node_state.disk_free_gb.unwrap_or(0.0)
                ),
                "Leassh Pro would fire a webhook to your cleanup script automatically".to_string(),
            ),
            "disk_filling_fast" | "disk_steady_drain" => (
                "disk_cleanup".to_string(),
                event.message.clone(),
                "Leassh Pro would fire a webhook to trigger disk cleanup before it becomes critical".to_string(),
            ),
            "ram_exhaustion" => (
                "process_management".to_string(),
                format!("{}: RAM exhaustion detected", event.node),
                "Leassh Pro would kill the heaviest non-essential process to free memory".to_string(),
            ),
            "temperature_critical" => (
                "thermal_management".to_string(),
                event.message.clone(),
                "Leassh Pro would throttle GPU workloads or send a Telegram alert to take immediate action".to_string(),
            ),
            _ => return, // info-level triggers (login, back_online) don't generate value gap
        };

        self.total_missed_actions += 1;
        *self.missed_by_category.entry(category).or_insert(0) += 1;

        let example = ValueGapExample {
            timestamp: now,
            node: event.node.clone(),
            what_happened,
            what_pro_would_do,
        };

        self.recent_examples.push(example);
        if self.recent_examples.len() > MAX_RECENT_EXAMPLES {
            self.recent_examples.remove(0);
        }
    }

    /// Record an idle-machine heavy-process value gap (not from a trigger).
    pub fn record_idle_heavy_process(
        &mut self,
        node_name: &str,
        process_name: &str,
        cpu_percent: f64,
        gpu_percent: Option<f64>,
        idle_minutes: u64,
    ) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let resource = if let Some(gpu) = gpu_percent {
            if gpu > cpu_percent {
                format!("{:.0}% GPU", gpu)
            } else {
                format!("{:.0}% CPU", cpu_percent)
            }
        } else {
            format!("{:.0}% CPU", cpu_percent)
        };

        let what_happened = format!(
            "{}: {} using {}, idle {}min",
            node_name, process_name, resource, idle_minutes
        );
        let what_pro_would_do = format!(
            "Leassh Pro would kill {} and free the resources for your workloads",
            process_name
        );

        self.total_missed_actions += 1;
        *self
            .missed_by_category
            .entry("idle_process_kill".to_string())
            .or_insert(0) += 1;

        let example = ValueGapExample {
            timestamp: now,
            node: node_name.to_string(),
            what_happened,
            what_pro_would_do,
        };

        self.recent_examples.push(example);
        if self.recent_examples.len() > MAX_RECENT_EXAMPLES {
            self.recent_examples.remove(0);
        }
    }
}
