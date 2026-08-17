use crate::rpc::{BlockFetchOutcome, RpcClient};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinSet;

pub async fn run(
    mut slots_rx: Receiver<u64>,
    blocks_tx: Sender<(u64, BlockFetchOutcome)>,
    rpc: Arc<RpcClient>,
    max_concurrent: usize,
) {
    let semaphore = Arc::new(Semaphore::new(max_concurrent));
    let mut tasks = JoinSet::new();

    while let Some(slot) = slots_rx.recv().await {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let rpc = rpc.clone();
        let blocks_tx = blocks_tx.clone();

        tasks.spawn(async move {
            let outcome = rpc.get_block(slot).await;
            let _ = blocks_tx.send((slot, outcome)).await;
            drop(permit);
        });
    }

    while tasks.join_next().await.is_some() {}
}
