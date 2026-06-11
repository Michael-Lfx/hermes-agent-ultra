//! Approval and pairing slash commands (`/approve`, `/deny`, `/gquota`).

use std::fmt::Write as _;

use hermes_core::AgentError;

use crate::commands::{CommandResult, emit_command_output};
use crate::pairing_store::{PairingStatus, PairingStore};

pub(crate) async fn handle_gquota_command(
    host: &mut impl crate::app::SlashCommandHost,
    _args: &[&str],
) -> Result<CommandResult, AgentError> {
    let provider = host
        .current_model()
        .split_once(':')
        .map(|(p, _)| p.trim().to_ascii_lowercase())
        .unwrap_or_else(|| "unknown".to_string());
    let gemini_vars = [
        "HERMES_GEMINI_OAUTH_API_KEY",
        "GOOGLE_API_KEY",
        "GEMINI_API_KEY",
    ];
    let mut present = Vec::new();
    for key in gemini_vars {
        if std::env::var(key)
            .ok()
            .is_some_and(|v| !v.trim().is_empty())
        {
            present.push(key.to_string());
        }
    }
    let oauth_state = crate::auth::read_provider_auth_state("google-gemini-cli")
        .ok()
        .flatten();
    let expires_at = oauth_state
        .as_ref()
        .and_then(|v| v.get("expires_at_ms"))
        .and_then(|v| v.as_i64());
    let mut out = String::new();
    let _ = writeln!(out, "Gemini quota/auth diagnostics");
    let _ = writeln!(out, "  - active provider: {}", provider);
    let _ = writeln!(
        out,
        "  - gemini creds in env: {} ({})",
        if present.is_empty() { "no" } else { "yes" },
        if present.is_empty() {
            "none".to_string()
        } else {
            present.join(", ")
        }
    );
    let _ = writeln!(
        out,
        "  - oauth state file: {}",
        if oauth_state.is_some() {
            "present"
        } else {
            "missing"
        }
    );
    if let Some(ms) = expires_at {
        let ts = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms)
            .map(|d| d.to_rfc3339())
            .unwrap_or_else(|| "invalid".to_string());
        let _ = writeln!(out, "  - token expiry: {}", ts);
    }
    let _ = writeln!(
        out,
        "  - live quota API: unavailable in local CLI; check provider dashboard for hard usage limits."
    );
    emit_command_output(host, out.trim_end());
    Ok(CommandResult::Handled)
}

pub(crate) fn handle_approve_command(
    host: &mut impl crate::app::SlashCommandHost,
    args: &[&str],
) -> Result<CommandResult, AgentError> {
    let store = PairingStore::open_default();
    if args.is_empty() || args[0].eq_ignore_ascii_case("list") {
        let pending: Vec<_> = store
            .list()
            .unwrap_or_default()
            .into_iter()
            .filter(|d| d.status == PairingStatus::Pending)
            .collect();
        if pending.is_empty() {
            emit_command_output(
                host,
                "No pending devices to approve. Use `hermes pairing list` for full inventory.",
            );
            return Ok(CommandResult::Handled);
        }
        let mut out = String::from("Pending pairing devices:\n");
        for dev in pending {
            out.push_str(&format!(
                "  - {} ({})\n",
                dev.device_id,
                dev.name.unwrap_or_else(|| "unnamed".to_string())
            ));
        }
        out.push_str("Approve one with `/approve <device-id>` or all with `/approve all`.");
        emit_command_output(host, out.trim_end());
        return Ok(CommandResult::Handled);
    }

    if args[0].eq_ignore_ascii_case("all") {
        let mut approved = 0usize;
        for dev in store.list().unwrap_or_default() {
            if dev.status == PairingStatus::Pending && store.approve(&dev.device_id).is_ok() {
                approved += 1;
            }
        }
        emit_command_output(host, format!("Approved {} pending device(s).", approved));
        return Ok(CommandResult::Handled);
    }

    match store.approve(args[0]) {
        Ok(dev) => emit_command_output(
            host,
            format!(
                "Approved device '{}' (name={}).",
                dev.device_id,
                dev.name.unwrap_or_else(|| "unnamed".to_string())
            ),
        ),
        Err(err) => emit_command_output(
            host,
            format!(
                "Approve failed: {}. Use `/approve list` or `hermes pairing list`.",
                err
            ),
        ),
    }
    Ok(CommandResult::Handled)
}

pub(crate) fn handle_deny_command(
    host: &mut impl crate::app::SlashCommandHost,
    args: &[&str],
) -> Result<CommandResult, AgentError> {
    let store = PairingStore::open_default();
    if args.is_empty() || args[0].eq_ignore_ascii_case("list") {
        let entries = store.list().unwrap_or_default();
        let mut out = String::from("Pairing devices (deny/revoke candidates):\n");
        if entries.is_empty() {
            out.push_str("  - none\n");
        } else {
            for dev in entries {
                out.push_str(&format!("  - {} [{}]\n", dev.device_id, dev.status));
            }
        }
        out.push_str("Revoke one with `/deny <device-id>` or purge pending with `/deny pending`.");
        emit_command_output(host, out.trim_end());
        return Ok(CommandResult::Handled);
    }

    if args[0].eq_ignore_ascii_case("pending") || args[0].eq_ignore_ascii_case("clear-pending") {
        match store.clear_pending() {
            Ok(count) => emit_command_output(host, format!("Removed {} pending device(s).", count)),
            Err(err) => {
                emit_command_output(host, format!("Failed clearing pending devices: {}", err))
            }
        }
        return Ok(CommandResult::Handled);
    }

    match store.revoke(args[0]) {
        Ok(dev) => emit_command_output(
            host,
            format!(
                "Revoked device '{}' (name={}).",
                dev.device_id,
                dev.name.unwrap_or_else(|| "unnamed".to_string())
            ),
        ),
        Err(err) => emit_command_output(
            host,
            format!(
                "Deny failed: {}. Use `/deny list` or `hermes pairing list`.",
                err
            ),
        ),
    }
    Ok(CommandResult::Handled)
}
