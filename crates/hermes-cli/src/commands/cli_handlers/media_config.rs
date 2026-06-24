//! `hermes media config` — command-line image/video generation setup.

use std::path::{Path, PathBuf};

use hermes_config::{
    GatewayConfig, MediaGenConfig, apply_user_config_patch, hermes_home, load_user_config_file,
    save_config_yaml, user_config_field_display, validate_config,
};
use hermes_core::AgentError;

pub fn config_yaml_path(config_dir: Option<&str>) -> PathBuf {
    config_dir
        .map(PathBuf::from)
        .unwrap_or_else(hermes_home)
        .join("config.yaml")
}

pub fn normalize_media_config_key(key: &str) -> Result<String, AgentError> {
    let normalized = key.trim().to_ascii_lowercase().replace('-', "_");
    let full = match normalized.as_str() {
        "provider" => "media.provider".to_string(),
        "image_model" | "image" => "media.image.model".to_string(),
        "image_save_locally" | "save_image_locally" => "media.image.save_locally".to_string(),
        "video_model" | "video" => "media.video.model".to_string(),
        "video_duration" | "default_duration" | "duration" => {
            "media.video.default_duration".to_string()
        }
        "aspect_ratio" | "video_aspect_ratio" => "media.video.default_aspect_ratio".to_string(),
        "video_resolution" | "resolution" => "media.video.default_resolution".to_string(),
        "video_poll_timeout" | "poll_timeout" => "media.video.poll_timeout_seconds".to_string(),
        "video_save_locally" | "save_video_locally" => "media.video.save_locally".to_string(),
        "workflows_enabled" | "workflows" => "media.workflows.enabled".to_string(),
        "workflow_retries" | "max_retries" => "media.workflows.max_retries".to_string(),
        "txt2img_template" | "template_txt2img" => {
            "media.workflows.default_templates.txt2img".to_string()
        }
        "txt2video_template" | "template_txt2video" => {
            "media.workflows.default_templates.txt2video".to_string()
        }
        "img2video_template" | "template_img2video" => {
            "media.workflows.default_templates.img2video".to_string()
        }
        "storyboard_template" | "template_storyboard" => {
            "media.workflows.default_templates.storyboard".to_string()
        }
        "async_execution" | "workflow_async" => "media.workflows.async_execution".to_string(),
        "llm_prompt_refine" | "llm_refine" => "media.workflows.llm_prompt_refine".to_string(),
        "check_credits" | "credits_check" => "media.workflows.check_credits".to_string(),
        "image_min_credits" => "media.workflows.image_min_credits".to_string(),
        "video_credits_per_second" => "media.workflows.video_credits_per_second".to_string(),
        "storyboard_max_shots" | "max_shots" => "media.workflows.storyboard_max_shots".to_string(),
        other if other.starts_with("media.") => other.to_string(),
        _ => {
            return Err(AgentError::Config(format!(
                "unknown media config key '{key}' (try: provider, image_model, video_model, workflows_enabled, ...)"
            )));
        }
    };
    Ok(full)
}

pub fn save_media_field(
    config_dir: Option<&str>,
    key: &str,
    value: &str,
) -> Result<PathBuf, AgentError> {
    let cfg_path = config_yaml_path(config_dir);
    let mut disk =
        load_user_config_file(&cfg_path).map_err(|e| AgentError::Config(e.to_string()))?;
    let full_key = normalize_media_config_key(key)?;
    apply_user_config_patch(&mut disk, &full_key, value)
        .map_err(|e| AgentError::Config(e.to_string()))?;
    validate_config(&disk).map_err(|e| AgentError::Config(e.to_string()))?;
    save_config_yaml(&cfg_path, &disk).map_err(|e| AgentError::Config(e.to_string()))?;
    Ok(cfg_path)
}

