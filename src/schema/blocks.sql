CREATE TABLE IF NOT EXISTS blocks (
    slot        INTEGER PRIMARY KEY,
    blockhash   TEXT,
    block_time  INTEGER,
    skipped     INTEGER NOT NULL DEFAULT 0  -- 0/1
);
