# Upstream Missing Patch Queue

Generated: `2026-05-12T07:51:36.363760+00:00`

- Range: `main..upstream/main`; total commits tracked: `2536`.

| Ticket | Label | Commit Count |
| ---: | --- | ---: |
| #20 | GPAR-01 tests+CI parity | 1018 |
| #21 | GPAR-02 skills parity | 82 |
| #22 | GPAR-03 UX parity | 485 |
| #23 | GPAR-04 gateway/plugin-memory parity | 206 |
| #24 | GPAR-05 environments+parsers+benchmarks | 11 |
| #25 | GPAR-06 packaging/docs/install parity | 37 |
| #26 | GPAR-07 upstream queue backfill | 697 |

| Disposition | Commit Count |
| --- | ---: |
| mirrored | 64 |
| pending | 612 |
| ported | 60 |
| superseded | 1800 |

## First 100 Pending Commits

| SHA | Ticket | Subject |
| --- | ---: | --- |
| `42627b4eafbe` | #22 | refactor(tui): bundle with esbuild, drop runtime node_modules |
| `279504d5b887` | #26 | fix(nix): refresh npm lockfile hashes |
| `42e166c7ea2e` | #20 | refactor(docker): drop manual @hermes/ink build, rely on esbuild bundle |
| `42df7ec597e0` | #22 | fix(tui): update comments |
| `242659f5af31` | #26 | fix(tui): don't hardcode /home/bb |
| `9d645d98c445` | #22 | fix(tui): update README |
| `05bec0ac79ad` | #26 | fix: pluralization |
| `b162f9ef9a92` | #26 | fix(nix): refresh hermes-tui npmDepsHash for ui-tui lockfile |
| `401aadb5b892` | #26 | docs(security): rewrite policy around OS-level isolation as the boundary |
| `0d1cbc2dda28` | #26 | changes from feedback |
| `aa88dcc57b17` | #26 | fix: salvage batch — compaction guidance, memory authority, cache eviction after compression |
| `395dbcc873c8` | #20 | feat(browser): add Lightpanda engine support with automatic Chrome fallback |
| `3ebdd26449dc` | #20 | fix(browser): surface Lightpanda Chrome fallback warnings |
| `d78c34928fe9` | #22 | feat(tui): collapsible sections in startup banner (skills, system prompt, MCP) |
| `629d8b843d8d` | #20 | fix(browser): tighten Lightpanda fallback edge cases |
| `466f3a11de47` | #20 | fix(gateway): preserve model picker current context |
| `a6f5f9c484ae` | #26 | fix(update): drop pip --quiet so slow installs don't look hung (#20679) |
| `e70e49016fe2` | #20 | fix(cli): guard logger.debug in signal handler (#13710 regression) (#20673) |
| `043a118d4128` | #25 | fix: harden install.sh against inherited Python env leakage |
| `a869a523eec4` | #26 | chore: AUTHOR_MAP entry for adybag14-cyber |
| `e45df2e81ec8` | #22 | fix(ui): reduce status-line jitter while scrolling |
| `b1e0ef82f6a7` | #26 | chore(release): map liuguangyong@hellobike -> liuguangyong93 |
| `17687911b7c5` | #26 | fix(kanban): reset code element background inside board |
| `76074d9ee6e4` | #20 | fix(cli): recover classic CLI output after resize |
| `a0fedfbb1b7e` | #22 | feat(checkpoints): v2 single-store rewrite with real pruning + disk guardrails (#20709) |
| `906881c38bdd` | #20 | fix(cli): catch OSError in _resolve_attachment_path to prevent ENAMETOOLONG dropping long slash commands |
| `3ce1233ae49a` | #26 | chore(release): map cleo@edaphic.xyz → curiouscleo |
| `b62a82e0c3fb` | #22 | docs: pluggable surfaces coverage — model-provider guide, full plugin map, opt-in fix (#20749) |
| `63c51d89628a` | #26 | change: enable ruff/ty |
| `9627ee70e57a` | #25 | feat(ci): add typecheck (warnings only in CI) |
| `ad7aad251c60` | #21 | feat(skills/linear): add Documents support + Python helper script (#20752) |
| `773cf48c50b4` | #22 | docs(plugins): close the gaps \u2014 image-gen-provider-plugin guide + publishing a skill tap (#20800) |
| `09a491464c5f` | #22 | feat(tui): add /sessions slash command for browsing and resuming previous sessions |
| `a24789d738b1` | #20 | fix(opencode-go): keep users on opencode-go instead of hijacking to native providers (#20802) |
| `6388aafbd6cb` | #22 | feat(dashboard): add 'default-large' built-in theme with 18px base size (#20820) |
| `cd2cbc73b7c5` | #20 | refactor(web): per-capability backend selection for search/extract split |
| `5c906d70266c` | #20 | feat(web): add SearXNG as a native search-only backend |
| `94016dd1aa7e` | #22 | docs+skill: add searxng-search optional skill and documentation |
| `48c241840aa2` | #22 | docs: add Web Search + Extract feature page with SearXNG setup guide |
| `441ef75d157d` | #20 | fix(feishu): keep topic replies in threads |
| `28299afc21a3` | #23 | chore: follow-up cleanup for Feishu topic thread fix |
| `b1d420e75f42` | #20 | fix(kanban): avoid fragile failure-column renames |
| `a2ff193050b8` | #20 | chore: follow-up cleanup for Kanban migration fix |
| `f4031df05dd4` | #25 | ci(docker): don't cancel overlapping builds, guard :latest |
| `33bf5f6292f4` | #20 | fix(auth): fall back to global-root auth.json for providers missing in profile |
| `b71f80e6ce2a` | #20 | feat(gateway): per-platform gateway_restart_notification flag |
| `7df611519927` | #20 | feat(gateway): also gate pre-restart "Gateway restarting" notification |
| `d8b85bfd1c9d` | #26 | chore: add guillaumemeyer to AUTHOR_MAP |
| `5044e1cbf135` | #20 | fix(cli): submit LF enter in thin PTYs (#20896) |
| `f1a8e99942e6` | #22 | fix(tui): honor skin highlight colors (#20895) |
| `04cf4788ccc0` | #22 | fix(tui): restore voice push-to-talk parity (#20897) |
| `3cdbf334d507` | #20 | fix(gateway): don't dead-end setup wizard when only system-scope unit is installed |
| `65c762b2e83e` | #20 | fix(tui): preserve session when switching personality |
| `d797755a1c17` | #20 | fix(gateway): wait for systemd restart readiness |
| `5a3cadf6ebcb` | #23 | fix(discord): narrow rate-limit catch and move sync state under gateway/ |
| `45cbf93899a9` | #22 | docs(kanban): fix orchestrator skill setup instructions (#20958) |
| `49c3c2e0d37c` | #22 | docs(kanban): fix worker skill setup instructions too (#20960) |
| `51f9953e69d3` | #20 | feat(profiles): --no-skills flag for empty profile creation (#20986) |
| `bd0c54d171ef` | #23 | fix: route Telegram image documents through photo handling |
| `e7eb07cec7ea` | #26 | chore: AUTHOR_MAP entry for mrcoferland |
| `a5c9c83b7861` | #22 | fix(web): force light color-scheme on docs iframe |
| `7d36e8346bbe` | #20 | fix(security): close TOCTOU window when saving MCP OAuth credentials |
| `11b9b146f111` | #20 | fix(image-routing): expose attached image paths in native multimodal text part |
| `7c0766e06ad8` | #26 | fix(gateway): translate inbound document host paths to container paths for Docker backend |
| `bf843adf05b8` | #20 | feat(gateway): opt-in cleanup of temporary progress bubbles (#21186) |
| `40b51c93a2d9` | #20 | fix(kanban): heartbeat tool extends claim TTL, not just last_heartbeat_at |
| `63bd690a5011` | #26 | chore(release): map stephen0110 noreply email |
| `06f24351c576` | #20 | fix(kanban): stop reclaimed workers before retry |
| `b49a3f847499` | #26 | fix(kanban): reap completed worker children in dispatch_once |
| `595e906698c1` | #26 | chore(release): map sonic-netizen noreply email |
| `411cfa26e31d` | #20 | fix: auto-block repeated kanban retries |
| `233bfd3621f1` | #26 | chore(release): map mwnickerson noreply email |
| `fad684b1f35b` | #20 | feat(gateway): auto-resume interrupted sessions after restart |
| `961a3535fa37` | #20 | fix(gateway): preserve resume marker on interrupted restart |
| `38b1c7dce558` | #20 | refactor(gateway): simplify auto-resume + extend to crash recovery |
| `a84e56d4c662` | #20 | fix(auth): sync shared Nous refresh tokens |
| `429e78589b63` | #26 | refactor(auth): dedupe file-lock helper; document Nous lock order |
| `8a96fa48c10d` | #23 | fix(gateway): avoid duplicated responses history |
| `73d637176240` | #26 | chore: add AUTHOR_MAP entries for thelumiereguy and counterposition |
| `6e8f1e09a995` | #23 | fix(gateway): use monotonic deadlines in QR onboarding flows |
| `2e00bcaaab09` | #26 | fix(oauth,gateway): monotonic deadlines for polling/timeout loops |
| `3a0d52d57992` | #23 | fix(weixin): replace all aiohttp ClientTimeout with asyncio.wait_for() |
| `d856f4535d33` | #26 | chore: AUTHOR_MAP entry for chenlinfeng@ruije / @noOne-list |
| `fb1ce793e6ad` | #20 | feat(security): enable secret redaction by default (#17691, #20785) (#21193) |
| `8b32a9d0f170` | #20 | feat: add Discord message deletion action |
| `991df4ef8140` | #26 | chore: AUTHOR_MAP entry for @likejudy |
| `042eb930e212` | #20 | fix(security): close TOCTOU window in hermes_cli/auth.py credential writers (#21194) |
| `69692039e916` | #26 | fix(delegate): correct ACP docs — Claude Code CLI has no --acp flag |
| `5bf12eb44aec` | #26 | fix: exclude hidden and archive dirs from _find_skill rglob |
| `176b93575af3` | #20 | fix(gateway): preserve thread routing from cached live session sources |
| `333598cb0e2e` | #20 | fix(gateway): cap cached session sources with LRU eviction |
| `4f364c4e99d4` | #20 | fix(mcp): give 'mcp add --command' a distinct argparse dest |
| `f0dd5b9c10e2` | #26 | chore: add discodirector email to AUTHOR_MAP |
| `8d363f8d54ba` | #20 | fix(bedrock): preserve reasoningContent across converse normalization |
| `d34f03c32a28` | #23 | feat(gateway): support [[as_document]] directive for skill media routing |
| `4e27e4e05a87` | #26 | chore: AUTHOR_MAP entry for @leon7609 |
| `abe5a3c93750` | #20 | fix(model_switch): live model discovery for custom_providers in /model picker |
| `aa9a2091f649` | #26 | chore(release): add AUTHOR_MAP entries for ggnnggez and ehz0ah |
| `699c770e5c06` | #25 | docs(readme): drop misleading RL install-extras claim, defer to CONTRIBUTING |
| `fdb9e0f6a65e` | #20 | fix(kanban): auto-block workers that exit without completing (#20894) (#21214) |

