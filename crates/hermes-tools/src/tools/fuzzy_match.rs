//! Fuzzy matching helpers for file patch operations.
//!
//! Corresponds to `hermes-agent/tools/fuzzy_match.py`.

use std::collections::HashMap;

const UNICODE_MAP: &[(char, &str)] = &[
    ('\u{201c}', "\""),
    ('\u{201d}', "\""),
    ('\u{2018}', "'"),
    ('\u{2019}', "'"),
    ('\u{2014}', "--"),
    ('\u{2013}', "-"),
    ('\u{2026}', "..."),
    ('\u{00a0}', " "),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuzzyReplaceResult {
    pub content: String,
    pub match_count: usize,
    pub strategy: Option<&'static str>,
    pub error: Option<String>,
}

pub fn fuzzy_find_and_replace(
    content: &str,
    old_string: &str,
    new_string: &str,
    replace_all: bool,
) -> FuzzyReplaceResult {
    if old_string.is_empty() {
        return failed(content, "old_string cannot be empty");
    }
    if old_string == new_string {
        return failed(content, "old_string and new_string are identical");
    }

    let strategies: [(&'static str, fn(&str, &str) -> Vec<(usize, usize)>); 9] = [
        ("exact", strategy_exact),
        ("line_trimmed", strategy_line_trimmed),
        ("whitespace_normalized", strategy_whitespace_normalized),
        ("indentation_flexible", strategy_indentation_flexible),
        ("escape_normalized", strategy_escape_normalized),
        ("trimmed_boundary", strategy_trimmed_boundary),
        ("unicode_normalized", strategy_unicode_normalized),
        ("block_anchor", strategy_block_anchor),
        ("context_aware", strategy_context_aware),
    ];

    for (strategy_name, strategy_fn) in strategies {
        let matches = strategy_fn(content, old_string);
        if matches.is_empty() {
            continue;
        }
        if matches.len() > 1 && !replace_all {
            return failed(
                content,
                format!(
                    "Found {} matches for old_string. Provide more context to make it unique, or use replace_all=True.",
                    matches.len()
                ),
            );
        }
        if strategy_name != "exact" {
            if let Some(error) = detect_escape_drift(content, &matches, old_string, new_string) {
                return failed(content, error);
            }
        }

        let effective_new = maybe_unescape_new_string(new_string, content, &matches);
        let old_for_reindent = (strategy_name != "exact").then_some(old_string);
        let new_content = apply_replacements(content, &matches, &effective_new, old_for_reindent);
        return FuzzyReplaceResult {
            content: new_content,
            match_count: matches.len(),
            strategy: Some(strategy_name),
            error: None,
        };
    }

    failed(content, "Could not find a match for old_string in the file")
}

pub fn find_closest_lines(
    old_string: &str,
    content: &str,
    context_lines: usize,
    max_results: usize,
) -> String {
    if old_string.is_empty() || content.is_empty() {
        return String::new();
    }
    let old_lines: Vec<&str> = old_string.lines().collect();
    let content_lines: Vec<&str> = content.lines().collect();
    if old_lines.is_empty() || content_lines.is_empty() {
        return String::new();
    }
    let anchor = old_lines
        .iter()
        .map(|line| line.trim())
        .find(|line| !line.is_empty())
        .unwrap_or("");
    if anchor.is_empty() {
        return String::new();
    }

    let mut scored: Vec<(f64, usize)> = content_lines
        .iter()
        .enumerate()
        .filter_map(|(idx, line)| {
            let stripped = line.trim();
            if stripped.is_empty() {
                return None;
            }
            let ratio = sequence_similarity(anchor, stripped);
            (ratio > 0.3).then_some((ratio, idx))
        })
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut parts = Vec::new();
    let mut seen = Vec::new();
    for (_, line_idx) in scored.into_iter().take(max_results) {
        let start = line_idx.saturating_sub(context_lines);
        let end = (line_idx + old_lines.len() + context_lines).min(content_lines.len());
        if seen.contains(&(start, end)) {
            continue;
        }
        seen.push((start, end));
        let snippet = (start..end)
            .map(|idx| format!("{:4}| {}", idx + 1, content_lines[idx]))
            .collect::<Vec<_>>()
            .join("\n");
        parts.push(snippet);
    }
    parts.join("\n---\n")
}

pub fn format_no_match_hint(
    error: Option<&str>,
    match_count: usize,
    old_string: &str,
    content: &str,
) -> String {
    if match_count != 0 || !error.is_some_and(|e| e.starts_with("Could not find")) {
        return String::new();
    }
    let hint = find_closest_lines(old_string, content, 2, 3);
    if hint.is_empty() {
        String::new()
    } else {
        format!("\n\nDid you mean one of these sections?\n{hint}")
    }
}

fn failed(content: &str, error: impl Into<String>) -> FuzzyReplaceResult {
    FuzzyReplaceResult {
        content: content.to_string(),
        match_count: 0,
        strategy: None,
        error: Some(error.into()),
    }
}

fn unicode_normalize(text: &str) -> String {
    let replacements: HashMap<char, &str> = UNICODE_MAP.iter().copied().collect();
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if let Some(repl) = replacements.get(&ch) {
            out.push_str(repl);
        } else {
            out.push(ch);
        }
    }
    out
}

fn strategy_exact(content: &str, pattern: &str) -> Vec<(usize, usize)> {
    content
        .match_indices(pattern)
        .map(|(start, _)| (start, start + pattern.len()))
        .collect()
}

fn strategy_line_trimmed(content: &str, pattern: &str) -> Vec<(usize, usize)> {
    let content_lines: Vec<&str> = content.split('\n').collect();
    let normalized_lines: Vec<String> = content_lines
        .iter()
        .map(|line| line.trim().to_string())
        .collect();
    let pattern_normalized = pattern
        .split('\n')
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("\n");
    find_normalized_line_matches(
        content,
        &content_lines,
        &normalized_lines,
        &pattern_normalized,
    )
}

fn strategy_whitespace_normalized(content: &str, pattern: &str) -> Vec<(usize, usize)> {
    let pattern_normalized = collapse_spaces_tabs(pattern);
    let (content_normalized, norm_to_orig) = collapse_spaces_tabs_with_map(content);
    let norm_matches = strategy_exact(&content_normalized, &pattern_normalized);
    norm_matches
        .into_iter()
        .filter_map(|(start, end)| {
            let orig_start = *norm_to_orig.get(start)?;
            let orig_end = norm_to_orig
                .get(end)
                .copied()
                .or_else(|| norm_to_orig.last().copied())
                .unwrap_or(content.len());
            Some((orig_start, orig_end))
        })
        .collect()
}

fn strategy_indentation_flexible(content: &str, pattern: &str) -> Vec<(usize, usize)> {
    let content_lines: Vec<&str> = content.split('\n').collect();
    let stripped_lines: Vec<String> = content_lines
        .iter()
        .map(|line| line.trim_start_matches([' ', '\t']).to_string())
        .collect();
    let pattern_normalized = pattern
        .split('\n')
        .map(|line| line.trim_start_matches([' ', '\t']))
        .collect::<Vec<_>>()
        .join("\n");
    find_normalized_line_matches(
        content,
        &content_lines,
        &stripped_lines,
        &pattern_normalized,
    )
}

fn strategy_escape_normalized(content: &str, pattern: &str) -> Vec<(usize, usize)> {
    let unescaped = pattern
        .replace("\\n", "\n")
        .replace("\\t", "\t")
        .replace("\\r", "\r");
    if unescaped == pattern {
        Vec::new()
    } else {
        strategy_exact(content, &unescaped)
    }
}

fn strategy_trimmed_boundary(content: &str, pattern: &str) -> Vec<(usize, usize)> {
    let mut pattern_lines: Vec<String> = pattern.split('\n').map(str::to_string).collect();
    if pattern_lines.is_empty() {
        return Vec::new();
    }
    pattern_lines[0] = pattern_lines[0].trim().to_string();
    if pattern_lines.len() > 1 {
        let last = pattern_lines.len() - 1;
        pattern_lines[last] = pattern_lines[last].trim().to_string();
    }
    let modified_pattern = pattern_lines.join("\n");
    let content_lines: Vec<&str> = content.split('\n').collect();
    let count = pattern_lines.len();
    let mut matches = Vec::new();
    for idx in 0..=content_lines.len().saturating_sub(count) {
        let mut check: Vec<String> = content_lines[idx..idx + count]
            .iter()
            .map(|line| (*line).to_string())
            .collect();
        check[0] = check[0].trim().to_string();
        if check.len() > 1 {
            let last = check.len() - 1;
            check[last] = check[last].trim().to_string();
        }
        if check.join("\n") == modified_pattern {
            matches.push(calculate_line_positions(
                &content_lines,
                idx,
                idx + count,
                content.len(),
            ));
        }
    }
    matches
}

fn strategy_unicode_normalized(content: &str, pattern: &str) -> Vec<(usize, usize)> {
    let norm_content = unicode_normalize(content);
    let norm_pattern = unicode_normalize(pattern);
    if norm_content == content && norm_pattern == pattern {
        return Vec::new();
    }
    let mut norm_matches = strategy_exact(&norm_content, &norm_pattern);
    if norm_matches.is_empty() {
        norm_matches = strategy_line_trimmed(&norm_content, &norm_pattern);
    }
    map_normalized_unicode_positions(content, &norm_matches)
}

fn strategy_block_anchor(content: &str, pattern: &str) -> Vec<(usize, usize)> {
    let norm_content = unicode_normalize(content);
    let norm_pattern = unicode_normalize(pattern);
    let pattern_lines: Vec<&str> = norm_pattern.split('\n').collect();
    if pattern_lines.len() < 2 {
        return Vec::new();
    }
    let norm_content_lines: Vec<&str> = norm_content.split('\n').collect();
    let orig_content_lines: Vec<&str> = content.split('\n').collect();
    let first = pattern_lines[0].trim();
    let last = pattern_lines[pattern_lines.len() - 1].trim();
    let count = pattern_lines.len();
    let candidates: Vec<usize> = (0..=norm_content_lines.len().saturating_sub(count))
        .filter(|&idx| {
            norm_content_lines[idx].trim() == first
                && norm_content_lines[idx + count - 1].trim() == last
        })
        .collect();
    let threshold = if candidates.len() == 1 { 0.50 } else { 0.70 };
    candidates
        .into_iter()
        .filter_map(|idx| {
            let similarity = if count <= 2 {
                1.0
            } else {
                sequence_similarity(
                    &norm_content_lines[idx + 1..idx + count - 1].join("\n"),
                    &pattern_lines[1..count - 1].join("\n"),
                )
            };
            (similarity >= threshold).then(|| {
                calculate_line_positions(&orig_content_lines, idx, idx + count, content.len())
            })
        })
        .collect()
}

fn strategy_context_aware(content: &str, pattern: &str) -> Vec<(usize, usize)> {
    let pattern_lines: Vec<&str> = pattern.split('\n').collect();
    let content_lines: Vec<&str> = content.split('\n').collect();
    if pattern_lines.is_empty() {
        return Vec::new();
    }
    let count = pattern_lines.len();
    let mut matches = Vec::new();
    for idx in 0..=content_lines.len().saturating_sub(count) {
        let high_similarity = pattern_lines
            .iter()
            .zip(&content_lines[idx..idx + count])
            .filter(|(expected, actual)| {
                sequence_similarity(expected.trim(), actual.trim()) >= 0.80
            })
            .count();
        if (high_similarity as f64) >= (pattern_lines.len() as f64 * 0.5) {
            matches.push(calculate_line_positions(
                &content_lines,
                idx,
                idx + count,
                content.len(),
            ));
        }
    }
    matches
}

fn find_normalized_line_matches(
    content: &str,
    content_lines: &[&str],
    normalized_lines: &[String],
    pattern_normalized: &str,
) -> Vec<(usize, usize)> {
    let pattern_lines: Vec<&str> = pattern_normalized.split('\n').collect();
    let count = pattern_lines.len();
    let mut matches = Vec::new();
    for idx in 0..=normalized_lines.len().saturating_sub(count) {
        if normalized_lines[idx..idx + count].join("\n") == pattern_normalized {
            matches.push(calculate_line_positions(
                content_lines,
                idx,
                idx + count,
                content.len(),
            ));
        }
    }
    matches
}

fn calculate_line_positions(
    content_lines: &[&str],
    start_line: usize,
    end_line: usize,
    content_length: usize,
) -> (usize, usize) {
    let start = content_lines[..start_line]
        .iter()
        .map(|line| line.len() + 1)
        .sum();
    let end = content_lines[..end_line]
        .iter()
        .map(|line| line.len() + 1)
        .sum::<usize>()
        .saturating_sub(1)
        .min(content_length);
    (start, end)
}

fn collapse_spaces_tabs(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_run = false;
    for ch in text.chars() {
        if ch == ' ' || ch == '\t' {
            if !in_run {
                out.push(' ');
                in_run = true;
            }
        } else {
            out.push(ch);
            in_run = false;
        }
    }
    out
}

fn collapse_spaces_tabs_with_map(text: &str) -> (String, Vec<usize>) {
    let mut out = String::with_capacity(text.len());
    let mut map = Vec::new();
    let mut in_run = false;
    for (idx, ch) in text.char_indices() {
        if ch == ' ' || ch == '\t' {
            if !in_run {
                map.push(idx);
                out.push(' ');
                in_run = true;
            }
        } else {
            map.push(idx);
            out.push(ch);
            in_run = false;
        }
    }
    map.push(text.len());
    (out, map)
}

fn normalized_char_map(original: &str) -> Vec<usize> {
    let replacements: HashMap<char, &str> = UNICODE_MAP.iter().copied().collect();
    let mut map = Vec::new();
    let mut norm_pos = 0;
    for ch in original.chars() {
        map.push(norm_pos);
        norm_pos += replacements
            .get(&ch)
            .map_or(ch.len_utf8(), |repl| repl.len());
    }
    map.push(norm_pos);
    map
}

fn map_normalized_unicode_positions(
    original: &str,
    norm_matches: &[(usize, usize)],
) -> Vec<(usize, usize)> {
    let orig_to_norm = normalized_char_map(original);
    let original_byte_positions: Vec<usize> = original
        .char_indices()
        .map(|(idx, _)| idx)
        .chain(std::iter::once(original.len()))
        .collect();
    let mut norm_to_orig = HashMap::new();
    for (orig_char_idx, norm_pos) in orig_to_norm.iter().take(orig_to_norm.len() - 1).enumerate() {
        norm_to_orig.entry(*norm_pos).or_insert(orig_char_idx);
    }

    let mut results = Vec::new();
    for &(norm_start, norm_end) in norm_matches {
        let Some(&orig_start_char) = norm_to_orig.get(&norm_start) else {
            continue;
        };
        let mut orig_end_char = orig_start_char;
        while orig_end_char < orig_to_norm.len() - 1 && orig_to_norm[orig_end_char] < norm_end {
            orig_end_char += 1;
        }
        results.push((
            original_byte_positions[orig_start_char],
            original_byte_positions[orig_end_char],
        ));
    }
    results
}

fn detect_escape_drift(
    content: &str,
    matches: &[(usize, usize)],
    old_string: &str,
    new_string: &str,
) -> Option<String> {
    if !new_string.contains("\\'") && !new_string.contains("\\\"") {
        return None;
    }
    let matched_regions = matches
        .iter()
        .map(|&(start, end)| &content[start..end])
        .collect::<String>();
    for suspect in ["\\'", "\\\""] {
        if new_string.contains(suspect)
            && old_string.contains(suspect)
            && !matched_regions.contains(suspect)
        {
            let plain = &suspect[1..];
            return Some(format!(
                "Escape-drift detected: old_string and new_string contain the literal sequence {suspect:?} but the matched region of the file does not. This is almost always a tool-call serialization artifact where an apostrophe or quote got prefixed with a spurious backslash. Re-read the file with read_file and pass old_string/new_string without backslash-escaping {plain:?} characters."
            ));
        }
    }
    None
}

fn maybe_unescape_new_string(
    new_string: &str,
    content: &str,
    matches: &[(usize, usize)],
) -> String {
    if !new_string.contains("\\t") && !new_string.contains("\\r") {
        return new_string.to_string();
    }
    let matched_regions = matches
        .iter()
        .map(|&(start, end)| &content[start..end])
        .collect::<String>();
    let mut out = new_string.to_string();
    if out.contains("\\t") && matched_regions.contains('\t') {
        out = out.replace("\\t", "\t");
    }
    if out.contains("\\r") && matched_regions.contains('\r') {
        out = out.replace("\\r", "\r");
    }
    out
}

fn apply_replacements(
    content: &str,
    matches: &[(usize, usize)],
    new_string: &str,
    old_string: Option<&str>,
) -> String {
    let mut result = content.to_string();
    let mut sorted = matches.to_vec();
    sorted.sort_by(|a, b| b.0.cmp(&a.0));
    for (start, end) in sorted {
        let adjusted = old_string
            .map(|old| reindent_replacement(&content[start..end], old, new_string))
            .unwrap_or_else(|| new_string.to_string());
        result.replace_range(start..end, &adjusted);
    }
    result
}

fn leading_whitespace(line: &str) -> &str {
    let end = line
        .char_indices()
        .take_while(|(_, ch)| *ch == ' ' || *ch == '\t')
        .map(|(idx, ch)| idx + ch.len_utf8())
        .last()
        .unwrap_or(0);
    &line[..end]
}

fn first_meaningful_line(text: &str) -> Option<&str> {
    text.split('\n').find(|line| !line.trim().is_empty())
}

fn reindent_replacement(file_region: &str, old_string: &str, new_string: &str) -> String {
    if new_string.is_empty() {
        return new_string.to_string();
    }
    let Some(old_first) = first_meaningful_line(old_string) else {
        return new_string.to_string();
    };
    let Some(file_first) = first_meaningful_line(file_region) else {
        return new_string.to_string();
    };
    let old_indent = leading_whitespace(old_first);
    let file_indent = leading_whitespace(file_first);
    if old_indent == file_indent {
        return new_string.to_string();
    }
    new_string
        .split('\n')
        .map(|line| {
            if line.trim().is_empty() {
                line.to_string()
            } else if let Some(remainder) = line.strip_prefix(old_indent) {
                format!("{file_indent}{remainder}")
            } else {
                format!("{}{}", file_indent, line.trim_start_matches([' ', '\t']))
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn sequence_similarity(a: &str, b: &str) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let lcs = lcs_length(&a_chars, &b_chars);
    (2.0 * lcs as f64) / (a_chars.len() + b_chars.len()) as f64
}

fn lcs_length(a: &[char], b: &[char]) -> usize {
    let mut prev = vec![0; b.len() + 1];
    let mut curr = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            curr[j] = if a[i - 1] == b[j - 1] {
                prev[j - 1] + 1
            } else {
                prev[j].max(curr[j - 1])
            };
        }
        std::mem::swap(&mut prev, &mut curr);
        curr.fill(0);
    }
    *prev.iter().max().unwrap_or(&0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_replaces_unique_match() {
        let result = fuzzy_find_and_replace("hello world", "world", "rust", false);
        assert_eq!(result.content, "hello rust");
        assert_eq!(result.match_count, 1);
        assert_eq!(result.strategy, Some("exact"));
        assert!(result.error.is_none());
    }

    #[test]
    fn ambiguous_exact_requires_replace_all() {
        let result = fuzzy_find_and_replace("x x x", "x", "y", false);
        assert_eq!(result.content, "x x x");
        assert!(result.error.unwrap().contains("Found 3 matches"));
    }

    #[test]
    fn replace_all_replaces_multiple_matches() {
        let result = fuzzy_find_and_replace("x x x", "x", "y", true);
        assert_eq!(result.content, "y y y");
        assert_eq!(result.match_count, 3);
    }

    #[test]
    fn line_trimmed_matches_and_reindents() {
        let content = "    println!(\"hi\");\n";
        let old = "  println!(\"hi\");  ";
        let new = "  println!(\"bye\");";
        let result = fuzzy_find_and_replace(content, old, new, false);
        assert_eq!(result.strategy, Some("line_trimmed"));
        assert_eq!(result.content, "    println!(\"bye\");\n");
    }

    #[test]
    fn unicode_normalized_matches_smart_quotes() {
        let content = "let s = “hello”;\n";
        let result =
            fuzzy_find_and_replace(content, "let s = \"hello\";", "let s = \"bye\";", false);
        assert_eq!(result.strategy, Some("unicode_normalized"));
        assert_eq!(result.content, "let s = \"bye\";\n");
    }

    #[test]
    fn escape_drift_is_blocked_for_non_exact_match() {
        let content = "name = 'alice'\n";
        let old = " name = \\'alice\\'";
        let new = " name = \\'bob\\'";
        let result = fuzzy_find_and_replace(content, old, new, false);
        assert!(result.error.unwrap().contains("Escape-drift detected"));
    }

    #[test]
    fn tab_escape_is_unescaped_when_file_region_uses_real_tab() {
        let content = "\tlet x = 1;\n";
        let old = " let x = 1;";
        let new = "\\tlet x = 2;";
        let result = fuzzy_find_and_replace(content, old, new, false);
        assert_eq!(result.content, "\tlet x = 2;\n");
    }

    #[test]
    fn no_match_hint_points_to_similar_lines() {
        let hint = format_no_match_hint(
            Some("Could not find a match for old_string in the file"),
            0,
            "println!(\"helo\")",
            "fn main() {\n    println!(\"hello\");\n}\n",
        );
        assert!(hint.contains("Did you mean"));
        assert!(hint.contains("println!"));
    }

    #[test]
    fn reindent_preserves_file_indent_when_llm_indent_differs() {
        // File has 4-space indent, LLM sends 2-space indent
        // Both have +2 relative indent on second line
        let content = "    def foo():\n      pass\n";
        let old = "  def foo():\n    pass";
        let new = "  def bar():\n    return 42";

        let result = fuzzy_find_and_replace(content, old, new, false);

        // Should match via line_trimmed or indentation_flexible
        assert!(result.error.is_none(), "Should match successfully");
        assert_ne!(result.strategy, Some("exact"), "Should not be exact match");

        // Output should preserve file's 4-space base + 2-space relative
        let expected = "    def bar():\n      return 42\n";
        assert_eq!(
            result.content, expected,
            "Should preserve file's indent structure"
        );
    }
}
