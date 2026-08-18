use rusqlite::{Connection, OpenFlags};
use std::path::Path;

// static, not const: this is embedded file data (one fixed location in the
// binary), not a compile-time value meant to be substituted at each use site.
static SCHEMA_FILES: &[&str] = &[
    include_str!("schema/pragmas.sql"),
    include_str!("schema/blocks.sql"),
    include_str!("schema/transactions.sql"),
    include_str!("schema/account_locks.sql"),
    include_str!("schema/token_balances.sql"),
    include_str!("schema/ohlcv_candles.sql"),
    include_str!("schema/contention_summary.sql"),
];

pub fn open(path: impl AsRef<Path>) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "foreign_keys", true)?;
    Ok(conn)
}

pub fn open_read_only(path: impl AsRef<Path>) -> rusqlite::Result<Connection> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )?;
    conn.pragma_update(None, "foreign_keys", true)?;
    Ok(conn)
}

pub fn init_schema(conn: &Connection) -> rusqlite::Result<()> {
    for file in SCHEMA_FILES {
        conn.execute_batch(file)?;
    }
    Ok(())
}
