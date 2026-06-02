use std::path::PathBuf;
use hermes_core::errors::AgentError;
use indicatif::{ProgressBar, ProgressStyle};
use tokio::io::AsyncWriteExt;
use crate::update::platform::Platform;

/// 下载 artifact 并解压出 binary，返回临时文件路径
pub async fn download_and_extract(
    url: &str,
    platform: &Platform,
    show_progress: bool,
) -> Result<PathBuf, AgentError> {
    let client = reqwest::Client::builder()
        .user_agent("hermes-agent-ultra")
        .build()
        .map_err(|e| AgentError::Io(format!("Failed to create HTTP client: {e}")))?;

    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| AgentError::Io(format!("Download failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(AgentError::Io(format!("Download returned status {}", resp.status())));
    }

    let total_size = resp.content_length().unwrap_or(0);

    let pb = if show_progress && total_size > 0 {
        let pb = ProgressBar::new(total_size);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{msg} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
                .unwrap_or_else(|_| ProgressStyle::default_bar())
                .progress_chars("=>-"),
        );
        pb.set_message("Downloading");
        Some(pb)
    } else {
        None
    };

    // Download to temp file
    let temp_dir = std::env::temp_dir();
    let archive_path = temp_dir.join(platform.artifact_name());
    let mut file = tokio::fs::File::create(&archive_path)
        .await
        .map_err(|e| AgentError::Io(format!("Failed to create temp file: {e}")))?;

    let mut stream = resp.bytes_stream();
    use futures::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| AgentError::Io(format!("Download stream error: {e}")))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| AgentError::Io(format!("Failed to write temp file: {e}")))?;
        if let Some(ref pb) = pb {
            pb.inc(chunk.len() as u64);
        }
    }
    file.flush()
        .await
        .map_err(|e| AgentError::Io(format!("Failed to flush temp file: {e}")))?;
    drop(file);

    if let Some(pb) = pb {
        pb.finish_with_message("Download complete");
    }

    // Extract binary from archive
    let binary_name = platform.binary_name();
    let extracted_path = temp_dir.join(format!("hermes-update-{}", binary_name));

    if platform.artifact_name().ends_with(".zip") {
        extract_zip(&archive_path, binary_name, &extracted_path)?;
    } else {
        extract_tar_gz(&archive_path, binary_name, &extracted_path)?;
    }

    // Cleanup archive
    let _ = std::fs::remove_file(&archive_path);

    Ok(extracted_path)
}

fn extract_tar_gz(
    archive_path: &std::path::Path,
    binary_name: &str,
    output_path: &std::path::Path,
) -> Result<(), AgentError> {
    use flate2::read::GzDecoder;
    use tar::Archive;

    let file = std::fs::File::open(archive_path)
        .map_err(|e| AgentError::Io(format!("Failed to open archive: {e}")))?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);

    for entry in archive.entries().map_err(|e| AgentError::Io(format!("Failed to read archive: {e}")))? {
        let mut entry = entry.map_err(|e| AgentError::Io(format!("Failed to read entry: {e}")))?;
        let path = entry.path().map_err(|e| AgentError::Io(format!("Invalid entry path: {e}")))?;

        // Match binary by filename (may be nested in a directory)
        if path.file_name().and_then(|n| n.to_str()) == Some(binary_name) {
            let mut out = std::fs::File::create(output_path)
                .map_err(|e| AgentError::Io(format!("Failed to create output file: {e}")))?;
            std::io::copy(&mut entry, &mut out)
                .map_err(|e| AgentError::Io(format!("Failed to extract binary: {e}")))?;
            return Ok(());
        }
    }

    Err(AgentError::Io(format!("Binary '{}' not found in archive", binary_name)))
}

fn extract_zip(
    archive_path: &std::path::Path,
    binary_name: &str,
    output_path: &std::path::Path,
) -> Result<(), AgentError> {
    let file = std::fs::File::open(archive_path)
        .map_err(|e| AgentError::Io(format!("Failed to open zip archive: {e}")))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| AgentError::Io(format!("Failed to read zip archive: {e}")))?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)
            .map_err(|e| AgentError::Io(format!("Failed to read zip entry: {e}")))?;

        let entry_name = entry.name().to_string();
        // Match by filename (might be in a subdirectory)
        if entry_name.ends_with(binary_name) || entry_name == binary_name {
            let mut out = std::fs::File::create(output_path)
                .map_err(|e| AgentError::Io(format!("Failed to create output file: {e}")))?;
            std::io::copy(&mut entry, &mut out)
                .map_err(|e| AgentError::Io(format!("Failed to extract binary: {e}")))?;
            return Ok(());
        }
    }

    Err(AgentError::Io(format!("Binary '{}' not found in zip archive", binary_name)))
}
