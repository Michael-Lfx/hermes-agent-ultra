//! Shared platform helper functions.
//!
//! Common text manipulation utilities used across platform adapters.

use regex::Regex;

/// Image reference extracted from generated markdown / HTML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineImageRef {
    pub url: String,
    pub alt_text: Option<String>,
}

/// Split a long message into chunks that respect word and sentence boundaries.
///
/// Prefers breaking at sentence endings (`. `, `! `, `? `), then at newlines,
/// then at word boundaries (spaces), and only hard-splits as a last resort.
pub fn split_long_message(text: &str, max_len: usize) -> Vec<String> {
    if max_len == 0 {
        return vec![text.to_string()];
    }
    if text.len() <= max_len {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        if remaining.len() <= max_len {
            chunks.push(remaining.to_string());
            break;
        }

        let window = &remaining[..max_len];

        // Try sentence boundary first
        let break_at = find_last_sentence_break(window)
            .or_else(|| window.rfind('\n'))
            .or_else(|| window.rfind(' '))
            .unwrap_or(max_len);

        let break_at = if break_at == 0 { max_len } else { break_at };

        chunks.push(remaining[..break_at].trim_end().to_string());
        remaining = remaining[break_at..].trim_start();
    }

    chunks
}

fn find_last_sentence_break(text: &str) -> Option<usize> {
    let terminators = [". ", "! ", "? ", ".\n", "!\n", "?\n"];
    terminators
        .iter()
        .filter_map(|t| text.rfind(t).map(|i| i + t.len()))
        .max()
}

/// Escape Markdown special characters.
pub fn escape_markdown(text: &str) -> String {
    const SPECIAL_CHARS: &[char] = &[
        '\\', '`', '*', '_', '{', '}', '[', ']', '(', ')', '#', '+', '-', '.', '!', '|', '~', '>',
    ];

    let mut result = String::with_capacity(text.len() + text.len() / 8);
    for ch in text.chars() {
        if SPECIAL_CHARS.contains(&ch) {
            result.push('\\');
        }
        result.push(ch);
    }
    result
}

/// Truncate text to `max_len` characters, appending an ellipsis if truncated.
pub fn truncate_with_ellipsis(text: &str, max_len: usize) -> String {
    if max_len < 4 {
        return text.chars().take(max_len).collect();
    }
    if text.len() <= max_len {
        return text.to_string();
    }

    let truncated = &text[..text.floor_char_boundary(max_len - 3)];
    // Try to break at a word boundary
    let break_at = truncated.rfind(' ').unwrap_or(truncated.len());
    format!("{}...", &truncated[..break_at])
}

/// Extract all URLs from text.
pub fn extract_urls(text: &str) -> Vec<String> {
    let re = Regex::new(r"https?://[^\s<>\[\](){}]+").expect("valid regex");
    re.find_iter(text).map(|m| m.as_str().to_string()).collect()
}

/// Extract markdown image links from text.
///
/// Returns `(cleaned_text, image_urls)` where only image tags of the form
/// `![alt](url)` are removed from the cleaned text. Normal markdown links
/// `[text](url)` are preserved.
pub fn extract_markdown_images(text: &str) -> (String, Vec<String>) {
    let image_re = Regex::new(r"!\[[^\]]*\]\(([^)]+)\)").expect("valid regex");
    let mut images = Vec::new();

    let cleaned = image_re
        .replace_all(text, |caps: &regex::Captures| {
            if let Some(url) = caps.get(1).map(|m| m.as_str().trim()) {
                images.push(url.to_string());
            }
            ""
        })
        .to_string();

    let cleaned = cleaned
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string();
    (cleaned, images)
}

fn looks_like_image_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    if lower.contains("fal.media")
        || lower.contains("fal-cdn")
        || lower.contains("replicate.delivery")
    {
        return true;
    }
    let base = lower.split('?').next().unwrap_or(lower.as_str());
    [
        ".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".svg", ".avif", ".heic", ".heif",
        ".tiff", ".tif",
    ]
    .iter()
    .any(|ext| base.ends_with(ext))
}

