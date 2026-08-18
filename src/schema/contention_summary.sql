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
