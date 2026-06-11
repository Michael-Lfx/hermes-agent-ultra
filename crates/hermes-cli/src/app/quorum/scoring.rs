use super::super::App;

impl App {
    pub(crate) fn required_quorum_success(voter_count: usize) -> usize {
        let n = voter_count.max(1);
        (n / 2) + 1
    }
    pub(crate) fn quorum_output_is_degraded_non_answer(output: &str) -> bool {
        let lower = output.to_ascii_lowercase();
        lower.contains("objective delivery compromised")
            || lower.contains("reverting to hermes")
            || lower.contains("safe-mode response")
            || lower.contains("safe mode response")
            || (lower.contains("i do not have") && lower.contains("tools"))
            || (lower.contains("cannot access") && lower.contains("tools"))
    }
}
