//! WhatsApp Rust client CLI wizard.

use std::io::{self, BufRead, Write};

use hermes_gateway::platforms::whatsapp::{
    has_legacy_baileys_session, is_paired, session_db_path, WhatsAppConfig, WhatsAppRustClient,
};

fn session_path() -> std::path::PathBuf {
    hermes_config::hermes_home().join("whatsapp").join("session")
}

fn prompt_line(label: &str) -> Result<String, hermes_core::AgentError> {
    print!("{label}");
    io::stdout()
        .flush()
        .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
    let line = io::stdin()
        .lock()
        .lines()
        .next()
        .transpose()
        .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?
        .unwrap_or_default();
    Ok(line.trim().to_string())
}

fn persist_whatsapp_config(
    mode: &str,
    allow_from: &[String],
    enable: bool,
) -> Result<(), hermes_core::AgentError> {
    let config_path = hermes_config::hermes_home().join("config.yaml");
    let mut config: serde_yaml::Value = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)
            .map_err(|e| hermes_core::AgentError::Io(format!("Read error: {e}")))?;
        serde_yaml::from_str(&content).unwrap_or(serde_yaml::Value::Mapping(Default::default()))
    } else {
        serde_yaml::Value::Mapping(Default::default())
    };

    let platforms = config
        .as_mapping_mut()
        .unwrap()
        .entry(serde_yaml::Value::String("platforms".into()))
        .or_insert_with(|| serde_yaml::Value::Mapping(Default::default()));

    let wa = platforms
        .as_mapping_mut()
        .unwrap()
        .entry(serde_yaml::Value::String("whatsapp".into()))
        .or_insert_with(|| serde_yaml::Value::Mapping(Default::default()));

    let wa_map = wa.as_mapping_mut().unwrap();
    wa_map.insert(
        serde_yaml::Value::String("enabled".into()),
        serde_yaml::Value::Bool(enable),
    );
    if !allow_from.is_empty() {
        wa_map.insert(
            serde_yaml::Value::String("extra".into()),
            serde_yaml::Value::Mapping({
                let mut m = serde_yaml::Mapping::new();
                m.insert(
                    serde_yaml::Value::String("allow_from".into()),
                    serde_yaml::Value::Sequence(
                        allow_from
                            .iter()
                            .map(|u| serde_yaml::Value::String(u.clone()))
                            .collect(),
                    ),
                );
                m
            }),
        );
    }

    std::fs::create_dir_all(hermes_config::hermes_home())
        .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
    let yaml_str = serde_yaml::to_string(&config)
        .map_err(|e| hermes_core::AgentError::Config(e.to_string()))?;
    std::fs::write(&config_path, yaml_str)
        .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;

    // SAFETY: wizard runs single-threaded during CLI setup.
    unsafe {
        std::env::set_var("WHATSAPP_MODE", mode);
        if enable {
            std::env::set_var("WHATSAPP_ENABLED", "true");
        }
        if !allow_from.is_empty() {
            std::env::set_var("WHATSAPP_ALLOWED_USERS", allow_from.join(","));
        }
    }

    Ok(())
}

/// Interactive wa-rs setup wizard (QR pairing, no Node.js required).
pub async fn whatsapp_baileys_wizard() -> Result<(), hermes_core::AgentError> {
    println!("WhatsApp Setup (Rust / wa-rs)");
    println!("==============================\n");

    println!("Choose mode:");
    println!("  1) self-chat — message yourself on WhatsApp (quick test)");
    println!("  2) bot — dedicated bot number (recommended)");
    let mode_choice = prompt_line("Mode [1/2] (default 1): ")?;
    let mode = if mode_choice == "2" { "bot" } else { "self-chat" };

    let mut allow_from = Vec::new();
    if mode == "bot" {
        let users = prompt_line(
            "Allowed users (comma-separated phone numbers, or * for open bot): ",
        )?;
        if !users.is_empty() {
            allow_from = users
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }

    let session = session_path();
    if has_legacy_baileys_session(&session) && !is_paired(&session) {
        println!(
            "\nLegacy Baileys session found at {}.",
            session.display()
        );
        println!("The Rust client uses a new SQLite session — you must re-pair.");
        let cont = prompt_line("Continue with new pairing? [Y/n]: ")?;
        if cont.eq_ignore_ascii_case("n") {
            return Ok(());
        }
    }

    if is_paired(&session) {
        println!("\nExisting Rust session found at {}.", session.display());
        let reuse = prompt_line("Skip re-pairing? [Y/n]: ")?;
        if !reuse.eq_ignore_ascii_case("n") {
            persist_whatsapp_config(mode, &allow_from, true)?;
            println!("\nWhatsApp enabled. Run `hermes gateway` to start.");
            return Ok(());
        }
    }

    println!("\nStarting QR pairing — scan with WhatsApp → Linked Devices.\n");
    println!("Session database: {}\n", session_db_path(&session).display());

    let mut cfg = WhatsAppConfig::default();
    cfg.session_path = Some(session.to_string_lossy().into_owned());
    let client = WhatsAppRustClient::new(cfg);

    match client.run_pairing().await {
        Ok(()) if is_paired(&session) => {
            persist_whatsapp_config(mode, &allow_from, true)?;
            println!("\nPairing successful! WhatsApp is enabled.");
            println!("Run `hermes gateway` to connect.");
        }
        Ok(()) => {
            println!("\nPairing did not complete.");
            println!("WHATSAPP_ENABLED was not set — re-run when pairing succeeds.");
        }
        Err(e) => {
            println!("\nPairing failed: {e}");
        }
    }
    Ok(())
}

/// Show WhatsApp Rust client status.
pub async fn whatsapp_baileys_status() -> Result<(), hermes_core::AgentError> {
    println!("WhatsApp Status (Rust / wa-rs)");
    println!("--------------------------------");
    let session = session_path();
    let paired = is_paired(&session);
    let legacy = has_legacy_baileys_session(&session);
    println!("  Session dir:    {}", session.display());
    println!(
        "  Rust paired:    {}",
        if paired { "yes" } else { "no" }
    );
    if legacy {
        println!("  Legacy Baileys: creds.json present (re-pair if not migrated)");
    }
    println!("  SQLite db:      {}", session_db_path(&session).display());

    let config_path = hermes_config::hermes_home().join("config.yaml");
    if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)
            .map_err(|e| hermes_core::AgentError::Io(e.to_string()))?;
        let config: serde_yaml::Value =
            serde_yaml::from_str(&content).unwrap_or(serde_yaml::Value::Null);
        let enabled = config
            .get("platforms")
            .and_then(|p| p.get("whatsapp"))
            .and_then(|w| w.get("enabled"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        println!("  Enabled:        {enabled}");
    } else {
        println!("  Enabled:        false (no config.yaml)");
    }

    if !paired {
        println!("  Run `hermes whatsapp` to pair via QR.");
    }
    Ok(())
}

pub async fn whatsapp_cloud_setup() -> Result<(), hermes_core::AgentError> {
    crate::commands::whatsapp_cloud_setup_impl().await
}
