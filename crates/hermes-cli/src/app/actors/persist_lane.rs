//! Background actor lane for session snapshot disk writes.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use hermes_core::AgentError;

pub struct SessionSnapshotWrite {
    pub path: PathBuf,
    pub body: String,
    pub ack: Option<mpsc::Sender<Result<(), AgentError>>>,
}

enum PersistLaneCommand {
    Write(SessionSnapshotWrite),
}

/// Offloads snapshot JSON writes so the interactive loop does not block on fs I/O.
#[derive(Clone)]
pub struct PersistLane {
    tx: Arc<mpsc::Sender<PersistLaneCommand>>,
    _worker: Arc<JoinHandle<()>>,
}

impl PersistLane {
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("hermes-persist-lane".into())
            .spawn(move || {
                while let Ok(PersistLaneCommand::Write(job)) = rx.recv() {
                    let result = write_snapshot(&job.path, &job.body);
                    if let Some(ack) = job.ack {
                        let _ = ack.send(result);
                    }
                }
            })
            .expect("persist lane thread");
        Self {
            tx: Arc::new(tx),
            _worker: Arc::new(worker),
        }
    }

    pub fn enqueue(&self, path: PathBuf, body: String) {
        let _ = self
            .tx
            .send(PersistLaneCommand::Write(SessionSnapshotWrite {
                path,
                body,
                ack: None,
            }));
    }

    pub fn write_blocking(&self, path: PathBuf, body: String) -> Result<(), AgentError> {
        let (ack_tx, ack_rx) = mpsc::channel();
        self.tx
            .send(PersistLaneCommand::Write(SessionSnapshotWrite {
                path,
                body,
                ack: Some(ack_tx),
            }))
            .map_err(|_| AgentError::Io("persist lane closed".to_string()))?;
        ack_rx
            .recv()
            .map_err(|_| AgentError::Io("persist lane dropped ack".to_string()))?
    }
}

pub(crate) fn write_snapshot(path: &std::path::Path, body: &str) -> Result<(), AgentError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            AgentError::Io(format!(
                "Failed to create snapshot directory '{}': {}",
                parent.display(),
                e
            ))
        })?;
    }
    std::fs::write(path, format!("{body}\n")).map_err(|e| {
        AgentError::Io(format!(
            "Failed to write session snapshot '{}': {}",
            path.display(),
            e
        ))
    })
}
