# ADR-0002: Use the Solana Labs public mainnet-beta RPC endpoint

Status: Superseded by ADR-0004

## Context

The assignment allows "any public or free-tier Solana RPC, with a
self-imposed cap of 10 requests/second." Two real options: the Solana
Labs public endpoint (api.mainnet-beta.solana.com), zero setup; or a
dedicated free-tier provider (e.g. Helius), which requires signup but
gives a private, predictable rate ceiling.

The public endpoint is explicitly documented by Solana as not intended
for production/sustained use and applies its own undocumented rate
limiting on top of whatever we do ourselves. That's a real risk for this
project specifically, since Part 5's experiments (backpressure, write-
path contention, throughput/latency numbers for FINDINGS.md) are
supposed to measure the effect of _our own_ rate limiter and pipeline
design — not get contaminated by an unrelated upstream throttle.

## Decision

Use the public endpoint (https://api.mainnet-beta.solana.com) for now.
Verified `getBlock` works with `encoding: jsonParsed` and
`maxSupportedTransactionVersion: 0` via a direct curl call against a
real recent finalized slot (slot 439874156) — returned full transaction
data with resolved account keys including writable/signer flags.

Rejected signing up for a dedicated provider (Helius) at this stage:
adds a setup step and an external account dependency for a marginal
reliability gain that may not matter in practice for a one-shot,
self-rate-limited 1,000-slot pull. The RPC URL is a single config value,
so switching later if the public endpoint proves too flaky is cheap.

## Consequences

If ingestion throughput or Part 5 experiment numbers look inconsistent
or unexplainably slow, the public endpoint's own undocumented rate
limiting is a plausible confound to rule out before concluding it's a
problem in our own pipeline — worth noting explicitly in FINDINGS.md if
it happens rather than silently attributing odd numbers to our code.
Swapping to a dedicated provider later requires no code changes, only
the RPC URL value.
