# astralane

Rust + Solana take-home: ingest a 1,000-slot mainnet range, analyze
account-lock contention, index token OHLCV candles, and serve both
through an HTTP API + dashboard — all from one binary.

## Setup

Requires Rust (edition 2024, tested on rustc 1.96) and SQLite (bundled
via `rusqlite`'s `bundled` feature, no separate install needed).

1. Get a Solana RPC endpoint. The public `api.mainnet-beta.solana.com`
   endpoint works for light testing but applies its own undocumented
   rate limiting under sustained load (see ADR-0002, ADR-0004) — a
   free-tier dedicated provider (e.g. [Helius](https://helius.dev)) is
   recommended for a full ingestion run.
2. Set the RPC URL as an environment variable — **not** a config file,
   since it may contain an API key:
   ```
   export RPC_URL="https://your-endpoint-here"
   ```
3. Build and test:
   ```
   cargo build --release
   cargo test
   ```
4. Run ingestion (populates `astralane.db` in the working directory):
   ```
   ./target/release/astralane
   ```

## Chosen slot range

**439865000–439865999** (1,000 consecutive slots), picked 2026-08-17
at ~10,049 slots behind the chain tip (slot 439875049 at pick time) —
comfortably finalized, an ordinary/unremarkable stretch of mainnet
activity, not chosen around any known network event.

## Architecture

One `tokio` runtime, one binary. Ingestion is a pipeline of stages
connected by bounded `tokio::sync::mpsc` channels:

```
slot_walker -> fetcher (rate-limited, concurrent) -> parser -> writer (SQLite)
```

See `docs/adr/` for the reasoning behind each major decision (RPC
access method, storage engine, RPC provider, retry placement, SQLite
transaction boundaries).

## Contention model — definitions and assumptions

**Conflict**: two transactions conflict on an account if either one
holds a *write* lock on it. Two read locks on the same account never
conflict.

**Step**: a set of transactions the scheduler considers safe to run in
parallel — none of them conflict with any other transaction in the
same step. A block's **schedule** is an ordered sequence of steps; the
**schedule depth** is the number of steps, and a step's **width** is
how many transactions it contains.

**Scheduling approach — heuristic, not exact.** Transactions are
processed in their original in-block order (`tx_index`, the position
they actually appeared in the block) and greedily assigned to the
earliest step where none of their account locks conflict with anything
already placed there. This is a list-scheduling heuristic, **not** the
validator's actual execution schedule — the RPC does not expose that,
and this reconstruction is an approximation based only on declared
account locks (`writable`/`signer` flags from `accountKeys`), not the
validator's real runtime decisions (which may also account for e.g.
compute budget, priority fees, or scheduling internals this heuristic
has no visibility into).

**Conflict attribution**: a conflict is counted once per
(transaction, account) rejection during scheduling — i.e., every time
the greedy algorithm tries to place a transaction into a step and
rejects it because of a specific account lock, that's one conflict,
attributed to that account and to the rejected transaction's
`program_ids`. This is schedule-derived (tied to what the heuristic
actually produced), not an independent pairwise count over all
transaction pairs in a block.

v0/address-lookup-table accounts are resolved by the RPC itself
(`jsonParsed` encoding + `maxSupportedTransactionVersion: 0`) — verified
directly against a real v0 transaction in the chosen range (33 total
`accountKeys`, 14 resolved via a lookup table, all with `writable`/
`signer` populated identically to directly-included accounts), so no
manual `getAddressLookupTable` fallback is needed.

## OHLCV model

_TODO — filled in once Part 3 is built._

## API

_TODO — filled in once Part 4 is built._

## Load experiment results

_TODO — see FINDINGS.md once Part 5 experiments are run._
