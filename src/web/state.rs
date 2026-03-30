use crate::events::SendEvent;
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub type SharedState = Arc<AppState>;

pub struct AppState {
    pub jobs: DashMap<Uuid, JobHandle>,
}

pub struct JobHandle {
    pub cancel_token: CancellationToken,
    pub event_tx: broadcast::Sender<SendEvent>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            jobs: DashMap::new(),
        }
    }
}
