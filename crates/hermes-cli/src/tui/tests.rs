use std::time::{Duration, Instant};

use hermes_core::{Message, StreamChunk};
use ratatui::text::Line;
use unicode_width::UnicodeWidthStr;

use super::render::{
    animated_processing_bar, append_transcript_message_lines, approximate_visual_rows,
    build_transcript_lines, collapse_render_lines_with_notice, count_renderable_messages,
    count_renderable_messages_before, find_anchor_line_index, find_stable_boundary,
    format_tool_message_lines, looks_like_internal_scaffold_line, max_tool_output_lines,
    pet_frame_token, project_transcript_window, render_assistant_markdown_lines,
    render_streaming_assistant_markdown_lines, should_redraw_stream_while_composing,
    should_render_completions_popup, should_route_prompt_via_managed_agent,
    split_reasoning_from_content, status_message_style, stream_event_completes_background_task,
    stream_lane_budget_from, strip_control_chars, tail_render_lines_with_notice,
    tool_complete_looks_failed, transcript_fingerprint, transcript_message_fingerprints,
    transcript_wrap_width,
};
use super::run_loop::{
    open_skin_modal, parse_interactive_question_request, stream_chunk_has_progress,
};
use super::text::{
    fit_status_line, hard_wrap_segments, is_ctrl_c, is_submit_shortcut, transcript_divider,
};
use super::transcript_cache::{
    TranscriptCache, expanded_tool_cards_signature, find_message_fingerprint_divergence,
};
use super::*;
#[test]
fn test_input_mode_display() {
    assert_eq!(InputMode::Normal.to_string(), "NORMAL");
    assert_eq!(InputMode::Insert.to_string(), "INSERT");
    assert_eq!(InputMode::Command.to_string(), "COMMAND");
}

#[test]
fn test_tui_state_default() {
    let state = TuiState::default();
    assert_eq!(state.phase.composer().mode, InputMode::Insert);
    assert!(state.phase.composer().input.is_empty());
    assert_eq!(state.phase.composer().cursor_position, 0);
    assert!(state.phase.composer().completions.is_empty());
    assert!(!state.phase.is_processing());
    assert!(state.phase.composer().selection_anchor.is_none());
    assert!(!state.phase.composer().history_search_active);
}

#[test]
fn test_spinner_char() {
    let mut state = TuiState::default();
    let c1 = state.spinner_char();
    state.tick_spinner();
    let c2 = state.spinner_char();
    assert_ne!(c1, c2);
}

#[test]
fn test_tool_output_section() {
    let section = ToolOutputSection::new(
        "test_tool".to_string(),
        "line1\nline2\nline3\nline4\nline5".to_string(),
    );
    assert!(!section.is_expanded);
    let display = section.display_text();
    assert!(display.contains("line1"));
    assert!(display.contains("more lines"));
}

#[test]
fn test_tui_state_completions_update() {
    let mut state = TuiState::default();
    state.phase.composer_mut().input = "/mod".to_string();
    state.update_completions();
    assert!(
        state
            .phase
            .composer_mut()
            .completions
            .contains(&"/model".to_string())
    );
    assert_eq!(state.phase.composer_mut().completion_index, Some(0));
}

#[test]
fn test_enter_accepts_slash_completion_instead_of_submit() {
    let mut state = TuiState::default();
    state.phase.composer_mut().input = "/mod".to_string();
    state.update_completions();
    assert!(state.try_accept_completion_on_enter());
    assert_eq!(state.phase.composer_mut().input, "/model");
    assert!(
        state
            .phase
            .composer_mut()
            .completions
            .contains(&"/model".to_string())
    );
}

#[test]
fn test_enter_submits_when_slash_completion_already_matches_input() {
    let mut state = TuiState::default();
    state.phase.composer_mut().input = "/model".to_string();
    state.update_completions();
    assert!(!state.try_accept_completion_on_enter());
    assert_eq!(state.phase.composer_mut().input, "/model");
}

#[test]
fn test_completion_popup_hidden_when_slash_deleted() {
    let mut state = TuiState::default();
    state.phase.composer_mut().input = "/model".to_string();
    state.update_completions();
    assert!(should_render_completions_popup(&state));

    state.phase.composer_mut().input.clear();
    state.refresh_completions();
    assert!(!should_render_completions_popup(&state));
    assert!(state.phase.composer_mut().completions.is_empty());
}