pub fn print_media_config(media: &MediaGenConfig, cfg_path: &Path, server_enabled: bool) {
    println!("Image & video generation configuration");
    println!("  config file: {}", cfg_path.display());
    println!("  provider: {}", media.provider);
    println!(
        "  image model: {}",
        display_or_auto(
            &media.image.model,
            "first available image model from server"
        )
    );
    println!("  image save_locally: {}", media.image.save_locally);
    println!(
        "  video model: {}",
        display_or_auto(
            &media.video.model,
            "first available video model from server"
        )
    );
    println!(
        "  video default_duration: {}s",
        media.video.default_duration
    );
    println!(
        "  video default_aspect_ratio: {}",
        media.video.default_aspect_ratio
    );
    println!(
        "  video default_resolution: {}",
        media.video.default_resolution
    );
    println!(
        "  video poll_timeout_seconds: {}",
        media.video.poll_timeout_seconds
    );
    println!("  video save_locally: {}", media.video.save_locally);
    println!("  workflows enabled: {}", media.workflows.enabled);
    println!("  workflows max_retries: {}", media.workflows.max_retries);
    println!(
        "  workflows async_execution: {}",
        media.workflows.async_execution
    );
    println!(
        "  workflows llm_prompt_refine: {}",
        media.workflows.llm_prompt_refine
    );
    println!(
        "  workflows check_credits: {} (image min {}, video {} credits/s)",
        media.workflows.check_credits,
        media.workflows.image_min_credits,
        media.workflows.video_credits_per_second
    );
    println!(
        "  workflows storyboard_max_shots: {}",
        media.workflows.storyboard_max_shots
    );
    let tpl = &media.workflows.default_templates;
    println!("  workflow templates:");
    println!(
        "    txt2img: {}",
        display_or_auto(&tpl.txt2img, "simple_txt2img")
    );
    println!(
        "    txt2video: {}",
        display_or_auto(&tpl.txt2video, "prompt_refine_txt2video")
    );
    println!(
        "    img2video: {}",
        display_or_auto(&tpl.img2video, "img2video")
    );
    println!(
        "    storyboard: {}",
        display_or_auto(&tpl.storyboard, "storyboard_to_video")
    );
    if media.uses_flowy() {
        if server_enabled {
            println!();
            println!(
                "  Flowy server routing: enabled (run `hermes server login` if not logged in)"
            );
        } else {
            println!();
            println!(
                "  Note: set `server.enabled=true` and run `hermes server login` for Flowy APIs"
            );
        }
    }
}

pub fn print_config_help() {
    println!("Configure image/video generation (saved to config.yaml)");
    println!();
    println!("Usage:");
    println!("  hermes media config              Show current settings");
    println!("  hermes media config init         Interactive setup wizard");
    println!("  hermes media config set <key> <value>");
    println!("  hermes media config get <key>");
    println!("  hermes media config path         Show config.yaml path");
    println!();
    println!("Keys (short names for `set` / `get`):");
    println!("  provider              flowy (default) or fal");
    println!("  image_model           Image model id (AIPC-... or flowy/...)");
    println!("  video_model           Video model id");
    println!("  video_duration        Default video length in seconds");
    println!("  aspect_ratio          Default aspect ratio (e.g. 16:9)");
    println!("  video_resolution      Default resolution (e.g. 720p)");
    println!("  image_save_locally    Save generated images locally (true/false)");
    println!("  video_save_locally    Save generated videos locally (true/false)");
    println!("  workflows_enabled     Enable workflow tools (true/false)");
    println!("  workflow_retries      Max workflow QA retries");
    println!("  async_execution       Run workflows in background (true/false)");
    println!("  llm_prompt_refine     LLM-based prompt/storyboard refine (true/false)");
    println!("  check_credits         Pre-check Flowy credit balance (true/false)");
    println!("  storyboard_max_shots  Max shots for multi-storyboard workflow");
    println!("  txt2img_template      Default txt2img workflow id");
    println!("  txt2video_template    Default txt2video workflow id");
    println!();
    println!("Model picker (requires `hermes server login`):");
    println!("  hermes media models              List image + video models");
    println!("  hermes media models image        List image models only");
    println!("  hermes media models video        List video models only");
    println!("  hermes media models pick image   Interactive image model picker");
    println!("  hermes media models pick video   Interactive video model picker");
    println!();
    println!("Examples:");
    println!("  hermes media config init");
    println!("  hermes media models pick image");
    println!("  hermes media config set image_model AIPC-z-image-turbo");
    println!("  hermes media config set video_duration 8");
}

pub async fn handle_media_config(
    rest: &[String],
    config_dir: Option<&str>,
    loaded: &GatewayConfig,
) -> Result<(), AgentError> {
    let cfg_path = config_yaml_path(config_dir);
    match rest.first().map(|s| s.as_str()) {
        None | Some("show") => {
            print_media_config(&loaded.media, &cfg_path, loaded.server.enabled);
            Ok(())
        }
        Some("help") | Some("--help") | Some("-h") => {
            print_config_help();
            Ok(())
        }
        Some("path") => {
            println!("{}", cfg_path.display());
            Ok(())
        }
        Some("init") | Some("setup") => run_config_init(config_dir, loaded).await,
        Some("set") => {
            let key = rest.get(1).ok_or_else(|| {
                AgentError::Config("usage: hermes media config set <key> <value>".into())
            })?;
            let value = rest.get(2..).ok_or_else(|| {
                AgentError::Config("usage: hermes media config set <key> <value>".into())
            })?;
            if value.is_empty() {
                return Err(AgentError::Config(
                    "usage: hermes media config set <key> <value>".into(),
                ));
            }
            let value = value.join(" ");
            let saved = save_media_field(config_dir, key, &value)?;
            let full_key = normalize_media_config_key(key)?;
            println!("Saved {full_key} = {value} → {}", saved.display());
            Ok(())
        }
        Some("get") => {
            let key = rest
                .get(1)
                .ok_or_else(|| AgentError::Config("usage: hermes media config get <key>".into()))?;
            let full_key = normalize_media_config_key(key)?;
            let disk =
                load_user_config_file(&cfg_path).map_err(|e| AgentError::Config(e.to_string()))?;
            match user_config_field_display(&disk, &full_key) {
                Ok(value) => println!("{value}"),
                Err(e) => return Err(AgentError::Config(e.to_string())),
            }
            Ok(())
        }
        Some(other) => Err(AgentError::Config(format!(
            "unknown media config subcommand '{other}'. Try: hermes media config help"
        ))),
    }
}

