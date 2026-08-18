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