#[test]
fn test_completion_popup_hidden_when_modal_or_processing_active() {
    let mut state = TuiState::default();
    state.phase.composer_mut().input = "/model".to_string();
    state.update_completions();
    assert!(should_render_completions_popup(&state));

    state.begin_processing_cycle("nous:test-model");
    assert!(!should_render_completions_popup(&state));
    state.finish_processing_cycle("done");

    state.open_modal(PickerModal::new(
        PickerKind::Personality,
        "personality",
        vec![PickerItem {
            label: "default".to_string(),
            detail: String::new(),
            value: "default".to_string(),
        }],
    ));
    assert!(!should_render_completions_popup(&state));
}

#[test]
fn test_managed_route_when_quorum_armed() {
    let messages: Vec<Message> = Vec::new();
    assert!(should_route_prompt_via_managed_agent(true, &messages));
}

#[test]
fn test_managed_route_when_quorum_hint_present() {
    let messages = vec![Message::system(
        "[QUORUM_MODE] Quorum reasoning is enabled for multi-voter fanout",
    )];
    assert!(should_route_prompt_via_managed_agent(false, &messages));
}

#[test]
fn test_background_route_without_quorum_state() {
    let messages = vec![Message::system("normal system message")];
    assert!(!should_route_prompt_via_managed_agent(false, &messages));
}

#[test]
fn test_background_completion_events_clear_task_handle() {
    let event = Event::AgentRunComplete {
        result: Err("stopped".to_string()),
        elapsed_secs: 1.0,
    };
    assert!(stream_event_completes_background_task(&event));
}

#[test]
fn test_open_skin_modal_populates_builtin_skin_items() {
    let mut state = TuiState::default();
    open_skin_modal(&mut state);
    let modal = state.phase.modal().expect("skin modal");
    assert!(matches!(&modal.kind, PickerKind::Skin));
    assert!(modal.items.iter().any(|item| item.value == "ultra-neon"));
    assert!(modal.items.iter().any(|item| item.value == "neon-glow"));
    assert!(
        modal
            .items
            .iter()
            .any(|item| item.value == "hyper-ultra-hyper-saturated")
    );
}

#[test]
fn test_event_debug() {
    let event = Event::Message("hello".to_string());
    let debug_str = format!("{:?}", event);
    assert!(debug_str.contains("hello"));
}

#[test]
fn test_activity_ring_buffer_caps_size() {
    let mut state = TuiState::default();
    for i in 0..30 {
        state.push_activity(format!("event-{i}"));
    }
    assert_eq!(state.recent_activity.len(), 16);
    assert!(
        state
            .recent_activity
            .first()
            .is_some_and(|line| line.ends_with("event-14"))
    );
    assert!(
        state
            .recent_activity
            .last()
            .is_some_and(|line| line.ends_with("event-29"))
    );
}

#[test]
fn test_fit_status_line_pads_and_respects_display_width() {
    let fitted = fit_status_line("ok", 6);
    assert_eq!(UnicodeWidthStr::width(fitted.as_str()), 6);
    assert!(fitted.starts_with("ok"));

    let wide = fit_status_line("界abc", 4);
    assert_eq!(UnicodeWidthStr::width(wide.as_str()), 4);
    assert!(wide.starts_with('界'));
}

#[test]
fn test_append_live_thinking_is_capped() {
    let mut state = TuiState::default();
    state.begin_processing_cycle("nous:test-model");
    let long = "x".repeat(400);
    state.append_live_thinking(&long);
    assert!(
        state
            .phase
            .processing_mut()
            .expect("processing")
            .live_thinking
            .chars()
            .count()
            <= 260
    );
    assert!(
        state
            .phase
            .processing_mut()
            .expect("processing")
            .live_thinking
            .starts_with('…')
    );
}

#[test]
fn test_processing_cycle_tracks_and_resets_stats() {
    let mut state = TuiState::default();
    state.begin_processing_cycle("nous:test-model");
    assert!(state.phase.is_processing());
    assert_eq!(
        state
            .phase
            .processing_mut()
            .expect("processing")
            .stream_chunk_count,
        0
    );
    assert_eq!(
        state
            .phase
            .processing_mut()
            .expect("processing")
            .stream_char_count,
        0
    );
    assert!(
        state
            .phase
            .processing()
            .expect("processing")
            .started_at
            .is_some()
    );
    assert!(
        state
            .recent_activity
            .last()
            .is_some_and(|line| line.contains("dispatching request"))
    );

    state
        .phase
        .processing_mut()
        .expect("processing")
        .stream_chunk_count = 7;
    state
        .phase
        .processing_mut()
        .expect("processing")
        .stream_char_count = 1234;
    state.finish_processing_cycle("✔ completed in");

    assert!(!state.phase.is_processing());
    assert!(
        state
            .recent_activity
            .last()
            .is_some_and(|line| line.contains("✔ completed in"))
    );
}