async fn run_config_init(
    config_dir: Option<&str>,
    loaded: &GatewayConfig,
) -> Result<(), AgentError> {
    println!("Image & video generation setup wizard");
    println!("Press Enter to accept [default] values.\n");

    if !loaded.server.enabled || loaded.server.base_url.trim().is_empty() {
        println!(
            "Tip: Flowy provider needs `hermes server config init` + `hermes server login` first."
        );
        println!();
    }

    let provider = prompt_with_default("Provider (flowy|fal)", &loaded.media.provider).await?;
    let workflows = prompt_with_default("Enable workflow tools? (true/false)", "true").await?;
    let image_save =
        prompt_with_default("Save generated images locally? (true/false)", "true").await?;
    let video_save =
        prompt_with_default("Save generated videos locally? (true/false)", "true").await?;
    let duration = prompt_with_default("Default video duration (seconds)", "5").await?;
    let aspect = prompt_with_default("Default aspect ratio", "16:9").await?;
    let resolution = prompt_with_default("Default resolution", "720p").await?;

    let mut image_model = loaded.media.image.model.clone();
    let mut video_model = loaded.media.video.model.clone();

    if provider.trim().eq_ignore_ascii_case("flowy") && loaded.server.api_ready() {
        let pick_image =
            prompt_with_default("Pick image model from server now? (true/false)", "false").await?;
        if pick_image.trim().eq_ignore_ascii_case("true")
            && let Some(id) = super::media::interactive_model_pick(config_dir, "image").await?
        {
            image_model = id;
        }
        let pick_video =
            prompt_with_default("Pick video model from server now? (true/false)", "false").await?;
        if pick_video.trim().eq_ignore_ascii_case("true")
            && let Some(id) = super::media::interactive_model_pick(config_dir, "video").await?
        {
            video_model = id;
        }
    }

    let patches = [
        ("provider", provider.trim()),
        ("workflows_enabled", workflows.trim()),
        ("image_save_locally", image_save.trim()),
        ("video_save_locally", video_save.trim()),
        ("video_duration", duration.trim()),
        ("aspect_ratio", aspect.trim()),
        ("video_resolution", resolution.trim()),
        ("image_model", image_model.trim()),
        ("video_model", video_model.trim()),
    ];

    for (key, value) in patches {
        if !value.is_empty() || key.ends_with("_model") {
            save_media_field(config_dir, key, value)?;
        }
    }

    let cfg_path = config_yaml_path(config_dir);
    println!();
    println!("Media configuration saved → {}", cfg_path.display());
    println!("Next: `hermes media models pick image` / `hermes media models pick video`");
    Ok(())
}

fn display_or_auto(value: &str, auto_label: &str) -> String {
    if value.trim().is_empty() {
        format!("(auto: {auto_label})")
    } else {
        value.to_string()
    }
}

async fn prompt_with_default(label: &str, default: &str) -> Result<String, AgentError> {
    let prompt = if default.is_empty() {
        format!("{label}: ")
    } else {
        format!("{label} [{default}]: ")
    };
    let line = prompt_line(&prompt).await?;
    if line.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(line)
    }
}

pub(crate) async fn prompt_line(prompt: &str) -> Result<String, AgentError> {
    let line = tokio::task::spawn_blocking({
        let prompt = prompt.to_string();
        move || {
            use std::io::{self, Write};
            print!("{prompt}");
            let _ = io::stdout().flush();
            let mut buf = String::new();
            io::stdin().read_line(&mut buf).map(|_| buf)
        }
    })
    .await
    .map_err(|e| AgentError::Io(format!("stdin task: {e}")))?
    .map_err(|e| AgentError::Io(format!("stdin: {e}")))?;
    Ok(line.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_key_aliases() {
        assert_eq!(
            normalize_media_config_key("image-model").unwrap(),
            "media.image.model"
        );
        assert_eq!(
            normalize_media_config_key("video_duration").unwrap(),
            "media.video.default_duration"
        );
        assert_eq!(
            normalize_media_config_key("media.provider").unwrap(),
            "media.provider"
        );
    }
}
