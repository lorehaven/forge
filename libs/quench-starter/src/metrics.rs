use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Metrics for inter-service HTTP calls
#[derive(Clone)]
pub struct RequestMetrics {
    request_count: Arc<AtomicU64>,
    error_count: Arc<AtomicU64>,
    total_latency_ms: Arc<AtomicU64>,
    last_error: Arc<std::sync::Mutex<Option<String>>>,
    last_error_time: Arc<std::sync::Mutex<Option<Instant>>>,
}

impl RequestMetrics {
    pub fn new() -> Self {
        Self {
            request_count: Arc::new(AtomicU64::new(0)),
            error_count: Arc::new(AtomicU64::new(0)),
            total_latency_ms: Arc::new(AtomicU64::new(0)),
            last_error: Arc::new(std::sync::Mutex::new(None)),
            last_error_time: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Record a successful request with latency
    pub fn record_success(&self, latency_ms: u64) {
        self.request_count.fetch_add(1, Ordering::Relaxed);
        self.total_latency_ms
            .fetch_add(latency_ms, Ordering::Relaxed);
    }

    /// Record a failed request
    pub fn record_error(&self, error_msg: &str) {
        self.request_count.fetch_add(1, Ordering::Relaxed);
        self.error_count.fetch_add(1, Ordering::Relaxed);

        let mut last_error = self.last_error.lock().unwrap();
        *last_error = Some(error_msg.to_string());

        let mut last_error_time = self.last_error_time.lock().unwrap();
        *last_error_time = Some(Instant::now());
    }

    /// Get current metrics snapshot
    pub fn snapshot(&self) -> MetricsSnapshot {
        let total_requests = self.request_count.load(Ordering::Relaxed);
        let error_count = self.error_count.load(Ordering::Relaxed);
        let total_latency_ms = self.total_latency_ms.load(Ordering::Relaxed);

        let avg_latency_ms = total_latency_ms.checked_div(total_requests).unwrap_or(0);

        let error_rate = if total_requests > 0 {
            (error_count as f64 / total_requests as f64) * 100.0
        } else {
            0.0
        };

        MetricsSnapshot {
            total_requests,
            error_count,
            success_count: total_requests.saturating_sub(error_count),
            avg_latency_ms,
            error_rate,
            last_error: self.last_error.lock().ok().and_then(|e| e.clone()),
            last_error_time: self.last_error_time.lock().ok().and_then(|t| *t),
        }
    }

    /// Reset metrics
    pub fn reset(&self) {
        self.request_count.store(0, Ordering::Relaxed);
        self.error_count.store(0, Ordering::Relaxed);
        self.total_latency_ms.store(0, Ordering::Relaxed);
        let mut last_error = self.last_error.lock().unwrap();
        *last_error = None;
        let mut last_error_time = self.last_error_time.lock().unwrap();
        *last_error_time = None;
    }
}

impl Default for RequestMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of metrics at a point in time
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub total_requests: u64,
    pub error_count: u64,
    pub success_count: u64,
    pub avg_latency_ms: u64,
    pub error_rate: f64,
    pub last_error: Option<String>,
    pub last_error_time: Option<Instant>,
}

impl MetricsSnapshot {
    pub fn to_prometheus(&self, metric_name: &str) -> String {
        format!(
            r#"# HELP {}_total_requests Total number of requests
# TYPE {}_total_requests counter
{}_total_requests {{}} {}

# HELP {}_errors Total number of failed requests
# TYPE {}_errors counter
{}_errors {{}} {}

# HELP {}_avg_latency_ms Average request latency in milliseconds
# TYPE {}_avg_latency_ms gauge
{}_avg_latency_ms {{}} {}

# HELP {}_error_rate Error rate as a percentage
# TYPE {}_error_rate gauge
{}_error_rate {{}} {:.2}
"#,
            metric_name,
            metric_name,
            metric_name,
            self.total_requests,
            metric_name,
            metric_name,
            metric_name,
            self.error_count,
            metric_name,
            metric_name,
            metric_name,
            self.avg_latency_ms,
            metric_name,
            metric_name,
            metric_name,
            self.error_rate
        )
    }
}

/// Timed block for measuring operation duration
pub struct TimedBlock {
    start: Instant,
}

impl TimedBlock {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }
}

impl Default for TimedBlock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_metrics_success() {
        let metrics = RequestMetrics::new();
        metrics.record_success(100);
        metrics.record_success(200);

        let snap = metrics.snapshot();
        assert_eq!(snap.total_requests, 2);
        assert_eq!(snap.success_count, 2);
        assert_eq!(snap.error_count, 0);
        assert_eq!(snap.avg_latency_ms, 150); // (100 + 200) / 2
    }

    #[test]
    fn test_metrics_error() {
        let metrics = RequestMetrics::new();
        metrics.record_success(100);
        metrics.record_error("Connection timeout");

        let snap = metrics.snapshot();
        assert_eq!(snap.total_requests, 2);
        assert_eq!(snap.success_count, 1);
        assert_eq!(snap.error_count, 1);
        assert!(snap.last_error.as_ref().unwrap().contains("timeout"));
    }

    #[test]
    fn test_timed_block() {
        let block = TimedBlock::new();
        std::thread::sleep(Duration::from_millis(10));
        assert!(block.elapsed_ms() >= 10);
    }
}