#[test]
fn test_progress_pulse_emits_activity_row() {
    let mut state = TuiState::default();
    state.begin_processing_cycle("nous:test-model");
    state.phase.processing_mut().expect("processing").started_at =
        Some(Instant::now() - Duration::from_secs(2));
    state
        .phase
        .processing_mut()
        .expect("processing")
        .last_progress_pulse_at = None;
    let before = state.recent_activity.len();
    state.maybe_emit_progress_pulse();
    assert!(state.recent_activity.len() > before);
    assert!(
        state
            .recent_activity
            .last()
            .is_some_and(|line| line.contains("working"))
    );
}

#[test]
fn test_processing_stage_labels() {
    let mut state = TuiState::default();
    assert_eq!(state.processing_stage_label(), "idle");
    state.begin_processing_cycle("nous:test-model");
    assert_eq!(state.processing_stage_label(), "phase-driven");
    state
        .phase
        .processing_mut()
        .expect("processing")
        .processing_phase_label
        .clear();
    state
        .phase
        .processing_mut()
        .expect("processing")
        .active_tools
        .push("terminal".to_string());
    assert_eq!(state.processing_stage_label(), "running tools (pre-token)");
    state
        .phase
        .processing_mut()
        .expect("processing")
        .saw_first_token = true;
    assert_eq!(state.processing_stage_label(), "running tools + streaming");
    state
        .phase
        .processing_mut()
        .expect("processing")
        .active_tools
        .clear();
    assert_eq!(state.processing_stage_label(), "streaming response");
}

#[test]
fn test_phase_updates_set_progress_and_activity() {
    let mut state = TuiState::default();
    state.begin_processing_cycle("nous:test-model");
    state.update_processing_phase("retrieval", "collecting evidence", Some(42));
    assert_eq!(
        state
            .phase
            .processing()
            .expect("processing")
            .processing_phase,
        "retrieval"
    );
    assert_eq!(
        state
            .phase
            .processing()
            .expect("processing")
            .processing_phase_label,
        "collecting evidence"
    );
    assert_eq!(
        state
            .phase
            .processing()
            .expect("processing")
            .processing_phase_progress,
        42
    );
    assert!(
        state
            .recent_activity
            .last()
            .is_some_and(|line| line.contains("phase 42%"))
    );
}

#[test]
fn test_animated_processing_bar_width_and_motion() {
    let bar_a = animated_processing_bar(0, 12);
    let bar_b = animated_processing_bar(4, 12);
    assert_eq!(bar_a.chars().count(), 12);
    assert_eq!(bar_b.chars().count(), 12);
    assert_ne!(bar_a, bar_b);
}

#[test]
fn test_find_anchor_line_index_prefers_near_expected_window() {
    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from("dup anchor"));
    for idx in 1..2500 {
        lines.push(Line::from(format!("line-{idx}")));
    }
    lines.push(Line::from("dup anchor"));
    let idx = find_anchor_line_index(&lines, "dup anchor", 2499).expect("anchor index");
    assert_eq!(idx, 2500);
}

#[test]
fn test_find_anchor_line_index_falls_back_to_global_search() {
    let lines = vec![Line::from("alpha"), Line::from("beta"), Line::from("gamma")];
    let idx = find_anchor_line_index(&lines, "gamma", 0).expect("anchor index");
    assert_eq!(idx, 2);
}

#[test]
fn test_stream_handle() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let handle: StreamHandle = tx.into();
    handle.send_delta("test delta");
    handle.send_done();
}

#[test]
fn test_is_ctrl_c_detection() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    let ctrl_upper_c = KeyEvent::new(KeyCode::Char('C'), KeyModifiers::CONTROL);
    let raw_etx = KeyEvent::new(KeyCode::Char('\u{3}'), KeyModifiers::NONE);
    let plain_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE);
    assert!(is_ctrl_c(&ctrl_c));
    assert!(is_ctrl_c(&ctrl_upper_c));
    assert!(is_ctrl_c(&raw_etx));
    assert!(!is_ctrl_c(&plain_c));
}

