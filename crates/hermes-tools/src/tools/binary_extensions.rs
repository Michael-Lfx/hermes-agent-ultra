//! Binary file extensions to skip for text-based operations.
//!
//! These files can't be meaningfully compared as text and are often large.
//!
//! # Python alignment
//!
//! Corresponds to `hermes-agent/tools/binary_extensions.py`.
//! Ported from free-code src/constants/files.ts.

use std::sync::LazyLock;
use std::collections::HashSet;

/// Binary file extensions that should be skipped for text-based operations.
pub static BINARY_EXTENSIONS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut set = HashSet::new();

    // Images
    set.insert(".png");
    set.insert(".jpg");
    set.insert(".jpeg");
    set.insert(".gif");
    set.insert(".bmp");
    set.insert(".ico");
    set.insert(".webp");
    set.insert(".tiff");
    set.insert(".tif");

    // Videos
    set.insert(".mp4");
    set.insert(".mov");
    set.insert(".avi");
    set.insert(".mkv");
    set.insert(".webm");
    set.insert(".wmv");
    set.insert(".flv");
    set.insert(".m4v");
    set.insert(".mpeg");
    set.insert(".mpg");

    // Audio
    set.insert(".mp3");
    set.insert(".wav");
    set.insert(".ogg");
    set.insert(".flac");
    set.insert(".aac");
    set.insert(".m4a");
    set.insert(".wma");
    set.insert(".aiff");
    set.insert(".opus");

    // Archives
    set.insert(".zip");
    set.insert(".tar");
    set.insert(".gz");
    set.insert(".bz2");
    set.insert(".7z");
    set.insert(".rar");
    set.insert(".xz");
    set.insert(".z");
    set.insert(".tgz");
    set.insert(".iso");

    // Executables/binaries
    set.insert(".exe");
    set.insert(".dll");
    set.insert(".so");
    set.insert(".dylib");
    set.insert(".bin");
    set.insert(".o");
    set.insert(".a");
    set.insert(".obj");
    set.insert(".lib");
    set.insert(".app");
    set.insert(".msi");
    set.insert(".deb");
    set.insert(".rpm");

    // Documents (exclude .pdf — text-based, agents may want to inspect)
    set.insert(".doc");
    set.insert(".docx");
    set.insert(".xls");
    set.insert(".xlsx");
    set.insert(".ppt");
    set.insert(".pptx");
    set.insert(".odt");
    set.insert(".ods");
    set.insert(".odp");

    // Fonts
    set.insert(".ttf");
    set.insert(".otf");
    set.insert(".woff");
    set.insert(".woff2");
    set.insert(".eot");

    // Bytecode / VM artifacts
    set.insert(".pyc");
    set.insert(".pyo");
    set.insert(".class");
    set.insert(".jar");
    set.insert(".war");
    set.insert(".ear");
    set.insert(".node");
    set.insert(".wasm");
    set.insert(".rlib");

    // Database files
    set.insert(".sqlite");
    set.insert(".sqlite3");
    set.insert(".db");
    set.insert(".mdb");
    set.insert(".idx");

    // Design / 3D
    set.insert(".psd");
    set.insert(".ai");
    set.insert(".eps");
    set.insert(".sketch");
    set.insert(".fig");
    set.insert(".xd");
    set.insert(".blend");
    set.insert(".3ds");
    set.insert(".max");

    // Flash
    set.insert(".swf");
    set.insert(".fla");

    // Lock/profiling data
    set.insert(".lockb");
    set.insert(".dat");
    set.insert(".data");

    set
});

