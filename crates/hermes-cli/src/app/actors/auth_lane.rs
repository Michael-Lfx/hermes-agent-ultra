//! Dedicated worker thread for serial provider credential refresh.

use std::sync::Arc;
use std::thread::{self, JoinHandle};

use tokio::sync::{mpsc, oneshot};

use crate::app::auth_refresh::{AuthRefreshJob, AuthRefreshOutcome, run_auth_refresh};

enum AuthLaneCommand {
    Refresh {
        job: AuthRefreshJob,
        ack: oneshot::Sender<AuthRefreshOutcome>,
    },
}

/// Serializes OAuth/API-key refresh on a background worker thread.
#[derive(Clone)]
pub struct AuthLane {
    tx: mpsc::UnboundedSender<AuthLaneCommand>,
    _worker: Arc<JoinHandle<()>>,
}

impl AuthLane {
    pub fn spawn() -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let worker = thread::Builder::new()
            .name("hermes-auth-lane".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("auth lane runtime");
                rt.block_on(async move {
                    while let Some(AuthLaneCommand::Refresh { job, ack }) = rx.recv().await {
                        let outcome = run_auth_refresh(job).await;
                        let _ = ack.send(outcome);
                    }
                });
            })
            .expect("auth lane thread");
        Self {
            tx,
            _worker: Arc::new(worker),
        }
    }

    pub async fn refresh(&self, provider: String, force_refresh: bool) -> AuthRefreshOutcome {
        let (tx, rx) = oneshot::channel();
        if self
            .tx
            .send(AuthLaneCommand::Refresh {
                job: AuthRefreshJob {
                    provider,
                    force_refresh,
                },
                ack: tx,
            })
            .is_err()
        {
            return AuthRefreshOutcome::default();
        }
        rx.await.unwrap_or_default()
    }
}

impl Default for AuthLane {
    fn default() -> Self {
        Self::spawn()
    }
}
