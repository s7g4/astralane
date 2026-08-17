# ADR-0001: Retry-with-backoff logic lives inside the RPC block-fetch function

Status: Accepted

## Context

Part 1 of the spec requires ingestion to "handle rate limits, retries
with backoff and partial failures" and to "finish unattended" without a
single bad block aborting the whole run. The block-fetch operation
(`getBlock` over JSON-RPC) can fail transiently (timeout, HTTP 5xx, rate
limit) and needs to retry those failures with backoff before giving up.

The open question was where that retry loop should live: inside the
function that fetches a single block, or in the fetcher pipeline stage
that calls it and pushes results downstream to the parser.

## Decision

Retry-with-backoff lives entirely inside the RPC wrapper's block-fetch
function. The fetcher stage calls this function once per slot and
receives only a final outcome — skipped, fetched, or failed — after all
internal retries are exhausted. It never sees individual retry attempts
or decides whether to retry.

Rejected: retry logic in the fetcher stage, with the fetch function
returning a plain `Result` per attempt. This was rejected because the
fetcher stage doesn't need attempt-level detail to do its job — it only
acts on the final result (push to channel on success, record
skipped=true, or log-and-continue on failure). Putting retry logic there
would mean every caller of the RPC wrapper (currently just the fetcher
stage, but potentially others later) would have to duplicate the same
backoff/jitter/transient-error-detection logic.

## Consequences

The fetcher stage's code stays simple: one call, one match on three
outcomes. Retry count, backoff base delay, and jitter strategy are
configured once, inside the RPC wrapper, not scattered across call
sites. Skipped-slot detection (an RPC `error` response, not an HTTP
failure) must be checked before the retry loop decides to retry, so it
doesn't waste attempts retrying something that was never going to
succeed. Testing the retry behavior means testing the RPC wrapper in
isolation rather than the fetcher stage.
