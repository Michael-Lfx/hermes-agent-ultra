---
name: flowy-media-generation
description: "Generate images and videos via Flowy cloud APIs (Hermes media tools). Covers prompt writing, workflow selection, async polling, and MEDIA: delivery."
version: 1.1.0
metadata:
  hermes:
    tags:
      - flowy
      - image-generation
      - video-generation
      - seedance
      - media
      - storyboard
    category: media
---

# Flowy Media Generation

Use Hermes Flowy-backed tools (`image_generate`, `video_generate`, `media_workflow_*`) when `media.provider=flowy` and the user is logged in.

## When to use which tool

| User intent | Tool |
|-------------|------|
| Single image, quick | `image_generate` |
| Video or best quality | `media_workflow_plan` → `media_workflow_run` |
| User provided an image URL | `media_workflow_plan` with `image_url` (`img2video_direct`) |
| Storyboard / multi-shot narrative | `media_workflow_plan` → `storyboard_multi` template |
| Seedance multimodal (first/last frame, ref video/audio) | `video_generate` or workflow with `last_frame_url`, `reference_*` |

## Async workflows (default)

Workflows run **in the background** by default (`media.workflows.async_execution=true`).

1. `media_workflow_run` returns `{ "run_id", "status": "running", "async": true }`
2. Poll `media_workflow_status` with `run_id` until `succeeded` or `failed`
3. Set `wait: true` on `media_workflow_run` to block until complete (CLI / debugging)

Each run writes `manifest.json` under the workflow run directory for provenance and artifacts.

## Prompt writing — images

Write **rich visual detail**, not one-line summaries:

1. **Subject** — what/who, pose, expression, key props
2. **Style / medium** — photo, illustration, 3D, watercolor, etc.
3. **Composition** — framing, foreground/background, 16:9 vs 9:16
4. **Lighting** — time of day, direction, mood
5. **Materials & textures** — fabric, metal, skin, weathered wood, etc.

When `llm_prompt_refine` is enabled, workflows call the server LLM to expand prompts; on failure they fall back to local templates.

Example (EN): *"A ceramic teapot on a walnut table, morning window light from the left, soft shadows, steam rising, shallow depth of field, product photography, ultra-detailed glaze reflections."*

Example (ZH): *"白瓷茶壶置于胡桃木桌面，左侧晨光，柔和阴影，热气袅袅，浅景深，产品摄影，釉面反光细节丰富。"*

## Prompt writing — videos

Separate **scene** from **motion**:

- **Scene**: detailed visuals (same richness as image prompts)
- **Motion**: camera move (dolly in, pan, orbit) + subject movement
- **Image-to-video**: describe motion/changes only; do not repeat the reference image look

Default aspect: **9:16** for mobile/WeCom short video; **16:9** for desktop.

Optional **negative_prompt**: blurry, watermark, subtitles, jitter, distorted, morphing.

### Seedance multimodal

For `video_generate` / video workflow steps, optional parameters:

| Parameter | Role |
|-----------|------|
| `image_url` | First frame (`first_frame`) |
| `last_frame_url` | Last frame (`last_frame`) |
| `reference_image_urls` | Style/character reference (`reference_image`) |
| `reference_video_url` | Motion/style reference video |
| `reference_audio_url` | Audio reference |
| `generate_audio` | Request model-generated audio when supported |

Model ids must come from `hermes media models` (catalog `id`), not guessed upstream names.

## Storyboard (`storyboard_multi`)

For 分镜 / storyboard / 叙事 requests:

- Template: `storyboard_multi` (default when intent matches)
- LLM plans up to `storyboard_max_shots` shots (default 3)
- Each shot: `scene_prompt` → keyframe image → `motion_prompt` → video clip

## Credits

When `check_credits` is enabled, generation pre-checks Flowy balance:

- Images: minimum `image_min_credits` (default 500)
- Video: `duration × video_credits_per_second` (default 1000/s)

If insufficient, tools fail fast with a clear balance message.

## Delivery

- Always use `MEDIA:/local_path` from tool `media_hint` or `assets[].local_path` when present
- Unified workflow responses include `assets[]` and `provenance` (template, steps, model ids)
- If only a remote URL, share the link and note download may be needed
- Never redirect users to Kling, Sora, Pika, 海螺 when Flowy tools are available

## Configuration

```bash
hermes media models pick image
hermes media models pick video
hermes media config set video_resolution 720p
hermes media config set async_execution true
hermes media config set llm_prompt_refine true
hermes media config set check_credits true
hermes media config set storyboard_max_shots 3
```

Workflow templates: `txt2img`, `prompt_refine_txt2video`, `img2video_direct`, `storyboard_multi`.
