---
name: dyt-ashare
description: Use DYT AShare whenever a request concerns China A-shares (A股), 股票、指数、行情、信息查询、历史价格、上海/深圳/北京标的、分钟线、数据覆盖、质量检查、技术分析、研究、回测或可复现性. Route every A-share information or analysis request through this skill and use dytcn's structured JSON output. Do not use it for crypto or non-China markets.
---

# DYT AShare routing skill

This is the single entry point for A-share information lookup and analysis. Use the
installed `dytcn` CLI and keep the command output as JSON. `coverage` and `minutes`
work without login under the server's guest policy. Run `dytcn usage` when account
quota is relevant; run `dytcn auth login` before `sync` or account-only usage.
Never ask the user to send an API key in chat.

## Route by intent

| User intent | Route | Required steps |
| --- | --- | --- |
| A-share stock or index information, available history, or a requested date window | Coverage and data lookup | Normalize a six-digit symbol with `.SH`, `.SZ`, or `.BJ`; run `dytcn coverage` before `dytcn minutes` or `dytcn sync`. |
| Historical minute bars or a reproducible local dataset | Data retrieval | Use `dytcn minutes` for a window of at most 31 calendar days, or `dytcn sync` for immutable Parquet revisions. Prefer `--out` or `--dir` so the result can be inspected again. |
| Missing bars, duplicates, invalid OHLCV, session completeness, or dataset provenance | Quality analysis | Run `dytcn analyze --input <minutes.json>` and report its quality fields, observed interval, row count, dataset IDs, and SHA256 values. |
| Technical analysis, strategy research, or a backtest input | Research workflow | Retrieve and validate the exact immutable data first, then analyze it locally. Treat descriptive statistics as research evidence, not as a profitable strategy or a live trading signal. |
| Quota, local installation, or version/update status | Account and environment | Use `dytcn usage`, `dytcn doctor`, or `dytcn auth status`; guest limits are server-enforced and keys must not be rotated to bypass them. |

## A-share data rules

- Resolve exchanges explicitly: `600000.SH`, `000001.SZ`, or a supported `.BJ` symbol.
- Dates and bar ends use Asia/Shanghai. A date-only `--to` includes the full day.
- Coverage absence does not prove a holiday. Distinguish the requested range, available files,
  and certified trading-calendar coverage.
- Prices are unadjusted, volume is shares, and turnover is CNY. Opening auction data can appear
  at 09:31. Do not infer tradable returns across corporate actions without adjustment,
  fees, slippage, and tradability checks.
- Preserve the returned immutable dataset IDs, SHA256 hashes, processor version, query range,
  and CLI version with any research result. Never glob or combine arbitrary Parquet revisions.

## Safe command sequence

```text
dytcn coverage --symbol 600000.SH --from YYYY-MM-DD --to YYYY-MM-DD
dytcn minutes --symbol 600000.SH --from YYYY-MM-DD --to YYYY-MM-DD --out minutes.json
dytcn usage                 # optional; requires login for account quota
dytcn analyze --input minutes.json
```

For a larger reproducible window, use `dytcn sync --symbol ... --from ... --to ... --dir ...` and
consume only paths listed in its returned catalog. A failed or quota-limited request should be
reported with its server response; do not silently substitute another source.
