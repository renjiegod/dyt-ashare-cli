use anyhow::{Context, Result, bail};
use chrono::{NaiveDate, NaiveDateTime};
use serde_json::{Value, json};
use std::{collections::BTreeSet, fmt::Write as _, path::PathBuf};

const TRADING_DAYS_PER_YEAR: f64 = 252.0;

#[derive(Clone, Debug)]
pub enum Strategy {
    BuyAndHold,
    SmaCross { fast: usize, slow: usize },
}

impl Strategy {
    pub fn parse(name: &str, fast: usize, slow: usize) -> Result<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "buy-and-hold" | "buy_hold" | "buyhold" => Ok(Self::BuyAndHold),
            "sma-cross" | "sma_cross" | "ma-cross" | "macross" => {
                if fast < 2 || slow < 2 || fast >= slow {
                    bail!("SMA strategy requires 2 <= fast < slow");
                }
                Ok(Self::SmaCross { fast, slow })
            }
            other => bail!("Unknown strategy `{other}`; use `buy-and-hold` or `sma-cross`"),
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Self::BuyAndHold => "buy-and-hold",
            Self::SmaCross { .. } => "sma-cross",
        }
    }

    fn parameters(&self) -> Value {
        match self {
            Self::BuyAndHold => json!({}),
            Self::SmaCross { fast, slow } => json!({"fast":fast,"slow":slow}),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    pub symbol: String,
    pub strategy: Strategy,
    pub initial_cash: f64,
    pub fee_bps: f64,
    pub slippage_bps: f64,
    pub lot_size: u64,
    pub report_path: Option<PathBuf>,
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        if self.initial_cash <= 0.0 || !self.initial_cash.is_finite() {
            bail!("initial cash must be finite and greater than zero");
        }
        for (name, value) in [
            ("fee-bps", self.fee_bps),
            ("slippage-bps", self.slippage_bps),
        ] {
            if value < 0.0 || !value.is_finite() || value >= 10_000.0 {
                bail!("{name} must be finite and in [0, 10000)");
            }
        }
        if self.lot_size == 0 {
            bail!("lot size must be greater than zero");
        }
        Ok(())
    }

    fn fee_rate(&self) -> f64 {
        self.fee_bps / 10_000.0
    }

    fn slippage_rate(&self) -> f64 {
        self.slippage_bps / 10_000.0
    }
}

#[derive(Clone, Debug)]
struct Bar {
    time: NaiveDateTime,
    time_text: String,
    date: NaiveDate,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
    turnover: f64,
}

#[derive(Clone, Debug)]
struct Trade {
    side: &'static str,
    time: String,
    execution: &'static str,
    shares: u64,
    price: f64,
    gross: f64,
    fee: f64,
    slippage: f64,
    pnl: Option<f64>,
}

#[derive(Clone, Debug)]
struct EquityPoint {
    time: String,
    close: f64,
    equity: f64,
    drawdown: f64,
    shares: u64,
}

struct Costs {
    fees: f64,
    slippage: f64,
    turnover: f64,
}

#[derive(Clone, Debug)]
pub struct RunOutput {
    pub value: Value,
    pub html: String,
}