/// Check if a file path has a binary extension.
///
/// Pure string check, no I/O.
///
/// # Examples
///
/// ```
/// use hermes_tools::tools::binary_extensions::has_binary_extension;
///
/// assert!(has_binary_extension("image.png"));
/// assert!(has_binary_extension("app.exe"));
/// assert!(!has_binary_extension("script.py"));
/// assert!(!has_binary_extension("README"));
/// ```
pub fn has_binary_extension(path: &str) -> bool {
    if let Some(dot_pos) = path.rfind('.') {
        let extension = &path[dot_pos..];
        BINARY_EXTENSIONS.contains(extension.to_lowercase().as_str())
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_binary_extensions_images() {
        assert!(has_binary_extension("photo.png"));
        assert!(has_binary_extension("photo.PNG")); // case insensitive
        assert!(has_binary_extension("image.jpg"));
        assert!(has_binary_extension("icon.ico"));
        assert!(has_binary_extension("graphic.gif"));
        assert!(has_binary_extension("picture.webp"));
    }

    #[test]
    fn test_binary_extensions_videos() {
        assert!(has_binary_extension("video.mp4"));
        assert!(has_binary_extension("movie.mov"));
        assert!(has_binary_extension("clip.avi"));
        assert!(has_binary_extension("film.mkv"));
    }

    #[test]
    fn test_binary_extensions_audio() {
        assert!(has_binary_extension("song.mp3"));
        assert!(has_binary_extension("audio.wav"));
        assert!(has_binary_extension("music.flac"));
        assert!(has_binary_extension("track.ogg"));
    }

    #[test]
    fn test_binary_extensions_archives() {
        assert!(has_binary_extension("archive.zip"));
        assert!(has_binary_extension("backup.tar"));
        assert!(has_binary_extension("compressed.gz"));
        assert!(has_binary_extension("package.7z"));
    }

    #[test]
    fn test_binary_extensions_executables() {
        assert!(has_binary_extension("program.exe"));
        assert!(has_binary_extension("library.dll"));
        assert!(has_binary_extension("shared.so"));
        assert!(has_binary_extension("dynamic.dylib"));
        assert!(has_binary_extension("binary.bin"));
    }

    #[test]
    fn test_binary_extensions_documents() {
        assert!(has_binary_extension("report.doc"));
        assert!(has_binary_extension("spreadsheet.xlsx"));
        assert!(has_binary_extension("presentation.pptx"));
    }

    #[test]
    fn test_binary_extensions_fonts() {
        assert!(has_binary_extension("font.ttf"));
        assert!(has_binary_extension("typeface.otf"));
        assert!(has_binary_extension("webfont.woff2"));
    }

    #[test]
    fn test_binary_extensions_bytecode() {
        assert!(has_binary_extension("module.pyc"));
        assert!(has_binary_extension("class.class"));
        assert!(has_binary_extension("library.jar"));
        assert!(has_binary_extension("package.wasm"));
        assert!(has_binary_extension("lib.rlib"));
    }

    #[test]
    fn test_binary_extensions_databases() {
        assert!(has_binary_extension("data.sqlite"));
        assert!(has_binary_extension("db.sqlite3"));
        assert!(has_binary_extension("store.db"));
    }

    #[test]
    fn test_text_extensions() {
        assert!(!has_binary_extension("script.py"));
        assert!(!has_binary_extension("source.rs"));
        assert!(!has_binary_extension("code.js"));
        assert!(!has_binary_extension("style.css"));
        assert!(!has_binary_extension("page.html"));
        assert!(!has_binary_extension("data.json"));
        assert!(!has_binary_extension("config.yaml"));
        assert!(!has_binary_extension("notes.txt"));
        assert!(!has_binary_extension("doc.md"));
    }

    #[test]
    fn test_no_extension() {
        assert!(!has_binary_extension("README"));
        assert!(!has_binary_extension("Makefile"));
        assert!(!has_binary_extension("LICENSE"));
        assert!(!has_binary_extension("path/to/file"));
    }

    #[test]
    fn test_edge_cases() {
        assert!(!has_binary_extension("")); // empty string
        assert!(!has_binary_extension(".")); // just dot
        assert!(has_binary_extension(".png")); // extension only
        assert!(has_binary_extension("path/to/file.EXE")); // mixed case in path
        assert!(!has_binary_extension("file.unknown")); // unknown extension
    }

    #[test]
    fn test_pdf_not_binary() {
        // PDF is explicitly excluded from binary list (agents may want to inspect)
        assert!(!has_binary_extension("document.pdf"));
    }

    #[test]
    fn test_multiple_dots() {
        assert!(has_binary_extension("archive.tar.gz"));
        assert!(has_binary_extension("file.backup.zip"));
        assert!(!has_binary_extension("script.test.py"));
    }

    #[test]
    fn test_case_insensitivity() {
        assert!(has_binary_extension("FILE.PNG"));
        assert!(has_binary_extension("File.Png"));
        assert!(has_binary_extension("file.pNg"));
    }
}
