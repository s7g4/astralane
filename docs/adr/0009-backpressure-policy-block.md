# ADR-0009: Backpressure policy is block, not shed or buffer

Status: Accepted

## Context

Part 5 requires an explicit backpressure policy for the bounded
channels connecting pipeline stages, demonstrated by pausing the DB
writer for 10 seconds mid-ingestion. Three standard options: block
(the sender waits until space is free), shed (drop items when full),
buffer (grow the channel to absorb bursts).

## Decision

Block. This is also just what a bounded `tokio::sync::mpsc` channel
does by default via `Sender::send().await` — no extra code needed
beyond choosing sensible channel bounds (see the `BLOCKS_CHANNEL_BOUND`
comment in `main.rs`, and the crash post-mortem in `DEVLOG.md` for what
happens when a bound is too large relative to payload size).

Rejected shed: this is a one-shot, finite ingestion job that has to
finish unattended and be safe to re-run — silently dropping
transaction data on a busy stretch directly conflicts with both of
those requirements, and would need extra bookkeeping to even detect
what was dropped. Rejected unbounded buffering: defeats the entire
point of bounded channels, and is exactly the failure mode that
crashed the ingestion run once already (large raw blocks accumulating
in memory faster than the writer could drain them).

## Consequences

A slow or paused writer propagates backpressure upstream automatically
through the channel chain — verified experimentally (FINDINGS.md):
the writer's immediate upstream channel visibly fills during a
10-second pause. Throughput is bounded by the slowest stage rather
than the fastest, which is the correct tradeoff here: correctness and
completeness of a finite ingestion run matter more than raw throughput.
