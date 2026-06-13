## hermes_team_roster
*Saved: 2026-06-01*

李智杨(rk3588+m50), 程泳珽(频道+POI资讯), 申长鑫(POI本地学习), 张燕杰(OTA), 杨赛尔/羊(产品需求), 赖锦锋(定时任务)
§
## rk3588_m50_market_position
*Saved: 2026-06-01*

rk3588+m50 board targets overseas users primarily; overseas channels first, domestic WeChat as backup. helicon search agent shelved pending Intel cooperation.
§
## hermes_3_user_scenarios
*Saved: 2026-06-01*

3 core user scenarios: 1) personal assistant, 2) WeChat quick notes (随手记), 3) LLM-organized knowledge base (大模型整理成体系的资料)
§
## user_work_style
*Saved: 2026-06-01*

User tracks work via todo lists with @mention assignments; sets cron reminders for follow-ups; currently porting agent browser and coordinating with 程泳珽 on user scenarios.
§
cronjob_schedule_must_use_utc: When user says Beijing time, convert to UTC (Beijing - 8h). Always state BOTH times in response. Skill: cronjob-scheduling
§
Killing an MCP server process (cua-driver, Chrome CDP) that hermes connected to at startup severs the named-pipe connection irreversibly. Restarting the MCP server does NOT restore the link — only a full hermes restart does. Never Stop-Process on cua-driver as a debugging step.
§
WeCom channel limitation: intelligent bots (智能机器人) currently cannot @mention specific people in group chats. Message push webhooks (消息推送webhook) CAN @mention people. Use webhook approach for notifications that need to target specific users.
§
User is CTO of a small team: 李智杨 (hardware/embedded, rk3588/m50), 程泳珽 (full-stack, channels/browser), 杨赛尔 (product side). Recurring unfixed bugs: cronjob-not-executing, agent-all-talk-no-action.
§
## clarify-tool-blocking-bug
*Saved: 2026-06-04*

Bug: `clarify` tool blocks agent loop in both TUI and gateway (WeCom) paths. Root causes: (1) `ChannelClarifyBackend.ask()` calls `wait_for()` which blocks the agent loop, preventing concurrent message processing; (2) TUI's `StdioClarifyBackend` uses `stdin.read_line()` but stdin is owned by crossterm TUI loop; (3) Gateway session lock prevents concurrent clarify respond. Fix: set `HERMES_CLARIFY_ASYNC=1` as default for gateway mode so `ask()` returns immediately with `clarify_pending` JSON instead of blocking.
