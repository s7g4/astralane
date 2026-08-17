use crate::rpc::BlockFetchOutcome;
use tokio::sync::mpsc::Receiver;

pub async fn run(mut parsed_rx: Receiver<(u64, BlockFetchOutcome)>) {
    while let Some((slot, outcome)) = parsed_rx.recv().await {
        match outcome {
            BlockFetchOutcome::Skipped => println!("slot {slot}: skipped"),
            BlockFetchOutcome::Fetched(_) => println!("slot {slot}: fetched"),
            BlockFetchOutcome::Failed(msg) => println!("slot {slot}: failed ({msg})"),
        }
    }
}
