//! Initialize `$HERMES_HOME/hermes-talk` layout.

use std::fs;
use std::path::Path;

use crate::error::{DemoError, Result};

#[cfg(windows)]
const CONFIG_EXAMPLE: &str = include_str!("../config.example.windows.toml");
#[cfg(not(windows))]
const CONFIG_EXAMPLE: &str = include_str!("../config.example.toml");

const SUBDIRS: &[&str] = &[
    "auth",
    "data",
    "frontend_extras",
    "models/vad",
    "models/denoise",
    "models/speaker",
    "models/kws-zh-en",
    "models/rk3588",
    "models/sensevoice",
    "models/kokoro",
];

/// Create talk home directory tree and default `config.toml` if missing (quiet; for auto-init).
pub fn ensure_talk_home() -> Result<()> {
    let home = hermes_config::talk_dir();
    fs::create_dir_all(&home)
        .map_err(|e| DemoError::Config(format!("mkdir {}: {e}", home.display())))?;

    for sub in SUBDIRS {
        let dir = home.join(sub);
        fs::create_dir_all(&dir)
            .map_err(|e| DemoError::Config(format!("mkdir {}: {e}", dir.display())))?;
    }

    seed_bundled_assets(&home)?;
    seed_gateway_config_from_bundle()?;

    let config_path = hermes_config::talk_config_path();
    if !config_path.exists() {
        fs::write(&config_path, CONFIG_EXAMPLE)
            .map_err(|e| DemoError::Config(format!("write {}: {e}", config_path.display())))?;
        tracing::info!("created talk config at {}", config_path.display());
    }
    Ok(())
}

/// When running from `make package-talk-rockchip` layout, link bundled models into talk home.
fn seed_bundled_assets(talk_home: &Path) -> Result<()> {
    let Some(bundle_root) = bundle_root_for_talk_home() else {
        return Ok(());
    };

    let config_path = talk_home.join("config.toml");
    let bundle_example = bundle_root.join("config.example.toml");
    let needs_talk_config = if !config_path.exists() {
        true
    } else if let Ok(content) = fs::read_to_string(&config_path) {
        content.contains("11888")
            || content.contains("/home/key.lic")
            || content.contains("/root/rktts/")
            || content.contains(r#""license_path": "key.lic""#)
    } else {
        false
    };
    if bundle_example.is_file() && needs_talk_config {
        fs::copy(&bundle_example, &config_path).map_err(|e| {
            DemoError::Config(format!(
                "copy {} -> {}: {e}",
                bundle_example.display(),
                config_path.display()
            ))
        })?;
        tracing::info!(
            "installed talk config from bundle at {}",
            config_path.display()
        );
    }

    for item in ["auth", "data", "models", "frontend_extras"] {
        let src = bundle_root.join(item);
        let dst = talk_home.join(item);
        if !src.exists() || dst.exists() {
            continue;
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&src, &dst).map_err(|e| {
                DemoError::Config(format!("link {} -> {}: {e}", dst.display(), src.display()))
            })?;
            tracing::info!("linked bundled {} into talk home", item);
        }
        #[cfg(not(unix))]
        {
            let _ = (src, dst);
        }
    }
    Ok(())
}

fn seed_gateway_config_from_bundle() -> Result<()> {
    let Some(bundle_root) = bundle_root_for_talk_home() else {
        return Ok(());
    };
    let example = bundle_root.join("config.example.yaml");
    if !example.is_file() {
        return Ok(());
    }

    let dest = hermes_config::config_path();
    let needs_write = if !dest.exists() {
        true
    } else {
        fs::read_to_string(&dest)
            .map(|content| content.contains("11888"))
            .unwrap_or(false)
    };
    if !needs_write {
        return Ok(());
    }

    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| DemoError::Config(format!("mkdir {}: {e}", parent.display())))?;
    }
    fs::copy(&example, &dest).map_err(|e| {
        DemoError::Config(format!(
            "copy {} -> {}: {e}",
            example.display(),
            dest.display()
        ))
    })?;
    tracing::info!("installed Hermes config at {}", dest.display());
    Ok(())
}

fn bundle_root_for_talk_home() -> Option<std::path::PathBuf> {
    if let Ok(dir) = std::env::var("HERMES_TALK_BUNDLE_DIR") {
        let bundle = std::path::PathBuf::from(dir);
        if bundle.join("start.sh").is_file() {
            return Some(bundle);
        }
    }
    None
}

/// Create talk home directory tree and default `config.toml` if missing.
pub fn init_talk_home() -> Result<()> {
    let home = hermes_config::talk_dir();
    let config_path = hermes_config::talk_config_path();
    let created = !config_path.exists();
    ensure_talk_home()?;
    if created {
        println!("Created {}", config_path.display());
    } else {
        println!("Config already exists: {}", config_path.display());
    }
    print_post_init_notes(&home);
    Ok(())
}

fn print_post_init_notes(home: &Path) {
    println!();
    println!("Talk home: {}", home.display());
    println!();
    println!("Next steps:");
    println!(
        "  1. Edit {} with your API keys and backends.",
        hermes_config::talk_config_path().display()
    );
    println!(
        "  2. Place ONNX models under {}/models/ (vad, denoise, speaker, kws).",
        home.display()
    );
    println!("     For `make package-talk-rockchip`, mirror the same tree under <repo>/.models/.");
    println!(
        "  3. For Rockchip local ASR/TTS, copy SDK data to {}/data, {}/models/rk3588, and licenses to {}/auth/.",
        home.display(),
        home.display(),
        home.display()
    );
    println!("  4. Run `hermes talk list-devices` to verify audio devices.");
    println!("  5. Run `hermes talk` to start the voice dialog loop.");
    println!();
    println!(
        "Note: `call_hermes` uses in-process channel transport by default (transport = \"channel\")."
    );
    println!(
        "      Set transport = \"ws\" and url = \"ws://127.0.0.1:9100\" for remote Hermes bridge."
    );
}
