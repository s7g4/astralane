use astralane::db;
use astralane::ohlcv::build_and_store_candles;

const DB_PATH: &str = "astralane.db";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = db::open(DB_PATH)?;
    let count = build_and_store_candles(&mut conn)?;
    println!("wrote {count} candles (1m + 5m combined)");
    Ok(())
}
