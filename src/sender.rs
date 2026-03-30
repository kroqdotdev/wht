use crate::events::SendEvent;
use crate::rate::{self, Limiter};
use crate::stats::Stats;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::{Client, Method};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{self, AsyncBufReadExt, BufReader};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

pub struct SendConfig {
    pub url: String,
    pub method: Method,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub content_type: String,
}

/// Send a single request, returning (status_code, latency) or error.
pub(crate) async fn fire(
    client: &Client,
    cfg: &SendConfig,
) -> Result<(u16, Duration), (Duration, String)> {
    let start = Instant::now();

    let mut req = client.request(cfg.method.clone(), &cfg.url);
    req = req.header("Content-Type", &cfg.content_type);

    for (k, v) in &cfg.headers {
        req = req.header(k, v);
    }

    if let Some(body) = &cfg.body {
        req = req.body(body.clone());
    }

    match req.send().await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let latency = start.elapsed();
            Ok((status, latency))
        }
        Err(e) => {
            let latency = start.elapsed();
            Err((latency, e.to_string()))
        }
    }
}

pub(crate) fn record_result(stats: &Stats, result: &Result<(u16, Duration), (Duration, String)>) {
    match result {
        Ok((status, latency)) => {
            if (200..300).contains(status) {
                stats.record_success(*latency);
            } else {
                stats.record_failure(*latency, format!("HTTP {status}"));
            }
        }
        Err((latency, err)) => {
            stats.record_failure(*latency, err.clone());
        }
    }
}

fn emit_result(tx: &broadcast::Sender<SendEvent>, result: &Result<(u16, Duration), (Duration, String)>) {
    match result {
        Ok((status, latency)) => {
            let _ = tx.send(SendEvent::RequestDone {
                status: *status,
                latency_ms: latency.as_secs_f64() * 1000.0,
                success: (200..300).contains(status),
            });
        }
        Err((latency, err)) => {
            let _ = tx.send(SendEvent::RequestFailed {
                error: err.clone(),
                latency_ms: latency.as_secs_f64() * 1000.0,
            });
        }
    }
}

// ── Core variants (used by web) ─────────────────────────────────────

/// Fire a single request and return the result directly.
pub(crate) async fn send_single_core(
    client: &Client,
    cfg: &SendConfig,
) -> Result<(u16, Duration), (Duration, String)> {
    fire(client, cfg).await
}

/// Send N requests, emitting events. Supports cancellation.
pub(crate) async fn send_count_core(
    client: &Client,
    cfg: Arc<SendConfig>,
    count: u64,
    concurrency: usize,
    limiter: Option<Limiter>,
    event_tx: broadcast::Sender<SendEvent>,
    cancel: CancellationToken,
) {
    let stats = Arc::new(Stats::new());
    let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));
    let mut handles = Vec::new();
    let completed = Arc::new(std::sync::atomic::AtomicU64::new(0));

    for _ in 0..count {
        if cancel.is_cancelled() {
            break;
        }

        rate::wait(&limiter).await;

        if cancel.is_cancelled() {
            break;
        }

        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let client = client.clone();
        let cfg = cfg.clone();
        let stats = stats.clone();
        let event_tx = event_tx.clone();
        let completed = completed.clone();

        let handle = tokio::spawn(async move {
            let result = fire(&client, &cfg).await;
            record_result(&stats, &result);
            emit_result(&event_tx, &result);
            let done = completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            let _ = event_tx.send(SendEvent::Progress {
                completed: done,
                total: Some(count),
            });
            drop(permit);
        });
        handles.push(handle);
    }

    for h in handles {
        let _ = h.await;
    }

    let _ = event_tx.send(SendEvent::Finished {
        summary: stats.summary(),
    });
}

/// Send requests for a duration, emitting events. Supports cancellation.
pub(crate) async fn send_timed_core(
    client: &Client,
    cfg: Arc<SendConfig>,
    duration_secs: f64,
    concurrency: usize,
    limiter: Option<Limiter>,
    event_tx: broadcast::Sender<SendEvent>,
    cancel: CancellationToken,
) {
    let stats = Arc::new(Stats::new());
    let deadline = Instant::now() + Duration::from_secs_f64(duration_secs);
    let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));
    let completed = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let mut handles = Vec::new();

    while Instant::now() < deadline && !cancel.is_cancelled() {
        rate::wait(&limiter).await;

        if Instant::now() >= deadline || cancel.is_cancelled() {
            break;
        }

        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let client = client.clone();
        let cfg = cfg.clone();
        let stats = stats.clone();
        let event_tx = event_tx.clone();
        let completed = completed.clone();

        let handle = tokio::spawn(async move {
            let result = fire(&client, &cfg).await;
            record_result(&stats, &result);
            emit_result(&event_tx, &result);
            let done = completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            let _ = event_tx.send(SendEvent::Progress {
                completed: done,
                total: None,
            });
            drop(permit);
        });
        handles.push(handle);
    }

    for h in handles {
        let _ = h.await;
    }

    let _ = event_tx.send(SendEvent::Finished {
        summary: stats.summary(),
    });
}

// ── CLI helpers ─────────────────────────────────────────────────────

