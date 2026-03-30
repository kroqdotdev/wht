use crate::rate::{self, Limiter};
use crate::stats::Stats;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::{Client, Method};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{self, AsyncBufReadExt, BufReader};

pub struct SendConfig {
    pub url: String,
    pub method: Method,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub content_type: String,
}

/// Send a single request, returning (status_code, latency) or error.
async fn fire(client: &Client, cfg: &SendConfig) -> Result<(u16, Duration), (Duration, String)> {
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

fn record_result(stats: &Stats, result: Result<(u16, Duration), (Duration, String)>) {
    match result {
        Ok((status, latency)) => {
            if (200..300).contains(&status) {
                stats.record_success(latency);
            } else {
                stats.record_failure(latency, format!("HTTP {status}"));
            }
        }
        Err((latency, err)) => {
            stats.record_failure(latency, err);
        }
    }
}

/// Send a single request (default mode).
pub async fn send_single(client: &Client, cfg: &SendConfig) {
    println!(
        "{} {} {}",
        "→".bold(),
        cfg.method.as_str().cyan(),
        cfg.url
    );

    let start = Instant::now();
    match fire(client, cfg).await {
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
        Err((_latency, err)) => {
            let elapsed = start.elapsed();
            println!(
                "{} {} after {:.0}ms",
                "✗".red().bold(),
                err,
                elapsed.as_secs_f64() * 1000.0
            );
        }
    }
}

/// Send N requests with optional rate limiting and concurrency.
pub async fn send_count(
    client: &Client,
    cfg: Arc<SendConfig>,
    count: u64,
    concurrency: usize,
    limiter: Option<Limiter>,
) {
    let stats = Arc::new(Stats::new());

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

    let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));
    let mut handles = Vec::new();

    for _ in 0..count {
        rate::wait(&limiter).await;

        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let client = client.clone();
        let cfg = cfg.clone();
        let stats = stats.clone();
        let pb = pb.clone();

        let handle = tokio::spawn(async move {
            let result = fire(&client, &cfg).await;
            record_result(&stats, result);
            pb.inc(1);
            drop(permit);
        });
        handles.push(handle);
    }

    for h in handles {
        let _ = h.await;
    }

    pb.finish_and_clear();
    stats.print_summary();
}

/// Send requests for a duration with optional rate limiting and concurrency.
pub async fn send_timed(
    client: &Client,
    cfg: Arc<SendConfig>,
    duration_secs: f64,
    concurrency: usize,
    limiter: Option<Limiter>,
) {
    let stats = Arc::new(Stats::new());
    let deadline = Instant::now() + Duration::from_secs_f64(duration_secs);

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

    let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));
    let start = Instant::now();
    let mut handles = Vec::new();

    while Instant::now() < deadline {
        rate::wait(&limiter).await;

        if Instant::now() >= deadline {
            break;
        }

        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let client = client.clone();
        let cfg = cfg.clone();
        let stats = stats.clone();
        let pb_inner = pb.clone();

        let handle = tokio::spawn(async move {
            let result = fire(&client, &cfg).await;
            record_result(&stats, result);
            pb_inner.set_message(format!("{} sent", stats.total()));
            drop(permit);
        });
        handles.push(handle);

        // Update time progress
        let elapsed = start.elapsed().as_secs();
        pb.set_position(elapsed.min(duration_secs as u64));
    }

    // Wait for in-flight requests
    for h in handles {
        let _ = h.await;
    }

    pb.finish_and_clear();
    stats.print_summary();
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
            Ok(None) => break, // EOF
            Err(_) => break,
        }
    }
}