/// Extract inline images from markdown and HTML.
///
/// Supports:
/// - Markdown images: `![alt](https://...)`
/// - HTML images: `<img src="https://...">`
///
/// Returns `(cleaned_text, images)` where matched image tags are removed from
/// the text and captured in `images`.
pub fn extract_inline_images(text: &str) -> (String, Vec<InlineImageRef>) {
    let md_re = Regex::new(r"!\[([^\]]*)\]\((https?://[^\s)]+)\)").expect("valid regex");
    let html_re =
        Regex::new(r#"<img[^>]*\bsrc=["']?(https?://[^"' >]+)["']?[^>]*>"#).expect("valid regex");

    let mut images: Vec<InlineImageRef> = Vec::new();

    let without_md = md_re
        .replace_all(text, |caps: &regex::Captures| {
            let url = caps.get(2).map(|m| m.as_str().trim()).unwrap_or_default();
            let alt = caps
                .get(1)
                .map(|m| m.as_str().trim())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            if looks_like_image_url(url) {
                images.push(InlineImageRef {
                    url: url.to_string(),
                    alt_text: alt,
                });
                "".to_string()
            } else {
                caps.get(0)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default()
            }
        })
        .to_string();

    let without_html = html_re
        .replace_all(&without_md, |caps: &regex::Captures| {
            let url = caps.get(1).map(|m| m.as_str().trim()).unwrap_or_default();
            if looks_like_image_url(url) {
                images.push(InlineImageRef {
                    url: url.to_string(),
                    alt_text: None,
                });
                "".to_string()
            } else {
                caps.get(0)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default()
            }
        })
        .to_string();

    let cleaned = without_html
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string();

    (cleaned, images)
}

/// Format a code block with optional language tag.
pub fn format_code_block(code: &str, lang: Option<&str>) -> String {
    match lang {
        Some(l) if !l.is_empty() => format!("```{}\n{}\n```", l, code),
        _ => format!("```\n{}\n```", code),
    }
}

/// Sanitize HTML by stripping tags, keeping only text content.
pub fn sanitize_html(text: &str) -> String {
    let re = Regex::new(r"<[^>]+>").expect("valid regex");
    let cleaned = re.replace_all(text, "");
    // Decode common HTML entities
    cleaned
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

/// Estimate reading time in seconds for the given text.
///
/// Assumes an average reading speed of 200 words per minute.
pub fn estimate_read_time(text: &str) -> u32 {
    let word_count = text.split_whitespace().count() as f64;
    let minutes = word_count / 200.0;
    (minutes * 60.0).ceil() as u32
}

/// Detect MIME type from a file extension.
pub fn mime_from_extension(ext: &str) -> &'static str {
    match ext.to_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "avi" => "video/x-msvideo",
        "mp3" => "audio/mpeg",
        "ogg" | "oga" => "audio/ogg",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "aac" => "audio/aac",
        "pdf" => "application/pdf",
        "doc" | "docx" => "application/msword",
        "xls" | "xlsx" => "application/vnd.ms-excel",
        "zip" => "application/zip",
        "json" => "application/json",
        "xml" => "application/xml",
        "txt" => "text/plain",
        "html" | "htm" => "text/html",
        "csv" => "text/csv",
        _ => "application/octet-stream",
    }
}

/// Determine the file's media category from its extension.
pub fn media_category(ext: &str) -> &'static str {
    match ext.to_lowercase().as_str() {
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "svg" | "bmp" | "tiff" => "image",
        "mp4" | "webm" | "mov" | "avi" | "mkv" => "video",
        "mp3" | "ogg" | "oga" | "wav" | "flac" | "aac" | "m4a" => "audio",
        _ => "document",
    }
}

// ---------------------------------------------------------------------------
// Boolean / environment helpers (shared across platform adapters)
// ---------------------------------------------------------------------------

