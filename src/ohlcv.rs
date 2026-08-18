use rusqlite::{Connection, params};
use std::collections::HashMap;

// Native SOL (system program) mint address, used for wrapped SOL token accounts.
const WSOL_MINT: &str = "So11111111111111111111111111111111111111112";
// Below this, a WSOL-side amount is treated as dust (ADR: 0.0001 SOL).
const DUST_THRESHOLD_LAMPORTS: i128 = 100_000;

#[derive(Clone, Debug, PartialEq)]
pub struct Trade {
    pub mint: String,
    pub block_time: i64,
    pub slot: i64,
    pub tx_index: i64,
    pub price: f64,  // SOL per token
    pub volume: f64, // base-token amount moved
}

#[derive(Clone, Debug)]
pub struct Candle {
    pub mint: String,
    pub interval: &'static str,
    pub bucket_start: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

struct BalanceRow {
    signature: String,
    mint: String,
    pre_amount: String,
    post_amount: String,
    decimals: i64,
    slot: i64,
    tx_index: i64,
    block_time: i64,
}

fn raw_delta(pre: &str, post: &str) -> i128 {
    let pre: i128 = pre.parse().unwrap_or(0);
    let post: i128 = post.parse().unwrap_or(0);
    post - pre
}

pub fn extract_trades(conn: &Connection) -> rusqlite::Result<Vec<Trade>> {
    let mut stmt = conn.prepare(
        "SELECT tb.signature, tb.mint, tb.pre_amount, tb.post_amount, tb.decimals,
                t.slot, t.tx_index, b.block_time
         FROM token_balances tb
         JOIN transactions t ON t.signature = tb.signature
         JOIN blocks b ON b.slot = t.slot
         WHERE b.skipped = 0 AND b.block_time IS NOT NULL
         ORDER BY t.slot, t.tx_index, tb.account_index",
    )?;

    let rows: Vec<BalanceRow> = stmt
        .query_map([], |row| {
            Ok(BalanceRow {
                signature: row.get(0)?,
                mint: row.get(1)?,
                pre_amount: row.get(2)?,
                post_amount: row.get(3)?,
                decimals: row.get(4)?,
                slot: row.get(5)?,
                tx_index: row.get(6)?,
                block_time: row.get(7)?,
            })
        })?
        .collect::<Result<_, _>>()?;

    Ok(trades_from_rows(&rows))
}

fn trades_from_rows(rows: &[BalanceRow]) -> Vec<Trade> {
    let mut trades = Vec::new();
    let mut i = 0;
    while i < rows.len() {
        let mut j = i;
        while j < rows.len() && rows[j].signature == rows[i].signature {
            j += 1;
        }
        if let Some(trade) = trade_from_group(&rows[i..j]) {
            trades.push(trade);
        }
        i = j;
    }
    trades
}

/// One transaction's token_balances rows -> at most one inferred trade.
///
/// Excludes (by construction, not as separate special cases):
/// - multi-hop routes: more than one non-WSOL mint with a nonzero delta
/// - SOL wrap/unwrap only: zero non-WSOL mints touched
/// - tokens with no SOL pair: never produce a WSOL-paired group at all
/// - internal routing that nets to zero: same mint across multiple
///   accounts in one tx, deltas cancel out (real, common in this data —
///   see ADR-0008)
/// - dust: WSOL-side amount below DUST_THRESHOLD_LAMPORTS
/// - same-direction deltas (both legs increase or both decrease): also
///   rejects simple two-sided liquidity add/remove as a side effect, not
///   a guaranteed complete liquidity filter
/// - round-number token amounts (exact multiples of 1,000,000 units):
///   token creation/mint events (e.g. pump.fun's 1B initial supply)
///   co-occurring with an unrelated small SOL fee, not a trade
fn trade_from_group(rows: &[BalanceRow]) -> Option<Trade> {
    let mut per_mint: HashMap<&str, (i128, i64)> = HashMap::new();
    for row in rows {
        let delta = raw_delta(&row.pre_amount, &row.post_amount);
        let entry = per_mint
            .entry(row.mint.as_str())
            .or_insert((0, row.decimals));
        entry.0 += delta;
    }

    let mut wsol_delta: i128 = 0;
    let mut base: Option<(&str, i128, i64)> = None;
    let mut multiple_base_mints = false;

    for (mint, (delta, decimals)) in &per_mint {
        if *delta == 0 {
            continue;
        }
        if *mint == WSOL_MINT {
            wsol_delta += *delta;
        } else if base.is_none() {
            base = Some((mint, *delta, *decimals));
        } else {
            multiple_base_mints = true;
        }
    }

    if multiple_base_mints || wsol_delta == 0 {
        return None;
    }
    let (base_mint, base_delta, base_decimals) = base?;

    if wsol_delta.abs() < DUST_THRESHOLD_LAMPORTS {
        return None;
    }
    if (base_delta > 0) == (wsol_delta > 0) {
        return None;
    }
    let round_unit = 10u128.pow(base_decimals as u32) * 1_000_000;
    if base_delta.unsigned_abs() % round_unit == 0 {
        return None; // looks like a mint/creation event, not a trade
    }

    let base_amount = base_delta.unsigned_abs() as f64 / 10f64.powi(base_decimals as i32);
    let sol_amount = wsol_delta.unsigned_abs() as f64 / 1e9;
    if base_amount == 0.0 {
        return None;
    }

    let first = &rows[0];
    Some(Trade {
        mint: base_mint.to_string(),
        block_time: first.block_time,
        slot: first.slot,
        tx_index: first.tx_index,
        price: sol_amount / base_amount,
        volume: base_amount,
    })
}

pub fn build_candles(trades: &[Trade], interval: &'static str, bucket_seconds: i64) -> Vec<Candle> {
    let mut grouped: HashMap<(String, i64), Vec<&Trade>> = HashMap::new();
    for trade in trades {
        let bucket_start = trade.block_time - trade.block_time.rem_euclid(bucket_seconds);
        grouped
            .entry((trade.mint.clone(), bucket_start))
            .or_default()
            .push(trade);
    }

    let mut candles = Vec::new();
    for ((mint, bucket_start), mut group) in grouped {
        group.sort_by_key(|t| (t.slot, t.tx_index));
        let open = group.first().unwrap().price;
        let close = group.last().unwrap().price;
        let high = group.iter().map(|t| t.price).fold(f64::MIN, f64::max);
        let low = group.iter().map(|t| t.price).fold(f64::MAX, f64::min);
        let volume = group.iter().map(|t| t.volume).sum();

        candles.push(Candle {
            mint,
            interval,
            bucket_start,
            open,
            high,
            low,
            close,
            volume,
        });
    }
    candles
}

pub fn build_and_store_candles(conn: &mut Connection) -> rusqlite::Result<usize> {
    let trades = extract_trades(conn)?;
    let one_min = build_candles(&trades, "1m", 60);
    let five_min = build_candles(&trades, "5m", 300);
    let total = one_min.len() + five_min.len();

    let tx = conn.transaction()?;
    let mut insert = tx.prepare_cached(
        "INSERT INTO ohlcv_candles (mint, interval, bucket_start, open, high, low, close, volume)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(mint, interval, bucket_start) DO UPDATE SET
            open = excluded.open, high = excluded.high, low = excluded.low,
            close = excluded.close, volume = excluded.volume",
    )?;
    for candle in one_min.iter().chain(five_min.iter()) {
        insert.execute(params![
            candle.mint,
            candle.interval,
            candle.bucket_start,
            candle.open,
            candle.high,
            candle.low,
            candle.close,
            candle.volume
        ])?;
    }
    drop(insert);
    tx.commit()?;

    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(sig: &str, mint: &str, pre: &str, post: &str, decimals: i64, slot: i64, tx_index: i64, block_time: i64) -> BalanceRow {
        BalanceRow {
            signature: sig.to_string(),
            mint: mint.to_string(),
            pre_amount: pre.to_string(),
            post_amount: post.to_string(),
            decimals,
            slot,
            tx_index,
            block_time,
        }
    }

    #[test]
    fn simple_swap_produces_one_trade() {
        // user receives 100 TOKEN (6 decimals), pays 1 SOL (9 decimals)
        let rows = vec![
            row("s1", "TOKEN", "0", "100000000", 6, 1, 0, 1000),
            row("s1", WSOL_MINT, "2000000000", "1000000000", 9, 1, 0, 1000),
        ];
        let trades = trades_from_rows(&rows);
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].mint, "TOKEN");
        assert_eq!(trades[0].volume, 100.0);
        assert!((trades[0].price - 0.01).abs() < 1e-9); // 1 SOL / 100 TOKEN
    }

    #[test]
    fn multi_hop_excluded() {
        // two different non-WSOL mints change in the same tx -> not a direct pair
        let rows = vec![
            row("s1", "TOKEN_A", "100", "0", 6, 1, 0, 1000),
            row("s1", "TOKEN_B", "0", "50", 6, 1, 0, 1000),
            row("s1", WSOL_MINT, "1000000000", "0", 9, 1, 0, 1000),
        ];
        assert!(trades_from_rows(&rows).is_empty());
    }

    #[test]
    fn wrap_unwrap_only_excluded() {
        // only WSOL changes, no other token touched
        let rows = vec![row("s1", WSOL_MINT, "0", "1000000000", 9, 1, 0, 1000)];
        assert!(trades_from_rows(&rows).is_empty());
    }

    #[test]
    fn dust_excluded() {
        let rows = vec![
            row("s1", "TOKEN", "0", "1", 6, 1, 0, 1000),
            row("s1", WSOL_MINT, "1000", "0", 9, 1, 0, 1000), // far below dust floor
        ];
        assert!(trades_from_rows(&rows).is_empty());
    }

    #[test]
    fn same_direction_deltas_excluded() {
        // both legs decrease together (e.g. liquidity add) -> not a swap
        let rows = vec![
            row("s1", "TOKEN", "100000000", "0", 6, 1, 0, 1000),
            row("s1", WSOL_MINT, "1000000000", "0", 9, 1, 0, 1000),
        ];
        assert!(trades_from_rows(&rows).is_empty());
    }

    #[test]
    fn candle_aggregates_open_high_low_close_volume() {
        let trades = vec![
            Trade { mint: "TOKEN".into(), block_time: 1000, slot: 1, tx_index: 0, price: 1.0, volume: 10.0 },
            Trade { mint: "TOKEN".into(), block_time: 1005, slot: 2, tx_index: 0, price: 1.5, volume: 5.0 },
            Trade { mint: "TOKEN".into(), block_time: 1015, slot: 3, tx_index: 0, price: 0.8, volume: 2.0 },
        ];
        let candles = build_candles(&trades, "1m", 60);
        assert_eq!(candles.len(), 1); // all within the same 60s bucket
        let c = &candles[0];
        assert_eq!(c.open, 1.0);
        assert_eq!(c.close, 0.8);
        assert_eq!(c.high, 1.5);
        assert_eq!(c.low, 0.8);
        assert_eq!(c.volume, 17.0);
    }

    #[test]
    fn candle_separates_buckets() {
        let trades = vec![
            Trade { mint: "TOKEN".into(), block_time: 0, slot: 1, tx_index: 0, price: 1.0, volume: 1.0 },
            Trade { mint: "TOKEN".into(), block_time: 65, slot: 2, tx_index: 0, price: 2.0, volume: 1.0 },
        ];
        let candles = build_candles(&trades, "1m", 60);
        assert_eq!(candles.len(), 2);
    }
}
