//! Per-session iteration budget with consume/refund — parity with Python `IterationBudget`.

/// Tool-loop iteration budget (distinct from API-call retry budget).
#[derive(Debug, Clone)]
pub struct IterationBudget {
    pub max: u32,
    pub remaining: u32,
}

impl IterationBudget {
    pub fn new(max: u32) -> Self {
        let max = max.max(1);
        Self {
            max,
            remaining: max,
        }
    }

    pub fn consume(&mut self) -> bool {
        if self.remaining == 0 {
            return false;
        }
        self.remaining -= 1;
        true
    }

    pub fn refund(&mut self, amount: u32) {
        self.remaining = self.remaining.saturating_add(amount).min(self.max);
    }

    pub fn exhausted(&self) -> bool {
        self.remaining == 0
    }

    pub fn child_budget(&self, child_max: u32) -> Self {
        let cap = child_max.min(self.remaining).max(1);
        Self::new(cap)
    }
}
