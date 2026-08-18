mod db;
mod pipeline;
mod rpc;

use pipeline::{fetcher, parser, slot_walker, writer};
use rpc::RpcClient;
use std::sync::Arc;

const RPC_URL: &str = "https://api.mainnet-beta.solana.com";
const START_SLOT: u64 = 439865000;
const END_SLOT: u64 = 439865999;
const MAX_CONCURRENT_FETCHES: usize = 10;
const CHANNEL_BOUND: usize = 64;
const DB_PATH: &str = "astralane.db";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_conn = db::open(DB_PATH)?;
    db::init_schema(&db_conn)?;

    let rpc = Arc::new(RpcClient::new(RPC_URL.to_string()));

    let (slots_tx, slots_rx) = tokio::sync::mpsc::channel(CHANNEL_BOUND);
    let (blocks_tx, blocks_rx) = tokio::sync::mpsc::channel(CHANNEL_BOUND);
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
