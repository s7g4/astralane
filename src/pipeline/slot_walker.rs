use tokio::sync::mpsc::Sender;

pub async fn walk_slots(tx: Sender<u64>, start: u64, end: u64) {
    for slot in start..=end {
        if tx.send(slot).await.is_err() {
            break;
        }
    }
}
