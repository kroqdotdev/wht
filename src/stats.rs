use colored::Colorize;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct Stats {
    inner: Mutex<StatsInner>,
    started: Instant,
}

struct StatsInner {
    success: u64,
    failed: u64,
    latencies: Vec<Duration>,
    errors: Vec<String>,
}

impl Stats {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(StatsInner {
                success: 0,
                failed: 0,
                latencies: Vec::new(),
                errors: Vec::new(),
            }),
            started: Instant::now(),
        }
    }

    pub fn record_success(&self, latency: Duration) {
        let mut inner = self.inner.lock().unwrap();
        inner.success += 1;
        inner.latencies.push(latency);
    }

    pub fn record_failure(&self, latency: Duration, error: String) {
        let mut inner = self.inner.lock().unwrap();
        inner.failed += 1;
        inner.latencies.push(latency);
        if inner.errors.len() < 10 {
            inner.errors.push(error);
        }
    }

    pub fn total(&self) -> u64 {
        let inner = self.inner.lock().unwrap();
        inner.success + inner.failed
    }

    pub fn print_summary(&self) {
        let inner = self.inner.lock().unwrap();
        let elapsed = self.started.elapsed();
        let total = inner.success + inner.failed;

        println!("\n{}", "--- Results ---".bold());
        println!("Total:      {} requests", total);
        println!(
            "Success:    {}",
            format!("{} (2xx)", inner.success).green()
        );
        if inner.failed > 0 {
            println!("Failed:     {}", format!("{}", inner.failed).red());
        } else {
            println!("Failed:     {}", "0".green());
        }
        println!("Duration:   {:.1}s", elapsed.as_secs_f64());

        if total > 0 && elapsed.as_secs_f64() > 0.0 {
            println!(
                "Throughput: {:.1} req/s",
                total as f64 / elapsed.as_secs_f64()
            );
        }

        if !inner.latencies.is_empty() {
            let mut sorted: Vec<f64> = inner
                .latencies
                .iter()
                .map(|d| d.as_secs_f64() * 1000.0)
                .collect();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

            let avg: f64 = sorted.iter().sum::<f64>() / sorted.len() as f64;
            let min = sorted[0];
            let max = sorted[sorted.len() - 1];
            let p50 = percentile(&sorted, 50.0);
            let p95 = percentile(&sorted, 95.0);
            let p99 = percentile(&sorted, 99.0);

            println!(
                "Latency:    avg={:.0}ms  min={:.0}ms  max={:.0}ms  p50={:.0}ms  p95={:.0}ms  p99={:.0}ms",
                avg, min, max, p50, p95, p99
            );
        }

        if !inner.errors.is_empty() {
            println!("\n{}", "Sample errors:".red());
            for (i, e) in inner.errors.iter().enumerate() {
                println!("  {}. {}", i + 1, e);
            }
        }
    }
}

fn percentile(sorted: &[f64], pct: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = (pct / 100.0 * (sorted.len() as f64 - 1.0)).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}