fn print_stats_summary(s: &crate::events::StatsSummary) {
    println!("\n{}", "--- Results ---".bold());
    println!("Total:      {} requests", s.total);
    println!("Success:    {}", format!("{} (2xx)", s.success).green());
    if s.failed > 0 {
        println!("Failed:     {}", format!("{}", s.failed).red());
    } else {
        println!("Failed:     {}", "0".green());
    }
    println!("Duration:   {:.1}s", s.duration_secs);
    if s.total > 0 {
        println!("Throughput: {:.1} req/s", s.throughput);
        println!(
            "Latency:    avg={:.0}ms  min={:.0}ms  max={:.0}ms  p50={:.0}ms  p95={:.0}ms  p99={:.0}ms",
            s.avg_ms, s.min_ms, s.max_ms, s.p50_ms, s.p95_ms, s.p99_ms
        );
    }
    if !s.errors.is_empty() {
        println!("\n{}", "Sample errors:".red());
        for (i, e) in s.errors.iter().enumerate() {
            println!("  {}. {}", i + 1, e);
        }
    }
}

// ── CLI wrappers (terminal output) ──────────────────────────────────

/// Send a single request (default mode) — prints to terminal.
pub async fn send_single(client: &Client, cfg: &SendConfig) {
    println!(
        "{} {} {}",
        "→".bold(),
        cfg.method.as_str().cyan(),
        cfg.url
    );

    match send_single_core(client, cfg).await {
        Ok((status, latency)) => {
            let status_str = format!("{status}");
            let colored_status = if (200..300).contains(&status) {
                status_str.green()
            } else if (400..500).contains(&status) {
                status_str.yellow()
            } else {
                status_str.red()
            };
            println!(
                "{} {} in {:.0}ms",
                "←".bold(),
                colored_status,
                latency.as_secs_f64() * 1000.0
            );
        }
        Err((latency, err)) => {
            println!(
                "{} {} after {:.0}ms",
                "✗".red().bold(),
                err,
                latency.as_secs_f64() * 1000.0
            );
        }
    }
}

/// Send N requests with optional rate limiting and concurrency — terminal progress bar.
pub async fn send_count(
    client: &Client,
    cfg: Arc<SendConfig>,
    count: u64,
    concurrency: usize,
    limiter: Option<Limiter>,
) {
    let rps_str = if limiter.is_some() {
        " (rate limited)".to_string()
    } else {
        String::new()
    };

    println!(
        "Sending {} requests to {} [{}]{} (concurrency: {})",
        count,
        cfg.url.cyan(),
        cfg.method.as_str().bold(),
        rps_str,
        concurrency
    );

    let pb = ProgressBar::new(count);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({per_sec})")
            .unwrap()
            .progress_chars("█▉▊▋▌▍▎▏  "),
    );

    let (tx, mut rx) = broadcast::channel::<SendEvent>(1024);
    let cancel = CancellationToken::new();

    let pb_clone = pb.clone();
    let progress_handle = tokio::spawn(async move {
        let mut summary = None;
        while let Ok(event) = rx.recv().await {
            match event {
                SendEvent::Progress { completed, .. } => {
                    pb_clone.set_position(completed);
                }
                SendEvent::Finished { summary: s } => {
                    summary = Some(s);
                }
                _ => {}
            }
        }
        summary
    });

    send_count_core(client, cfg, count, concurrency, limiter, tx.clone(), cancel).await;
    drop(tx);

    if let Ok(Some(summary)) = progress_handle.await {
        pb.finish_and_clear();
        print_stats_summary(&summary);
    } else {
        pb.finish_and_clear();
    }
}

/// Send requests for a duration — terminal progress bar.
pub async fn send_timed(
    client: &Client,
    cfg: Arc<SendConfig>,
    duration_secs: f64,
    concurrency: usize,
    limiter: Option<Limiter>,
) {
    println!(
        "Sending to {} [{}] for {:.0}s (concurrency: {})",
        cfg.url.cyan(),
        cfg.method.as_str().bold(),
        duration_secs,
        concurrency
    );

    let pb = ProgressBar::new(duration_secs as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {elapsed}/{eta} ({msg})")
            .unwrap()
            .progress_chars("█▉▊▋▌▍▎▏  "),
    );

    let (tx, mut rx) = broadcast::channel::<SendEvent>(1024);
    let cancel = CancellationToken::new();

    let pb_clone = pb.clone();
    let start = Instant::now();
    let dur = duration_secs;
    let progress_handle = tokio::spawn(async move {
        let mut summary = None;
        while let Ok(event) = rx.recv().await {
            match event {
                SendEvent::Progress { completed, .. } => {
                    let elapsed = start.elapsed().as_secs();
                    pb_clone.set_position(elapsed.min(dur as u64));
                    pb_clone.set_message(format!("{completed} sent"));
                }
                SendEvent::Finished { summary: s } => {
                    summary = Some(s);
                }
                _ => {}
            }
        }
        summary
    });

    send_timed_core(client, cfg, duration_secs, concurrency, limiter, tx.clone(), cancel).await;
    drop(tx);

    if let Ok(Some(summary)) = progress_handle.await {
        pb.finish_and_clear();
        print_stats_summary(&summary);
    } else {
        pb.finish_and_clear();
    }
}

/// Interactive mode: press Enter to send one request at a time.
pub async fn send_interactive(client: &Client, cfg: &SendConfig) {
    println!(
        "Interactive mode: {} {} — press {} to send, {} to quit",
        cfg.method.as_str().bold(),
        cfg.url.cyan(),
        "Enter".green().bold(),
        "q+Enter".red().bold()
    );

    let stdin = BufReader::new(io::stdin());
    let mut lines = stdin.lines();

    let mut count: u64 = 0;

    loop {
        eprint!("  [{}] > ", count + 1);

        match lines.next_line().await {
            Ok(Some(line)) => {
                if line.trim().eq_ignore_ascii_case("q") {
                    println!("Done. Sent {} request(s).", count);
                    break;
                }
                send_single(client, cfg).await;
                count += 1;
            }
            Ok(None) => break,
            Err(_) => break,
        }
    }
}