pub fn run(input: &Value, config: &Config) -> Result<RunOutput> {
    config.validate()?;
    if input["contract"]["version"] != "daily-from-minute-v1" {
        bail!("Backtest requires the daily-from-minute-v1 data contract");
    }
    if input["datasets"].as_array().is_none_or(Vec::is_empty) {
        bail!("Backtest requires dataset provenance from the remote API");
    }
    let bars = parse_bars(input, &config.symbol)?;
    if bars.len() < 2 {
        bail!("Backtest needs at least two valid daily bars");
    }
    if let Strategy::SmaCross { slow, .. } = &config.strategy
        && bars.len() <= *slow
    {
        bail!("SMA slow window needs more than {slow} daily bars; extend the range");
    }

    let mut cash = config.initial_cash;
    let mut shares = 0_u64;
    let mut position_cost = 0.0;
    let mut fees = 0.0;
    let mut slippage = 0.0;
    let mut turnover = 0.0;
    let mut trades = Vec::new();
    let mut equity = Vec::with_capacity(bars.len());
    let signals = signals(&bars, &config.strategy);

    for (index, bar) in bars.iter().enumerate() {
        if signals[index] == Some(Action::Buy) && shares == 0 {
            let (bought, spent, fee, slip) = buy(
                cash,
                bar.open,
                config.lot_size,
                config.fee_rate(),
                config.slippage_rate(),
            );
            if bought > 0 {
                cash -= spent;
                shares = bought;
                position_cost = spent;
                fees += fee;
                slippage += slip;
                turnover += spent - fee;
                trades.push(Trade {
                    side: "buy",
                    time: bar.time_text.clone(),
                    execution: "next_open",
                    shares: bought,
                    price: bar.open * (1.0 + config.slippage_rate()),
                    gross: spent - fee,
                    fee,
                    slippage: slip,
                    pnl: None,
                });
            }
        } else if signals[index] == Some(Action::Sell) && shares > 0 {
            let sold = sell(
                shares,
                bar.open,
                position_cost,
                config.fee_rate(),
                config.slippage_rate(),
            );
            cash += sold.net;
            fees += sold.fee;
            slippage += sold.slippage;
            turnover += sold.gross;
            trades.push(Trade {
                side: "sell",
                time: bar.time_text.clone(),
                execution: "next_open",
                shares,
                price: sold.price,
                gross: sold.gross,
                fee: sold.fee,
                slippage: sold.slippage,
                pnl: Some(sold.net - position_cost),
            });
            shares = 0;
            position_cost = 0.0;
        }

        let mut marked_equity = cash + shares as f64 * bar.close;
        if index + 1 == bars.len() && shares > 0 {
            let sold = sell(
                shares,
                bar.close,
                position_cost,
                config.fee_rate(),
                config.slippage_rate(),
            );
            cash += sold.net;
            fees += sold.fee;
            slippage += sold.slippage;
            turnover += sold.gross;
            trades.push(Trade {
                side: "sell",
                time: bar.time_text.clone(),
                execution: "terminal_close",
                shares,
                price: sold.price,
                gross: sold.gross,
                fee: sold.fee,
                slippage: sold.slippage,
                pnl: Some(sold.net - position_cost),
            });
            shares = 0;
            position_cost = 0.0;
            marked_equity = cash;
        }
        equity.push((bar.time_text.clone(), bar.close, marked_equity, shares));
    }

    let points = with_drawdown(equity);
    let observed_dates = bars.iter().map(|bar| bar.date).collect::<BTreeSet<_>>();
    let metrics = metrics(
        &points,
        &trades,
        config,
        bars[0].date,
        bars[bars.len() - 1].date,
        Costs {
            fees,
            slippage,
            turnover,
        },
    );
    let value = json!({
        "cli_version": env!("CARGO_PKG_VERSION"),
        "kind": "local_backtest",
        "status": "completed",
        "symbol": config.symbol,
        "strategy": {"name": config.strategy.name(), "parameters": config.strategy.parameters()},
        "bars": bars.len(),
        "data": {
            "first_bar": bars[0].time_text,
            "last_bar": bars[bars.len() - 1].time_text,
            "frequency": "1d",
            "observed_trading_days": observed_dates.len(),
            "missing_dates_unknown": true,
            "price_high": bars.iter().map(|bar| bar.high).fold(f64::NEG_INFINITY, f64::max),
            "price_low": bars.iter().map(|bar| bar.low).fold(f64::INFINITY, f64::min),
            "volume_shares": bars.iter().map(|bar| bar.volume).sum::<f64>(),
            "turnover_cny": bars.iter().map(|bar| bar.turnover).sum::<f64>(),
            "contract": input["contract"],
            "datasets": input["datasets"],
            "query": input["query"],
            "auth": input["auth"],
            "response_sha256": crate::state::hash(&serde_json::to_vec(input)?),
            "coverage": input["coverage"],
        },
        "assumptions": {
            "execution": "signal_at_close_fills_next_bar_open; terminal position is liquidated at the final close",
            "t_plus_one": "a position is only sold on a later bar or at terminal close; same-day round trips are not allowed",
            "price": "unadjusted daily OHLC from the remote DYT AShare API",
            "position": "long-only; one position at a time; integer shares rounded down to lot size",
            "corporate_actions": "not adjusted; split and dividend effects can distort returns",
            "limit_and_tradability": "涨跌停、停牌、排队与部分成交未建模; inspect source coverage before using results",
        },
        "metrics": metrics,
        "trades": trades.iter().map(trade_json).collect::<Vec<_>>(),
        "equity_curve": points.iter().map(point_json).collect::<Vec<_>>(),
        "interpretation": "Research evidence from one fixed data window and cost model; not investment advice or a profitability guarantee",
    });
    let html = render_html(&value, &bars, &points, &trades, config);
    Ok(RunOutput { value, html })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Buy,
    Sell,
}

