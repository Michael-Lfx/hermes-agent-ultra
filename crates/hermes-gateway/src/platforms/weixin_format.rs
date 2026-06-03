//! Markdown formatting helpers for the Weixin (WeChat) platform adapter.
//!
//! WeChat does not render arbitrary Markdown. These functions normalise
//! headings, tables, long lines and blank-line runs so that messages look
//! good in the WeChat chat bubble.
//!
//! Aligns with Python `gateway/platforms/weixin.py` formatting functions
//! (`_rewrite_headers_for_weixin`, `_rewrite_table_block_for_weixin`,
//! `_normalize_markdown_blocks`, `_wrap_copy_friendly_lines_for_weixin`,
//! `_split_text_for_weixin_delivery`, `_pack_markdown_blocks_for_weixin`).

/// Maximum display-line width before word-wrap kicks in.
const WEIXIN_COPY_LINE_WIDTH: usize = 120;

/// Default maximum length of a single delivery unit (WeChat message).
pub const DEFAULT_MAX_DELIVERY_LENGTH: usize = 2000;

// ---------------------------------------------------------------------------
// Regex patterns (compiled lazily, match Python equivalents)
// ---------------------------------------------------------------------------

fn is_fence_line(stripped: &str) -> bool {
    // Python: _FENCE_RE = re.compile(r"^```([^\n`]*)\s*$")
    let s = stripped.trim();
    s.starts_with("```")
        && (s.len() == 3 || {
            let rest = &s[3..];
            !rest.contains('`') && !rest.contains('\n')
        })
}

fn is_table_rule_line(stripped: &str) -> bool {
    // Python: _TABLE_RULE_RE = re.compile(r"^\s*\|?(?:\s*:?-{3,}:?\s*\|)+\s*:?-{3,}:?\s*\|?\s*$")
    let s = stripped.trim();
    if s.is_empty() {
        return false;
    }
    let inner = s
        .strip_prefix('|')
        .unwrap_or(s)
        .strip_suffix('|')
        .unwrap_or(s.strip_prefix('|').unwrap_or(s));
    let cells: Vec<&str> = inner.split('|').map(|c| c.trim()).collect();
    if cells.is_empty() {
        return false;
    }
    cells.iter().all(|cell| {
        let c = cell.trim();
        !c.is_empty()
            && c.len() >= 3
            && c.chars()
                .all(|ch| ch == '-' || ch == ':' || ch == ' ')
            && c.contains('-')
    })
}

// ---------------------------------------------------------------------------
// Table helpers
// ---------------------------------------------------------------------------

fn split_table_row(line: &str) -> Vec<String> {
    let mut row = line.trim().to_string();
    if let Some(s) = row.strip_prefix('|') {
        row = s.to_string();
    }
    if let Some(s) = row.strip_suffix('|') {
        row = s.to_string();
    }
    row.split('|').map(|c| c.trim().to_string()).collect()
}

fn rewrite_table_block(lines: &[&str]) -> String {
    if lines.len() < 2 {
        return lines.join("\n");
    }
    let headers = split_table_row(lines[0]);
    let body_rows: Vec<Vec<String>> = lines[2..]
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| split_table_row(l))
        .collect();
    if headers.is_empty() || body_rows.is_empty() {
        return lines.join("\n");
    }

    let mut formatted_rows: Vec<String> = Vec::new();
    for row in &body_rows {
        let mut pairs: Vec<(String, String)> = Vec::new();
        for (idx, header) in headers.iter().enumerate() {
            if idx >= row.len() {
                break;
            }
            let label = if header.is_empty() {
                format!("Column {}", idx + 1)
            } else {
                header.clone()
            };
            let value = row[idx].trim().to_string();
            if !value.is_empty() {
                pairs.push((label, value));
            }
        }
        if pairs.is_empty() {
            continue;
        }
        match pairs.len() {
            1 => {
                formatted_rows.push(format!("- {}: {}", pairs[0].0, pairs[0].1));
            }
            2 => {
                formatted_rows.push(format!("- {}: {}", pairs[0].0, pairs[0].1));
                formatted_rows.push(format!("  {}: {}", pairs[1].0, pairs[1].1));
            }
            _ => {
                let summary: String = pairs
                    .iter()
                    .map(|(l, v)| format!("{l}: {v}"))
                    .collect::<Vec<_>>()
                    .join(" | ");
                formatted_rows.push(format!("- {summary}"));
            }
        }
    }
    if formatted_rows.is_empty() {
        lines.join("\n")
    } else {
        formatted_rows.join("\n")
    }
}

