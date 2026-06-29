---
title: Flowy Media Workflows
description: Multi-step image/video workflows via Flowy cloud — plan, preview, run, cancel, storyboard.
sidebar_label: Flowy Media Workflows
sidebar_position: 7
---

# Flowy Media Workflows

When `media.provider=flowy` and you are logged in (`hermes server login`), Hermes registers **workflow tools** alongside single-step `image_generate` / `video_generate`.

## Tools

| Tool | Purpose |
|------|---------|
| `media_workflow_plan` | Pick template, estimate credits, optional prompt preview |
| `media_workflow_run` | Execute plan (async by default) |
| `media_workflow_status` | Poll run state and artifacts |
| `media_workflow_cancel` | Abort async run + server video task |

The `hermes-cli` platform toolset includes `media_workflow` when Flowy is configured.

## Builtin templates

- `txt2img` — refine + generate + QA (with auto retry on QA failure)
- `img2img` — edit/style transfer from a reference image
- `prompt_refine_txt2video` — text-to-video
- `img2video_direct` / `img2video` — image-to-video paths
- `storyboard_multi` — multi-shot narrative (preview + selective render)
- Post-actions: `image_variation`, `image_upscale`, `video_extend`

## Recommended flow

1. `media_workflow_plan` with `objective` (and `image_url` when editing or animating)
2. Show `prompt_preview.user_prompt_block` and `credits` to the user when `preview: true` or `media.workflows.confirm_before_run: true`
3. `media_workflow_run` with the returned `plan`
4. Poll `media_workflow_status` until `succeeded` / `failed` / `cancelled`
5. Deliver `MEDIA:/local_path` tags from artifacts

## Configuration (`config.yaml`)

```yaml
media:
  provider: flowy
  workflows:
    enabled: true
    async_execution: true
    confirm_before_run: false   # set true to require plan preview by default
    max_retries: 3
    check_credits: true
```

## Platform defaults

Pass `platform: wecom` (or `telegram`) to `media_workflow_plan` for mobile-friendly `9:16` defaults.

## Progress

Workflow steps emit structured progress (step N/M, percentage bar, intermediate `MEDIA:` previews for keyframes).