fn parse_bars(input: &Value, symbol: &str) -> Result<Vec<Bar>> {
    let items = input["items"]
        .as_array()
        .context("Remote daily response is missing items")?;
    let mut bars = Vec::with_capacity(items.len());
    for item in items {
        let item_symbol = item["ts_code"]
            .as_str()
            .context("Daily response item is missing ts_code")?;
        if item_symbol != symbol {
            bail!("Remote response contains symbol {item_symbol}, expected {symbol}");
        }
        let time_text = item["bar_end_time"]
            .as_str()
            .context("Daily response item is missing bar_end_time")?
            .to_owned();
        let time = NaiveDateTime::parse_from_str(&time_text, "%Y-%m-%d %H:%M:%S")
            .with_context(|| format!("Invalid bar_end_time {time_text}"))?;
        let numbers = [
            number(item, "px_open")?,
            number(item, "px_high")?,
            number(item, "px_low")?,
            number(item, "px_close")?,
            number(item, "volume_shares")?,
            number(item, "turnover_cny")?,
        ];
        if numbers.iter().any(|value| !value.is_finite())
            || numbers[..4].iter().any(|value| *value <= 0.0)
            || numbers[1] < numbers[0].max(numbers[3])
            || numbers[2] > numbers[0].min(numbers[3])
            || numbers[1] < numbers[2]
            || numbers[4] < 0.0
            || numbers[5] < 0.0
        {
            bail!("Invalid OHLCV values at {time_text}");
        }
        bars.push(Bar {
            time,
            date: time.date(),
            time_text,
            open: numbers[0],
            high: numbers[1],
            low: numbers[2],
            close: numbers[3],
            volume: numbers[4],
            turnover: numbers[5],
        });
    }
    bars.sort_by_key(|bar| bar.time);
    if bars.windows(2).any(|pair| pair[0].time == pair[1].time) {
        bail!("Remote daily response contains duplicate timestamps");
    }
    Ok(bars)
}

fn number(item: &Value, field: &str) -> Result<f64> {
    item[field]
        .as_f64()
        .with_context(|| format!("Daily response item has invalid {field}"))
}

fn signals(bars: &[Bar], strategy: &Strategy) -> Vec<Option<Action>> {
    let mut result = vec![None; bars.len()];
    match strategy {
        Strategy::BuyAndHold => result[0] = Some(Action::Buy),
        Strategy::SmaCross { fast, slow } => {
            if average_close(bars, *slow - 1, *fast) > average_close(bars, *slow - 1, *slow)
                && *slow < bars.len()
            {
                result[*slow] = Some(Action::Buy);
            }
            for close_index in *slow..bars.len() {
                let previous_fast = average_close(bars, close_index - 1, *fast);
                let previous_slow = average_close(bars, close_index - 1, *slow);
                let current_fast = average_close(bars, close_index, *fast);
                let current_slow = average_close(bars, close_index, *slow);
                let action = if previous_fast <= previous_slow && current_fast > current_slow {
                    Some(Action::Buy)
                } else if previous_fast >= previous_slow && current_fast < current_slow {
                    Some(Action::Sell)
                } else {
                    None
                };
                if let Some(action) = action
                    && close_index + 1 < bars.len()
                {
                    result[close_index + 1] = Some(action);
                }
            }
        }
    }
    result
}

fn average_close(bars: &[Bar], end_index: usize, window: usize) -> f64 {
    let start = end_index + 1 - window;
    bars[start..=end_index]
        .iter()
        .map(|bar| bar.close)
        .sum::<f64>()
        / window as f64
}

