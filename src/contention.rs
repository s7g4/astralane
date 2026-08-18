use rusqlite::{Connection, params};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq)]
pub struct AccountLock {
    pub account_pubkey: String,
    pub is_writable: bool,
}

#[derive(Clone, Debug)]
pub struct Transaction {
    pub signature: String,
    pub tx_index: i64,
    pub program_ids: Vec<String>,
    pub locks: Vec<AccountLock>,
}

/// One rejection: a transaction couldn't go in a step because of this lock.
#[derive(Clone, Debug)]
pub struct ConflictEvent {
    pub account_pubkey: String,
    pub is_writable: bool,
    pub program_ids: Vec<String>,
}

pub struct Schedule {
    pub steps: Vec<Vec<String>>, // signatures, in placement order, per step
    pub conflicts: Vec<ConflictEvent>,
}

struct StepState {
    written: HashSet<String>,
    touched: HashSet<String>,
    signatures: Vec<String>,
}

impl StepState {
    fn new() -> Self {
        Self {
            written: HashSet::new(),
            touched: HashSet::new(),
            signatures: Vec::new(),
        }
    }

    fn conflicting_locks<'a>(&self, locks: &'a [AccountLock]) -> Vec<&'a AccountLock> {
        locks
            .iter()
            .filter(|lock| {
                if lock.is_writable {
                    self.touched.contains(&lock.account_pubkey)
                } else {
                    self.written.contains(&lock.account_pubkey)
                }
            })
            .collect()
    }

    fn add(&mut self, signature: &str, locks: &[AccountLock]) {
        for lock in locks {
            self.touched.insert(lock.account_pubkey.clone());
            if lock.is_writable {
                self.written.insert(lock.account_pubkey.clone());
            }
        }
        self.signatures.push(signature.to_string());
    }
}

/// Heuristic, not the validator's real schedule. Callers must pass
/// transactions in tx_index order.
pub fn schedule_block(transactions: &[Transaction]) -> Schedule {
    let mut steps: Vec<StepState> = Vec::new();
    let mut conflicts = Vec::new();

    for txn in transactions {
        let mut placed_step = None;

        for (i, step) in steps.iter().enumerate() {
            let conflicting = step.conflicting_locks(&txn.locks);
            if conflicting.is_empty() {
                placed_step = Some(i);
                break;
            }
            for lock in conflicting {
                conflicts.push(ConflictEvent {
                    account_pubkey: lock.account_pubkey.clone(),
                    is_writable: lock.is_writable,
                    program_ids: txn.program_ids.clone(),
                });
            }
        }

        match placed_step {
            Some(i) => steps[i].add(&txn.signature, &txn.locks),
            None => {
                let mut new_step = StepState::new();
                new_step.add(&txn.signature, &txn.locks);
                steps.push(new_step);
            }
        }
    }

    Schedule {
        steps: steps.into_iter().map(|s| s.signatures).collect(),
        conflicts,
    }
}

pub fn load_block_transactions(conn: &Connection, slot: i64) -> rusqlite::Result<Vec<Transaction>> {
    let mut tx_stmt = conn.prepare_cached(
        "SELECT signature, tx_index, program_ids FROM transactions WHERE slot = ?1 ORDER BY tx_index",
    )?;
    let mut transactions: Vec<Transaction> = tx_stmt
        .query_map(params![slot], |row| {
            let program_ids_json: String = row.get(2)?;
            Ok(Transaction {
                signature: row.get(0)?,
                tx_index: row.get(1)?,
                program_ids: serde_json::from_str(&program_ids_json).unwrap_or_default(),
                locks: Vec::new(),
            })
        })?
        .collect::<Result<_, _>>()?;

    let mut lock_stmt = conn.prepare_cached(
        "SELECT account_locks.signature, account_pubkey, is_writable
         FROM account_locks
         JOIN transactions ON transactions.signature = account_locks.signature
         WHERE transactions.slot = ?1",
    )?;
    let mut locks_by_sig: HashMap<String, Vec<AccountLock>> = HashMap::new();
    let rows = lock_stmt.query_map(params![slot], |row| {
        let sig: String = row.get(0)?;
        Ok((
            sig,
            AccountLock {
                account_pubkey: row.get(1)?,
                is_writable: row.get::<_, i64>(2)? != 0,
            },
        ))
    })?;
    for row in rows {
        let (sig, lock) = row?;
        locks_by_sig.entry(sig).or_default().push(lock);
    }

    for txn in &mut transactions {
        if let Some(locks) = locks_by_sig.remove(&txn.signature) {
            txn.locks = locks;
        }
    }

    Ok(transactions)
}

pub struct BlockReport {
    pub slot: i64,
    pub schedule_depth: usize,
    pub step_widths: Vec<usize>,
}

pub struct AccountConflictCount {
    pub account_pubkey: String,
    pub write_conflicts: u64,
    pub read_conflicts: u64,
}

pub struct ProgramConflictCount {
    pub program_id: String,
    pub write_conflicts: u64,
    pub read_conflicts: u64,
}