/// Return `true` for the common truthy string values `"1"`, `"true"`, `"yes"`,
/// `"on"`; return `false` for everything else (including empty strings).
///
/// Comparison is case-insensitive.
pub fn parse_bool_str(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// Read an environment variable and interpret it as a boolean.
///
/// Returns `default` when the variable is unset. The string value is parsed
/// with [`parse_bool_str`].
pub fn parse_env_bool(key: &str, default: bool) -> bool {
    std::env::var(key)
        .map(|v| parse_bool_str(&v))
        .unwrap_or(default)
}

// ---------------------------------------------------------------------------
// Message splitting (shared across platform adapters)
// ---------------------------------------------------------------------------

/// Split `text` into chunks of at most `max_chars` Unicode scalar values.
///
/// Prefers breaking at a preceding newline to avoid cutting in the middle of
/// a line. Falls back to a hard character boundary when no newline is found.
///
/// This is the correct way to enforce per-message length limits on platforms
/// that count Unicode characters (Discord, Telegram, Slack) rather than raw
/// bytes: UTF-8 multi-byte sequences must not be cut mid-character.
///
/// Returns a `vec![""]` for empty input so callers always get at least one
/// chunk (matching Discord's existing contract).
pub fn split_message_by_chars(text: &str, max_chars: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    if text.chars().count() <= max_chars {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut rest = text;

    while !rest.is_empty() {
        if rest.chars().count() <= max_chars {
            chunks.push(rest.to_string());
            break;
        }

        // Walk to the max_chars-th character and record the byte boundary.
        let mut end_byte = 0;
        let mut count = 0;
        for (byte_idx, ch) in rest.char_indices() {
            count += 1;
            end_byte = byte_idx + ch.len_utf8();
            if count >= max_chars {
                break;
            }
        }

        if end_byte >= rest.len() {
            chunks.push(rest.to_string());
            break;
        }

        // Prefer breaking at the last newline before the limit.
        let break_at = rest[..end_byte]
            .rfind('\n')
            .map(|pos| pos + 1)
            .filter(|&pos| pos > 0)
            .unwrap_or(end_byte);

        chunks.push(rest[..break_at].to_string());
        rest = &rest[break_at..];
        // Safety valve: if break_at == 0 (no newline, boundary is char 0),
        // advance by exactly one character to avoid an infinite loop.
        if break_at == 0 {
            let ch_len = rest.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
            chunks.push(rest[..ch_len.min(rest.len())].to_string());
            rest = &rest[ch_len.min(rest.len())..];
        }
    }
    chunks
}

// ---------------------------------------------------------------------------
// Remote image helpers (shared across platform adapters)
// ---------------------------------------------------------------------------

/// Normalise an HTTP `Content-Type` header value to a bare `image/*` MIME
/// type, stripping parameters (e.g. `; charset=binary`) and lowercasing.
/// Returns `None` for non-image types or absent values.
pub fn normalized_image_content_type(content_type: Option<&str>) -> Option<String> {
    let normalized = content_type?
        .split(';')
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())?
        .to_ascii_lowercase();
    if normalized.starts_with("image/") {
        Some(normalized)
    } else {
        None
    }
}

/// Map a normalised `image/*` MIME type to a file extension.
/// Returns `None` for unknown subtypes.
pub fn image_extension_from_content_type(content_type: Option<&str>) -> Option<&'static str> {
    let normalized = normalized_image_content_type(content_type)?;
    match normalized.as_str() {
        "image/jpeg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/bmp" => Some("bmp"),
        "image/tiff" => Some("tiff"),
        "image/svg+xml" => Some("svg"),
        "image/heic" => Some("heic"),
        "image/heif" => Some("heif"),
        "image/avif" => Some("avif"),
        _ => None,
    }
}

