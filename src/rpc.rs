use governor::clock::DefaultClock;
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::num::NonZeroU32;
use std::time::Duration;

type Limiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock>;

const MAX_RETRIES: u32 = 5;
const BASE_DELAY_MS: u64 = 200;
const MAX_DELAY_MS: u64 = 5_000;

#[derive(Serialize)]
struct RpcRequest<'a, T: Serialize> {
    jsonrpc: &'a str,
    id: u64,
    method: &'a str,
    params: T,
}

#[derive(Deserialize)]
struct RpcResponse<T> {
    result: Option<T>,
    error: Option<RpcError>,
}

#[derive(Deserialize)]
struct RpcError {
    #[allow(dead_code)]
    code: i64,
    message: String,
}

pub enum BlockFetchOutcome {
    Skipped,
    Fetched(Value),
    Failed(String),
}

enum CallOutcome {
    Skipped,
    Ok(Value),
    Transient(String),
    Permanent(String),
}

pub struct RpcClient {
    http: reqwest::Client,
    limiter: Limiter,
    url: String,
}

impl RpcClient {
    pub fn new(url: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("failed to build reqwest client");
        let quota = Quota::per_second(NonZeroU32::new(10).unwrap());
        Self {
            http,
            limiter: RateLimiter::direct(quota),
            url,
        }
    }

    pub async fn get_block(&self, slot: u64) -> BlockFetchOutcome {
        let mut attempt = 0;
        loop {
            self.limiter.until_ready().await;
            match self.try_get_block(slot).await {
                CallOutcome::Skipped => return BlockFetchOutcome::Skipped,
                CallOutcome::Ok(v) => return BlockFetchOutcome::Fetched(v),
                CallOutcome::Permanent(msg) => return BlockFetchOutcome::Failed(msg),
                CallOutcome::Transient(msg) => {
                    if attempt >= MAX_RETRIES {
                        return BlockFetchOutcome::Failed(format!(
                            "gave up after {attempt} retries: {msg}"
                        ));
                    }
                    tokio::time::sleep(backoff_with_jitter(attempt)).await;
                    attempt += 1;
                }
            }
        }
    }

    async fn try_get_block(&self, slot: u64) -> CallOutcome {
        let body = RpcRequest {
            jsonrpc: "2.0",
            id: 1,
            method: "getBlock",
            params: json!([
                slot,
                {
                    "encoding": "jsonParsed",
                    "maxSupportedTransactionVersion": 0,
                    "transactionDetails": "full",
                    "rewards": false
                }
            ]),
        };

        let resp = match self.http.post(&self.url).json(&body).send().await {
            Ok(r) => r,
            Err(e) => return CallOutcome::Transient(e.to_string()),
        };

        let status = resp.status();
        if status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return CallOutcome::Transient(format!("http status {status}"));
        }
        if !status.is_success() {
            return CallOutcome::Permanent(format!("http status {status}"));
        }

        let parsed: RpcResponse<Value> = match resp.json().await {
            Ok(p) => p,
            Err(e) => return CallOutcome::Permanent(format!("bad json: {e}")),
        };

        if let Some(err) = parsed.error {
            if err.message.to_lowercase().contains("skipped") {
                return CallOutcome::Skipped;
            }
            return CallOutcome::Transient(err.message);
        }

        match parsed.result {
            Some(v) => CallOutcome::Ok(v),
            None => CallOutcome::Skipped,
        }
    }
}

fn backoff_with_jitter(attempt: u32) -> Duration {
    let exp = BASE_DELAY_MS.saturating_mul(1u64 << attempt.min(10));
    let capped = exp.min(MAX_DELAY_MS);
    let jittered = rand::random::<u64>() % (capped + 1);
    Duration::from_millis(jittered)
}
