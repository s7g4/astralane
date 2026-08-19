use astralane::contention::{load_block_transactions, schedule_block};
use astralane::db;
use std::time::{Duration, Instant};

const DB_PATH: &str = "astralane.db";
const LIMIT: usize = 200;

// No perf/cargo-flamegraph available on this machine - this is the basic
// substitute: time the two phases of build_range_report separately (DB load
// vs the scheduling computation itself) to see where the 11+ minutes on the
// full range actually goes.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let conn = db::open_read_only(DB_PATH)?;

    let mut slot_stmt = conn.prepare("SELECT slot FROM blocks WHERE skipped = 0 ORDER BY slot LIMIT ?1")?;
    let slots: Vec<i64> = slot_stmt
        .query_map([LIMIT as i64], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    drop(slot_stmt);

    let mut load_time = Duration::ZERO;
    let mut schedule_time = Duration::ZERO;
    let mut total_tx = 0usize;
    let mut total_steps = 0usize;

    for slot in &slots {
        let t0 = Instant::now();
        let transactions = load_block_transactions(&conn, *slot)?;
        load_time += t0.elapsed();
        total_tx += transactions.len();

        let t1 = Instant::now();
        let schedule = schedule_block(&transactions);
        schedule_time += t1.elapsed();
        total_steps += schedule.steps.len();
    }

    let total = load_time + schedule_time;
    println!("blocks: {}", slots.len());
    println!("total transactions: {total_tx}");
    println!("total steps produced: {total_steps}");
    println!(
        "DB load time:  {load_time:?} ({:.1}%)",
        load_time.as_secs_f64() / total.as_secs_f64() * 100.0
    );
    println!(
        "schedule time: {schedule_time:?} ({:.1}%)",
        schedule_time.as_secs_f64() / total.as_secs_f64() * 100.0
    );
    println!("total: {total:?}");
    println!(
        "avg per block: {:?}",
        total / slots.len().max(1) as u32
    );

    Ok(())
}
