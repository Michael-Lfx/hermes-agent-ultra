//! Synthetic 200-turn hot-path baseline.
//!
//! Measures wall-clock time for `assemble_api_messages_from_ctx` under a
//! realistic 200-turn conversation load, and prints the result so it can be
//! used as a baseline when evaluating future optimisations.
//!
//! # Heap profiling
//!
//! ```bash
//! cargo test -p hermes-agent --features dhat-heap \
//!   --test hotpath_baseline -- --nocapture
//! ```
//!
//! DHAT writes `dhat-heap.json`; open with
//! <https://nnethercote.github.io/dh_view/dh_view.html>.

use std::time::Instant;

use hermes_agent::api_messages::assemble_api_messages_from_ctx;
use hermes_agent::profiling::DhatGuard;
use hermes_core::Message;

fn make_conversation(turns: usize, chars_per_message: usize) -> Vec<Message> {
    let content = "x".repeat(chars_per_message);
    let mut msgs = Vec::with_capacity(turns * 2);
    for _ in 0..turns {
        msgs.push(Message::user(content.clone()));
        msgs.push(Message::assistant(content.clone()));
    }
    msgs
}

/// Synthetic 200-turn assembly benchmark.
///
/// Knuth analysis prediction: clone of ~200 KB of message content is
/// microseconds-level and should complete well under 50 ms even in debug mode.
#[test]
fn hotpath_200_turn_baseline() {
    let _guard = DhatGuard::new();

    const TURNS: usize = 200;
    const CHARS_PER_MSG: usize = 500;

    let messages = make_conversation(TURNS, CHARS_PER_MSG);
    let total_input_chars: usize = messages
        .iter()
        .filter_map(|m| m.content.as_deref())
        .map(|c| c.len())
        .sum();

    let t0 = Instant::now();
    let result = assemble_api_messages_from_ctx(
        &messages,
        "",
        None,
        "gpt-4o",
        "ephemeral",
        false,
        false,
        false,
    );
    let elapsed = t0.elapsed();

    assert_eq!(result.len(), TURNS * 2, "message count preserved");

    let total_output_chars: usize = result
        .iter()
        .filter_map(|m| m.content.as_deref())
        .map(|c| c.len())
        .sum();
    assert_eq!(
        total_input_chars, total_output_chars,
        "byte count preserved"
    );

    println!(
        "[hotpath_200_turn_baseline]\n  \
         {} messages × {} chars/msg\n  \
         wall-clock: {} ms\n  \
         input:  {:.1} KB\n  \
         output: {:.1} KB\n  \
         \n  \
         Interpretation: LLM API latency (1–30 s) dominates.\n  \
         Clone overhead confirmed negligible unless this exceeds ~1 ms in release.",
        messages.len(),
        CHARS_PER_MSG,
        elapsed.as_millis(),
        total_input_chars as f64 / 1024.0,
        total_output_chars as f64 / 1024.0,
    );

    assert!(
        elapsed.as_millis() < 50,
        "assembly took {} ms — unexpectedly slow; run with --features dhat-heap to investigate",
        elapsed.as_millis()
    );
}

/// Verifies the assembly is deterministic across two calls (same input → same output).
#[test]
fn hotpath_assembly_is_deterministic() {
    const TURNS: usize = 50;

    let messages = make_conversation(TURNS, 200);

    let r1 = assemble_api_messages_from_ctx(&messages, "", None, "gpt-4o", "", false, false, false);
    let r2 = assemble_api_messages_from_ctx(&messages, "", None, "gpt-4o", "", false, false, false);

    assert_eq!(r1.len(), r2.len());
    for (m1, m2) in r1.iter().zip(r2.iter()) {
        assert_eq!(m1.role, m2.role);
        assert_eq!(m1.content, m2.content);
    }
}
