use std::path::Path;

/// Resolve `@file:filename` references in text by reading uploaded files.
///
/// Files are looked up in `~/.hermes/uploads/{session_id}/` directory.
/// Each uploaded file is stored with a prefix like `{uuid}-{filename}`,
/// so we use glob pattern matching to find the actual file.
///
/// If a file cannot be found or read, the original `@file:` reference is preserved.
pub async fn resolve_file_refs(
    text: &str,
    session_id: &str,
    hermes_home: &Path,
) -> String {
    let upload_dir = hermes_home.join("uploads").join(session_id);
    if !upload_dir.exists() {
        return text.to_string();
    }

    let mut result = text.to_string();

    // Find all @file: references using simple string scanning
    let prefix = "@file:";
    let mut search_start = 0;

    while let Some(pos) = result[search_start..].find(prefix) {
        let actual_pos = search_start + pos;
        let after_prefix = actual_pos + prefix.len();

        // Extract filename (until whitespace, newline, or end of string)
        let file_name: String = result[after_prefix..]
            .chars()
            .take_while(|c| !c.is_whitespace())
            .collect();

        if file_name.is_empty() {
            search_start = after_prefix;
            continue;
        }

        // Try to find the uploaded file
        let resolved = match resolve_single_file(
            &upload_dir,
            &file_name,
        ).await {
            Ok(content) => {
                format!(
                    "[Attached file: {}]\n```\n{}\n```",
                    file_name,
                    content
                )
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to resolve @file:{} for session {}: {}",
                    file_name,
                    session_id,
                    e
                );
                // Keep original reference on failure
                format!("@file:{}", file_name)
            }
        };

        // Replace the @file: reference in the result
        let old_ref = format!("@file:{}", file_name);
        result = result.replacen(&old_ref, &resolved, 1);

        // Continue searching after the replacement
        search_start = actual_pos + resolved.len();
    }

    result
}

async fn resolve_single_file(
    upload_dir: &Path,
    file_name: &str,
) -> Result<String, String> {
    // Read directory entries and find matching file
    let mut entries = tokio::fs::read_dir(upload_dir)
        .await
        .map_err(|e| format!("Cannot read upload dir: {}", e))?;

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| format!("Dir entry error: {}", e))?
    {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Files are stored as `{uuid}-{filename}` or similar pattern
        // We check if the filename ends with the requested file name
        if name_str.ends_with(file_name) || name_str == file_name {
            // Check file size before reading (limit to 1MB)
            let metadata = entry
                .metadata()
                .await
                .map_err(|e| format!("Metadata error: {}", e))?;

            const MAX_FILE_SIZE: u64 = 1024 * 1024; // 1MB
            if metadata.len() > MAX_FILE_SIZE {
                return Err(format!(
                    "File too large ({} bytes, max {} bytes)",
                    metadata.len(),
                    MAX_FILE_SIZE
                ));
            }

            let content = tokio::fs::read_to_string(entry.path())
                .await
                .map_err(|e| format!("Read error: {}", e))?;

            // Truncate very long content
            const MAX_CONTENT_LEN: usize = 50_000;
            if content.len() > MAX_CONTENT_LEN {
                return Ok(format!(
                    "{}\n\n[File truncated - {} total characters]",
                    &content[..MAX_CONTENT_LEN],
                    content.len()
                ));
            }

            return Ok(content);
        }
    }

    Err(format!("File '{}' not found in uploads", file_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_resolve_no_refs() {
        let text = "Hello world, no file refs here.";
        let result = resolve_file_refs(text, "test-session", Path::new("/tmp")).await;
        assert_eq!(result, text);
    }

    #[tokio::test]
    async fn test_resolve_single_ref() {
        let tmp = tempfile::tempdir().unwrap();
        let uploads = tmp.path().join("uploads").join("sess1");
        tokio::fs::create_dir_all(&uploads).await.unwrap();
        tokio::fs::write(uploads.join("abc123-test.py"), "print('hello')").await.unwrap();

        let text = "Check this code: @file:test.py";
        let result = resolve_file_refs(text, "sess1", tmp.path()).await;

        assert!(result.contains("print('hello')"));
        assert!(!result.contains("@file:test.py"));
    }

    #[tokio::test]
    async fn test_resolve_missing_file_keeps_ref() {
        let tmp = tempfile::tempdir().unwrap();
        let uploads = tmp.path().join("uploads").join("sess1");
        tokio::fs::create_dir_all(&uploads).await.unwrap();

        let text = "Missing file: @file:nonexistent.txt";
        let result = resolve_file_refs(text, "sess1", tmp.path()).await;

        // Should keep original reference when file not found
        assert!(result.contains("@file:nonexistent.txt"));
    }
}
