//! Background actor lanes for agent turns and session snapshot I/O.

mod agent_lane;
mod persist_lane;

pub use agent_lane::{AgentLane, StandardAgentRunRequest};
pub use persist_lane::{PersistLane, SessionSnapshotWrite};
