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
