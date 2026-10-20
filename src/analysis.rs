use anyhow::{Context, Result, bail};
use chrono::{NaiveDateTime, Timelike};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
pub fn analyze(input: &[u8]) -> Result<Value> {
    let value: Value = serde_json::from_slice(input)?;
    let mut rows = value["items"]
        .as_array()
        .context("Expected a dyt-cn minutes JSON file with items")?
        .iter()
        .collect::<Vec<_>>();
    if rows.is_empty() {
        bail!("No bars to analyze");
    }
    rows.sort_by(|a, b| a["bar_end_time"].as_str().cmp(&b["bar_end_time"].as_str()));
    let mut times = HashSet::new();
    let mut days = BTreeMap::<String, usize>::new();
    let mut invalid = 0;
    let mut off_session = 0;
    let mut duplicates = 0;
    let mut peak = 0.0_f64;
    let mut drawdown = 0.0_f64;
    let mut volume = 0.0;
    let mut turnover = 0.0;
    let mut first = None;
    let mut last = None;
    for r in &rows {
        let time = r["bar_end_time"].as_str().context("bar_end_time missing")?;
        let t = NaiveDateTime::parse_from_str(time, "%Y-%m-%d %H:%M:%S")?;
        let symbol = r["ts_code"].as_str().context("ts_code missing")?;
        if let Some(expected) = rows[0]["ts_code"].as_str()
            && symbol != expected
        {
            bail!("Analyze one symbol at a time");
        }
        if !times.insert(time) {
            duplicates += 1;
        }
        *days.entry(t.date().to_string()).or_default() += 1;
        let minute = t.hour() * 60 + t.minute();
        if !((571..=690).contains(&minute) || (781..=900).contains(&minute)) || t.second() != 0 {
            off_session += 1;
        }
        let fields = [
            "px_open",
            "px_high",
            "px_low",
            "px_close",
            "volume_shares",
            "turnover_cny",
        ];
        let mut n = Vec::new();
        for f in fields {
            n.push(r[f].as_f64().context("Invalid numeric OHLCV field")?);
        }
        if n.iter().any(|v| !v.is_finite())
            || n[..4].iter().any(|v| *v <= 0.0)
            || n[1] < n[0].max(n[3])
            || n[2] > n[0].min(n[3])
            || n[1] < n[2]
            || n[4] < 0.0
            || n[5] < 0.0
        {
            invalid += 1;
        }
        first.get_or_insert(n[3]);
        last = Some(n[3]);
        peak = peak.max(n[3]);
        drawdown = drawdown.max(1.0 - n[3] / peak);
        volume += n[4];
        turnover += n[5];
    }
    let expected = match value["query"]["frequency"].as_str().unwrap_or("1min") {
        "1min" => 240,
        "5min" => 48,
        "15min" => 16,
        "30min" => 8,
        "60min" => 4,
        _ => 0,
    };
    let incomplete = days
        .iter()
        .filter(|(_, n)| **n != expected)
        .map(|(day, n)| json!({"date":day,"observed_rows":n,"full_session_expected_rows":expected}))
        .collect::<Vec<_>>();
    Ok(
        json!({"cli_version":env!("CARGO_PKG_VERSION"),"input_sha256":crate::state::hash(input),"symbol":rows[0]["ts_code"],"rows":rows.len(),"first_bar":rows[0]["bar_end_time"],"last_bar":rows[rows.len()-1]["bar_end_time"],"raw_close_change":last.zip(first).map(|(l,f)|l/f-1.0),"raw_close_max_drawdown":drawdown,"volume_shares":volume,"turnover_cny":turnover,"observed_day_rows":days,"observed_days_with_partial_or_invalid_counts":incomplete,"duplicate_timestamps":duplicates,"invalid_ohlcv":invalid,"off_session_timestamps":off_session,"datasets":value["datasets"],"contract":value["contract"],"interpretation":"unadjusted descriptive statistics; not strategy returns; no certified calendar or fee model"}),
    )
}
