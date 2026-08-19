use crate::rpc::BlockFetchOutcome;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use tokio::sync::mpsc::{Receiver, Sender};

pub struct ParsedBlock {
    pub slot: u64,
    pub blockhash: Option<String>,
    pub block_time: Option<i64>,
    pub skipped: bool,
    pub transactions: Vec<ParsedTransaction>,
}

pub struct ParsedTransaction {
    pub signature: String,
    pub tx_index: i64,
    pub success: bool,
    pub fee: i64,
    pub program_ids: Vec<String>,
    pub account_locks: Vec<AccountLock>,
    pub token_balances: Vec<TokenBalanceRow>,
}

pub struct AccountLock {
    pub account_pubkey: String,
    pub is_writable: bool,
}

pub struct TokenBalanceRow {
    pub account_index: i64,
    pub mint: String,
    pub pre_amount: String,
    pub post_amount: String,
    pub decimals: i64,
}

pub async fn run(
    mut blocks_rx: Receiver<(u64, BlockFetchOutcome)>,
    parsed_tx: Sender<ParsedBlock>,
) {
    while let Some((slot, outcome)) = blocks_rx.recv().await {
        let parsed = match outcome {
            BlockFetchOutcome::Skipped => ParsedBlock {
                slot,
                blockhash: None,
                block_time: None,
                skipped: true,
                transactions: vec![],
            },
            BlockFetchOutcome::Failed(msg) => {
                eprintln!("slot {slot}: ingestion failed, skipping: {msg}");
                continue;
            }
            BlockFetchOutcome::Fetched(block) => match parse_block(slot, &block) {
                Some(parsed) => parsed,
                None => {
                    eprintln!("slot {slot}: unexpected block shape, skipping");
                    continue;
                }
            },
        };

        if parsed_tx.send(parsed).await.is_err() {
            break;
        }
    }
}

fn parse_block(slot: u64, block: &Value) -> Option<ParsedBlock> {
    let blockhash = block.get("blockhash")?.as_str()?.to_string();
    let block_time = block.get("blockTime").and_then(Value::as_i64);

    let transactions = block
        .get("transactions")?
        .as_array()?
        .iter()
        .enumerate()
        .filter_map(|(i, tx)| parse_transaction(i as i64, tx))
        .collect();

    Some(ParsedBlock {
        slot,
        blockhash: Some(blockhash),
        block_time,
        skipped: false,
        transactions,
    })
}

fn parse_transaction(tx_index: i64, tx: &Value) -> Option<ParsedTransaction> {
    let meta = tx.get("meta")?;
    let message = tx.get("transaction")?.get("message")?;

    let signature = tx
        .get("transaction")?
        .get("signatures")?
        .get(0)?
        .as_str()?
        .to_string();

    let success = meta.get("err")?.is_null();
    let fee = meta.get("fee")?.as_i64()?;

    let program_ids = message
        .get("instructions")?
        .as_array()?
        .iter()
        .filter_map(|ix| ix.get("programId")?.as_str().map(String::from))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    let account_locks = message
        .get("accountKeys")?
        .as_array()?
        .iter()
        .filter_map(|key| {
            Some(AccountLock {
                account_pubkey: key.get("pubkey")?.as_str()?.to_string(),
                is_writable: key.get("writable")?.as_bool()?,
            })
        })
        .collect();

    let token_balances = parse_token_balances(meta);

    Some(ParsedTransaction {
        signature,
        tx_index,
        success,
        fee,
        program_ids,
        account_locks,
        token_balances,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Mirrors the real shape seen from a live v0 transaction (slot 439865010,
    // signature 3jRBii2q...): jsonParsed + maxSupportedTransactionVersion=0
    // resolves lookup-table accounts directly into accountKeys, each with the
    // same pubkey/signer/writable fields as directly-included ones, just
    // source: "lookupTable" instead of "transaction". Our extraction doesn't
    // read `source` at all - this test proves that's fine, not an oversight,
    // by asserting lookup-table accounts come through with correct locks.
    fn v0_tx_with_alt_accounts() -> serde_json::Value {
        json!({
            "meta": {
                "err": null,
                "fee": 5000,
                "preTokenBalances": [],
                "postTokenBalances": []
            },
            "transaction": {
                "signatures": ["sig_v0_alt_test"],
                "message": {
                    "accountKeys": [
                        {"pubkey": "DirectSigner", "signer": true, "source": "transaction", "writable": true},
                        {"pubkey": "DirectReadonly", "signer": false, "source": "transaction", "writable": false},
                        {"pubkey": "LookupWritable", "signer": false, "source": "lookupTable", "writable": true},
                        {"pubkey": "LookupReadonly", "signer": false, "source": "lookupTable", "writable": false}
                    ],
                    "instructions": [
                        {"programId": "SomeProgram1111111111111111111111111111"}
                    ]
                }
            }
        })
    }

    #[test]
    fn v0_lookup_table_accounts_resolved_into_locks() {
        let tx = v0_tx_with_alt_accounts();
        let parsed = parse_transaction(0, &tx).expect("should parse");

        assert_eq!(parsed.signature, "sig_v0_alt_test");
        assert_eq!(parsed.account_locks.len(), 4);

        let find = |pubkey: &str| {
            parsed
                .account_locks
                .iter()
                .find(|l| l.account_pubkey == pubkey)
                .unwrap_or_else(|| panic!("missing lock for {pubkey}"))
        };

        // directly-included accounts
        assert!(find("DirectSigner").is_writable);
        assert!(!find("DirectReadonly").is_writable);

        // lookup-table-resolved accounts - same extraction, no special-casing
        assert!(find("LookupWritable").is_writable);
        assert!(!find("LookupReadonly").is_writable);
    }
}

struct BalanceEntry {
    mint: String,
    amount: String,
    decimals: i64,
}

fn collect_balances(balances: Option<&Value>) -> BTreeMap<i64, BalanceEntry> {
    balances
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let account_index = entry.get("accountIndex")?.as_i64()?;
            let mint = entry.get("mint")?.as_str()?.to_string();
            let token_amount = entry.get("uiTokenAmount")?;
            let amount = token_amount.get("amount")?.as_str()?.to_string();
            let decimals = token_amount.get("decimals")?.as_i64()?;
            Some((
                account_index,
                BalanceEntry {
                    mint,
                    amount,
                    decimals,
                },
            ))
        })
        .collect()
}

fn parse_token_balances(meta: &Value) -> Vec<TokenBalanceRow> {
    let pre = collect_balances(meta.get("preTokenBalances"));
    let post = collect_balances(meta.get("postTokenBalances"));

    let mut indices: BTreeSet<i64> = pre.keys().copied().collect();
    indices.extend(post.keys().copied());

    indices
        .into_iter()
        .filter_map(|idx| {
            let pre_entry = pre.get(&idx);
            let post_entry = post.get(&idx);
            let reference = post_entry.or(pre_entry)?;
            Some(TokenBalanceRow {
                account_index: idx,
                mint: reference.mint.clone(),
                pre_amount: pre_entry
                    .map(|e| e.amount.clone())
                    .unwrap_or_else(|| "0".into()),
                post_amount: post_entry
                    .map(|e| e.amount.clone())
                    .unwrap_or_else(|| "0".into()),
                decimals: reference.decimals,
            })
        })
        .collect()
}
