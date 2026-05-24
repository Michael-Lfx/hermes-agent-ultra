//! Streaming output scrubbers — parity with Python `StreamingThinkScrubber`.

/// Strip think/redacted blocks from streamed assistant deltas.
#[derive(Debug, Default)]
pub struct ThinkBlockScrubber {
    in_block: bool,
    partial: String,
}

impl ThinkBlockScrubber {
    const OPEN_TAGS: &'static [&'static str] = &["<think>"];
    const CLOSE_TAGS: &'static [&'static str] = &["</think>"];

    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn scrub(&mut self, delta: &str) -> String {
        let mut input = format!("{}{}", self.partial, delta);
        self.partial.clear();
        let mut out = String::new();
        while !input.is_empty() {
            if self.in_block {
                if let Some((_, rest)) = Self::find_earliest_close(&input) {
                    input = rest.to_string();
                    self.in_block = false;
                } else {
                    self.partial = input;
                    return out;
                }
            } else if let Some((open_idx, tag)) = Self::find_earliest_open(&input) {
                out.push_str(&input[..open_idx]);
                input = input[open_idx + tag.len()..].to_string();
                self.in_block = true;
            } else {
                if input.ends_with('<') || input.ends_with("</") {
                    if let Some(split) = input.rfind('<') {
                        self.partial = input[split..].to_string();
                        out.push_str(&input[..split]);
                    } else {
                        out.push_str(&input);
                    }
                    return out;
                }
                out.push_str(&input);
                break;
            }
        }
        out
    }

    pub fn flush(&mut self) -> String {
        let tail = std::mem::take(&mut self.partial);
        if self.in_block {
            self.in_block = false;
            String::new()
        } else {
            tail
        }
    }

    fn find_earliest_open(s: &str) -> Option<(usize, &'static str)> {
        Self::OPEN_TAGS
            .iter()
            .filter_map(|tag| s.find(tag).map(|idx| (idx, *tag)))
            .min_by_key(|(idx, _)| *idx)
    }

    fn find_earliest_close(s: &str) -> Option<(usize, &str)> {
        Self::CLOSE_TAGS
            .iter()
            .filter_map(|tag| s.find(tag).map(|idx| (idx, &s[idx + tag.len()..])))
            .min_by_key(|(idx, _)| *idx)
    }
}