fn buy(
    cash: f64,
    open: f64,
    lot_size: u64,
    fee_rate: f64,
    slippage_rate: f64,
) -> (u64, f64, f64, f64) {
    let price = open * (1.0 + slippage_rate);
    let per_share = price * (1.0 + fee_rate);
    let shares = ((cash / per_share).floor() as u64 / lot_size) * lot_size;
    let gross = shares as f64 * price;
    let fee = gross * fee_rate;
    let slippage = shares as f64 * (price - open);
    (shares, gross + fee, fee, slippage)
}

struct SellResult {
    price: f64,
    gross: f64,
    net: f64,
    fee: f64,
    slippage: f64,
}

fn sell(
    shares: u64,
    raw_price: f64,
    _position_cost: f64,
    fee_rate: f64,
    slippage_rate: f64,
) -> SellResult {
    let price = raw_price * (1.0 - slippage_rate);
    let gross = shares as f64 * price;
    let fee = gross * fee_rate;
    SellResult {
        price,
        gross,
        net: gross - fee,
        fee,
        slippage: shares as f64 * (raw_price - price),
    }
}

fn with_drawdown(points: Vec<(String, f64, f64, u64)>) -> Vec<EquityPoint> {
    let mut peak = 0.0_f64;
    points
        .into_iter()
        .map(|(time, close, equity, shares)| {
            peak = peak.max(equity);
            EquityPoint {
                time,
                close,
                equity,
                drawdown: if peak > 0.0 { 1.0 - equity / peak } else { 0.0 },
                shares,
            }
        })
        .collect()
}

fn metrics(
    points: &[EquityPoint],
    trades: &[Trade],
    config: &Config,
    first_date: NaiveDate,
    last_date: NaiveDate,
    costs: Costs,
) -> Value {
    let starting = config.initial_cash;
    let ending = points.last().map_or(starting, |point| point.equity);
    let total_return = ending / starting - 1.0;
    let days = (last_date - first_date).num_days().max(1) as f64;
    let annualized = if ending > 0.0 {
        (ending / starting).powf(365.0 / days) - 1.0
    } else {
        -1.0
    };
    let returns = points
        .windows(2)
        .filter_map(|pair| (pair[0].equity > 0.0).then_some(pair[1].equity / pair[0].equity - 1.0))
        .collect::<Vec<_>>();
    let mean = if returns.is_empty() {
        0.0
    } else {
        returns.iter().sum::<f64>() / returns.len() as f64
    };
    let variance = if returns.len() < 2 {
        0.0
    } else {
        returns
            .iter()
            .map(|value| (value - mean).powi(2))
            .sum::<f64>()
            / returns.len() as f64
    };
    let volatility = variance.sqrt() * TRADING_DAYS_PER_YEAR.sqrt();
    let sharpe = if volatility > 0.0 {
        mean / variance.sqrt() * TRADING_DAYS_PER_YEAR.sqrt()
    } else {
        0.0
    };
    let max_drawdown = points
        .iter()
        .map(|point| point.drawdown)
        .fold(0.0_f64, f64::max);
    let wins = trades
        .iter()
        .filter(|trade| trade.side == "sell" && trade.pnl.is_some_and(|pnl| pnl > 0.0))
        .count();
    let completed_trades = trades.iter().filter(|trade| trade.side == "sell").count();
    let exposure =
        points.iter().filter(|point| point.shares > 0).count() as f64 / points.len().max(1) as f64;
    json!({
        "starting_cash": starting,
        "ending_equity": ending,
        "total_return_pct": total_return * 100.0,
        "annualized_return_pct": annualized * 100.0,
        "max_drawdown_pct": max_drawdown * 100.0,
        "annualized_volatility_pct": volatility * 100.0,
        "sharpe": sharpe,
        "exposure_pct": exposure * 100.0,
        "completed_trades": completed_trades,
        "winning_trades": wins,
        "win_rate_pct": if completed_trades == 0 { 0.0 } else { wins as f64 / completed_trades as f64 * 100.0 },
        "fees": costs.fees,
        "slippage_cost": costs.slippage,
        "turnover": costs.turnover,
        "terminal_state": {"cash":ending,"shares":0,"flat":true},
        "fee_bps": config.fee_bps,
        "slippage_bps": config.slippage_bps,
        "lot_size": config.lot_size,
        "calendar_days": days,
    })
}

