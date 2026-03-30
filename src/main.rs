mod events;
mod payload;
mod rate;
mod sender;
mod stats;
mod templates;
mod web;

use clap::{Parser, Subcommand};
use reqwest::{Client, Method};
use sender::SendConfig;
use std::sync::Arc;
use std::time::Duration;

#[derive(Parser)]
#[command(
    name = "wht",
    about = "Webhook Tester — send HTTP requests with flexible rate, volume, and payload control",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Target URL (for direct send mode)
    url: Option<String>,

    /// HTTP method (GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS)
    #[arg(short, long, default_value = "POST")]
    method: String,

    /// Number of requests to send
    #[arg(short = 'n', long = "count")]
    count: Option<u64>,

    /// Requests per second (rate limit)
    #[arg(short, long)]
    rps: Option<f64>,

    /// Send for a duration in seconds
    #[arg(short = 't', long = "duration")]
    duration: Option<f64>,

    /// Interactive mode: press Enter to send one at a time
    #[arg(short, long)]
    interactive: bool,

    /// Number of concurrent connections
    #[arg(short, long, default_value = "1")]
    concurrency: usize,

    /// Inline request body
    #[arg(short, long)]
    data: Option<String>,

    /// Read body from file
    #[arg(short, long)]
    file: Option<String>,

    /// Use a built-in template (github-push, stripe-payment, slack-msg, json-simple, form-data)
    #[arg(long)]
    template: Option<String>,

    /// Generate a payload of this size (e.g., "1kb", "5mb", "25mb")
    #[arg(long)]
    size: Option<String>,

    /// Add a header (format: "Key: Value"), can be repeated
    #[arg(short = 'H', long = "header")]
    headers: Vec<String>,

    /// Content-Type header
    #[arg(long, default_value = "application/json")]
    content_type: String,
}

#[derive(Subcommand)]
enum Command {
    /// Launch the web GUI
    Gui {
        /// Port to listen on
        #[arg(short, long, default_value = "3000")]
        port: u16,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Handle subcommands
    if let Some(Command::Gui { port }) = cli.command {
        web::start_server(port).await;
        return;
    }

    // Direct send mode — URL is required
    let url = match cli.url {
        Some(u) => u,
        None => {
            eprintln!("Error: URL is required. Use `wht <URL>` or `wht gui` for the web interface.");
            std::process::exit(1);
        }
    };

    // Parse method
    let method: Method = cli.method.to_uppercase().parse().unwrap_or_else(|_| {
        eprintln!("Invalid HTTP method: {}", cli.method);
        std::process::exit(1);
    });

    // Handle --template list
    if cli.template.as_deref() == Some("list") {
        templates::list_templates();
        return;
    }

    // Resolve body
    let body = payload::resolve_body(
        cli.data.as_deref(),
        cli.file.as_deref(),
        cli.template.as_deref(),
        cli.size.as_deref(),
    )
    .unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        std::process::exit(1);
    });

    // Auto-detect content type for form-data template
    let content_type = if cli.template.as_deref() == Some("form-data")
        && cli.content_type == "application/json"
    {
        "application/x-www-form-urlencoded".to_string()
    } else {
        cli.content_type
    };

    // Parse headers
    let headers: Vec<(String, String)> = cli
        .headers
        .iter()
        .map(|h| {
            let parts: Vec<&str> = h.splitn(2, ':').collect();
            if parts.len() != 2 {
                eprintln!("Invalid header format (expected 'Key: Value'): {h}");
                std::process::exit(1);
            }
            (parts[0].trim().to_string(), parts[1].trim().to_string())
        })
        .collect();

    if let Some(ref body) = body {
        let size = body.len();
        let size_str = if size >= 1024 * 1024 {
            format!("{:.1}MB", size as f64 / (1024.0 * 1024.0))
        } else if size >= 1024 {
            format!("{:.1}KB", size as f64 / 1024.0)
        } else {
            format!("{size}B")
        };
        eprintln!("Payload: {size_str}");
    }

    let cfg = SendConfig {
        url,
        method,
        headers,
        body,
        content_type,
    };

    // Build HTTP client
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(cli.concurrency.max(10))
        .build()
        .expect("Failed to build HTTP client");

    // Dispatch to the right send mode
    if cli.interactive {
        sender::send_interactive(&client, &cfg).await;
    } else if let Some(duration) = cli.duration {
        let limiter = rate::create_limiter(cli.rps);
        sender::send_timed(&client, Arc::new(cfg), duration, cli.concurrency, limiter).await;
    } else if let Some(count) = cli.count {
        let limiter = rate::create_limiter(cli.rps);
        sender::send_count(&client, Arc::new(cfg), count, cli.concurrency, limiter).await;
    } else {
        // Single request mode
        sender::send_single(&client, &cfg).await;
    }
}
