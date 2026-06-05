//! cpal device enumeration and selection (substring / index / prefer USB).

use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, Host};
use tracing::info;

use crate::config::AudioConfig;
use crate::error::{HalfDuplexError, Result};

/// Prefer WASAPI on Windows so USB headsets expose separate in/out endpoints.
pub fn audio_host() -> Host {
    #[cfg(target_os = "windows")]
    {
        if let Ok(host) = cpal::host_from_id(cpal::HostId::Wasapi) {
            return host;
        }
    }
    cpal::default_host()
}

pub fn list_input_names(host: &Host) -> Result<Vec<String>> {
    Ok(host
        .input_devices()
        .map_err(|e| HalfDuplexError::Audio(e.to_string()))?
        .filter_map(|d| d.name().ok())
        .collect())
}

pub fn list_output_names(host: &Host) -> Result<Vec<String>> {
    Ok(host
        .output_devices()
        .map_err(|e| HalfDuplexError::Audio(e.to_string()))?
        .filter_map(|d| d.name().ok())
        .collect())
}

pub fn resolve_input_device(host: &Host, cfg: &AudioConfig) -> Result<Device> {
    let available = list_input_names(host)?;
    let default_name = host
        .default_input_device()
        .and_then(|d| d.name().ok());
    let chosen = resolve_device_spec(
        &available,
        &cfg.input_device,
        cfg.prefer_usb,
        default_name.as_deref(),
        "input",
    )?;
    info!(
        kind = "input",
        chosen = %chosen,
        prefer_usb = cfg.prefer_usb,
        available = ?available,
        "audio device selected"
    );
    find_device_by_name(
        host.input_devices()
            .map_err(|e| HalfDuplexError::Audio(e.to_string()))?,
        &chosen,
    )
}

pub fn resolve_output_device(host: &Host, cfg: &AudioConfig) -> Result<Device> {
    let available = list_output_names(host)?;
    let default_name = host
        .default_output_device()
        .and_then(|d| d.name().ok());
    let chosen = resolve_device_spec(
        &available,
        &cfg.output_device,
        cfg.prefer_usb,
        default_name.as_deref(),
        "output",
    )?;
    info!(
        kind = "output",
        chosen = %chosen,
        prefer_usb = cfg.prefer_usb,
        available = ?available,
        "audio device selected"
    );
    find_device_by_name(
        host.output_devices()
            .map_err(|e| HalfDuplexError::Audio(e.to_string()))?,
        &chosen,
    )
}

pub fn format_device_list() -> Result<String> {
    let host = audio_host();
    let mut out = format!("Audio host: {:?}\n", host.id());

    out.push_str("\nInput (microphone) devices:\n");
    let inputs = list_input_names(&host)?;
    if inputs.is_empty() {
        out.push_str("  (none)\n");
    } else {
        let default_in = host
            .default_input_device()
            .and_then(|d| d.name().ok())
            .unwrap_or_default();
        for (i, name) in inputs.iter().enumerate() {
            let tags = device_tags(name);
            let def = if name == &default_in { " [system default]" } else { "" };
            out.push_str(&format!("  #{i}: {name}{tags}{def}\n"));
        }
    }

    out.push_str("\nOutput (speaker) devices:\n");
    let outputs = list_output_names(&host)?;
    if outputs.is_empty() {
        out.push_str("  (none)\n");
    } else {
        let default_out = host
            .default_output_device()
            .and_then(|d| d.name().ok())
            .unwrap_or_default();
        for (i, name) in outputs.iter().enumerate() {
            let tags = device_tags(name);
            let def = if name == &default_out { " [system default]" } else { "" };
            out.push_str(&format!("  #{i}: {name}{tags}{def}\n"));
        }
    }

    out.push_str(
        "\nSet in ~/.hermes/half_duplex.toml:\n\
          [audio]\n\
          input_device = \"USB\"          # substring match\n\
          output_device = \"USB\"\n\
          prefer_usb = true               # when empty, pick USB over Bluetooth default\n\
         Or use index: input_device = \"#0\"\n",
    );
    Ok(out)
}

fn find_device_by_name(
    mut devices: impl Iterator<Item = Device>,
    name: &str,
) -> Result<Device> {
    devices
        .find(|d| d.name().ok().as_deref() == Some(name))
        .ok_or_else(|| HalfDuplexError::Audio(format!("device disappeared: {name}")))
}

fn resolve_device_spec(
    available: &[String],
    spec: &str,
    prefer_usb: bool,
    default_name: Option<&str>,
    kind: &str,
) -> Result<String> {
    let spec = spec.trim();
    if available.is_empty() {
        return Err(HalfDuplexError::Audio(format!("no {kind} devices found")));
    }

    if let Some(idx_str) = spec.strip_prefix('#') {
        let idx: usize = idx_str.parse().map_err(|_| {
            HalfDuplexError::Audio(format!("invalid {kind} device index: {spec}"))
        })?;
        return available.get(idx).cloned().ok_or_else(|| {
            HalfDuplexError::Audio(format!(
                "{kind} device index #{idx} out of range (0..{})",
                available.len().saturating_sub(1)
            ))
        });
    }

    if !spec.is_empty() {
        if let Some(name) = available.iter().find(|n| name_matches_spec(n, spec)) {
            return Ok(name.clone());
        }
        return Err(HalfDuplexError::Audio(format!(
            "{kind} device not found: \"{spec}\". Available: {}",
            available.join(" | ")
        )));
    }

    if prefer_usb {
        if let Some(name) = available.iter().find(|n| is_usb_like(n)) {
            return Ok(name.clone());
        }
    }

    if let Some(name) = default_name {
        if available.iter().any(|n| n == name) {
            return Ok(name.to_string());
        }
    }

    Ok(available[0].clone())
}

fn name_matches_spec(device_name: &str, spec: &str) -> bool {
    if device_name == spec {
        return true;
    }
    if device_name.eq_ignore_ascii_case(spec) {
        return true;
    }
    device_name
        .to_lowercase()
        .contains(&spec.to_lowercase())
}

fn is_usb_like(name: &str) -> bool {
    let n = name.to_lowercase();
    (n.contains("usb") || n.contains("uac") || n.contains("type-c") || n.contains("typec"))
        && !is_bluetooth_like(name)
}

fn is_bluetooth_like(name: &str) -> bool {
    let n = name.to_lowercase();
    n.contains("bluetooth")
        || n.contains("hands-free")
        || n.contains("hands free")
        || n.contains("airpods")
        || n.contains("bt ")
        || n.contains("wireless")
}

fn device_tags(name: &str) -> &'static str {
    if is_usb_like(name) {
        " [usb]"
    } else if is_bluetooth_like(name) {
        " [bluetooth]"
    } else {
        ""
    }
}
