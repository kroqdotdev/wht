use crate::events::SendEvent;
use crate::payload;
use crate::rate;
use crate::sender::{self, SendConfig};
use crate::templates;
use crate::web::state::{JobHandle, SharedState};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{Html, IntoResponse};
use axum::Json;
use reqwest::{Client, Method};
use serde::Deserialize;
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

static INDEX_HTML: &str = include_str!("../static/index.html");

pub async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

pub async fn list_templates() -> Json<Vec<&'static str>> {
    Json(templates::template_names())
}

pub async fn get_template(Path(name): Path<String>) -> Result<String, (StatusCode, String)> {
    templates::get_template(&name).map_err(|e| (StatusCode::NOT_FOUND, e))
}

#[derive(Deserialize)]
pub struct SendRequest {
    pub url: String,
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default = "default_content_type")]
    pub content_type: String,
    pub body: Option<String>,
    pub template: Option<String>,
    pub size: Option<String>,
    #[serde(default = "default_mode")]
    pub mode: String,
    pub count: Option<u64>,
    pub duration: Option<f64>,
    pub concurrency: Option<usize>,
    pub rps: Option<f64>,
}

fn default_method() -> String {
    "POST".to_string()
}
fn default_content_type() -> String {
    "application/json".to_string()
}
fn default_mode() -> String {
    "single".to_string()
}

pub async fn start_send(
    State(state): State<SharedState>,
    Json(req): Json<SendRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // Parse method
    let method: Method = req
        .method
        .to_uppercase()
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, format!("Invalid method: {}", req.method)))?;

    // Resolve body
    let body = payload::resolve_body(
        req.body.as_deref(),
        None, // no file upload in web
        req.template.as_deref(),
        req.size.as_deref(),
    )
    .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    // Auto-detect content type for form-data template
    let content_type = if req.template.as_deref() == Some("form-data")
        && req.content_type == "application/json"
    {
        "application/x-www-form-urlencoded".to_string()
    } else {
        req.content_type
    };

    let headers: Vec<(String, String)> = req.headers.into_iter().collect();

    let cfg = Arc::new(SendConfig {
        url: req.url,
        method,
        headers,
        body,
        content_type,
    });

    let concurrency = req.concurrency.unwrap_or(1).max(1);
    let rps = req.rps;

    let (event_tx, _) = broadcast::channel::<SendEvent>(4096);
    let cancel_token = CancellationToken::new();
    let job_id = Uuid::new_v4();

    state.jobs.insert(
        job_id,
        JobHandle {
            cancel_token: cancel_token.clone(),
            event_tx: event_tx.clone(),
        },
    );

    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(concurrency.max(10))
        .build()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mode = req.mode.clone();
    let state_clone = state.clone();

    tokio::spawn(async move {
        match mode.as_str() {
            "single" => {
                let result = sender::send_single_core(&client, &cfg).await;
                let (success, failed, latency_ms, errors) = match &result {
                    Ok((status, d)) => {
                        let ok = (200..300).contains(status);
                        let _ = event_tx.send(SendEvent::RequestDone {
                            status: *status,
                            latency_ms: d.as_secs_f64() * 1000.0,
                            success: ok,
                        });
                        (
                            if ok { 1 } else { 0 },
                            if ok { 0 } else { 1 },
                            d.as_secs_f64() * 1000.0,
                            if ok { vec![] } else { vec![format!("HTTP {status}")] },
                        )
                    }
                    Err((d, err)) => {
                        let _ = event_tx.send(SendEvent::RequestFailed {
                            error: err.clone(),
                            latency_ms: d.as_secs_f64() * 1000.0,
                        });
                        (0, 1, d.as_secs_f64() * 1000.0, vec![err.clone()])
                    }
                };
                let _ = event_tx.send(SendEvent::Finished {
                    summary: crate::events::StatsSummary {
                        total: 1,
                        success,
                        failed,
                        duration_secs: latency_ms / 1000.0,
                        throughput: 0.0,
                        avg_ms: latency_ms,
                        min_ms: latency_ms,
                        max_ms: latency_ms,
                        p50_ms: latency_ms,
                        p95_ms: latency_ms,
                        p99_ms: latency_ms,
                        errors,
                    },
                });
            }
            "count" => {
                let count = req.count.unwrap_or(1);
                let limiter = rate::create_limiter(rps);
                sender::send_count_core(
                    &client,
                    cfg,
                    count,
                    concurrency,
                    limiter,
                    event_tx,
                    cancel_token,
                )
                .await;
            }
            "timed" => {
                let duration = req.duration.unwrap_or(10.0);
                let limiter = rate::create_limiter(rps);
                sender::send_timed_core(
                    &client,
                    cfg,
                    duration,
                    concurrency,
                    limiter,
                    event_tx,
                    cancel_token,
                )
                .await;
            }
            _ => {
                let _ = event_tx.send(SendEvent::RequestFailed {
                    error: format!("Unknown mode: {mode}"),
                    latency_ms: 0.0,
                });
            }
        }

        // Clean up job after a delay
        tokio::time::sleep(Duration::from_secs(60)).await;
        state_clone.jobs.remove(&job_id);
    });

    Ok(Json(serde_json::json!({ "job_id": job_id.to_string() })))
}

pub async fn job_events(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let job_id: Uuid = id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid job ID".to_string()))?;

    let job = state
        .jobs
        .get(&job_id)
        .ok_or((StatusCode::NOT_FOUND, "Job not found".to_string()))?;

    let rx = job.event_tx.subscribe();
    drop(job);

    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(event) => {
            let json = serde_json::to_string(&event).ok()?;
            let event_type = match &event {
                SendEvent::Progress { .. } => "progress",
                SendEvent::RequestDone { .. } => "request_done",
                SendEvent::RequestFailed { .. } => "request_failed",
                SendEvent::Finished { .. } => "finished",
            };
            Some(Ok::<_, Infallible>(
                Event::default().event(event_type).data(json),
            ))
        }
        Err(_) => None,
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

pub async fn cancel_job(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let job_id: Uuid = id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid job ID".to_string()))?;

    let job = state
        .jobs
        .get(&job_id)
        .ok_or((StatusCode::NOT_FOUND, "Job not found".to_string()))?;

    job.cancel_token.cancel();

    Ok(Json(serde_json::json!({ "cancelled": true })))
}