pub struct RangeReport {
    pub blocks: Vec<BlockReport>,
    pub accounts: Vec<AccountConflictCount>, // sorted by total conflicts, descending
    pub programs: Vec<ProgramConflictCount>, // sorted by total conflicts, descending
}

// Conflict counting: see ADR-0005 (schedule-derived, not pairwise).
pub fn build_range_report(conn: &Connection) -> rusqlite::Result<RangeReport> {
    let mut slot_stmt = conn.prepare("SELECT slot FROM blocks WHERE skipped = 0 ORDER BY slot")?;
    let slots: Vec<i64> = slot_stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    drop(slot_stmt);

    let mut blocks = Vec::new();
    let mut account_counts: HashMap<String, (u64, u64)> = HashMap::new(); // (write, read)
    let mut program_counts: HashMap<String, (u64, u64)> = HashMap::new();

    for slot in slots {
        let transactions = load_block_transactions(conn, slot)?;
        let schedule = schedule_block(&transactions);

        blocks.push(BlockReport {
            slot,
            schedule_depth: schedule.steps.len(),
            step_widths: schedule.steps.iter().map(|s| s.len()).collect(),
        });

        for event in &schedule.conflicts {
            let entry = account_counts
                .entry(event.account_pubkey.clone())
                .or_insert((0, 0));
            if event.is_writable {
                entry.0 += 1;
            } else {
                entry.1 += 1;
            }

            for program in &event.program_ids {
                let entry = program_counts.entry(program.clone()).or_insert((0, 0));
                if event.is_writable {
                    entry.0 += 1;
                } else {
                    entry.1 += 1;
                }
            }
        }
    }

    let mut accounts: Vec<AccountConflictCount> = account_counts
        .into_iter()
        .map(
            |(account_pubkey, (write_conflicts, read_conflicts))| AccountConflictCount {
                account_pubkey,
                write_conflicts,
                read_conflicts,
            },
        )
        .collect();
    accounts.sort_by_key(|a| std::cmp::Reverse(a.write_conflicts + a.read_conflicts));

    let mut programs: Vec<ProgramConflictCount> = program_counts
        .into_iter()
        .map(
            |(program_id, (write_conflicts, read_conflicts))| ProgramConflictCount {
                program_id,
                write_conflicts,
                read_conflicts,
            },
        )
        .collect();
    programs.sort_by_key(|p| std::cmp::Reverse(p.write_conflicts + p.read_conflicts));

    Ok(RangeReport {
        blocks,
        accounts,
        programs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tx(sig: &str, index: i64, locks: Vec<AccountLock>) -> Transaction {
        Transaction {
            signature: sig.to_string(),
            tx_index: index,
            program_ids: vec!["SomeProgram".to_string()],
            locks,
        }
    }

    fn read(account: &str) -> AccountLock {
        AccountLock {
            account_pubkey: account.to_string(),
            is_writable: false,
        }
    }

    fn write(account: &str) -> AccountLock {
        AccountLock {
            account_pubkey: account.to_string(),
            is_writable: true,
        }
    }

    #[test]
    fn read_read_no_conflict_same_step() {
        let txs = vec![
            tx("t1", 0, vec![read("A")]),
            tx("t2", 1, vec![read("A")]),
        ];
        let schedule = schedule_block(&txs);
        assert_eq!(schedule.steps.len(), 1);
        assert_eq!(schedule.steps[0], vec!["t1", "t2"]);
        assert!(schedule.conflicts.is_empty());
    }

    #[test]
    fn write_write_conflict_separate_steps() {
        let txs = vec![
            tx("t1", 0, vec![write("A")]),
            tx("t2", 1, vec![write("A")]),
        ];
        let schedule = schedule_block(&txs);
        assert_eq!(schedule.steps.len(), 2);
        assert_eq!(schedule.steps[0], vec!["t1"]);
        assert_eq!(schedule.steps[1], vec!["t2"]);
        assert_eq!(schedule.conflicts.len(), 1);
        assert_eq!(schedule.conflicts[0].account_pubkey, "A");
        assert!(schedule.conflicts[0].is_writable);
    }

    #[test]
    fn read_write_conflict_separate_steps() {
        let txs = vec![
            tx("t1", 0, vec![write("A")]),
            tx("t2", 1, vec![read("A")]),
        ];
        let schedule = schedule_block(&txs);
        assert_eq!(schedule.steps.len(), 2);
        assert_eq!(schedule.conflicts.len(), 1);
        assert_eq!(schedule.conflicts[0].account_pubkey, "A");
        assert!(!schedule.conflicts[0].is_writable);
    }

    #[test]
    fn independent_accounts_same_step() {
        let txs = vec![
            tx("t1", 0, vec![write("A")]),
            tx("t2", 1, vec![write("B")]),
        ];
        let schedule = schedule_block(&txs);
        assert_eq!(schedule.steps.len(), 1);
        assert_eq!(schedule.steps[0], vec!["t1", "t2"]);
    }
}
