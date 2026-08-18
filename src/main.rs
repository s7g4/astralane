use astralane::db;
use astralane::pipeline::{fetcher, parser, slot_walker, writer};
use astralane::rpc::RpcClient;
use std::sync::Arc;

const START_SLOT: u64 = 439865000;
const END_SLOT: u64 = 439865999;
const MAX_CONCURRENT_FETCHES: usize = 10;
const CHANNEL_BOUND: usize = 64;
// Raw fetched blocks can be 20+ MB each (jsonParsed + full tx details on
// busy blocks); keep this bound small so a slow writer can't cause dozens
// of them to pile up in memory waiting to be parsed.
const BLOCKS_CHANNEL_BOUND: usize = 4;
const DB_PATH: &str = "astralane.db";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_conn = db::open(DB_PATH)?;
    db::init_schema(&db_conn)?;

    let rpc_url = std::env::var("RPC_URL")
        .expect("RPC_URL environment variable must be set (see README) — no default, to avoid silently falling back to a rate-limited public endpoint");
    let rpc = Arc::new(RpcClient::new(rpc_url));

    let (slots_tx, slots_rx) = tokio::sync::mpsc::channel(CHANNEL_BOUND);
    let (blocks_tx, blocks_rx) = tokio::sync::mpsc::channel(BLOCKS_CHANNEL_BOUND);
    let (parsed_tx, parsed_rx) = tokio::sync::mpsc::channel(CHANNEL_BOUND);

    let walker = tokio::spawn(slot_walker::walk_slots(slots_tx, START_SLOT, END_SLOT));
    let fetch = tokio::spawn(fetcher::run(
        slots_rx,
        blocks_tx,
        rpc,
        MAX_CONCURRENT_FETCHES,
    ));
    let parse = tokio::spawn(parser::run(blocks_rx, parsed_tx));
    let write = tokio::task::spawn_blocking(move || writer::run_blocking(parsed_rx, db_conn));

    let _ = tokio::join!(walker, fetch, parse, write);
    Ok(())
}
