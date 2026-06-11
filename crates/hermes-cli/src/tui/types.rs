use std::collections::HashSet;

use ratatui::text::Line;
/// Current input mode for the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// Normal mode: keys are interpreted as commands.
    Normal,
    /// Insert mode: keys are inserted into the input buffer.
    Insert,
    /// Command mode: entering a slash command with auto-completion.
    Command,
}

impl std::fmt::Display for InputMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InputMode::Normal => write!(f, "NORMAL"),
            InputMode::Insert => write!(f, "INSERT"),
            InputMode::Command => write!(f, "COMMAND"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViewDensity {
    Compact,
    Detailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityLaneMode {
    Live,
    Cockpit,
}

#[derive(Debug, Clone)]
pub(crate) enum PickerKind {
    ModelProvider,
    ModelForProvider { provider: String },
    Personality,
    Skin,
    InteractiveQuestion { prompt: String },
}

#[derive(Debug, Clone)]
pub(crate) struct PickerItem {
    pub(crate) label: String,
    pub(crate) detail: String,
    pub(crate) value: String,
}

#[derive(Debug, Clone)]
pub(crate) struct PickerModal {
    pub(crate) kind: PickerKind,
    pub(crate) title: String,
    pub(crate) query: String,
    pub(crate) items: Vec<PickerItem>,
    pub(crate) filtered_indices: Vec<usize>,
    pub(crate) selected_filtered: usize,
    pub(crate) page_size: usize,
    pub(crate) allow_multi: bool,
    pub(crate) selected_values: HashSet<String>,
}

impl PickerModal {
    pub(crate) fn new(kind: PickerKind, title: impl Into<String>, items: Vec<PickerItem>) -> Self {
        let mut this = Self {
            kind,
            title: title.into(),
            query: String::new(),
            items,
            filtered_indices: Vec::new(),
            selected_filtered: 0,
            page_size: 10,
            allow_multi: false,
            selected_values: HashSet::new(),
        };
        this.refresh_filter();
        this
    }

    pub(crate) fn refresh_filter(&mut self) {
        let needle = self.query.trim().to_ascii_lowercase();
        if needle.is_empty() {
            self.filtered_indices = (0..self.items.len()).collect();
        } else {
            let mut ranked: Vec<(usize, i32)> = self
                .items
                .iter()
                .enumerate()
                .filter_map(|(idx, item)| {
                    let label = item.label.to_ascii_lowercase();
                    let detail = item.detail.to_ascii_lowercase();
                    if label == needle {
                        return Some((idx, 1200));
                    }
                    if label.starts_with(&needle) {
                        return Some((
                            idx,
                            1000 - (label.len().saturating_sub(needle.len()) as i32),
                        ));
                    }
                    if label.contains(&needle) {
                        return Some((idx, 850));
                    }
                    if detail.contains(&needle) {
                        return Some((idx, 700));
                    }
                    let subseq = fuzzy_subsequence_score(&needle, &label);
                    if subseq > 0 {
                        return Some((idx, 500 + subseq));
                    }
                    None
                })
                .collect();
            ranked.sort_by(|(a_idx, a_score), (b_idx, b_score)| {
                b_score
                    .cmp(a_score)
                    .then_with(|| self.items[*a_idx].label.cmp(&self.items[*b_idx].label))
            });
            self.filtered_indices = ranked.into_iter().map(|(idx, _)| idx).collect();
        }
        if self.filtered_indices.is_empty() {
            self.selected_filtered = 0;
        } else if self.selected_filtered >= self.filtered_indices.len() {
            self.selected_filtered = self.filtered_indices.len() - 1;
        }
    }

    pub(crate) fn selected_item(&self) -> Option<&PickerItem> {
        let idx = self.filtered_indices.get(self.selected_filtered).copied()?;
        self.items.get(idx)
    }

    pub(crate) fn move_selection(&mut self, delta: isize) {
        if self.filtered_indices.is_empty() {
            self.selected_filtered = 0;
            return;
        }
        let len = self.filtered_indices.len() as isize;
        let mut next = self.selected_filtered as isize + delta;
        while next < 0 {
            next += len;
        }
        next %= len;
        self.selected_filtered = next as usize;
    }

    pub(crate) fn page_move(&mut self, pages: isize) {
        let step = self.page_size.max(1) as isize;
        self.move_selection(pages * step);
    }

    pub(crate) fn visible_window(&self) -> (usize, usize) {
        if self.filtered_indices.is_empty() {
            return (0, 0);
        }
        let rows = self.page_size.max(1);
        let mut start = 0usize;
        if self.selected_filtered >= rows {
            start = self.selected_filtered + 1 - rows;
        }
        let end = (start + rows).min(self.filtered_indices.len());
        (start, end)
    }

    pub(crate) fn toggle_selected(&mut self) {
        if !self.allow_multi {
            return;
        }
        if let Some(value) = self.selected_item().map(|item| item.value.clone()) {
            if !self.selected_values.insert(value.clone()) {
                self.selected_values.remove(&value);
            }
        }
    }
}

/// Cached stable-prefix markdown for in-flight assistant streaming (Python `StreamingMd` parity).
#[derive(Debug, Clone, Default)]
pub(crate) struct StreamMarkdownCache {
    pub(crate) stable_prefix: String,
    pub(crate) stable_lines: Vec<Line<'static>>,
    pub(crate) cached_width: u16,
}

impl StreamMarkdownCache {
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModalAction {
    None,
    Close,
    Confirm,
    DisconnectProvider,
}

/// A section of tool output that can be folded/expanded.
#[derive(Debug, Clone)]
pub struct ToolOutputSection {
    /// Name of the tool that produced this output.
    pub tool_name: String,
    /// Full output text.
    pub output: String,
    /// Whether the section is expanded (showing full output).
    pub is_expanded: bool,
    /// Number of preview lines to show when collapsed.
    pub preview_lines: usize,
}

impl ToolOutputSection {
    pub fn new(tool_name: String, output: String) -> Self {
        Self {
            tool_name,
            output,
            is_expanded: false,
            preview_lines: 3,
        }
    }

    /// Get the display text (collapsed or expanded).
    pub fn display_text(&self) -> String {
        if self.is_expanded {
            self.output.clone()
        } else {
            let lines: Vec<&str> = self.output.lines().take(self.preview_lines).collect();
            let total_lines = self.output.lines().count();
            let mut text = lines.join("\n");
            if total_lines > self.preview_lines {
                text.push_str(&format!(
                    "\n  ... ({} more lines, press Enter to expand)",
                    total_lines - self.preview_lines
                ));
            }
            text
        }
    }
}

fn fuzzy_subsequence_score(needle: &str, haystack: &str) -> i32 {
    if needle.is_empty() || haystack.is_empty() {
        return 0;
    }
    let mut score = 0i32;
    let mut idx = 0usize;
    let chars: Vec<char> = haystack.chars().collect();
    for ch in needle.chars() {
        let mut found = false;
        while idx < chars.len() {
            if chars[idx] == ch {
                score += 2;
                if idx > 0 && chars[idx - 1] == '-' {
                    score += 1;
                }
                idx += 1;
                found = true;
                break;
            }
            idx += 1;
        }
        if !found {
            return 0;
        }
    }
    score
}