fn trade_json(trade: &Trade) -> Value {
    json!({
        "side": trade.side,
        "time": trade.time,
        "execution": trade.execution,
        "shares": trade.shares,
        "price": trade.price,
        "gross": trade.gross,
        "fee": trade.fee,
        "slippage": trade.slippage,
        "pnl": trade.pnl,
    })
}

fn point_json(point: &EquityPoint) -> Value {
    json!({"time":point.time,"close":point.close,"equity":point.equity,"drawdown_pct":point.drawdown*100.0,"shares":point.shares})
}

fn report_path(config: &Config) -> PathBuf {
    config.report_path.clone().unwrap_or_else(|| {
        PathBuf::from(format!(
            "dytcn-backtest-{}-{}.html",
            config.symbol,
            config.strategy.name()
        ))
    })
}

pub fn default_report_path(config: &Config) -> PathBuf {
    report_path(config)
}

fn render_html(
    value: &Value,
    bars: &[Bar],
    points: &[EquityPoint],
    trades: &[Trade],
    config: &Config,
) -> String {
    let metrics = &value["metrics"];
    let bar_times = bars
        .iter()
        .map(|bar| bar.time_text.clone())
        .collect::<Vec<_>>();
    let point_times = points
        .iter()
        .map(|point| point.time.clone())
        .collect::<Vec<_>>();
    let price_chart = line_chart(
        "Price",
        bars.iter()
            .map(|bar| bar.close)
            .collect::<Vec<_>>()
            .as_slice(),
        "#7dd3fc",
        trades,
        &bar_times,
    );
    let equity_chart = line_chart(
        "Equity",
        points
            .iter()
            .map(|point| point.equity)
            .collect::<Vec<_>>()
            .as_slice(),
        "#a7f3d0",
        trades,
        &point_times,
    );
    let drawdown_chart = line_chart(
        "Drawdown",
        points
            .iter()
            .map(|point| -point.drawdown * 100.0)
            .collect::<Vec<_>>()
            .as_slice(),
        "#fda4af",
        &[],
        &point_times,
    );
    let mut table = String::new();
    for trade in trades {
        let pnl = trade.pnl.map_or("—".into(), |pnl| format!("{pnl:.2}"));
        let _ = write!(
            table,
            "<tr><td>{}</td><td class=\"{}\">{}</td><td>{}</td><td>{}</td><td>{:.2}</td><td>{}</td></tr>",
            escape(&trade.time),
            trade.side,
            trade.side.to_ascii_uppercase(),
            trade.execution,
            trade.shares,
            trade.price,
            pnl
        );
    }
    let data_note = value["data"]["response_sha256"]
        .as_str()
        .unwrap_or("unknown");
    format!(
        r##"<!doctype html>
<html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>{symbol} · DYT 本地回测</title>
<style>
:root{{color-scheme:dark;--bg:#07111f;--panel:#0e1b2d;--muted:#91a4bc;--text:#e9f1fb;--line:#20324c;--cyan:#7dd3fc;--mint:#a7f3d0;--rose:#fda4af}}
*{{box-sizing:border-box}}body{{margin:0;background:radial-gradient(circle at 15% 0,#183453 0,#07111f 45%);font:14px/1.55 Inter,ui-sans-serif,system-ui,-apple-system,sans-serif;color:var(--text)}}
.wrap{{max-width:1180px;margin:0 auto;padding:36px 22px 64px}}.eyebrow{{letter-spacing:.14em;text-transform:uppercase;color:var(--cyan);font-size:11px;font-weight:700}}
h1{{font-size:clamp(28px,5vw,48px);line-height:1.04;margin:10px 0 8px}}h2{{font-size:18px;margin:0 0 12px}}p{{color:var(--muted);margin:8px 0}}.meta{{display:flex;flex-wrap:wrap;gap:8px;margin:18px 0 26px}}.chip{{border:1px solid var(--line);border-radius:999px;padding:6px 10px;color:#bdd0e8;background:#0b1728}}
.grid{{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:10px;margin:18px 0}}.card,.panel{{background:rgba(14,27,45,.86);border:1px solid var(--line);border-radius:16px;box-shadow:0 14px 40px #02081255}}.card{{padding:15px}}.label{{font-size:11px;color:var(--muted);text-transform:uppercase;letter-spacing:.06em}}.metric{{font-size:24px;font-weight:750;margin-top:3px}}.good{{color:var(--mint)}}.bad{{color:var(--rose)}}
.panel{{padding:18px;margin-top:14px}}.chart{{width:100%;height:auto;display:block;background:linear-gradient(180deg,#0f2035,#0b1728);border-radius:12px}}.legend{{color:var(--muted);font-size:12px;margin-top:8px}}.legend b{{color:var(--text)}}
.two{{display:grid;grid-template-columns:1fr 1fr;gap:14px}}table{{width:100%;border-collapse:collapse;font-size:12px}}th,td{{padding:9px 8px;border-bottom:1px solid var(--line);text-align:right;white-space:nowrap}}th:first-child,td:first-child{{text-align:left}}th{{color:var(--muted);font-weight:600}}td.buy{{color:var(--cyan)}}td.sell{{color:var(--rose)}}code{{color:#c8d8ef;overflow-wrap:anywhere}}.foot{{font-size:12px;margin-top:22px;color:var(--muted)}}
@media(max-width:800px){{.grid{{grid-template-columns:repeat(2,minmax(0,1fr))}}.two{{grid-template-columns:1fr}}.wrap{{padding:24px 14px 42px}}th,td{{padding:8px 4px;font-size:11px}}}}
</style></head><body><main class="wrap">
<div class="eyebrow">DYT AShare · Local research replay</div><h1>{symbol}</h1>
<p>{strategy} · {first} → {last} · {bars} daily bars</p>
<div class="meta"><span class="chip">未复权价格</span><span class="chip">信号收盘，下一根开盘成交</span><span class="chip">终点平仓</span><span class="chip">数据哈希 <code>{hash}</code></span></div>
<section class="grid"><div class="card"><div class="label">总收益</div><div class="metric {return_class}">{total_return:.2}%</div></div><div class="card"><div class="label">最大回撤</div><div class="metric bad">-{max_dd:.2}%</div></div><div class="card"><div class="label">年化波动</div><div class="metric">{vol:.2}%</div></div><div class="card"><div class="label">Sharpe</div><div class="metric">{sharpe:.2}</div></div></section>
<section class="panel"><h2>价格与交易</h2>{price_chart}<div class="legend"><b>蓝线</b> 收盘价 · 青色圆点买入 · 粉色圆点卖出</div></section>
<div class="two"><section class="panel"><h2>权益曲线</h2>{equity_chart}</section><section class="panel"><h2>回撤</h2>{drawdown_chart}</section></div>
<section class="panel"><h2>回测参数与成本</h2><div class="grid"><div><div class="label">起始资金</div><div class="metric">¥{starting:.2}</div></div><div><div class="label">结束权益</div><div class="metric">¥{ending:.2}</div></div><div><div class="label">手续费 / 滑点</div><div class="metric">¥{fees:.2} / ¥{slippage:.2}</div></div><div><div class="label">交易次数 / 胜率</div><div class="metric">{trades_count} / {win_rate:.1}%</div></div></div><p>费率 {fee_bps:.2} bps · 滑点 {slippage_bps:.2} bps · 每手 {lot_size} 股 · 暴露 {exposure:.1}%</p></section>
<section class="panel"><h2>成交明细</h2><div style="overflow:auto"><table><thead><tr><th>时间</th><th>方向</th><th>成交方式</th><th>股数</th><th>价格</th><th>净盈亏</th></tr></thead><tbody>{table}</tbody></table></div></section>
<p class="foot">本报告由 dytcn {version} 生成。仅用于固定窗口的研究复盘，不构成投资建议；数据缺失、复权、涨跌停、停牌、排队和部分成交可能改变实际结果。</p>
</main></body></html>"##,
        symbol = escape(config.symbol.as_str()),
        strategy = escape(config.strategy.name()),
        first = escape(bars[0].time_text.as_str()),
        last = escape(bars[bars.len() - 1].time_text.as_str()),
        bars = bars.len(),
        hash = escape(data_note),
        total_return = metrics["total_return_pct"].as_f64().unwrap_or(0.0),
        return_class = if metrics["total_return_pct"].as_f64().unwrap_or(0.0) >= 0.0 {
            "good"
        } else {
            "bad"
        },
        max_dd = metrics["max_drawdown_pct"].as_f64().unwrap_or(0.0),
        vol = metrics["annualized_volatility_pct"].as_f64().unwrap_or(0.0),
        sharpe = metrics["sharpe"].as_f64().unwrap_or(0.0),
        starting = metrics["starting_cash"].as_f64().unwrap_or(0.0),
        ending = metrics["ending_equity"].as_f64().unwrap_or(0.0),
        fees = metrics["fees"].as_f64().unwrap_or(0.0),
        slippage = metrics["slippage_cost"].as_f64().unwrap_or(0.0),
        trades_count = metrics["completed_trades"].as_u64().unwrap_or(0),
        win_rate = metrics["win_rate_pct"].as_f64().unwrap_or(0.0),
        fee_bps = config.fee_bps,
        slippage_bps = config.slippage_bps,
        lot_size = config.lot_size,
        exposure = metrics["exposure_pct"].as_f64().unwrap_or(0.0),
        version = env!("CARGO_PKG_VERSION"),
    )
}

fn line_chart(
    label: &str,
    values: &[f64],
    color: &str,
    trades: &[Trade],
    times: &[String],
) -> String {
    let width = 1100.0;
    let height = 300.0;
    let pad_x = 30.0;
    let pad_y = 24.0;
    let bottom = height - pad_y;
    let (mut min, mut max) = values
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(min, max), value| {
            (min.min(*value), max.max(*value))
        });
    if !min.is_finite() || !max.is_finite() {
        min = 0.0;
        max = 1.0;
    }
    if (max - min).abs() < f64::EPSILON {
        min -= 1.0;
        max += 1.0;
    }
    let x = |index: usize| {
        pad_x
            + index as f64 / (values.len().saturating_sub(1).max(1) as f64) * (width - 2.0 * pad_x)
    };
    let y = |value: f64| height - pad_y - (value - min) / (max - min) * (height - 2.0 * pad_y);
    let polyline = values
        .iter()
        .enumerate()
        .map(|(index, value)| format!("{:.2},{:.2}", x(index), y(*value)))
        .collect::<Vec<_>>()
        .join(" ");
    let mut markers = String::new();
    for trade in trades {
        if let Some(index) = index_for_time(trade.time.as_str(), times) {
            let value = values[index];
            let marker_color = if trade.side == "buy" {
                "#7dd3fc"
            } else {
                "#fda4af"
            };
            let _ = write!(
                markers,
                "<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"5\" fill=\"{}\" stroke=\"#07111f\" stroke-width=\"2\"/>",
                x(index),
                y(value),
                marker_color
            );
        }
    }
    format!(
        "<svg class=\"chart\" viewBox=\"0 0 {width} {height}\" role=\"img\" aria-label=\"{label}\"><defs><linearGradient id=\"grad-{label}\" x1=\"0\" x2=\"0\" y1=\"0\" y2=\"1\"><stop offset=\"0\" stop-color=\"{color}\" stop-opacity=\".2\"/><stop offset=\"1\" stop-color=\"{color}\" stop-opacity=\"0\"/></linearGradient></defs><path d=\"M {pad_x} {bottom} H {right}\" stroke=\"#20324c\"/><polyline fill=\"none\" stroke=\"{color}\" stroke-width=\"3\" stroke-linecap=\"round\" stroke-linejoin=\"round\" points=\"{polyline}\"/><polyline fill=\"url(#grad-{label})\" stroke=\"none\" points=\"{pad_x},{bottom} {polyline} {right},{bottom}\"/>{markers}</svg>",
        right = width - pad_x,
    )
}

fn index_for_time(time: &str, times: &[String]) -> Option<usize> {
    times.iter().position(|candidate| candidate == time)
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