#[test]
fn test_submit_shortcuts_are_detected() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let plain_enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    let ctrl_enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL);
    let alt_enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT);
    let ctrl_m = KeyEvent::new(KeyCode::Char('m'), KeyModifiers::CONTROL);

    assert!(is_submit_shortcut(&plain_enter, "hello"));
    assert!(is_submit_shortcut(&ctrl_enter, "hello"));
    assert!(is_submit_shortcut(&alt_enter, "hello"));
    assert!(is_submit_shortcut(&ctrl_m, "hello"));
}

#[test]
fn test_submit_shortcuts_exclude_newline_shortcuts() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let shift_enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT);
    let ctrl_j = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL);

    assert!(!is_submit_shortcut(&shift_enter, "hello"));
    assert!(!is_submit_shortcut(&ctrl_j, "hello"));
}

#[test]
fn test_submit_shortcut_rejects_multiline_slash_commands() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let plain_enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    assert!(is_submit_shortcut(&plain_enter, "/model\nlist"));
}

#[test]
fn test_bracketed_paste_inserts_multiline_text_without_submit_shortcut() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut state = TuiState::default();
    state.phase.composer_mut().input = "before  after".to_string();
    state.phase.composer_mut().cursor_position = "before ".len();

    state.insert_paste_at_cursor("line1\r\nline2\rline3");

    assert_eq!(
        state.phase.composer_mut().input,
        "before line1\nline2\nline3 after"
    );
    assert_eq!(
        state.phase.composer_mut().cursor_position,
        "before line1\nline2\nline3".len()
    );

    let shift_enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT);
    assert!(!is_submit_shortcut(
        &shift_enter,
        &state.phase.composer_mut().input
    ));
}

#[test]
fn test_parse_interactive_question_request_pipe_syntax() {
    let request = parse_interactive_question_request(
        "/ask Proceed with deploy? | yes (recommended)::ship now | no::pause and inspect",
    )
    .expect("parse request");
    assert_eq!(request.prompt, "Proceed with deploy?");
    assert_eq!(request.options.len(), 2);
    assert_eq!(request.options[0].label, "yes (recommended)");
    assert_eq!(request.options[0].detail, "ship now");
    assert_eq!(request.options[1].label, "no");
}

#[test]
fn test_parse_interactive_question_request_multiline_syntax() {
    let request = parse_interactive_question_request(
        "/question\nWhat path should we take?\n- continue implementation\n- pause for diagnosis",
    )
    .expect("parse request");
    assert_eq!(request.prompt, "What path should we take?");
    assert_eq!(request.options.len(), 2);
    assert_eq!(request.options[0].label, "continue implementation");
}

#[test]
fn test_parse_interactive_question_request_requires_two_options() {
    let err = parse_interactive_question_request("/ask choose one | only-one-option")
        .expect_err("expected parse error");
    assert!(err.contains("at least 2 options"));
}

#[test]
fn test_insert_newline_at_cursor_updates_input_and_cursor() {
    let mut state = TuiState::default();
    state.phase.composer_mut().input = "hello".to_string();
    state.phase.composer_mut().cursor_position = 5;
    state.insert_newline_at_cursor();
    assert_eq!(state.phase.composer_mut().input, "hello\n");
    assert_eq!(state.phase.composer_mut().cursor_position, 6);
}

#[test]
fn test_insert_newline_at_cursor_clamps_non_char_boundary() {
    let mut state = TuiState::default();
    state.phase.composer_mut().input = "éx".to_string();
    state.phase.composer_mut().cursor_position = 1; // interior byte of 'é'
    state.insert_newline_at_cursor();
    assert_eq!(state.phase.composer_mut().input, "\néx");
    assert_eq!(state.phase.composer_mut().cursor_position, 1);
}

#[test]
fn test_cursor_row_col_clamps_non_char_boundary() {
    assert_eq!(TuiState::cursor_row_col("éx", 1), (0, 0));
    assert_eq!(TuiState::cursor_row_col("éx", 2), (0, 1));
}

#[test]
fn test_scroll_history_offset_not_capped_to_u16() {
    let mut state = TuiState::default();
    state.scroll_offset = (u16::MAX as usize).saturating_sub(1);
    state.scroll_history_up(8);
    assert_eq!(state.scroll_offset, (u16::MAX as usize).saturating_add(7));
    assert!(!state.auto_follow_transcript);
}

