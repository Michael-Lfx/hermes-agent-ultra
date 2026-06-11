use crossterm::event::{KeyEvent, MouseEvent};
use hermes_core::{AgentResult, StreamChunk};
use tokio::sync::mpsc;

use crate::app::App;

/// Events that the TUI can process.
#[derive(Debug, Clone)]
pub enum Event {
    /// A keyboard key was pressed.
    Key(KeyEvent),
    /// The terminal was resized.
    Resize(u16, u16),
    /// An asynchronous message (e.g. from agent streaming).
    Message(String),
    /// Agent produced a streaming delta.
    StreamDelta(String),
    /// Agent produced a full stream chunk (including control metadata).
    StreamChunk(StreamChunk),
    /// Agent finished processing.
    AgentDone,
    /// Background agent run completed.
    AgentRunComplete {
        result: Result<AgentResult, String>,
        elapsed_secs: f64,
    },
    /// Background app-owned run completed. Used for slash commands and
    /// managed quorum/swarm turns that must mutate App state while keeping the
    /// render loop responsive.
    ManagedAppRunComplete {
        result: Result<Box<App>, String>,
        elapsed_secs: f64,
    },
    /// Interrupt signal (Ctrl+C).
    Interrupt,
    /// External shutdown signal (SIGINT/SIGTERM/SIGHUP).
    Shutdown,
    /// Mouse interaction.
    Mouse(MouseEvent),
    /// Terminal bracketed paste payload.
    Paste(String),
}

/// A handle for sending streaming deltas to the TUI.
///
/// Clone this and pass it to the agent loop's streaming callback.
/// The TUI will accumulate deltas and display them in real time.
#[derive(Clone)]
pub struct StreamHandle {
    sender: mpsc::UnboundedSender<Event>,
}

impl StreamHandle {
    /// Send a streaming text delta to the TUI.
    pub fn send_delta(&self, text: &str) {
        let _ = self.sender.send(Event::StreamDelta(text.to_string()));
    }

    /// Send a full streaming chunk to the TUI event loop.
    pub fn send_chunk(&self, chunk: StreamChunk) {
        let _ = self.sender.send(Event::StreamChunk(chunk));
    }

    /// Signal that the agent has finished.
    pub fn send_done(&self) {
        let _ = self.sender.send(Event::AgentDone);
    }
}

impl From<mpsc::UnboundedSender<Event>> for StreamHandle {
    fn from(sender: mpsc::UnboundedSender<Event>) -> Self {
        Self { sender }
    }
}
