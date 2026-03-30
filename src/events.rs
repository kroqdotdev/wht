use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type")]
pub enum SendEvent {
    #[serde(rename = "progress")]
    Progress {
        completed: u64,
        total: Option<u64>,
    },
    #[serde(rename = "request_done")]
    RequestDone {
        status: u16,
        latency_ms: f64,
        success: bool,
    },
    #[serde(rename = "request_failed")]
    RequestFailed {
        error: String,
        latency_ms: f64,
    },
    #[serde(rename = "finished")]
    Finished {
        summary: StatsSummary,
    },
}

#[derive(Clone, Debug, Serialize)]
pub struct StatsSummary {
    pub total: u64,
    pub success: u64,
    pub failed: u64,
    pub duration_secs: f64,
    pub throughput: f64,
    pub avg_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub errors: Vec<String>,
}