/// Derive a local file name for a remote image URL.
///
/// Uses the last path segment of the URL. When the segment has no extension,
/// one is appended based on `content_type`; falls back to `"png"`.
pub fn remote_image_file_name(image_url: &str, content_type: Option<&str>) -> String {
    let stripped = image_url
        .split('#')
        .next()
        .unwrap_or(image_url)
        .split('?')
        .next()
        .unwrap_or(image_url)
        .trim_end_matches('/');
    let base = stripped.rsplit('/').next().unwrap_or("").trim();
    let mut file_name = if base.is_empty() {
        "image".to_string()
    } else {
        base.to_string()
    };

    let has_extension = std::path::Path::new(&file_name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some();
    if !has_extension {
        let ext = image_extension_from_content_type(content_type).unwrap_or("png");
        file_name.push('.');
        file_name.push_str(ext);
    }
    file_name
}

/// Build a plain-text fallback message for an image that could not be
/// delivered as a native attachment.
pub fn image_fallback_text(image_url: &str, caption: Option<&str>) -> String {
    match caption.map(str::trim).filter(|s| !s.is_empty()) {
        Some(c) => format!("{c}\n{image_url}"),
        None => image_url.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_long_message_short() {
        let chunks = split_long_message("hello world", 100);
        assert_eq!(chunks, vec!["hello world"]);
    }

    #[test]
    fn test_split_long_message_sentence_break() {
        let text = "First sentence. Second sentence. Third sentence is long.";
        let chunks = split_long_message(text, 35);
        assert!(chunks.len() >= 2);
        assert!(chunks[0].ends_with('.'));
    }

    #[test]
    fn test_escape_markdown() {
        assert_eq!(escape_markdown("hello *world*"), "hello \\*world\\*");
        assert_eq!(escape_markdown("no_special"), "no\\_special");
    }

    #[test]
    fn test_truncate_with_ellipsis() {
        assert_eq!(truncate_with_ellipsis("short", 100), "short");
        let result = truncate_with_ellipsis("this is a long sentence that should be truncated", 20);
        assert!(result.ends_with("..."));
        assert!(result.len() <= 20);
    }

    #[test]
    fn test_extract_urls() {
        let text = "Visit https://example.com and http://foo.bar/baz for more.";
        let urls = extract_urls(text);
        assert_eq!(urls.len(), 2);
        assert!(urls[0].starts_with("https://"));
    }

    #[test]
    fn test_extract_markdown_images_preserves_non_image_links() {
        let text =
            "See ![diagram](https://img.example.com/a.png) and [doc](https://example.com/a.pdf)";
        let (cleaned, images) = extract_markdown_images(text);
        assert_eq!(images, vec!["https://img.example.com/a.png"]);
        assert_eq!(cleaned, "See and [doc](https://example.com/a.pdf)");
    }

    #[test]
    fn test_extract_markdown_images_multiple_tags() {
        let text = "A ![one](https://i/1.png) B ![two](https://i/2.jpg) C";
        let (cleaned, images) = extract_markdown_images(text);
        assert_eq!(images.len(), 2);
        assert_eq!(images[0], "https://i/1.png");
        assert_eq!(images[1], "https://i/2.jpg");
        assert_eq!(cleaned, "A B C");
    }

    #[test]
    fn test_extract_inline_images_markdown_and_html() {
        let text = "Start ![chart](https://cdn.example.com/a.png) and <img src=\"https://fal.media/b/c\"> end";
        let (cleaned, images) = extract_inline_images(text);
        assert_eq!(cleaned, "Start and end");
        assert_eq!(images.len(), 2);
        assert_eq!(images[0].url, "https://cdn.example.com/a.png");
        assert_eq!(images[0].alt_text.as_deref(), Some("chart"));
        assert_eq!(images[1].url, "https://fal.media/b/c");
        assert_eq!(images[1].alt_text, None);
    }

    #[test]
    fn test_extract_inline_images_keeps_non_image_html() {
        let text = "A <img src=\"https://example.com/not-image\"> B";
        let (cleaned, images) = extract_inline_images(text);
        assert_eq!(images.len(), 0);
        assert_eq!(cleaned, "A <img src=\"https://example.com/not-image\"> B");
    }

    #[test]
    fn test_format_code_block() {
        assert_eq!(
            format_code_block("let x = 1;", Some("rust")),
            "```rust\nlet x = 1;\n```"
        );
        assert_eq!(format_code_block("hello", None), "```\nhello\n```");
    }

    #[test]
    fn test_sanitize_html() {
        assert_eq!(
            sanitize_html("<b>bold</b> &amp; <i>italic</i>"),
            "bold & italic"
        );
    }

    #[test]
    fn test_estimate_read_time() {
        let words_200: String = (0..200).map(|_| "word").collect::<Vec<_>>().join(" ");
        let time = estimate_read_time(&words_200);
        assert_eq!(time, 60);
    }

    #[test]
    fn test_mime_from_extension() {
        assert_eq!(mime_from_extension("png"), "image/png");
        assert_eq!(mime_from_extension("mp4"), "video/mp4");
        assert_eq!(mime_from_extension("xyz"), "application/octet-stream");
    }

    #[test]
    fn test_media_category() {
        assert_eq!(media_category("jpg"), "image");
        assert_eq!(media_category("mp4"), "video");
        assert_eq!(media_category("mp3"), "audio");
        assert_eq!(media_category("pdf"), "document");
    }

    #[test]
    fn test_parse_bool_str() {
        for truthy in ["1", "true", "True", "TRUE", "yes", "YES", "on", "ON"] {
            assert!(parse_bool_str(truthy), "{truthy} should be truthy");
        }
        for falsy in ["0", "false", "no", "off", "", "  ", "maybe"] {
            assert!(!parse_bool_str(falsy), "{falsy} should be falsy");
        }
    }

    #[test]
    fn test_parse_env_bool_default_when_unset() {
        assert!(parse_env_bool("HERMES_TEST_BOOL_UNSET_12345", true));
        assert!(!parse_env_bool("HERMES_TEST_BOOL_UNSET_12345", false));
    }

    #[test]
    fn test_split_message_by_chars_empty() {
        assert_eq!(split_message_by_chars("", 10), vec![""]);
    }

    #[test]
    fn test_split_message_by_chars_short() {
        assert_eq!(split_message_by_chars("hello", 4000), vec!["hello"]);
    }

    #[test]
    fn test_split_message_by_chars_exact() {
        let text = "a".repeat(100);
        assert_eq!(split_message_by_chars(&text, 100).len(), 1);
    }

    #[test]
    fn test_split_message_by_chars_long() {
        let text = "a".repeat(150);
        let chunks = split_message_by_chars(&text, 100);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), 100);
        assert_eq!(chunks[1].len(), 50);
    }

    #[test]
    fn test_split_message_by_chars_prefers_newline() {
        let text = "line1\nline2\nline3 with extra padding to exceed limit";
        let chunks = split_message_by_chars(text, 20);
        assert!(chunks[0].ends_with('\n') || chunks[0].len() <= 20);
    }

    #[test]
    fn test_split_message_by_chars_unicode() {
        // Each Chinese character is 3 bytes but 1 char.
        let text = "中".repeat(200);
        let chunks = split_message_by_chars(&text, 100);
        // All chunks should be valid UTF-8 and at most 100 chars.
        for chunk in &chunks {
            assert!(chunk.chars().count() <= 100);
            assert!(std::str::from_utf8(chunk.as_bytes()).is_ok());
        }
    }

    #[test]
    fn test_normalized_image_content_type_strips_params() {
        assert_eq!(
            normalized_image_content_type(Some("image/png; charset=binary")).as_deref(),
            Some("image/png")
        );
        assert_eq!(
            normalized_image_content_type(Some("IMAGE/JPEG")).as_deref(),
            Some("image/jpeg")
        );
        assert_eq!(normalized_image_content_type(Some("text/plain")), None);
        assert_eq!(normalized_image_content_type(None), None);
    }

    #[test]
    fn test_image_extension_from_content_type() {
        assert_eq!(
            image_extension_from_content_type(Some("image/jpeg")),
            Some("jpg")
        );
        assert_eq!(
            image_extension_from_content_type(Some("image/webp; q=0.9")),
            Some("webp")
        );
        assert_eq!(image_extension_from_content_type(Some("text/plain")), None);
    }

    #[test]
    fn test_remote_image_file_name_keeps_extension() {
        let name = remote_image_file_name("https://cdn.example.com/photo.jpg", None);
        assert_eq!(name, "photo.jpg");
    }

    #[test]
    fn test_remote_image_file_name_adds_extension_from_content_type() {
        let name =
            remote_image_file_name("https://cdn.example.com/path/diagram", Some("image/jpeg"));
        assert_eq!(name, "diagram.jpg");
    }

    #[test]
    fn test_remote_image_file_name_fallback_png() {
        let name = remote_image_file_name("https://cdn.example.com/image", None);
        assert_eq!(name, "image.png");
    }

    #[test]
    fn test_remote_image_file_name_empty_url() {
        let name = remote_image_file_name("", None);
        assert_eq!(name, "image.png");
    }

    #[test]
    fn test_image_fallback_text_with_caption() {
        assert_eq!(
            image_fallback_text("https://example.com/plot.png", Some("Daily chart")),
            "Daily chart\nhttps://example.com/plot.png"
        );
        assert_eq!(
            image_fallback_text("https://example.com/plot.png", Some("   ")),
            "https://example.com/plot.png"
        );
        assert_eq!(
            image_fallback_text("https://example.com/plot.png", None),
            "https://example.com/plot.png"
        );
    }
}
