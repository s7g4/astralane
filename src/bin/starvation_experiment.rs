use astralane::api::{self, AppState};
use std::sync::Arc;

const DB_PATH: &str = "astralane.db";
const ADDR: &str = "127.0.0.1:3099";

// Single worker thread on purpose: this is what makes the naive-vs-
// spawn_blocking difference obvious. With many cores (this dev machine has
// 12), one blocked thread still leaves others free to serve other
// requests; with a single worker thread, a blocking call on it starves
// everything else on this runtime - which is the actual point of the
// experiment (and a realistic constrained-deployment scenario, e.g. a
// small container).
#[tokio::main(flavor = "multi_thread", worker_threads = 1)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let state = AppState {
        db_path: Arc::new(DB_PATH.to_string()),
    };
    let listener = tokio::net::TcpListener::bind(ADDR).await?;
    println!("starvation experiment server on http://{ADDR} (1 worker thread)");
    axum::serve(listener, api::router(state)).await?;
    Ok(())
}
