use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::services::ServeDir;

#[derive(Clone)]
pub struct AppState {
    pub db_path: Arc<String>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/contention", get(get_contention))
        .route("/api/tokens", get(get_tokens))
        .route("/api/ohlcv", get(get_ohlcv))
        .nest_service("/", ServeDir::new("dashboard"))
        .with_state(state)
}

async fn spawn_db<F, T>(f: F) -> Result<T, (StatusCode, String)>
where
    F: FnOnce() -> rusqlite::Result<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

#[derive(Deserialize)]
struct ContentionQuery {
    from: Option<i64>,
    to: Option<i64>,
}

#[derive(Serialize)]
struct BlockDepth {
    slot: i64,
    schedule_depth: i64,
    step_widths: Vec<i64>,
}

#[derive(Serialize)]
struct ConflictRow {
    key: String,
    write_conflicts: i64,
    read_conflicts: i64,
}

#[derive(Serialize)]
struct ContentionResponse {
    block_count: i64,
    blocks: Vec<BlockDepth>,
    top_accounts: Vec<ConflictRow>,
    top_programs: Vec<ConflictRow>,
}

async fn get_contention(
    State(state): State<AppState>,
    Query(q): Query<ContentionQuery>,
) -> Result<Json<ContentionResponse>, (StatusCode, String)> {
    let db_path = state.db_path.clone();
    let from = q.from.unwrap_or(i64::MIN);
    let to = q.to.unwrap_or(i64::MAX);

    let response = spawn_db(move || -> rusqlite::Result<ContentionResponse> {
        let conn = crate::db::open_read_only(db_path.as_str())?;

        let mut block_stmt = conn.prepare(
            "SELECT slot, schedule_depth, step_widths FROM contention_blocks
             WHERE slot BETWEEN ?1 AND ?2 ORDER BY slot",
        )?;
        let blocks: Vec<BlockDepth> = block_stmt
            .query_map(params![from, to], |row| {
                let widths_json: String = row.get(2)?;
                let step_widths: Vec<i64> = serde_json::from_str(&widths_json).unwrap_or_default();
                Ok(BlockDepth {
                    slot: row.get(0)?,
                    schedule_depth: row.get(1)?,
                    step_widths,
                })
            })?
            .collect::<Result<_, _>>()?;

        let mut acc_stmt = conn.prepare(
            "SELECT account_pubkey, write_conflicts, read_conflicts FROM contention_accounts
             ORDER BY (write_conflicts + read_conflicts) DESC LIMIT 20",
        )?;
        let top_accounts: Vec<ConflictRow> = acc_stmt
            .query_map([], |row| {
                Ok(ConflictRow {
                    key: row.get(0)?,
                    write_conflicts: row.get(1)?,
                    read_conflicts: row.get(2)?,
                })
            })?
            .collect::<Result<_, _>>()?;

        let mut prog_stmt = conn.prepare(
            "SELECT program_id, write_conflicts, read_conflicts FROM contention_programs
             ORDER BY (write_conflicts + read_conflicts) DESC LIMIT 20",
        )?;
        let top_programs: Vec<ConflictRow> = prog_stmt
            .query_map([], |row| {
                Ok(ConflictRow {
                    key: row.get(0)?,
                    write_conflicts: row.get(1)?,
                    read_conflicts: row.get(2)?,
                })
            })?
            .collect::<Result<_, _>>()?;

        Ok(ContentionResponse {
            block_count: blocks.len() as i64,
            blocks,
            top_accounts,
            top_programs,
        })
    })
    .await?;

    Ok(Json(response))
}

#[derive(Serialize)]
struct TokenRow {
    mint: String,
    activity_count: i64,
    has_candles: bool,
}

async fn get_tokens(
    State(state): State<AppState>,
) -> Result<Json<Vec<TokenRow>>, (StatusCode, String)> {
    let db_path = state.db_path.clone();
    let rows = spawn_db(move || -> rusqlite::Result<Vec<TokenRow>> {
        let conn = crate::db::open_read_only(db_path.as_str())?;
        // Tokens with real OHLCV candles sort first - most mints touched in a
        // transaction are counter-legs (WSOL, USDC) or otherwise never
        // produce a matched trade (see ADR-0008), so defaulting to
        // "most active" alone tends to pick a mint with an empty chart.
        let mut stmt = conn.prepare(
            "SELECT tb.mint, count(*) AS activity_count,
                    EXISTS(SELECT 1 FROM ohlcv_candles oc WHERE oc.mint = tb.mint) AS has_candles
             FROM token_balances tb
             GROUP BY tb.mint
             ORDER BY has_candles DESC, activity_count DESC",
        )?;
        stmt.query_map([], |row| {
            Ok(TokenRow {
                mint: row.get(0)?,
                activity_count: row.get(1)?,
                has_candles: row.get(2)?,
            })
        })?
        .collect()
    })
    .await?;

    Ok(Json(rows))
}

#[derive(Deserialize)]
struct OhlcvQuery {
    mint: String,
    interval: String,
}

#[derive(Serialize)]
struct CandleRow {
    bucket_start: i64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
}

#[derive(Serialize)]
struct OhlcvResponse {
    mint: String,
    interval: String,
    candles: Vec<CandleRow>,
}

async fn get_ohlcv(
    State(state): State<AppState>,
    Query(q): Query<OhlcvQuery>,
) -> Result<Json<OhlcvResponse>, (StatusCode, String)> {
    if q.interval != "1m" && q.interval != "5m" {
        return Err((StatusCode::BAD_REQUEST, "interval must be 1m or 5m".into()));
    }

    let db_path = state.db_path.clone();
    let mint = q.mint.clone();
    let interval = q.interval.clone();

    let candles = spawn_db(move || -> rusqlite::Result<Vec<CandleRow>> {
        let conn = crate::db::open_read_only(db_path.as_str())?;
        let mut stmt = conn.prepare(
            "SELECT bucket_start, open, high, low, close, volume FROM ohlcv_candles
             WHERE mint = ?1 AND interval = ?2 ORDER BY bucket_start",
        )?;
        stmt.query_map(params![mint, interval], |row| {
            Ok(CandleRow {
                bucket_start: row.get(0)?,
                open: row.get(1)?,
                high: row.get(2)?,
                low: row.get(3)?,
                close: row.get(4)?,
                volume: row.get(5)?,
            })
        })?
        .collect()
    })
    .await?;

    Ok(Json(OhlcvResponse {
        mint: q.mint,
        interval: q.interval,
        candles,
    }))
}
