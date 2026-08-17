# DEVLOG

## 2026-08-17

Scaffolded the project, Day 1 deps in (tokio, reqwest, serde, governor,
rusqlite). Wrote the SQLite schema (5 tables) + db.rs — WAL mode,
foreign_keys pragma has to be set per-connection since it doesn't
persist to the file like journal_mode does. Added a read-only open()
for the API to use later.

ADR-0001: retry/backoff lives inside the RPC wrapper, not the fetcher
stage. Fetcher only ever needs a final yes/no per slot.

rpc.rs: envelope structs, Skipped/Fetched/Failed outcome type,
governor for the 10 req/s cap, retry loop with backoff + jitter.

ADR-0002: sticking with the public mainnet-beta endpoint for now
instead of signing up for Helius. Confirmed getBlock + jsonParsed
actually works with a raw curl before building anything on top of it.

Picked the slot range: 439865000-439865999, ~10k slots behind tip at
pick time.

Slot walker ended up being nothing — a for loop pushing into a bounded
channel, backpressure and shutdown just fall out of send().await and
Drop. Wired the full skeleton: walker -> fetcher -> parser (stub) ->
writer (stub). Fetcher uses a semaphore for bounded concurrency +
JoinSet so it actually waits on in-flight fetches before returning
instead of leaking them.

Bugs:
- `edition = "2026"` in Cargo.toml, not a real edition, wouldn't even
  parse. Typo for 2024.
- schema file was named `init.spl` not `.sql`. Didn't error because
  main.rs didn't have `mod db;` yet, so db.rs wasn't even in the
  build. cargo build passing proves nothing about code that isn't
  wired in yet.
- forgot rand needs to be a direct dep, not just pulled in
  transitively via governor.
- the real one: 10-slot smoke test against real mainnet, 3 failed with
  "bad json: error decoding response body" at 20 concurrent fetches.
  Pulled one of the failing slots with curl standalone — turned out to
  be a legit ~21MB block, downloaded fine alone in 4.6s. So not a bad
  response, just 20 concurrent multi-MB fetches stepping on each
  other. Also had that decode-error branch marked Permanent (no
  retry), which was just wrong here — dropped 3 real slots for
  nothing. Reclassified as Transient, dropped concurrency 20 -> 10,
  reran, 10/10.

Next: parser/writer stop being stubs, real INSERT OR IGNORE. Still
haven't checked ALT resolution against a real v0 tx. Skipped-slot
detection is a string match on the error message and untested against
an actual skipped slot — probably wrong, need to find one and check.
