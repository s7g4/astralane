use rusqlite::{Connection, OpenFlags};
use std::path::Path;

const SCHEMA: &str = include_str!("schema/init.sql");

/// Opens a read-write connection with per-connection pragmas applied.
pub fn open(path: impl AsRef<Path>) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "foreign_keys", true)?;
    Ok(conn)
}

/// Opens a read-only connection, for the API layer.
pub fn open_read_only(path: impl AsRef<Path>) -> rusqlite::Result<Connection> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )?;
    conn.pragma_update(None, "foreign_keys", true)?;
    Ok(conn)
}

/// Creates all tables/indexes if they don't already exist. Call once at
/// startup, before ingestion or the API server start.
pub fn init_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(SCHEMA)
}
