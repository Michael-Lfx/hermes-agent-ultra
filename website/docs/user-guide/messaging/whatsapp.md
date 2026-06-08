---
sidebar_position: 5
title: "WhatsApp"
description: "Set up Hermes Agent as a WhatsApp bot via the built-in Rust client (wa-rs)"
---

# WhatsApp Setup

Hermes connects to WhatsApp through a **native Rust client** ([wa-rs](https://github.com/homunbot/wa-rs)) that speaks the WhatsApp Web protocol — **not** through the official WhatsApp Business API. No Meta developer account or Business verification is required. **Node.js is not required** for WhatsApp; pairing and messaging run inside the Hermes gateway process.

:::info Official Cloud API
For Meta's official HTTP API (no QR pairing, Business verification required), use `hermes whatsapp cloud` and enable the `whatsapp-cloud` gateway feature instead of the default Web client.
:::

:::warning Unofficial API — Ban Risk
WhatsApp does **not** officially support third-party bots outside the Business API. Using a third-party client carries a small risk of account restrictions. To minimize risk:
- **Use a dedicated phone number** for the bot (not your personal number)
- **Don't send bulk/spam messages** — keep usage conversational
- **Don't automate outbound messaging** to people who haven't messaged first
:::

:::warning WhatsApp Web Protocol Updates
WhatsApp periodically updates their Web protocol, which can temporarily break compatibility
with third-party clients. When this happens, Hermes will update the wa-rs dependency. If the
bot stops working after a WhatsApp update, pull the latest Hermes version and re-pair.
:::

## Two Modes

| Mode | How it works | Best for |
|------|-------------|----------|
| **Separate bot number** (recommended) | Dedicate a phone number to the bot. People message that number directly. | Clean UX, multiple users, lower ban risk |
| **Personal self-chat** | Use your own WhatsApp. You message yourself to talk to the agent. | Quick setup, single user, testing |

---

## Prerequisites

- **A phone with WhatsApp** installed (for scanning the QR code)
- **Hermes gateway** built with the `whatsapp` feature (included in default CLI builds)

No Chromium, Puppeteer, npm, or Node.js is needed for the WhatsApp Web client.

---

## Step 1: Run the Setup Wizard

```bash
hermes whatsapp
```

The wizard will:

1. Ask which mode you want (**bot** or **self-chat**)
2. Start the Rust pairing client and print a **QR code** in your terminal (via gateway logs / stdout)
3. Wait for you to scan it

**To scan the QR code:**

1. Open WhatsApp on your phone
2. Go to **Settings → Linked Devices**
3. Tap **Link a Device**
4. Point your camera at the terminal QR code

Once paired, the wizard confirms the connection and exits. Your session is saved automatically.

:::tip Migrating from an older Hermes install
If you previously used the removed Node/Baileys bridge (`scripts/whatsapp-bridge/`), your old
`creds.json` is **not** reused. The wizard detects legacy sessions and asks you to re-pair.
The new session lives under `~/.hermes/whatsapp/session/` (SQLite + a `.paired` marker).
:::

:::tip
If the QR code looks garbled, make sure your terminal is at least 60 columns wide and supports
Unicode. You can also try a different terminal emulator.
:::

Check status anytime:

```bash
hermes whatsapp status
```

---

## Step 2: Getting a Second Phone Number (Bot Mode)

For bot mode, you need a phone number that isn't already registered with WhatsApp. Three options:

| Option | Cost | Notes |
|--------|------|-------|
| **Google Voice** | Free | US only. Get a number at [voice.google.com](https://voice.google.com). Verify WhatsApp via SMS through the Google Voice app. |
| **Prepaid SIM** | $5–15 one-time | Any carrier. Activate, verify WhatsApp, then the SIM can sit in a drawer. Number must stay active (make a call every 90 days). |
| **VoIP services** | Free–$5/month | TextNow, TextFree, or similar. Some VoIP numbers are blocked by WhatsApp — try a few if the first doesn't work. |

After getting the number:

1. Install WhatsApp on a phone (or use WhatsApp Business app with dual-SIM)
2. Register the new number with WhatsApp
3. Run `hermes whatsapp` and scan the QR code from that WhatsApp account

---

## Step 3: Configure Hermes

Add the following to your `~/.hermes/.env` file:

```bash
# Required
WHATSAPP_ENABLED=true
WHATSAPP_MODE=bot                          # "bot" or "self-chat"

# Access control — pick ONE of these options:
WHATSAPP_ALLOWED_USERS=15551234567         # Comma-separated phone numbers (with country code, no +)
# WHATSAPP_ALLOWED_USERS=*                 # OR use * to allow everyone
# WHATSAPP_ALLOW_ALL_USERS=true            # OR set this flag instead (same effect as *)
```

:::tip Allow-all shorthand
Setting `WHATSAPP_ALLOWED_USERS=*` allows **all** senders (equivalent to `WHATSAPP_ALLOW_ALL_USERS=true`).
This is consistent with [Signal group allowlists](/docs/reference/environment-variables).
To use the pairing flow instead, remove both variables and rely on the
[DM pairing system](/docs/user-guide/security#dm-pairing-system).
:::

Optional behavior settings in `~/.hermes/config.yaml`:

```yaml
unauthorized_dm_behavior: pair

whatsapp:
  unauthorized_dm_behavior: ignore
```

- `unauthorized_dm_behavior: pair` is the global default. Unknown DM senders get a pairing code.
- `whatsapp.unauthorized_dm_behavior: ignore` makes WhatsApp stay silent for unauthorized DMs, which is usually the better choice for a private number.

Then start the gateway:

```bash
hermes gateway              # Foreground
hermes gateway install      # Install as a user service
sudo hermes gateway install --system   # Linux only: boot-time system service
```

The gateway connects to WhatsApp in-process using the saved session — there is no separate bridge subprocess or HTTP port.

---

## Session Persistence

The Rust client stores its session under `~/.hermes/whatsapp/session/`:

| File | Purpose |
|------|---------|
| `whatsapp.db` | SQLite store (encryption keys, device state) |
| `.paired` | Marker written after successful QR pairing |

This means:

- **Sessions survive restarts** — you don't need to re-scan the QR code every time
- The session data includes encryption keys and device credentials
- **Do not share or commit this directory** — it grants full access to the WhatsApp account

Optional override in `~/.hermes/config.yaml`:

```yaml
platforms:
  whatsapp:
    extra:
      session_path: /path/to/custom/session
```

---

## Re-pairing

If the session breaks (phone reset, WhatsApp update, manually unlinked), you'll see connection
errors in the gateway logs. To fix it:

```bash
hermes whatsapp
```

This generates a fresh QR code. Scan it again and the session is re-established. The client
handles **temporary** disconnections (network blips, phone going offline briefly) with automatic
reconnection when possible.

---

## Voice Messages

Hermes supports voice on WhatsApp:

- **Incoming:** Voice messages (`.ogg` opus) are automatically transcribed using the configured STT provider: local `faster-whisper`, Groq Whisper (`GROQ_API_KEY`), or OpenAI Whisper (`VOICE_TOOLS_OPENAI_KEY`)
- **Outgoing:** TTS responses are sent as MP3 audio file attachments
- Agent responses are prefixed with "⚕ **Hermes Agent**" by default. You can customize or disable this in `config.yaml`:

```yaml
# ~/.hermes/config.yaml
whatsapp:
  reply_prefix: ""                          # Empty string disables the header
  # reply_prefix: "🤖 *My Bot*\n──────\n"  # Custom prefix (supports \n for newlines)
```

---

## Message Formatting & Delivery

WhatsApp supports **streaming (progressive) responses** — the bot edits its message in real-time as the AI generates text, just like Discord and Telegram. Internally, WhatsApp is classified as a TIER_MEDIUM platform for delivery capabilities.

### Chunking

Long responses are automatically split into multiple messages at **4,096 characters** per chunk (WhatsApp's practical display limit). You don't need to configure anything — the gateway handles splitting and sends chunks sequentially.

### WhatsApp-Compatible Markdown

Standard Markdown in AI responses is automatically converted to WhatsApp's native formatting:

| Markdown | WhatsApp | Renders as |
|----------|----------|------------|
| `**bold**` | `*bold*` | **bold** |
| `~~strikethrough~~` | `~strikethrough~` | ~~strikethrough~~ |
| `# Heading` | `*Heading*` | Bold text (no native headings) |
| `[link text](url)` | `link text (url)` | Inline URL |

Code blocks and inline code are preserved as-is since WhatsApp supports triple-backtick formatting natively.

### Tool Progress

When the agent calls tools (web search, file operations, etc.), WhatsApp displays real-time progress indicators showing which tool is running. This is enabled by default — no configuration needed.

---

## Troubleshooting

| Problem | Solution |
|---------|----------|
| **QR code not scanning** | Ensure terminal is wide enough (60+ columns). Try a different terminal. Make sure you're scanning from the correct WhatsApp account (bot number, not personal). |
| **QR code expires** | QR codes refresh periodically. If pairing times out, restart `hermes whatsapp`. |
| **Session not persisting** | Check that `~/.hermes/whatsapp/session/` exists, contains `whatsapp.db` and `.paired`, and is writable. If containerized, mount `~/.hermes` as a persistent volume. |
| **Legacy Baileys session** | Delete or ignore old `creds.json` under the session dir and run `hermes whatsapp` to re-pair with the Rust client. |
| **Logged out unexpectedly** | WhatsApp unlinks devices after long inactivity. Keep the phone on and connected to the network, then re-pair with `hermes whatsapp` if needed. |
| **Reconnect loops / disconnects** | Restart the gateway, update Hermes, and re-pair if WhatsApp invalidated the session or after a protocol update. |
| **Bot stops working after WhatsApp update** | Update Hermes to get the latest wa-rs version, then re-pair. |
| **Messages not being received** | Verify `WHATSAPP_ALLOWED_USERS` includes the sender's number (with country code, no `+` or spaces), or set it to `*` to allow everyone. Check `~/.hermes/logs/gateway.log` for connection or policy drops. |
| **Bot replies to strangers with a pairing code** | Set `whatsapp.unauthorized_dm_behavior: ignore` in `~/.hermes/config.yaml` if you want unauthorized DMs to be silently ignored instead. |

---

## Security

:::warning
**Configure access control** before going live. Set `WHATSAPP_ALLOWED_USERS` with specific
phone numbers (including country code, without the `+`), use `*` to allow everyone, or set
`WHATSAPP_ALLOW_ALL_USERS=true`. Without any of these, the gateway **denies all incoming
messages** as a safety measure.
:::

By default, unauthorized DMs still receive a pairing code reply. If you want a private WhatsApp number to stay completely silent to strangers, set:

```yaml
whatsapp:
  unauthorized_dm_behavior: ignore
```

- The `~/.hermes/whatsapp/session/` directory contains full session credentials — protect it like a password
- Set file permissions: `chmod 700 ~/.hermes/whatsapp/session`
- Use a **dedicated phone number** for the bot to isolate risk from your personal account
- If you suspect compromise, unlink the device from WhatsApp → Settings → Linked Devices
- Phone numbers in logs are partially redacted, but review your log retention policy
