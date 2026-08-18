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
