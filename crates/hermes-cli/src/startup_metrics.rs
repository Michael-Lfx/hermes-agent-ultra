//! Gateway startup timing instrumentation.
//!
//! Records wall-clock elapsed time for each startup phase and logs
//! a structured summary at the end. This is the "RecordPlay" for
//! startup performance — grep `gateway_startup` in logs to find it.

use std::time::Instant;

/// A named phase in the gateway startup sequence.
pub struct StartupPhase {
    name: &'static str,
    start: Instant,
}

/// Collector for gateway startup timing metrics.
pub struct StartupMetrics {
    gateway_created: bool,
    phases: Vec<PhaseRecord>,
    /// `Instant` at which `hermes gateway start` began.
    boot: Instant,
}

#[derive(Debug, Clone)]
pub struct PhaseRecord {
    pub name: &'static str,
    pub elapsed_ms: u64,
}

impl StartupMetrics {
    /// Begin recording startup.
    pub fn begin() -> Self {
        tracing::info!(target: "gateway_startup", phase = "boot", "gateway startup begin");
        Self {
            gateway_created: false,
            phases: Vec::with_capacity(16),
            boot: Instant::now(),
        }
    }

    /// Start a named phase. Call `.finish()` on the returned guard.
    pub fn phase(&mut self, name: &'static str) -> StartupPhaseGuard<'_> {
        let phase = StartupPhase {
            name,
            start: Instant::now(),
        };
        tracing::debug!(target: "gateway_startup", phase = name, "phase start");
        StartupPhaseGuard {
            metrics: self,
            phase,
        }
    }

    /// Mark a phase as complete and record its duration.
    pub fn record_phase(&mut self, name: &'static str, elapsed_ms: u64) {
        self.phases.push(PhaseRecord { name, elapsed_ms });
        tracing::debug!(target: "gateway_startup", phase = name, elapsed_ms, "phase end");
    }

    /// Mark that Gateway::new() has completed (used for a special metric).
    pub fn mark_gateway_created(&mut self) {
        self.gateway_created = true;
    }

    /// Finalize and emit a structured startup summary.
    #[must_use]
    pub fn finish(mut self) -> StartupSummary {
        let total_ms = self.boot.elapsed().as_millis() as u64;

        // Synthesize total phase for convenience.
        self.phases.push(PhaseRecord {
            name: "_total",
            elapsed_ms: total_ms,
        });

        tracing::info!(
            target: "gateway_startup",
            phase = "_summary",
            total_ms,
            gateway_created = self.gateway_created,
            phase_count = self.phases.len(),
            "gateway startup complete"
        );

        // Emit each phase as a structured trace event.
        for p in &self.phases {
            tracing::info!(
                target: "gateway_startup_timing",
                phase = p.name,
                elapsed_ms = p.elapsed_ms,
            );
        }

        StartupSummary {
            total_ms,
            phases: self.phases,
        }
    }
}

/// RAII guard that records phase duration on drop.
pub struct StartupPhaseGuard<'a> {
    metrics: &'a mut StartupMetrics,
    phase: StartupPhase,
}

impl<'a> Drop for StartupPhaseGuard<'a> {
    fn drop(&mut self) {
        let elapsed_ms = self.phase.start.elapsed().as_millis() as u64;
        self.metrics.record_phase(self.phase.name, elapsed_ms);
    }
}

/// Structured summary emitted after gateway is ready.
#[derive(Debug, Clone)]
pub struct StartupSummary {
    pub total_ms: u64,
    pub phases: Vec<PhaseRecord>,
}

impl StartupSummary {
    /// Print a human-readable summary to stdout (after the "ready" line).
    pub fn print_summary(&self) {
        // Only print to stdout if startup took > 500ms (avoid noise for fast boots).
        if self.total_ms < 500 {
            return;
        }
        let slow: Vec<&PhaseRecord> = self
            .phases
            .iter()
            .filter(|p| p.elapsed_ms > 50 && p.name != "_total")
            .collect();
        if slow.is_empty() {
            return;
        }
        println!(
            "  ⚡ Gateway startup: {}ms total (slow phases: {})",
            self.total_ms,
            slow.iter()
                .map(|p| format!("{}={}ms", p.name, p.elapsed_ms))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
}
