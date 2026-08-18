# ADR-0006: Restructure into a lib crate plus multiple binaries

Status: Accepted

## Context

Building the tx_index backfill tool (ADR-0007) needed the same RPC
client and rate limiter as the main ingestion binary. Everything lived
in `src/main.rs`'s private module tree (`mod db; mod pipeline; mod
rpc;`), which only that one binary can see — a second binary in
`src/bin/` can't reach modules declared privately inside another
binary's `main.rs`.

## Decision

Moved `db`, `pipeline`, and the new `contention` module into
`src/lib.rs` as `pub mod`s, making the package a library crate with
multiple binary targets: `src/main.rs` (ingestion), `src/bin/
backfill_order.rs` (one-off migration), `src/bin/contention_report.rs`
(analysis/reporting). All three depend on the same library crate and
reuse `RpcClient`, the rate limiter, and the DB helpers without
duplication.

Rejected duplicating the RPC/rate-limiting logic directly inside each
one-off binary: would mean re-implementing (and re-testing) retry and
backoff behavior in multiple places, with real risk of the copies
drifting apart.

## Consequences

Any code intended to be shared across binaries must live in the lib
crate as `pub`; anything binary-specific (like the ingestion pipeline's
`main` wiring, or a report tool's argument handling) stays in its own
`src/bin/*.rs` or `src/main.rs`. `contention_report` and
`backfill_order` are operational/analysis tools, not part of the
graded ingestion pipeline itself, but they reuse real, tested
production code (the same `RpcClient` retry/rate-limit path used by
the main binary) rather than throwaway scripts.