#[test]
fn test_jump_to_oldest_sets_unbounded_offset() {
    let mut state = TuiState::default();
    state.jump_to_oldest();
    assert_eq!(state.scroll_offset, usize::MAX);
    assert!(!state.auto_follow_transcript);
}

#[test]
fn test_project_transcript_window_virtualizes_large_offsets() {
    let lines: Vec<Line<'static>> = (0..100_000)
        .map(|idx| Line::from(format!("line-{idx}")))
        .collect();
    let (window, local_scroll) = project_transcript_window(&lines, 80, 70_000, 30);
    assert!(!window.is_empty());
    assert_eq!(local_scroll, 0);
    assert_eq!(window[0].to_string(), "line-70000");
}

#[test]
fn test_status_message_style_critical_for_error() {
    let colors = Theme::default_theme().colors.to_ratatui_colors();
    let style = status_message_style("Error: boom", &colors);
    assert_eq!(style.fg, Some(colors.status_bar_critical));
    assert_eq!(style.bg, Some(colors.status_bar_bg));
}

#[test]
fn test_status_message_style_warn_for_warning() {
    let colors = Theme::default_theme().colors.to_ratatui_colors();
    let style = status_message_style("Warning: retrying", &colors);
    assert_eq!(style.fg, Some(colors.status_bar_warn));
    assert_eq!(style.bg, Some(colors.status_bar_bg));
}

#[test]
fn test_pet_frame_token_hidden_when_disabled() {
    let settings = crate::app::PetSettings {
        enabled: false,
        ..crate::app::PetSettings::default()
    };
    assert!(pet_frame_token(&settings, 0, false).is_none());
}

#[test]
fn test_pet_frame_token_returns_species_specific_frame() {
    let settings = crate::app::PetSettings {
        enabled: true,
        species: "fox".to_string(),
        mood: "ready".to_string(),
        dock: crate::app::PetDock::Right,
        tick_ms: 400,
    };
    let frame0 = pet_frame_token(&settings, 0, false).expect("frame");
    let frame1 = pet_frame_token(&settings, 1, false).expect("frame");
    assert_ne!(frame0, frame1);
    assert!(frame0.contains('{'));
}

#[test]
fn test_transcript_hides_system_messages() {
    let theme = Theme::default_theme();
    let colors = theme.colors.to_ratatui_colors();
    let styles = theme.resolved_styles();
    let mut state = TuiState::default();
    let messages = vec![
        Message::system("internal system payload"),
        Message::user("reply with 1"),
        Message::assistant("1"),
    ];
    let rendered = build_transcript_lines(&messages, &mut state, &styles, &colors, 80).lines;
    let rendered_text = rendered
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!rendered_text.contains("SYSTEM"));
    assert!(!rendered_text.contains("internal system payload"));
    assert!(rendered_text.contains("USER"));
    assert!(rendered_text.contains("HERMES"));
    assert!(rendered_text.contains("reply with 1"));
    assert!(rendered_text.contains("1"));
}

#[test]
fn test_transcript_placeholder_shows_when_empty() {
    let theme = Theme::default_theme();
    let colors = theme.colors.to_ratatui_colors();
    let styles = theme.resolved_styles();
    let mut state = TuiState::default();
    let rendered = build_transcript_lines(&[], &mut state, &styles, &colors, 80).lines;
    let rendered_text = rendered
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered_text.contains("Start chatting"));
}

#[test]
fn test_count_renderable_messages_ignores_system() {
    let messages = vec![
        Message::system("hidden"),
        Message::user("u"),
        Message::assistant("a"),
    ];
    assert_eq!(count_renderable_messages(&messages), 2);
}

#[test]
fn test_format_tool_message_lines_parses_json_payload() {
    let payload = r#"{"result":"line1\nline2","_budget_warning":"[BUDGET WARNING: Iteration 40/50.]","error":"boom"}"#;
    let lines = format_tool_message_lines(payload);
    let joined = lines.join("\n");
    assert!(joined.contains("[BUDGET WARNING"));
    assert!(joined.contains("[result]"));
    assert!(joined.contains("line1"));
    assert!(joined.contains("[error]"));
    assert!(joined.contains("boom"));
}

#[test]
fn test_format_tool_message_lines_adds_policy_remediation_block() {
    let payload = r#"{
  "error":"Blocked by tool policy: tool params matched deny pattern '(?i)api[_-]?key'",
  "policy":{"code":"params_pattern_denied","mode":"enforce"}
}"#;
    let lines = format_tool_message_lines(payload);
    let joined = lines.join("\n");
    assert!(joined.contains("[remediation]"));
    assert!(joined.contains("Remove secret-like parameter names"));
}

