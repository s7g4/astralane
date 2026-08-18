# ADR-0007: Backfill transaction order via a lightweight signatures-only fetch

Status: Accepted

## Context

The greedy scheduler requires each transaction's original position
within its block (`tx_index`), which the original schema didn't
capture — `transactions` only had `signature`/`slot`. The full
1,000-slot range was already ingested by the time this gap was found.

## Decision

Added a `tx_index` column and a separate one-off binary
(`backfill_order`) that re-fetches only the ordered signature list per
block — `getBlock` with `transactionDetails: "signatures"` instead of
the full `jsonParsed` + `transactionDetails: "full"` used by real
ingestion — then `UPDATE`s `tx_index` by matching signatures. This is
a much smaller payload per request (an array of strings vs. full
parsed transaction data), so backfilling all 1,000 blocks costs
roughly 1,000 lightweight requests at the same 10 req/s cap, not a
full re-fetch of every block's complete data.

Rejected a full re-ingestion of the range: correct, but re-fetching
gigabytes of already-correctly-ingested data just to add one integer
column is wasteful, and would cost the same wall-clock time as the
original ~45-60 minute run (dominated by the same rate cap and payload
size either way).

## Consequences

`parser.rs`/`writer.rs` now capture `tx_index` correctly on any future
run, so this backfill is a one-time fix for data ingested before the
gap was caught, not a permanent extra step. Like the initial writer
fix (see the writer prepared-statement-caching commit), the backfill
tool batches its `UPDATE`s in one SQLite transaction per block rather
than one per row, for the same reason: avoiding the write-throughput
bottleneck that caused the earlier crash.
