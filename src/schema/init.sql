-- init.sql: schema for astralane ingestion + analysis DB.
-- Idempotent: safe to run against an existing database.

PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS blocks (
    slot        INTEGER PRIMARY KEY,
    blockhash   TEXT,
    block_time  INTEGER,
    skipped     INTEGER NOT NULL DEFAULT 0  -- 0/1
);

CREATE TABLE IF NOT EXISTS transactions (
    signature    TEXT PRIMARY KEY,
    slot         INTEGER NOT NULL REFERENCES blocks(slot),
    tx_index     INTEGER NOT NULL DEFAULT -1, -- position within the block; -1 = unknown
    success      INTEGER NOT NULL,
    fee          INTEGER NOT NULL,
    program_ids  TEXT NOT NULL              -- JSON array
);

CREATE INDEX IF NOT EXISTS idx_transactions_slot
    ON transactions(slot);

CREATE TABLE IF NOT EXISTS account_locks (
    signature      TEXT NOT NULL REFERENCES transactions(signature),
    account_pubkey TEXT NOT NULL,
    is_writable    INTEGER NOT NULL,
    PRIMARY KEY (signature, account_pubkey)
);

CREATE INDEX IF NOT EXISTS idx_account_locks_pubkey
    ON account_locks(account_pubkey);

CREATE TABLE IF NOT EXISTS token_balances (
    signature      TEXT NOT NULL REFERENCES transactions(signature),
    account_index  INTEGER NOT NULL,
    mint           TEXT NOT NULL,
    pre_amount     TEXT NOT NULL,           -- raw u64 amount as string (avoids f64 precision loss)
    post_amount    TEXT NOT NULL,
    decimals       INTEGER NOT NULL,
    PRIMARY KEY (signature, account_index)
);

CREATE INDEX IF NOT EXISTS idx_token_balances_mint
    ON token_balances(mint);

CREATE TABLE IF NOT EXISTS ohlcv_candles (
    mint         TEXT NOT NULL,
    interval     TEXT NOT NULL CHECK (interval IN ('1m', '5m')),
    bucket_start INTEGER NOT NULL,
    open         REAL NOT NULL,
    high         REAL NOT NULL,
    low          REAL NOT NULL,
    close        REAL NOT NULL,
    volume       REAL NOT NULL,
    PRIMARY KEY (mint, interval, bucket_start)
);

-- Precomputed contention results (the schedule computation itself takes
-- minutes over the full range, so the API serves these instead of
-- recomputing per request).
CREATE TABLE IF NOT EXISTS contention_blocks (
    slot           INTEGER PRIMARY KEY REFERENCES blocks(slot),
    schedule_depth INTEGER NOT NULL,
    step_widths    TEXT NOT NULL -- JSON array
);

CREATE TABLE IF NOT EXISTS contention_accounts (
    account_pubkey  TEXT PRIMARY KEY,
    write_conflicts INTEGER NOT NULL,
    read_conflicts  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS contention_programs (
    program_id      TEXT PRIMARY KEY,
    write_conflicts INTEGER NOT NULL,
    read_conflicts  INTEGER NOT NULL
);
