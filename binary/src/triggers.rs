use std::collections::HashMap;

use crate::state::NodeState;
use crate::trend::TrendTracker;

/// An event emitted when a default trigger fires.
#[derive(Debug, Clone)]
pub struct TriggerEvent {
    pub name: String,
    pub severity: String,
    pub message: String,
    pub node: String,
}

/// Built-in triggers that ship active by default.
/// Users can disable individual triggers by name.
pub struct DefaultTriggers {
    enabled: HashMap<String, bool>,
}

impl DefaultTriggers {
    /// Create with all default triggers enabled.
    pub fn new() -> Self {
        let mut enabled = HashMap::new();
        let trigger_names = [
            "node_offline",
            "node_back_online",
            "disk_critically_low",
            "disk_filling_fast",
            "disk_steady_drain",
            "ram_exhaustion",
            "temperature_critical",
            "ssh_auth_failure",
            "login_event",
        ];
        for name in &trigger_names {
            enabled.insert(name.to_string(), true);
        }
        Self { enabled }
    }

    /// Enable or disable a specific trigger.
    pub fn set_enabled(&mut self, name: &str, enabled: bool) {
        self.enabled.insert(name.to_string(), enabled);
    }

    /// Check if a trigger is enabled.
    fn is_enabled(&self, name: &str) -> bool {
        self.enabled.get(name).copied().unwrap_or(false)
    }

    /// Evaluate all enabled triggers for a single node.
    ///
    /// `prev_state` is the node state from the previous probe cycle.
    /// `trend` provides slope/prediction data for disk metrics.
    pub fn evaluate(
        &self,
        node: &NodeState,
        prev_state: &NodeState,
        trend: &TrendTracker,
    ) -> Vec<TriggerEvent> {
        let mut events = Vec::new();

        // node_offline: was reachable, now unreachable
        if self.is_enabled("node_offline") {
            let was_online = prev_state.consecutive_failures < 2;
            let now_offline = node.consecutive_failures >= 2;
            if was_online && now_offline {
                events.push(TriggerEvent {
                    name: "node_offline".to_string(),
                    severity: "critical".to_string(),
                    message: format!("Node {} is offline (unreachable)", node.name),
                    node: node.name.clone(),
                });
            }
        }

        // node_back_online: was offline, now online
        if self.is_enabled("node_back_online") {
            let was_offline = prev_state.consecutive_failures >= 2;
            let now_online = node.consecutive_failures < 2;
            if was_offline && now_online {
                events.push(TriggerEvent {
                    name: "node_back_online".to_string(),
                    severity: "info".to_string(),
                    message: format!("Node {} is back online", node.name),
                    node: node.name.clone(),
                });
            }
        }

        // disk_critically_low: disk_free_gb < 2.0
        if self.is_enabled("disk_critically_low") {
            if let Some(free) = node.disk_free_gb {
                if free < 2.0 {
                    events.push(TriggerEvent {
                        name: "disk_critically_low".to_string(),
                        severity: "critical".to_string(),
                        message: format!(
                            "Node {} disk critically low: {:.1} GB free",
                            node.name, free
                        ),
                        node: node.name.clone(),
                    });
                }
            }
        }

        // disk_filling_fast: predicted < 2GB within 1 hour
        if self.is_enabled("disk_filling_fast") {
            if let Some(free) = node.disk_free_gb {
                if let Some(time_to_critical) =
                    trend.predict_time_to_threshold(&node.name, "disk_free_gb", 2.0)
                {
                    let one_hour = std::time::Duration::from_secs(3600);
                    if time_to_critical <= one_hour && free > 2.0 {
                        events.push(TriggerEvent {
                            name: "disk_filling_fast".to_string(),
                            severity: "critical".to_string(),
                            message: format!(
                                "Node {} disk filling fast: {:.1} GB free, predicted critical in {:.0} min",
                                node.name,
                                free,
                                time_to_critical.as_secs_f64() / 60.0
                            ),
                            node: node.name.clone(),
                        });
                    }
                }
            }
        }

        // disk_steady_drain: depletion rate > 10 GB/h sustained
        if self.is_enabled("disk_steady_drain") {
            if let Some(slope) = trend.slope_per_hour(&node.name, "disk_free_gb") {
                if slope < -10.0 {
                    events.push(TriggerEvent {
                        name: "disk_steady_drain".to_string(),
                        severity: "warning".to_string(),
                        message: format!(
                            "Node {} disk draining at {:.1} GB/h",
                            node.name,
                            slope.abs()
                        ),
                        node: node.name.clone(),
                    });
                }
            }
        }

        // ram_exhaustion: ram_percent > 95
        if self.is_enabled("ram_exhaustion") {
            if let (Some(used), Some(total)) = (node.ram_used_mb, node.ram_total_mb) {
                if total > 0 {
                    let pct = (used as f64 / total as f64) * 100.0;
                    if pct > 95.0 {
                        events.push(TriggerEvent {
                            name: "ram_exhaustion".to_string(),
                            severity: "critical".to_string(),
                            message: format!(
                                "Node {} RAM exhaustion: {:.1}% used",
                                node.name, pct
                            ),
                            node: node.name.clone(),
                        });
                    }
                }
            }
        }

        // temperature_critical: gpu_temp > 90 OR cpu_temp > 95
        if self.is_enabled("temperature_critical") {
            let gpu_hot = node.gpu_temp_c.map_or(false, |t| t > 90.0);
            let cpu_hot = node.cpu_temp_c.map_or(false, |t| t > 95.0);
            if gpu_hot || cpu_hot {
                let mut parts = Vec::new();
                if let Some(gt) = node.gpu_temp_c {
                    if gt > 90.0 {
                        parts.push(format!("GPU {:.0}C", gt));
                    }
                }
                if let Some(ct) = node.cpu_temp_c {
                    if ct > 95.0 {
                        parts.push(format!("CPU {:.0}C", ct));
                    }
                }
                events.push(TriggerEvent {
                    name: "temperature_critical".to_string(),
                    severity: "warning".to_string(),
                    message: format!(
                        "Node {} temperature critical: {}",
                        node.name,
                        parts.join(", ")
                    ),
                    node: node.name.clone(),
                });
            }
        }

        // ssh_auth_failure: consecutive_failures went from 0 to > 0
        if self.is_enabled("ssh_auth_failure") {
            if prev_state.consecutive_failures == 0 && node.consecutive_failures > 0 {
                events.push(TriggerEvent {
                    name: "ssh_auth_failure".to_string(),
                    severity: "warning".to_string(),
                    message: format!(
                        "Node {} SSH probe failure (first failure)",
                        node.name
                    ),
                    node: node.name.clone(),
                });
            }
        }

        // login_event: logged_in_users changed between prev and current
        if self.is_enabled("login_event") {
            if prev_state.logged_in_users != node.logged_in_users {
                let prev_count = prev_state.logged_in_users.len();
                let curr_count = node.logged_in_users.len();
                let direction = if curr_count > prev_count {
                    "login"
                } else if curr_count < prev_count {
                    "logout"
                } else {
                    "change"
                };
                events.push(TriggerEvent {
                    name: "login_event".to_string(),
                    severity: "info".to_string(),
                    message: format!(
                        "Node {} user {}: {} -> {} users",
                        node.name, direction, prev_count, curr_count
                    ),
                    node: node.name.clone(),
                });
            }
        }

        events
    }
}
