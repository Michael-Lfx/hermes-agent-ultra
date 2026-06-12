//! # hermes-cron
//!
//! Cron job scheduler for Hermes Agent (Requirement 13).
//!
//! Provides a cron-based scheduler that can create, manage, persist, and
//! execute recurring agent tasks. Jobs are defined by a cron expression
//! schedule, an agent prompt, and optional skill/model/deliver configurations.

pub mod backend;
pub mod cli_support;
pub mod completion;
pub mod delivery;
pub mod job;
pub mod persistence;
pub mod python_job;
pub mod runner;
pub mod schedule;
pub mod scheduler;
pub mod timing;

// Re-export primary types
pub use backend::ScheduledCronjobBackend;
pub use cli_support::{MinimalCronLlm, cron_scheduler_for_data_dir};
pub use completion::CronCompletionEvent;
pub use delivery::{CronDeliveryBackend, ResolvedDelivery};
pub use job::{CronJob, DeliverConfig, DeliverTarget, JobStatus, ModelConfig};
pub use persistence::{FileJobPersistence, JobPersistence, SqliteJobPersistence};
pub use python_job::JobOrigin;
pub use runner::{CronRunOutcome, CronRunner};
pub use schedule::{ScheduleParseError, ScheduleSpec, normalize_schedule_input, parse_schedule};
pub use scheduler::{CronError, CronScheduler};