#[test]
fn test_format_tool_message_lines_truncates_large_payload() {
    let cap = max_tool_output_lines();
    let long = (0..(cap + 40))
        .map(|idx| format!("row-{idx}-{}", "x".repeat(120)))
        .collect::<Vec<_>>()
        .join("\\n");
    let payload = format!(r#"{{"result":"{}"}}"#, long);
    let lines = format_tool_message_lines(&payload);
    let joined = lines.join("\n");
    assert!(joined.contains("tool output truncated"));
    assert!(lines.len() <= cap + 8);
}

#[test]
fn test_approximate_visual_rows_wraps_long_lines() {
    let lines = vec![Line::from("x".repeat(120))];
    assert_eq!(approximate_visual_rows(&lines, 40), 3);
    assert_eq!(approximate_visual_rows(&lines, 80), 2);
}

#[test]
fn test_transcript_wrap_width_caps_at_80() {
    assert_eq!(transcript_wrap_width(12), 12);
    assert_eq!(transcript_wrap_width(80), 80);
    assert_eq!(transcript_wrap_width(140), 80);
}

#[test]
fn test_hard_wrap_segments_prefers_word_boundaries() {
    let wrapped = hard_wrap_segments("context lattice integration is core", 12);
    assert_eq!(
        wrapped,
        vec![
            "context".to_string(),
            "lattice".to_string(),
            "integration".to_string(),
            "is core".to_string(),
        ]
    );
}

#[test]
fn test_hard_wrap_segments_splits_overlong_token() {
    let wrapped = hard_wrap_segments("supercalifragilisticexpialidocious", 8);
    assert_eq!(
        wrapped,
        vec![
            "supercal".to_string(),
            "ifragili".to_string(),
            "sticexpi".to_string(),
            "alidocio".to_string(),
            "us".to_string(),
        ]
    );
}

#[test]
fn test_stream_chunk_has_progress_for_extra_only_events() {
    let chunk = StreamChunk {
        delta: Some(hermes_core::StreamDelta {
            content: None,
            tool_calls: None,
            extra: Some(serde_json::json!({
                "ui_event": "lifecycle",
                "message": "dispatching request"
            })),
        }),
        finish_reason: None,
        usage: None,
    };
    assert!(stream_chunk_has_progress(&chunk));
}

#[test]
fn test_stream_lane_budget_defaults_balanced() {
    let (cap, budget) = stream_lane_budget_from("advisory", "balanced", false, 0);
    assert_eq!(cap, 96);
    assert_eq!(budget, Duration::from_millis(6));
}

#[test]
fn test_stream_lane_budget_throughput_profile_expands() {
    let (cap, budget) = stream_lane_budget_from("advisory", "throughput", false, 0);
    assert!(cap >= 320);
    assert!(budget >= Duration::from_millis(16));
}

#[test]
fn test_stream_lane_budget_off_mode_uses_baseline() {
    let (cap, budget) = stream_lane_budget_from("off", "throughput", true, 200);
    assert_eq!(cap, 96);
    assert_eq!(budget, Duration::from_millis(6));
}

#[test]
fn test_should_redraw_stream_while_composing_throttles() {
    let mut idle = Instant::now();
    assert!(should_redraw_stream_while_composing(false, &mut idle));

    let mut last = Instant::now() - Duration::from_millis(200);
    assert!(should_redraw_stream_while_composing(true, &mut last));
    assert!(!should_redraw_stream_while_composing(true, &mut last));
    std::thread::sleep(Duration::from_millis(130));
    assert!(should_redraw_stream_while_composing(true, &mut last));
}

#[test]
fn test_input_paint_snapshot_detects_composer_changes() {
    let mut state = TuiState::default();
    let first = state.phase.composer_mut().input_paint_snapshot();
    state.phase.composer_mut().input.push('a');
    let second = state.phase.composer_mut().input_paint_snapshot();
    assert_ne!(first, second);
}

#[test]
fn test_append_message_renderer_matches_full_builder() {
    let theme = Theme::default_theme();
    let colors = theme.colors.to_ratatui_colors();
    let styles = theme.resolved_styles();
    let messages = vec![Message::user("hello"), Message::assistant("world")];

    let mut full_state = TuiState::default();
    let full = build_transcript_lines(&messages, &mut full_state, &styles, &colors, 80).lines;

    let mut inc_state = TuiState::default();
    let divider = transcript_divider(80);
    let mut lines = Vec::new();
    let mut rendered = 0usize;
    for (idx, msg) in messages.iter().enumerate() {
        append_transcript_message_lines(
            &mut lines,
            msg,
            idx,
            &mut rendered,
            &mut inc_state,
            &styles,
            &colors,
            &divider,
        );
    }

    let as_text = |v: &[Line<'static>]| -> Vec<String> { v.iter().map(Line::to_string).collect() };
    assert_eq!(as_text(&full), as_text(&lines));
}

#[test]
fn test_rebuild_from_divergence_matches_full_builder() {
    let theme = Theme::default_theme();
    let colors = theme.colors.to_ratatui_colors();
    let styles = theme.resolved_styles();
    let original = vec![
        Message::user("one"),
        Message::assistant("two"),
        Message::user("three"),
    ];
    let edited = vec![
        Message::user("one"),
        Message::assistant("TWO"),
        Message::user("three"),
    ];

    let mut full_state = TuiState::default();
    let full = build_transcript_lines(&edited, &mut full_state, &styles, &colors, 80);

    let mut cache_state = TuiState::default();
    let initial = build_transcript_lines(&original, &mut cache_state, &styles, &colors, 80);
    let visual_rows = approximate_visual_rows(&initial.lines, 80);
    cache_state.transcript_cache = TranscriptCache {
        fingerprint: transcript_fingerprint(&original, &cache_state, 80),
        width: 80,
        lines: initial.lines,
        visual_rows,
        total_messages: original.len(),
        rendered_messages: initial.rendered_messages,
        message_fingerprints: transcript_message_fingerprints(&original),
        message_line_ends: initial.message_line_ends,
        messages_only_len: initial.messages_only_len,
        show_timestamps: false,
        view_density: ViewDensity::Detailed,
        had_streaming: false,
        expanded_tool_cards_sig: expanded_tool_cards_signature(&cache_state.expanded_tool_cards),
    };

    let diverge = find_message_fingerprint_divergence(
        &cache_state.transcript_cache.message_fingerprints,
        &transcript_message_fingerprints(&edited),
    );
    assert_eq!(diverge, 1);
    let truncate_at = cache_state.transcript_cache.line_start_for_message(diverge);
    let mut lines = cache_state.transcript_cache.lines[..truncate_at].to_vec();
    let mut rendered = count_renderable_messages_before(&edited, diverge);
    let divider = transcript_divider(80);
    for (idx, msg) in edited.iter().enumerate().skip(diverge) {
        append_transcript_message_lines(
            &mut lines,
            msg,
            idx,
            &mut rendered,
            &mut cache_state,
            &styles,
            &colors,
            &divider,
        );
    }

    let as_text = |v: &[Line<'static>]| -> Vec<String> { v.iter().map(Line::to_string).collect() };
    assert_eq!(as_text(&full.lines), as_text(&lines));
}

#[test]
fn test_transcript_fingerprint_tracks_toolcard_expand_state() {
    let messages = vec![Message::tool_result("call-1", "{}")];
    let mut state_a = TuiState::default();
    let mut state_b = TuiState::default();
    state_b.expanded_tool_cards.insert("__all__".to_string());

    let fp_a = transcript_fingerprint(&messages, &state_a, 80);
    let fp_b = transcript_fingerprint(&messages, &state_b, 80);
    assert_ne!(fp_a, fp_b);
}

#[test]
fn test_strip_control_chars_preserves_unicode() {
    let raw = "你好\x07世界";
    let cleaned = strip_control_chars(raw);
    assert!(cleaned.contains('你'));
    assert!(cleaned.contains('好'));
    assert!(!cleaned.contains('\x07'));
}

#[test]
fn test_split_reasoning_from_content() {
    let input = "hello\n<thinking>inner thought</thinking>\nworld";
    let (text, reasoning) = split_reasoning_from_content(input);
    assert_eq!(text, "hello\n\nworld");
    assert_eq!(reasoning, "inner thought");
}

#[test]
fn test_render_assistant_markdown_preserves_unicode_and_hides_scaffold() {
    let theme = Theme::default_theme();
    let colors = theme.colors.to_ratatui_colors();
    let styles = theme.resolved_styles();
    let content = "to=functions.memory 天安中彩樣\nregular line 大家好";
    let lines = render_assistant_markdown_lines(content, &styles, &colors);
    let joined = lines
        .iter()
        .map(Line::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(joined.contains("internal orchestration scaffold hidden"));
    assert!(!joined.contains("to=functions.memory"));
    assert!(joined.contains('好'));
    assert!(joined.contains("regular line"));
    assert!(joined.contains("大家"));
}

#[test]
fn test_tool_complete_looks_failed() {
    let extra = serde_json::json!({"error": "Tool execution failed: timeout"});
    assert!(tool_complete_looks_failed(&extra, ""));
    let ok = serde_json::json!({});
    assert!(!tool_complete_looks_failed(&ok, "done"));
    assert!(tool_complete_looks_failed(&ok, "Error: boom"));
}

#[test]
fn test_find_stable_boundary_no_blank_line() {
    assert_eq!(
        find_stable_boundary("partial line with no newline yet"),
        None
    );
    assert_eq!(find_stable_boundary("line one\nline two\nline three"), None);
    assert_eq!(find_stable_boundary(""), None);
}

#[test]
fn test_find_stable_boundary_after_paragraphs() {
    let text = "first paragraph\n\nsecond paragraph\n\nthird";
    let idx = find_stable_boundary(text).expect("boundary");
    assert_eq!(&text[..idx], "first paragraph\n\nsecond paragraph\n\n");
    assert_eq!(&text[idx..], "third");
}

#[test]
fn test_find_stable_boundary_inside_open_fence() {
    let text = "```ts\nfn();\n\nmore code here";
    assert_eq!(find_stable_boundary(text), None);
}

#[test]
fn test_find_stable_boundary_before_open_fence() {
    let text = "intro paragraph\n\n```ts\nfn();\n\nmore code";
    let idx = find_stable_boundary(text).expect("boundary");
    assert_eq!(&text[..idx], "intro paragraph\n\n");
    assert!(text[idx..].starts_with("```ts"));
}

#[test]
fn test_streaming_markdown_cache_reuses_stable_prefix() {
    let theme = Theme::default_theme();
    let colors = theme.colors.to_ratatui_colors();
    let styles = theme.resolved_styles();
    let mut cache = StreamMarkdownCache::default();

    let text_a = "first paragraph\n\nsecond";
    let lines_a =
        render_streaming_assistant_markdown_lines(&mut cache, text_a, &styles, &colors, 80);
    assert!(!lines_a.is_empty());
    assert_eq!(cache.stable_prefix, "first paragraph\n\n");

    let text_b = "first paragraph\n\nsecond paragraph\n\nthird";
    let lines_b =
        render_streaming_assistant_markdown_lines(&mut cache, text_b, &styles, &colors, 80);
    assert!(lines_b.len() >= lines_a.len());
    assert_eq!(
        cache.stable_prefix,
        "first paragraph\n\nsecond paragraph\n\n"
    );
}

#[test]
fn test_scaffold_detector_matches_embedded_tool_tags() {
    let line = "random prefix <tool_use><name>terminal</name></tool_use> suffix";
    assert!(looks_like_internal_scaffold_line(line));
}

#[test]
fn test_scaffold_detector_matches_escaped_tool_tags() {
    let line = "noise \\u003ctool_use\\u003e <argument name=\"skill\">x</argument>";
    assert!(looks_like_internal_scaffold_line(line));
}

#[test]
fn test_collapse_render_lines_adds_notice() {
    let theme = Theme::default_theme();
    let colors = theme.colors.to_ratatui_colors();
    let input: Vec<Line<'static>> = (0..40)
        .map(|idx| Line::from(format!("line-{idx}")))
        .collect();
    let collapsed = collapse_render_lines_with_notice(input, 12, &colors);
    let joined = collapsed
        .iter()
        .map(Line::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(collapsed.len() <= 13);
    assert!(joined.contains("transcript compressed for readability"));
}

#[test]
fn test_tail_render_lines_keeps_latest_rows() {
    let theme = Theme::default_theme();
    let colors = theme.colors.to_ratatui_colors();
    let input: Vec<Line<'static>> = (0..20)
        .map(|idx| Line::from(format!("tail-{idx}")))
        .collect();
    let tailed = tail_render_lines_with_notice(input, 5, &colors);
    let joined = tailed
        .iter()
        .map(Line::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(tailed.len() <= 6);
    assert!(joined.contains("tail-19"));
    assert!(joined.contains("live stream trimmed"));
}
