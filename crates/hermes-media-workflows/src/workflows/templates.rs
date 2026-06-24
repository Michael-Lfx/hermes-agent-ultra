//! Built-in workflow templates.

use serde_json::{Value, json};

use super::definition::WorkflowDefinition;

const SIMPLE_TXT2IMG: &str = r#"
id: simple_txt2img
version: 1
description: Single-step text-to-image via Flowy server
inputs:
  prompt: { type: string, required: true }
  model: { type: string, required: false }
steps:
  - id: generate
    kind: image_generate
    input:
      prompt: "$inputs.prompt"
      model: "$inputs.model"
"#;

const PROMPT_REFINE_TXT2VIDEO: &str = r#"
id: prompt_refine_txt2video
version: 1
description: Refine prompt then text-to-video
inputs:
  prompt: { type: string, required: true }
  duration: { type: integer, default: 5 }
  aspect_ratio: { type: string, default: "16:9" }
steps:
  - id: refine_prompt
    kind: prompt_refine
    input:
      prompt: "$inputs.prompt"
      medium: video
  - id: video
    kind: video_generate
    depends_on: [refine_prompt]
    input:
      prompt: "$steps.refine_prompt.output"
      duration: "$inputs.duration"
      aspect_ratio: "$inputs.aspect_ratio"
"#;

const IMG2VIDEO: &str = r#"
id: img2video
version: 1
description: Generate keyframe image then image-to-video
inputs:
  prompt: { type: string, required: true }
  duration: { type: integer, default: 5 }
  aspect_ratio: { type: string, default: "16:9" }
steps:
  - id: keyframe
    kind: image_generate
    input:
      prompt: "$inputs.prompt"
  - id: video
    kind: video_generate
    depends_on: [keyframe]
    input:
      prompt: "$inputs.prompt"
      image_url: "$steps.keyframe.best_url"
      duration: "$inputs.duration"
      aspect_ratio: "$inputs.aspect_ratio"
"#;

const STORYBOARD_VIDEO: &str = r#"
id: storyboard_to_video
version: 1
description: Refine prompt, generate keyframe, then image-to-video
inputs:
  prompt: { type: string, required: true }
  duration: { type: integer, default: 8 }
  aspect_ratio: { type: string, default: "16:9" }
steps:
  - id: refine_prompt
    kind: prompt_refine
    input:
      prompt: "$inputs.prompt"
      medium: video
  - id: keyframe
    kind: image_generate
    depends_on: [refine_prompt]
    input:
      prompt: "$steps.refine_prompt.output"
  - id: video
    kind: video_generate
    depends_on: [keyframe]
    input:
      prompt: "$steps.refine_prompt.output"
      image_url: "$steps.keyframe.best_url"
      duration: "$inputs.duration"
      aspect_ratio: "$inputs.aspect_ratio"
"#;

pub fn list_builtin_templates() -> Vec<&'static str> {
    vec![
        "simple_txt2img",
        "prompt_refine_txt2video",
        "img2video",
        "storyboard_to_video",
    ]
}

pub fn builtin_template(id: &str) -> Option<WorkflowDefinition> {
    let yaml = match id {
        "simple_txt2img" => SIMPLE_TXT2IMG,
        "prompt_refine_txt2video" => PROMPT_REFINE_TXT2VIDEO,
        "img2video" => IMG2VIDEO,
        "storyboard_to_video" => STORYBOARD_VIDEO,
        _ => return None,
    };
    serde_yaml::from_str(yaml).ok()
}

/// Pick a template id from user intent keywords.
pub fn suggest_template_id(objective: &str, has_image_input: bool) -> &'static str {
    let lower = objective.to_ascii_lowercase();
    if has_image_input || lower.contains("图生视频") || lower.contains("image to video") {
        return "img2video";
    }
    if lower.contains("分镜") || lower.contains("storyboard") || lower.contains("叙事") {
        return "storyboard_to_video";
    }
    if lower.contains("视频") || lower.contains("video") {
        return "prompt_refine_txt2video";
    }
    "simple_txt2img"
}

pub fn default_template_inputs(template_id: &str, prompt: &str) -> Value {
    match template_id {
        "simple_txt2img" => json!({ "prompt": prompt }),
        _ => json!({
            "prompt": prompt,
            "duration": 5,
            "aspect_ratio": "16:9"
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_builtin_templates_parse() {
        for id in list_builtin_templates() {
            let def = builtin_template(id).unwrap_or_else(|| panic!("missing {id}"));
            assert_eq!(def.id, id);
            assert!(!def.steps.is_empty());
        }
    }

    #[test]
    fn suggest_template_video() {
        assert_eq!(
            suggest_template_id("generate a short product video", false),
            "prompt_refine_txt2video"
        );
    }
}
