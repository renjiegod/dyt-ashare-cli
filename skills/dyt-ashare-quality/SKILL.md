---
name: dyt-ashare-quality
description: Verify DYT AShare minute-data integrity, immutable dataset revisions and research reproducibility.
---

Use `dytcn coverage`, `sync` and `analyze`; inspect their JSON and exit status.
`sync` verifies byte count, SHA256 and manifest SHA256 before publishing its catalog. A repeat
verifies local files and skips identical versions; revisions are retained. Consume only paths
listed in the returned catalog. Avoid combining revisions of the same stock/month.

For a JSON minute query run `analyze --input ...` to detect duplicate timestamps, invalid OHLCV,
non-session timestamps and incomplete observed sessions. A 240-bar expectation applies to an
observed full day of 1-minute data, not to every calendar date or a deliberately partial window.
Always distinguish requested range, available files and certified trading-calendar coverage.
Preserve dataset IDs, SHA256, processor version, original contract and query range with results.

Use `doctor` for installed CLI/Skills and update status. Modified Skills can remain pending after
a CLI update; report the actual pending list instead of claiming both versions updated.
Do not replace modified Skills without the user's explicit request. Credentials stay local.
