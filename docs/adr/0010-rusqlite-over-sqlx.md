# ADR-0010: rusqlite over sqlx

Status: Accepted

## Context

The original plan left storage-library choice open: "SQLite via
rusqlite (or sqlx if async pool ergonomics are preferred)." Both are
viable; needed to actually pick one and justify it rather than leave
it implicit.

sqlx is an async-native SQL toolkit with compile-time-checked queries
and connection pooling built in. For a true network-based database
(Postgres, MySQL) its async story reflects real async I/O over a
socket. For SQLite specifically, that story is different: SQLite is a
local, synchronous, file-based engine with no async I/O of its own —
sqlx's SQLite driver gets "async" by running the same synchronous C
library calls on a background thread pool internally. That's
conceptually the same thing this project does manually with
`rusqlite` + `tokio::task::spawn_blocking` in `writer.rs` and the API
handlers (`spawn_db` in `api.rs`).

## Decision

`rusqlite`, called from async code via explicit `spawn_blocking` at
every write and read call site, rather than `sqlx` with its built-in
pooling and implicit thread-offloading.

Rejected `sqlx`: for SQLite, it buys less than it would for a real
network database (no genuine async I/O underneath either way), at the
cost of a much heavier dependency tree (compile-time query-checking
macros, driver crates, its own runtime abstraction) — working against
the "keep the dependency list lean" principle held throughout this
project. More importantly: doing the blocking-isolation ourselves,
explicitly, is the same skill Part 5's async-starvation experiment is
built to demonstrate understanding of (see `FINDINGS.md`). If `sqlx`
handled that transparently, there'd be nothing to reason about or
show in that experiment.

## Consequences

Every call site that touches the database from async code — the
writer, every API handler — has to remember to wrap it in
`spawn_blocking` (or `blocking_recv`/a dedicated blocking thread, for
the writer specifically) by hand; nothing enforces this at compile
time the way it would be structurally harder to get wrong with a
fully async driver. In exchange, it's explicit and inspectable exactly
where blocking happens, rather than hidden inside a library.
