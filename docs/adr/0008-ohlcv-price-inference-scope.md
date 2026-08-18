# ADR-0008: OHLCV price inference — WSOL-only, net-per-mint, and why yield is low

Status: Accepted

## Context

Part 3 requires inferring prices by matching an SPL token balance
change against "an opposing SOL or wrapped-SOL balance change" in the
same transaction. Two gaps surfaced while building this:

1. Native SOL (`preBalances`/`postBalances`, lamports) was never
   captured during ingestion — only token balances were. Recovering it
   now would mean a full, expensive re-fetch of every block (same cost
   as the original ~45-60 minute ingestion), not a cheap backfill.
2. On the real 1,000-slot range, of 211,584 transactions that touch
   both a WSOL leg and exactly one other mint, **211,390 net to zero**
   token change when summing deltas across all of that mint's accounts
   in the transaction — real WSOL movement, no net token change. Of
   the remaining 194, several are exact multiples of 1,000,000 units
   (e.g. exactly 1,000,000,000 tokens — pump.fun's standard initial
   supply), i.e. token-creation events with an incidental SOL fee, not
   trades.

## Decision

Scope price inference to wrapped-SOL legs only (native SOL excluded,
per gap 1). Match by **summing all balance deltas per mint within a
transaction** (not per individual account) and require the net to be
nonzero, opposite in sign to the net WSOL delta, above a dust floor
(0.0001 SOL), and not an exact multiple of 1,000,000 token units.

Rejected spending more time to track per-account-owner deltas (would
need capturing the `owner` field from `preTokenBalances`/
`postTokenBalances`, which the schema doesn't have — another
column-add-plus-backfill cycle) to distinguish "two of the trader's
own accounts netting to zero" from "two different parties' balances
that happen to sum to zero coincidentally." Given the scale of the
effect (99.9% of candidates net to zero), a per-owner refinement was
judged unlikely to be worth the additional time today versus just
documenting the limitation honestly.

## Consequences

Real yield on the 1,000-slot range is low: 68 total candles (1m + 5m
combined) across all indexed mints. This is not a bug — it reflects
that clean, simple two-leg WSOL/token swaps are genuinely rare in this
sample; most WSOL-adjacent activity is either internal multi-account
routing (vault patterns, intermediate transfers — nets to zero at the
token level despite real SOL movement) or token-creation events, not
spot trades. This should be reported plainly in FINDINGS.md rather
than presented as if the pipeline captured most real trading activity
— it's a genuine, disclosed limitation of balance-delta-only inference
without instruction decoding, consistent with the spec's own warning
that multi-hop and complex routing produce noisy or missing signal.
