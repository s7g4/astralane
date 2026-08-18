CREATE TABLE IF NOT EXISTS account_locks (
    signature      TEXT NOT NULL REFERENCES transactions(signature),
    account_pubkey TEXT NOT NULL,
    is_writable    INTEGER NOT NULL,
    PRIMARY KEY (signature, account_pubkey)
);

CREATE INDEX IF NOT EXISTS idx_account_locks_pubkey
    ON account_locks(account_pubkey);
