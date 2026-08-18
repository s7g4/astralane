# ADR-0004: Switch from public mainnet-beta RPC to Helius free tier

Status: Accepted

## Context

ADR-0002 accepted the risk of using the public mainnet-beta endpoint,
naming exactly this failure mode as a known possibility to watch for.
It happened: a 200-slot test run (after fixing the memory issue from
the earlier crash) got only 20/200 blocks — the remaining 180 slots
all failed with HTTP 429 after exhausting retries, despite our own
governor limiter correctly holding to 10 req/s. This is consistent
with the public endpoint's own undocumented rate limiting reacting to
cumulative request volume from this machine across the session (many
curl checks, several smoke tests, and a ~470-slot partial run before
the crash), not anything wrong in our own pacing.

## Decision

Switch to a Helius free-tier RPC endpoint. Gives a private, predictable
rate ceiling instead of a shared public one subject to unpredictable
throttling outside our control.

Rejected waiting out the public endpoint's throttling window: unknown
duration, no way to confirm it's actually reset before spending another
attempt, and blocks forward progress with no guaranteed payoff.

## Consequences

The RPC URL now contains an API key, so it can no longer be a plain
hardcoded `const` in a committed source file — that would leak the key
into git history. It's read from an environment variable at startup
instead, documented in the README as a setup step. `.env`-style files
or the key itself must never be committed.
