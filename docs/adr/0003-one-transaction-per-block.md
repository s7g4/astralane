# ADR-0003: One SQLite transaction per block in the writer

Status: Accepted

## Context

The writer needs a transaction/batch boundary for committing ingested
data. Three obvious options: commit after every row (INSERT), commit
once for the entire 1,000-slot run, or commit once per block. Part 5
of the assignment explicitly wants this choice justified and later
measured under write-path contention.

## Decision

Commit once per block: all of a block's rows (the block row, its
transactions, account_locks, token_balances) are written inside a
single rusqlite `Connection::transaction()`, committed together via
`tx.commit()`.

Rejected per-row commits: every `INSERT` would force its own fsync,
which is expensive at the scale of a full range (tens of thousands of
rows per block). Rejected one commit for the whole run: a crash or
early exit partway through would either lose everything not yet
committed, or (with a single very long-lived transaction) hold a
write lock for the entire run's duration, which conflicts with
wanting concurrent API reads to stay responsive during ingestion.

## Consequences

A crash mid-run loses at most the one block being written when it
happened — every other already-committed block is intact, and the
crashed block gets safely re-fetched and re-written on the next run
thanks to INSERT OR IGNORE idempotency. This is a starting default,
not a tuned one: Part 5's write-path contention experiment will
actually measure this against alternatives (e.g. batching N blocks
per transaction) with real throughput numbers, and may supersede this
ADR if a different granularity turns out to perform meaningfully
better.
