use astralane::db;
use astralane::rpc::{BlockFetchOutcome, RpcClient};
use rusqlite::params;

const DB_PATH: &str = "astralane.db";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = db::open(DB_PATH)?;

    let rpc_url = std::env::var("RPC_URL").expect("RPC_URL environment variable must be set");
    let rpc = RpcClient::new(rpc_url);

    let mut slots_stmt = conn.prepare("SELECT slot FROM blocks WHERE skipped = 0 ORDER BY slot")?;
    let slots: Vec<i64> = slots_stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    drop(slots_stmt);

    println!("backfilling tx_index for {} blocks", slots.len());

    for (done, slot) in slots.iter().enumerate() {
        let outcome = rpc.get_block_signatures(*slot as u64).await;
        let signatures: Vec<String> = match outcome {
            BlockFetchOutcome::Fetched(v) => match v.get("signatures").and_then(|s| s.as_array()) {
                Some(arr) => arr
                    .iter()
                    .filter_map(|s| s.as_str().map(String::from))
                    .collect(),
                None => {
                    eprintln!("slot {slot}: no signatures field, skipping");
                    continue;
                }
            },
            BlockFetchOutcome::Skipped => continue,
            BlockFetchOutcome::Failed(msg) => {
                eprintln!("slot {slot}: fetch failed, skipping: {msg}");
                continue;
            }
        };

        let tx = conn.transaction()?;
        let mut update = tx.prepare_cached(
            "UPDATE transactions SET tx_index = ?1 WHERE signature = ?2 AND slot = ?3",
        )?;
        for (index, signature) in signatures.iter().enumerate() {
            update.execute(params![index as i64, signature, slot])?;
        }
        drop(update);
        tx.commit()?;

        if (done + 1) % 100 == 0 {
            println!("{}/{} blocks backfilled", done + 1, slots.len());
        }
    }

    println!("done");
    Ok(())
}
