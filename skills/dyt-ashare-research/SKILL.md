---
name: dyt-ashare-research
description: Use DYT AShare to fetch and analyze historical China A-share minute bars for research and backtests.
---

Use the installed `dyt` CLI. Run `dyt --help` and relevant subcommand help for current flags.
Read `dyt usage` before planning a large query. If authentication is missing, ask the user to
run `dyt auth login` locally; never ask them to send a key in chat.

Resolve the exchange explicitly: `600000.SH`, `000001.SZ`, or a supported `.BJ` symbol.
Use `coverage --symbol ... --from YYYY-MM-DD --to YYYY-MM-DD` before choosing a window.
Catalog absence does not establish a holiday. Dates and bar ends use Asia/Shanghai.
For a bounded query use `minutes`; prefer `--out` for files. For larger datasets use `sync`.
Use the returned catalog's exact immutable IDs; never glob every historical Parquet revision.

`analyze --input <minutes.json>` computes descriptive statistics locally without quota usage.
Report the dataset IDs/hashes, requested interval, observed rows, gaps and CLI version.
Prices are unadjusted, volume is shares and turnover is CNY. Opening auction can be included in
09:31. No automatic forward-adjustment, missing-bar fill or certified calendar is supplied.
Do not infer tradable returns from raw price changes across corporate actions. A descriptive
summary is not a backtest: apply fees, slippage, tradability and out-of-sample checks separately.
If quota is exhausted, explain the server response; do not rotate keys to bypass limits.
