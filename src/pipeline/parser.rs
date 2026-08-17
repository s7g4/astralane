use crate::rpc::BlockFetchOutcome;
use tokio::sync::mpsc::{Receiver, Sender};

pub async fn run(
    mut blocks_rx: Receiver<(u64, BlockFetchOutcome)>,
    parsed_tx: Sender<(u64, BlockFetchOutcome)>,
) {
    while let Some(item) = blocks_rx.recv().await {
        if parsed_tx.send(item).await.is_err() {
            break;
        }
    }
}
