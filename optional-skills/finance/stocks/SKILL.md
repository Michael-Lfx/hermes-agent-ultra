---
name: stocks
description: Stock quotes, history, search, compare, crypto via Yahoo.
version: 0.2.0
author: Mibay (Mibayy), Hermes Agent
license: MIT
platforms: [linux, macos, windows]
metadata:
  hermes:
    tags: [Stocks, Finance, Market, Crypto, Investing]
    category: finance
    related_skills: [trading-research, trading-debate, dcf-model, comps-analysis, lbo-model]
    requires_toolsets: [terminal]
---

# Stocks Skill

Read-only market data via Yahoo Finance. Five commands: `quote`, `search`,
`history`, `compare`, `crypto`. Python stdlib only — no API key, no pip
installs. Yahoo's endpoint is unofficial and may rate-limit or change.

**Install:** This skill lives in `optional-skills/`. Use `skills_install` (or copy
to `~/.hermes/skills/finance/stocks/`) before first use. Requires `terminal`
toolset and Python 3 on the host.

## When to Use

- User asks for a **current stock price** (AAPL, TSLA, MSFT, ...)
- User wants to **look up a ticker by company name** (`search`)
- User wants to **compare several tickers** side by side (`compare`)
- User asks for a **crypto spot price** via Yahoo (`crypto` — pass `BTC`, script appends `-USD`)
- User wants a **quick price history browse** (`history` — light Yahoo chart only; see routing table)

## When NOT to Use

- User wants **historical OHLCV for backtesting**, Sharpe, drawdown, or T+1 rules → use **`trading-research`** (`get_market_data` / `run_backtest`)
- User wants **A-share research with Eastmoney live data** → **`trading-research`**
- User wants **HK/US/A-share/crypto unified backtest pipeline** → **`trading-research`**
- User wants **investment-committee bull/bear debate** after a backtest → **`trading-debate`**
- User wants **news, fundamentals, or research reports** → `web_search`

## Skill routing (stocks vs trading-research)

| User intent | Skill | Tool / command |
|-------------|-------|----------------|
| 「苹果现在多少钱」 | **stocks** | `terminal` → `stocks_client.py quote AAPL` |
| 「特斯拉代码是什么」 | **stocks** | `search "Tesla"` |
| 「比一下 AAPL MSFT GOOGL」 | **stocks** | `compare` |
| 「拉 000001.SZ 180 天日 K 并回测 RSI」 | **trading-research** | `run_backtest` |
| 「0700.HK / AAPL 历史回测」 | **trading-research** | `get_market_data` + `run_backtest` |
| 「BTC-USDT 最近 30 天 K 线」 | **trading-research** | `get_market_data` |

If this skill is **not installed**, suggest `skills_install` for quick quotes, or use
`web_search` / `trading-research` as fallback depending on intent.

## Prerequisites

Python 3.8+ stdlib only. Optional: set `ALPHA_VANTAGE_KEY` to enrich
`market_cap`, `pe_ratio`, and 52-week levels when Yahoo's crumb-protected
fields come back null. Free key: https://www.alphavantage.co/support/#api-key

## How to Run

Invoke through the `terminal` tool. Once installed:

```
SCRIPT=~/.hermes/skills/finance/stocks/scripts/stocks_client.py
python3 $SCRIPT quote AAPL
```

All output is JSON on stdout — pipe through `jq` if you want to slice it.

## Quick Reference

```
python3 $SCRIPT quote AAPL
python3 $SCRIPT quote AAPL MSFT GOOGL TSLA
python3 $SCRIPT search "Tesla"
python3 $SCRIPT history NVDA --range 6mo
python3 $SCRIPT compare AAPL MSFT GOOGL
python3 $SCRIPT crypto BTC ETH SOL
```

## Commands

### `quote SYMBOL [SYMBOL2 ...]`

Current price, change, change%, volume, 52-week high/low.

### `search QUERY`

Find tickers by company name. Returns top 5: symbol, name, exchange, type.

### `history SYMBOL [--range RANGE]`

Daily OHLCV plus stats (min, max, avg, total return %). Ranges: `1mo`,
`3mo`, `6mo`, `1y`, `5y`. Default: `1mo`.

**Note:** For backtests or multi-market research, use **`trading-research`** instead.

### `compare SYMBOL1 SYMBOL2 [...]`

Side-by-side: price, change%, 52-week performance.

### `crypto SYMBOL [SYMBOL2 ...]`

Crypto prices. Pass `BTC` (the script appends `-USD` automatically).

For crypto **backtests**, use **`trading-research`** with `BTC-USDT`.

## Pitfalls

- Yahoo Finance's API is unofficial. Endpoints can change or rate-limit
  without notice — if requests start failing, that's why.
- `market_cap` and `pe_ratio` may return null on `quote` when Yahoo's
  crumb session isn't established. Set `ALPHA_VANTAGE_KEY` to backfill.
- Add a small delay between bulk requests to avoid rate-limiting.
- This is read-only — no order placement, no account integration.

## Verification

```
python3 ~/.hermes/skills/finance/stocks/scripts/stocks_client.py quote AAPL
```

Returns a JSON object with `symbol: "AAPL"` and a numeric `price` field.

Ask: "AAPL 现在多少钱"
Expected: Agent uses this skill (`quote AAPL`) or prompts to install if missing — **not** `run_backtest`.