// ---------------------------------------------------------------------------
// Heading rewrite
// ---------------------------------------------------------------------------

fn rewrite_header_line(line: &str) -> String {
    // Python: _HEADER_RE = re.compile(r"^(#{1,6})\s+(.+?)\s*$")
    let trimmed = line.trim_end();
    let hash_count = trimmed.chars().take_while(|&c| c == '#').count();
    if hash_count == 0 || hash_count > 6 {
        return trimmed.to_string();
    }
    let rest = &trimmed[hash_count..];
    if !rest.starts_with(' ') && !rest.starts_with('\t') {
        return trimmed.to_string();
    }
    let title = rest.trim();
    if title.is_empty() {
        return trimmed.to_string();
    }
    if hash_count == 1 {
        format!("\u{3010}{title}\u{3011}") // 【title】
    } else {
        format!("**{title}**")
    }
}

// ---------------------------------------------------------------------------
// Line wrapping
// ---------------------------------------------------------------------------

fn wrap_long_line(line: &str, width: usize) -> Vec<String> {
    if line.chars().count() <= width {
        return vec![line.to_string()];
    }
    let mut result: Vec<String> = Vec::new();
    let mut remaining = line.to_string();
    while remaining.chars().count() > width {
        if let Some(pos) = remaining.rfind(' ') {
            if pos == 0 {
                // No useful break; split by char
                let split_at = remaining
                    .char_indices()
                    .nth(width)
                    .map(|(i, _)| i)
                    .unwrap_or(remaining.len());
                result.push(remaining[..split_at].to_string());
                remaining = remaining[split_at..].to_string();
            } else {
                result.push(remaining[..pos].to_string());
                remaining = remaining[pos + 1..].to_string();
            }
        } else {
            let split_at = remaining
                .char_indices()
                .nth(width)
                .map(|(i, _)| i)
                .unwrap_or(remaining.len());
            result.push(remaining[..split_at].to_string());
            remaining = remaining[split_at..].to_string();
        }
    }
    if !remaining.is_empty() {
        result.push(remaining);
    }
    result
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Format a Markdown message for display in WeChat.
///
/// Steps (applied in order):
/// 1. Rewrite headings: `# Title` -> `【Title】`, `## Title` -> `**Title**`
/// 2. Convert Markdown table blocks to indented list format
/// 3. Collapse multiple consecutive blank lines into a single blank line
/// 4. Wrap lines longer than 120 characters at word boundaries
///
/// Fenced code blocks (```...```) are always preserved verbatim.
pub fn format_message_for_weixin(content: &str) -> String {
    if content.is_empty() {
        return String::new();
    }

    let lines: Vec<&str> = content.lines().collect();

    // --- Pass 1: heading rewrite + table conversion (code-block aware) -----
    let mut pass1: Vec<String> = Vec::with_capacity(lines.len());
    let mut in_code_block = false;
    let mut i = 0usize;

    while i < lines.len() {
        let line = lines[i];
        let stripped = line.trim();

        // Fence toggle
        if is_fence_line(stripped) {
            if in_code_block {
                // Closing fence
                in_code_block = false;
                pass1.push(line.to_string());
                i += 1;
                continue;
            } else {
                // Opening fence — flush any pending table accumulation first
                in_code_block = true;
                pass1.push(line.to_string());
                i += 1;
                continue;
            }
        }

        if in_code_block {
            pass1.push(line.to_string());
            i += 1;
            continue;
        }

        // Heading rewrite (only at line start, outside code blocks)
        if stripped.starts_with('#') {
            pass1.push(rewrite_header_line(line));
            i += 1;
            continue;
        }

        // Table block detection & conversion
        if stripped.starts_with('|') {
            let mut table_lines: Vec<&str> = Vec::new();
            let mut j = i;
            while j < lines.len() && lines[j].trim().starts_with('|') {
                table_lines.push(lines[j]);
                j += 1;
            }
            if table_lines.len() >= 3 && is_table_rule_line(table_lines[1].trim()) {
                let refs: Vec<&str> = table_lines.iter().copied().collect();
                pass1.push(rewrite_table_block(&refs));
                i = j;
                continue;
            }
            // Not a real table — emit lines as-is
            for tl in &table_lines {
                pass1.push(tl.to_string());
            }
            i = j;
            continue;
        }

        pass1.push(line.to_string());
        i += 1;
    }

    // --- Pass 2: collapse consecutive blank lines (code-block aware) -------
    let mut pass2: Vec<String> = Vec::with_capacity(pass1.len());
    in_code_block = false;
    let mut blank_run = 0u32;
    for line in &pass1 {
        let stripped = line.trim();
        if is_fence_line(stripped) {
            in_code_block = !in_code_block;
            pass2.push(line.clone());
            blank_run = 0;
            continue;
        }
        if in_code_block {
            pass2.push(line.clone());
            continue;
        }
        if stripped.is_empty() {
            blank_run += 1;
            if blank_run <= 1 {
                pass2.push(String::new());
            }
        } else {
            blank_run = 0;
            pass2.push(line.clone());
        }
    }

    // --- Pass 3: wrap long display lines (code-block aware) ----------------
    let mut pass3: Vec<String> = Vec::with_capacity(pass2.len());
    in_code_block = false;
    for line in &pass2 {
        let stripped = line.trim();
        if is_fence_line(stripped) {
            in_code_block = !in_code_block;
            pass3.push(line.clone());
            continue;
        }
        if in_code_block
            || line.chars().count() <= WEIXIN_COPY_LINE_WIDTH
            || stripped.is_empty()
            || stripped.starts_with('|')
            || is_table_rule_line(stripped)
        {
            pass3.push(line.clone());
            continue;
        }
        let wrapped = wrap_long_line(line, WEIXIN_COPY_LINE_WIDTH);
        pass3.extend(wrapped);
    }

    let joined = pass3.join("\n");
    joined.trim().to_string()
}

// ---------------------------------------------------------------------------
// Splitting helpers
// ---------------------------------------------------------------------------

/// Split content into Markdown blocks separated by blank lines.
/// Fenced code blocks are kept intact as single blocks.
fn split_markdown_blocks(content: &str) -> Vec<String> {
    if content.is_empty() {
        return Vec::new();
    }
    let mut blocks: Vec<String> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut in_code_block = false;

    for raw_line in content.lines() {
        let line = raw_line.trim_end();
        if is_fence_line(line.trim()) {
            if !in_code_block && !current.is_empty() {
                let block = current.join("\n");
                let trimmed = block.trim().to_string();
                if !trimmed.is_empty() {
                    blocks.push(trimmed);
                }
                current.clear();
            }
            current.push(line.to_string());
            in_code_block = !in_code_block;
            if !in_code_block {
                let block = current.join("\n");
                let trimmed = block.trim().to_string();
                if !trimmed.is_empty() {
                    blocks.push(trimmed);
                }
                current.clear();
            }
            continue;
        }
        if in_code_block {
            current.push(line.to_string());
            continue;
        }
        if line.trim().is_empty() {
            if !current.is_empty() {
                let block = current.join("\n");
                let trimmed = block.trim().to_string();
                if !trimmed.is_empty() {
                    blocks.push(trimmed);
                }
                current.clear();
            }
            continue;
        }
        current.push(line.to_string());
    }
    if !current.is_empty() {
        let block = current.join("\n");
        let trimmed = block.trim().to_string();
        if !trimmed.is_empty() {
            blocks.push(trimmed);
        }
    }
    blocks
}

/// Split content into delivery units for WeChat.
///
/// * Each Markdown block (paragraph / list / table conversion) stays in one unit when it fits.
/// * Adjacent units are merged up to `max_length` so replies are not one bubble per line.
/// * Code blocks are never split unless they exceed `max_length`.
pub fn split_delivery_units(content: &str, max_length: usize) -> Vec<String> {
    if content.is_empty() {
        return Vec::new();
    }

    let mut units: Vec<String> = Vec::new();

    for block in split_markdown_blocks(content) {
        if block.chars().count() <= max_length {
            units.push(block);
        } else {
            units.extend(force_split_block(&block, max_length));
        }
    }

    coalesce_delivery_units(units, max_length)
        .into_iter()
        .filter(|u| !u.is_empty())
        .collect()
}

/// Merge adjacent delivery units up to `max_length` (double-newline between merged blocks).
fn coalesce_delivery_units(units: Vec<String>, max_length: usize) -> Vec<String> {
    let mut packed: Vec<String> = Vec::new();
    let mut current = String::new();

    for unit in units {
        let candidate = if current.is_empty() {
            unit.clone()
        } else {
            format!("{current}\n\n{unit}")
        };
        if candidate.chars().count() <= max_length {
            current = candidate;
            continue;
        }
        if !current.is_empty() {
            packed.push(current);
            current = String::new();
        }
        if unit.chars().count() <= max_length {
            current = unit;
        } else {
            packed.extend(force_split_block(&unit, max_length));
        }
    }
    if !current.is_empty() {
        packed.push(current);
    }
    packed
}

/// Force-split an oversized block (possibly a code block) at line boundaries.
fn force_split_block(block: &str, max_length: usize) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();

    for line in block.lines() {
        if !current.is_empty() && current.chars().count() + 1 + line.chars().count() > max_length {
            parts.push(current);
            current = String::new();
        }
        if line.chars().count() > max_length {
            if !current.is_empty() {
                parts.push(current);
                current = String::new();
            }
            // Split oversized line at char boundaries
            let mut remaining = line.to_string();
            while remaining.chars().count() > max_length {
                let split_at = remaining
                    .char_indices()
                    .nth(max_length)
                    .map(|(i, _)| i)
                    .unwrap_or(remaining.len());
                parts.push(remaining[..split_at].to_string());
                remaining = remaining[split_at..].to_string();
            }
            if !remaining.is_empty() {
                current = remaining;
            }
        } else if current.is_empty() {
            current = line.to_string();
        } else {
            current.push('\n');
            current.push_str(line);
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_heading_rewrite() {
        assert_eq!(format_message_for_weixin("# Hello"), "\u{3010}Hello\u{3011}");
        assert_eq!(format_message_for_weixin("## World"), "**World**");
        assert_eq!(format_message_for_weixin("### Sub"), "**Sub**");
    }

    #[test]
    fn format_table_conversion() {
        let input = "| Name | Value |\n| --- | --- |\n| foo | bar |";
        let expected = "- Name: foo\n  Value: bar";
        assert_eq!(format_message_for_weixin(input), expected);
    }

    #[test]
    fn format_preserves_code_blocks() {
        let input = "```\n# not a heading\n```\n# real heading";
        let output = format_message_for_weixin(input);
        assert!(output.contains("# not a heading"));
        assert!(output.contains("\u{3010}real heading\u{3011}"));
    }

    #[test]
    fn format_collapses_blank_lines() {
        let input = "line1\n\n\n\nline2";
        let output = format_message_for_weixin(input);
        assert_eq!(output, "line1\n\nline2");
    }

    #[test]
    fn format_wraps_long_lines() {
        let long_line = "a ".repeat(100); // 200 chars
        let output = format_message_for_weixin(&long_line);
        for line in output.lines() {
            assert!(line.chars().count() <= WEIXIN_COPY_LINE_WIDTH);
        }
    }

    #[test]
    fn format_does_not_wrap_code_block_lines() {
        let long_code = format!("```\n{}\n```", "x".repeat(200));
        let output = format_message_for_weixin(&long_code);
        assert!(output.contains(&"x".repeat(200)));
    }

    #[test]
    fn format_empty_string() {
        assert_eq!(format_message_for_weixin(""), "");
    }

    #[test]
    fn split_units_basic() {
        let input = "block one\n\nblock two\n\nblock three";
        let units = split_delivery_units(input, 2000);
        assert_eq!(units.len(), 1);
        assert!(units[0].contains("block one"));
        assert!(units[0].contains("block three"));
    }

    #[test]
    fn split_units_code_block_stays_together() {
        let input = "text\n\n```\ncode\nline\n```\n\nmore text";
        let units = split_delivery_units(input, 2000);
        assert!(units.iter().any(|u| u.contains("```\ncode\nline\n```")));
    }

    #[test]
    fn split_units_oversized_block_gets_split() {
        let big_block = (0..100)
            .map(|i| format!("line {i} with some text to make it longer"))
            .collect::<Vec<_>>()
            .join("\n\n");
        let units = split_delivery_units(&big_block, 200);
        assert!(units.len() > 1);
        for u in &units {
            assert!(u.chars().count() <= 200, "unit too long: {} chars", u.chars().count());
        }
    }

    #[test]
    fn split_units_empty_input() {
        assert!(split_delivery_units("", 2000).is_empty());
    }

    #[test]
    fn split_units_continuation_lines_stay_with_parent() {
        let input = "- item one\n  continued\n- item two\n  continued";
        let units = split_delivery_units(input, 2000);
        assert_eq!(units.len(), 1);
        assert!(units[0].contains("item one"));
        assert!(units[0].contains("item two"));
    }

    #[test]
    fn split_units_book_list_not_one_bubble_per_line() {
        let input = "**男频精选**\n- 书名: **部族荣光**\n  作者: 丧狐\n- 书名: **莫若凌霄**\n  作者: 月关\n- 书名: **高手下山**\n  作者: 小殇殇";
        let units = split_delivery_units(input, 2000);
        assert_eq!(units.len(), 1);
        assert!(units[0].contains("部族荣光"));
        assert!(units[0].contains("莫若凌霄"));
    }

    #[test]
    fn split_units_coalesce_stops_at_max_length() {
        let block_a = "a".repeat(1200);
        let block_b = "b".repeat(1200);
        let input = format!("{block_a}\n\n{block_b}");
        let units = split_delivery_units(&input, 2000);
        assert_eq!(units.len(), 2);
        for u in &units {
            assert!(u.chars().count() <= 2000);
        }
    }

    /// Regression: fanqie-style agent reply (~800 chars, tables + sections) must not fan out
    /// into one WeChat bubble per line (see weixin delivery incident 2026-06-03).
    #[test]
    fn split_units_fanqie_recommendation_reply_fits_few_bubbles() {
        let input = r"好的！我抓取到了番茄小说首页今天（6月3日）的推荐内容，给你整理一下 👇

---

## 📚 男频精选

| 书名 | 作者 |
|------|------|
| **部族荣光** | 丧狐 |
| **莫若凌霄** | 月关 |
| **高手下山，我有九个无敌师父！** | 小殇殇 |

## 📚 女频精选

| 书名 | 作者 |
|------|------|
| **萤火之城** | 童童 |
| **娇养玫瑰** | 美绿哔哔 |

---

有感兴趣的吗？我可以帮你搜搜具体某本书的简介或章节内容～";
        let formatted = format_message_for_weixin(input);
        let units = split_delivery_units(&formatted, DEFAULT_MAX_DELIVERY_LENGTH);
        assert!(
            units.len() <= 3,
            "expected <=3 WeChat bubbles, got {}: {:?}",
            units.len(),
            units.iter().map(|u| u.chars().count()).collect::<Vec<_>>()
        );
        let joined = units.join("\n\n");
        assert!(joined.contains("部族荣光"));
        assert!(joined.contains("萤火之城"));
    }
}
