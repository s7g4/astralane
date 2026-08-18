use crate::pipeline::parser::ParsedBlock;
use rusqlite::{Connection, params};
use tokio::sync::mpsc::Receiver;

pub fn run_blocking(mut parsed_rx: Receiver<ParsedBlock>, mut conn: Connection) {
    while let Some(block) = parsed_rx.blocking_recv() {
        if let Err(e) = write_block(&mut conn, &block) {
            eprintln!("slot {}: write failed: {e}", block.slot);
        }
    }
}

fn write_block(conn: &mut Connection, block: &ParsedBlock) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;

    tx.prepare_cached(
        "INSERT OR IGNORE INTO blocks (slot, blockhash, block_time, skipped) VALUES (?1, ?2, ?3, ?4)",
    )?
    .execute(params![block.slot as i64, block.blockhash, block.block_time, block.skipped])?;

    let mut insert_tx = tx.prepare_cached(
        "INSERT OR IGNORE INTO transactions (signature, slot, tx_index, success, fee, program_ids) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    let mut insert_lock = tx.prepare_cached(
        "INSERT OR IGNORE INTO account_locks (signature, account_pubkey, is_writable) VALUES (?1, ?2, ?3)",
    )?;
    let mut insert_balance = tx.prepare_cached(
        "INSERT OR IGNORE INTO token_balances
         (signature, account_index, mint, pre_amount, post_amount, decimals)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;

    for txn in &block.transactions {
        let program_ids_json =
            serde_json::to_string(&txn.program_ids).unwrap_or_else(|_| "[]".to_string());

        insert_tx.execute(params![
            txn.signature,
            block.slot as i64,
            txn.tx_index,
            txn.success,
            txn.fee,
            program_ids_json
        ])?;

        for lock in &txn.account_locks {
            insert_lock.execute(params![txn.signature, lock.account_pubkey, lock.is_writable])?;
        }

        for bal in &txn.token_balances {
            insert_balance.execute(params![
                txn.signature,
                bal.account_index,
                bal.mint,
                bal.pre_amount,
                bal.post_amount,
                bal.decimals
            ])?;
        }
    }

    drop(insert_tx);
    drop(insert_lock);
    drop(insert_balance);
    tx.commit()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::parser::{AccountLock, ParsedTransaction, TokenBalanceRow};

    fn sample_block() -> ParsedBlock {
        ParsedBlock {
            slot: 1,
            blockhash: Some("testhash".to_string()),
            block_time: Some(1_700_000_000),
            skipped: false,
            transactions: vec![ParsedTransaction {
                signature: "sig1".to_string(),
                tx_index: 0,
                success: true,
                fee: 5000,
                program_ids: vec!["Program1".to_string()],
                account_locks: vec![
                    AccountLock {
                        account_pubkey: "acct1".to_string(),
                        is_writable: true,
                    },
                    AccountLock {
                        account_pubkey: "acct2".to_string(),
                        is_writable: false,
                    },
                ],
                token_balances: vec![TokenBalanceRow {
                    account_index: 0,
                    mint: "mint1".to_string(),
                    pre_amount: "100".to_string(),
                    post_amount: "50".to_string(),
                    decimals: 6,
                }],
            }],
        }
    }

    fn row_counts(conn: &Connection) -> (i64, i64, i64, i64) {
        let blocks: i64 = conn
            .query_row("SELECT count(*) FROM blocks", [], |r| r.get(0))
            .unwrap();
        let transactions: i64 = conn
            .query_row("SELECT count(*) FROM transactions", [], |r| r.get(0))
            .unwrap();
        let account_locks: i64 = conn
            .query_row("SELECT count(*) FROM account_locks", [], |r| r.get(0))
            .unwrap();
        let token_balances: i64 = conn
            .query_row("SELECT count(*) FROM token_balances", [], |r| r.get(0))
            .unwrap();
        (blocks, transactions, account_locks, token_balances)
    }

    #[test]
    fn write_block_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::init_schema(&conn).unwrap();

        let block = sample_block();

        write_block(&mut conn, &block).unwrap();
        let first = row_counts(&conn);

        write_block(&mut conn, &block).unwrap();
        let second = row_counts(&conn);

        assert_eq!(first, second);
        assert_eq!(first, (1, 1, 2, 1));
    }
}
