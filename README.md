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

**Price inference**: for each transaction, sum token balance deltas
per mint (across however many accounts of that mint appear in the
transaction). If exactly one non-WSOL mint has a nonzero net delta,
and wrapped-SOL (`So11111111111111111111111111111111111111112`) also
has a nonzero net delta in the opposite direction, treat it as a trade:
`price = |SOL delta| / |token delta|` (SOL per token).

**Volume**: base-token amount moved (not SOL-denominated) — the same
delta already used for price, decimal-adjusted.

**Candles**: matched trades bucketed by `block_time` into 1-minute and
5-minute windows (`bucket_start = block_time - block_time % interval`),
standard first/max/min/last/sum aggregation.

**Exclusions** (see ADR-0008 for full reasoning):
- **Native SOL legs**: not captured during ingestion (only wrapped-SOL
  token balances were). Price inference is wrapped-SOL-only; native
  lamport-denominated legs are untracked.
- **Multi-hop routes**: more than one non-WSOL mint touched in the same
  transaction — excluded, no reliable way to attribute price across a
  route without instruction decoding.
- **SOL wrap/unwrap only**: zero non-WSOL mints touched — not a trade.
- **Internal routing that nets to zero**: the same mint appearing
  across multiple accounts in one transaction with deltas that cancel
  out — real WSOL movement, no net token change. This turned out to be
  the dominant case in the real data (see below).
- **Liquidity add/remove**: not explicitly detected (would need
  instruction decoding, out of scope), but requiring opposing-sign
  deltas incidentally rejects simple two-sided liquidity operations
  where both legs move the same direction.
- **Tokens with no SOL pair**: never produce a WSOL-paired group —
  untracked, not an error.
- **Dust**: WSOL-side amount below 0.0001 SOL excluded.
- **Round-number token amounts** (exact multiples of 1,000,000 units):
  treated as token-creation/mint events (e.g. pump.fun's standard 1B
  initial supply) co-occurring with an incidental SOL fee, not trades.

**Real yield is low, and that's a genuine finding, not a bug**: on the
full 1,000-slot range, only 66 candles (1m+5m combined) across 27
distinct mints were produced. Of 211,584 transactions that touch both
a WSOL leg and exactly one other mint, 211,390 (99.9%) net to zero
token change despite real WSOL movement — internal multi-account
routing, not simple two-leg swaps. Clean, directly-matchable WSOL/token
swaps are genuinely rare in real Solana transaction patterns; most
volume goes through more complex routing than balance-delta-only
inference (without instruction decoding) can reliably attribute.

## API

Served from the same binary as ingestion (Axum), alongside the static
dashboard (`dashboard/`, plain HTML/CSS/vanilla JS, candlestick chart
via a CDN-hosted `lightweight-charts` script tag — no build step).

- `GET /` — the dashboard
- `GET /api/contention?from=<slot>&to=<slot>` — precomputed contention
  metrics (schedule depth/width per block filtered to the range, plus
  range-wide top accounts/programs by conflict count). Precomputed
  because the underlying scheduling computation takes 11+ minutes over
  the full range — see FINDINGS.md — so it's built once by
  `build_contention_summary` into dedicated summary tables, not
  computed live per request.
- `GET /api/tokens` — indexed mints with activity counts, sorted so
  mints with real OHLCV candles come first (most active mints, like
  wrapped-SOL and USDC, are usually the counter-leg of a trade rather
  than something with its own candle — see OHLCV model above).
- `GET /api/ohlcv?mint=<pubkey>&interval=1m|5m` — candles for one token.

`RPC_URL` env var still applies for the ingestion side; the API/dashboard
work standalone against an already-populated `astralane.db` by setting
`SKIP_INGESTION=1` (useful for iterating on the dashboard without
re-running the rate-limited fetch).

`src/bin/starvation_experiment.rs` and the two `/api/experiment/*`
routes are not part of the required API surface above — they're the
measurement harness for the Part 5 async-starvation experiment (see
FINDINGS.md), kept in the repo as evidence rather than thrown away
after producing the numbers.

## Load experiment results

Full methodology and numbers in `FINDINGS.md`. Summary:

- **Backpressure** (policy: block, via bounded `tokio::mpsc` channels):
  paused the writer 10s mid-ingestion, sampled channel capacity every
  200ms. The writer's immediate upstream channel visibly filled during
  the pause; resumed cleanly afterward with identical row counts on a
  repeat run (idempotent, no duplication).
- **Async starvation**: the real contention-scheduling computation run
  naively (inline in an async handler) blocked `/api/tokens` for the
  entire ~69s computation (1 request got through, in 69.37s). Moved to
  `spawn_blocking`: 56 requests got through during a comparable
  computation, averaging 256ms.
- **Write-path contention**: 478 concurrent API reads sampled during a
  real 150-slot ingestion run, zero over 500ms and zero lock/busy
  errors — consistent with WAL allowing concurrent readers without
  blocking on the writer.
