use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

/// Maintains rolling windows of numeric metric values per node.
/// Computes linear slope (via simple linear regression) for prediction.
pub struct TrendTracker {
    windows: HashMap<(String, String), VecDeque<(Instant, f64)>>,
    window_size: usize,
}

impl TrendTracker {
    pub fn new(window_size: usize) -> Self {
        Self {
            windows: HashMap::new(),
            window_size,
        }
    }

    /// Record a metric sample for a given node.
    pub fn record(&mut self, node: &str, metric: &str, value: f64) {
        self.record_at(node, metric, value, Instant::now());
    }

    /// Record a metric sample with an explicit timestamp (useful for testing).
    pub fn record_at(&mut self, node: &str, metric: &str, value: f64, when: Instant) {
        let key = (node.to_string(), metric.to_string());
        let window = self.windows.entry(key).or_insert_with(VecDeque::new);
        window.push_back((when, value));
        while window.len() > self.window_size {
            window.pop_front();
        }
    }

    /// Compute the slope (change in metric per hour) using simple linear regression.
    /// Returns None if fewer than 2 samples.
    pub fn slope_per_hour(&self, node: &str, metric: &str) -> Option<f64> {
        let key = (node.to_string(), metric.to_string());
        let window = self.windows.get(&key)?;
        if window.len() < 2 {
            return None;
        }

        // Use first sample's time as origin for numeric stability
        let t0 = window[0].0;

        let n = window.len() as f64;
        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let mut sum_xy = 0.0;
        let mut sum_xx = 0.0;

        for (instant, value) in window {
            let x = instant.duration_since(t0).as_secs_f64();
            let y = *value;
            sum_x += x;
            sum_y += y;
            sum_xy += x * y;
            sum_xx += x * x;
        }

        let denom = n * sum_xx - sum_x * sum_x;
        if denom.abs() < 1e-12 {
            return Some(0.0);
        }

        let slope_per_second = (n * sum_xy - sum_x * sum_y) / denom;
        Some(slope_per_second * 3600.0) // convert to per-hour
    }

    /// Predict how long until the metric crosses the given threshold.
    /// Extrapolates linearly from the latest value using the computed slope.
    /// Returns None if:
    ///   - insufficient data (< 2 samples)
    ///   - slope is heading away from threshold (will never cross)
    ///   - metric has already crossed the threshold
    pub fn predict_time_to_threshold(
        &self,
        node: &str,
        metric: &str,
        threshold: f64,
    ) -> Option<Duration> {
        let slope_per_hour = self.slope_per_hour(node, metric)?;

        let key = (node.to_string(), metric.to_string());
        let window = self.windows.get(&key)?;
        let (_, current) = window.back()?;

        let diff = threshold - current;

        // If slope is effectively zero, never reaches
        if slope_per_hour.abs() < 1e-12 {
            return None;
        }

        let hours = diff / slope_per_hour;

        // Negative hours means it already passed or slope going wrong way
        if hours <= 0.0 {
            return None;
        }

        Some(Duration::from_secs_f64(hours * 3600.0))
    }

    /// Get the latest recorded value for a metric.
    pub fn latest_value(&self, node: &str, metric: &str) -> Option<f64> {
        let key = (node.to_string(), metric.to_string());
        let window = self.windows.get(&key)?;
        window.back().map(|(_, v)| *v)
    }
}
