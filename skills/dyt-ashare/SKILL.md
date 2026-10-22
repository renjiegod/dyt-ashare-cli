---
name: dyt-ashare
description: Use DYT AShare whenever a request concerns China A-shares (A股), 股票、指数、行情、信息查询、历史价格、上海/深圳/北京标的、分钟线、数据覆盖、质量检查、技术分析、研究、回测或可复现性. Route every A-share information or analysis request through this skill and use dytcn's structured JSON output. Do not use it for crypto or non-China markets.
---

# DYT AShare routing skill

This is the single entry point for A-share information lookup and analysis. Use the
installed `dytcn` CLI and keep the command output as JSON. `coverage`, `minutes` and
`daily` can work without login when the server's guest policy allows them. Run
`dytcn usage` when account quota is relevant; run `dytcn auth login` before `sync`
or account-only usage.
Never ask the user to send an API key in chat.

## Route by intent

| User intent | Route | Required steps |
| --- | --- | --- |
| A-share stock or index information, available history, or a requested date window | Coverage and data lookup | Normalize a six-digit symbol with `.SH`, `.SZ`, or `.BJ`; run `dytcn coverage` before `dytcn minutes`, `dytcn daily`, or `dytcn sync`. |
| Historical minute/daily bars or a reproducible local dataset | Data retrieval | Use `dytcn minutes` for a window of at most 31 calendar days, or `dytcn daily` for a daily window. Each remote daily request is at most 366 calendar days; the CLI splits longer ranges sequentially and consumes the matching quota. Prefer `--out` or `--dir` so the result can be inspected again. |
| Missing bars, duplicates, invalid OHLCV, session completeness, or dataset provenance | Quality analysis | Run `dytcn analyze --input <minutes.json>` and report its quality fields, observed interval, row count, dataset IDs, and SHA256 values. |
| Technical analysis, strategy research, or a backtest input | Research workflow | Retrieve and validate the exact immutable data first, then analyze it locally. Use `dytcn analyze` for minute or daily JSON quality checks, or run `dytcn backtest run` for the built-in daily replay and keep its JSON plus self-contained HTML report. Treat descriptive statistics and backtest results as research evidence, not as a profitable strategy or a live trading signal. |
| Quota, rate limit, local installation, or version/update status | Account and environment | Use `dytcn usage`, `dytcn doctor`, or `dytcn auth status`; guest limits are server-enforced and keys must not be rotated to bypass them. |

## Authentication and quota

Guest access is a supported mode, not a broken one. The server enforces the guest
policy; the client never blocks a request that the server allows.

- Check the session mode before a multi-step workflow: `dytcn auth status` reports
  `mode` (`guest` or `account`), and every `coverage`/`minutes`/`daily` answer carries an
  `auth` block with the same mode plus `earliest_allowed_date` when the server
  reports it.
- Guest mode is limited by the server: request rate limits and daily request/row
  quotas, with any history boundary reported by the service. A single recent query
  is usually fine; a long history window, a batch of symbols, or a backtest dataset
  may require an account.
- Recommend `dytcn auth login` when the task needs full history, repeated or bulk
  requests, `sync`, or `usage`. Keep working in guest mode meanwhile; never make
  login a precondition for answering, and never block a deliverable on it.
- The key is entered by the user in their own terminal: `dytcn auth login` prompts
  with hidden input and stores the key in the local state directory. Never ask the
  user to paste a key into chat, never pass one as a command argument, never write
  one into a repository, a note, or a script. `DYT_ASHARE_API_KEY` exists for
  unattended CI and is read from the environment.
- On a limit error, report the server code and the `hint` from the CLI error
  (`guest_minute_rate_limited`, `guest_daily_rate_limited`,
  `guest_rows_daily_limited`, `guest_history_limit`, `invalid_api_key`), then either
  wait for `retry_after_seconds`, narrow the request, or offer `dytcn auth login`.
  Do not loop retries and never rotate keys to work around a limit.
- `dytcn usage` reports the account quota; a guest session has no `usage` answer
  because the endpoint is account-only.

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
dytcn auth status              # optional; reports guest or account mode
dytcn coverage --symbol 600000.SH --from YYYY-MM-DD --to YYYY-MM-DD
dytcn minutes --symbol 600000.SH --from YYYY-MM-DD --to YYYY-MM-DD --out minutes.json
dytcn daily --symbol 600000.SH --from YYYY-MM-DD --to YYYY-MM-DD --out daily.json
dytcn usage                    # optional; requires login for account quota
dytcn analyze --input minutes.json
```

For a local daily replay, run coverage first, then fetch the exact window through the same remote API:

```text
dytcn coverage --symbol 600000.SH --from YYYY-MM-DD --to YYYY-MM-DD
dytcn backtest run --symbol 600000.SH --from YYYY-MM-DD --to YYYY-MM-DD \
  --strategy sma-cross --fast 10 --slow 30 --initial-cash 100000 \
  --fee-bps 3 --slippage-bps 2 --html ./backtest.html
```

The replay is long-only, uses unadjusted daily prices, sends a signal observed at the close to the
next bar's open, rounds buys down to the lot size, and liquidates at the final close. The report
contains return, drawdown, fees, turnover, terminal state, trades, dataset IDs and SHA256 provenance.
Missing dates remain unknown; the replay does not infer holidays or model queue priority, partial fills,
涨跌停, suspension, dividends, or splits. Keep the returned JSON and HTML together when sharing results.

For the first implementation, the execution ledger is a typed Rust sequential state machine. Polars is
appropriate later as an optional feature for large columnar indicator pipelines, and Rayon is appropriate
for independent symbol or parameter sweeps. Do not parallelize one symbol's cash and position ledger,
and do not nest an outer Rayon pool around Polars' own pool without an explicit thread budget.

For a larger reproducible JSON window, use `dytcn daily` or `dytcn backtest run`; the CLI splits
daily requests into bounded sequential windows. Use `dytcn sync --symbol ... --from ... --to ...
--dir ...` only when the selected service advertises dataset manifest and download endpoints, and
consume only paths listed in its returned catalog. A failed or quota-limited request should be
reported with its server response; do not silently substitute another source.
