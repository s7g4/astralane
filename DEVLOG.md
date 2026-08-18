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

## 2026-08-18

Verified v0/ALT resolution against real data before writing the
parser: pulled a real v0 tx using a lookup table, accountKeys came
back with 33 entries (19 direct + 14 from the lookup table), all with
writable/signer populated the same way regardless of source. So
account_locks extraction doesn't need to special-case lookup-table
accounts.

Wrote real parser.rs — extracts blocks/transactions/account_locks/
token_balances out of the raw block JSON into proper structs instead
of passing the Value through. Two things I had to just decide and
write down rather than look up: Failed slots don't get a blocks row
(we don't actually know what happened there, unlike a real skip), and
program_ids only counts top-level instructions, not inner/CPI ones.

Wrote real writer.rs. rusqlite is sync, so it runs on spawn_blocking
instead of a normal async task — otherwise its blocking DB calls would
stall whatever tokio worker thread picked it up. One SQLite transaction
per block. Automated idempotency test (write the same block twice,
assert row counts don't move) passes, plus manually double-ran 10 real
slots earlier and got identical counts both times.

Tried the full 1,000-slot run. It crashed the whole machine partway
through.

After reboot: /tmp got wiped so no log survived, but the DB didn't —
WAL mode meant the 125 blocks that had committed were intact
(integrity_check: ok), everything after just didn't happen. Actual
cause, best I can tell: writer.rs was calling tx.execute() fresh for
every single row with no statement caching — tens of thousands of
un-cached prepares per block. That made the writer the bottleneck,
which meant the bounded channel between fetcher and parser (which
carries the raw, unparsed block JSON, sometimes 20+MB) filled up and
stayed full, so large blocks piled up in memory waiting for a slow
writer to catch up.

Fixed with prepare_cached (statements compiled once, reused across
every row and every block) and a separate, much smaller bound
specifically on that one channel. Reran a 200-slot test with active
memory logging this time — memory grew ~1GB and plateaued instead of
climbing, then dropped back down after the process exited. Much
healthier profile.

Then hit a second problem on the same 200-slot test: 180/200 slots
failed with 429 Too Many Requests, despite our own limiter correctly
holding to 10 req/s. Pretty sure this is cumulative — a lot of
requests against the public endpoint over the course of the day
(curl checks, several smoke tests, the crashed run) probably tripped
its own undocumented throttling. ADR-0002 called this out as a risk
going in, so switched to Helius (ADR-0004, supersedes ADR-0002).
Had to change RPC_URL from a hardcoded const to an env var, since it
now carries an API key that can't go in a committed file.

Reran the full 1,000-slot range on Helius with memory monitoring
running the whole time. Checked in on it periodically — memory
climbed higher than the 200-slot test (up to ~12.5GB used) but
plateaued rather than climbing indefinitely, so let it keep running
instead of killing it. Finished clean: 1000/1000 blocks, 0 skipped,
0 failed, no gaps, integrity_check ok. ~1.76M transactions, ~35M
account_locks, ~4M token_balances, 15GB on disk.

What I'd do differently: should have caught the missing statement
caching before running at scale, not after a crash — it's the kind of
thing that's obvious in hindsight once you know 38k rows/block times
1000 blocks is tens of millions of individual un-cached prepares.
Also should have anticipated the public RPC's cumulative throttling
sooner given ADR-0002 already named it as a risk — didn't take that
risk seriously enough until it actually happened.

What confused me: the first crash gave no real signal beforehand —
no warning, no gradual slowdown I noticed, just gone. Only in
hindsight, watching memory during the retry, was it obvious there
was a real upward trend the first time too, I just wasn't watching
for it.

## 2026-08-18, part 2 — Contention

Started Part 2 and immediately hit a gap: the greedy scheduler needs
each transaction's original position within its block, and the schema
never captured that — signature/slot alone give no ordering. Had to
add a tx_index column and backfill the already-ingested range rather
than redo the whole ingestion (getBlock with transactionDetails:
signatures instead of full jsonParsed — same 1000 requests, much
smaller payloads, few minutes instead of ~an hour).

Building the backfill tool needed the same RpcClient as main.rs, which
meant restructuring into a lib crate + multiple binaries (astralane,
backfill_order, contention_report) instead of duplicating the
rate-limited RPC logic. More structural churn than I expected for what
started as "add one column."

Implemented the greedy scheduler itself: track two sets per step
(written accounts, touched accounts overall), a lock conflicts with a
step if it wants to write something already touched, or read something
already written. Wrote the three required synthetic tests
(read-read/write-write/read-write) before touching real data, all
passed first try — the harder part was deciding what actually counts
as "one conflict" for the reporting, since the spec doesn't define it.
Went with counting each rejected placement attempt during scheduling,
tied directly to the schedule being reported rather than a separate
pairwise count.

Ran it for real against all 1000 blocks: took over 11 minutes. Not
surprised given the scheduler's complexity (checking every transaction
against every existing step's lock sets), but it's a good forcing
function — confirms the contention computation really is the
CPU-heavy thing Part 5 wants tested under spawn_blocking, and also
means /api/contention can't compute this live per request later, has
to serve something precomputed.

One artifact in the real numbers worth remembering for FINDINGS:
ComputeBudget and the System Program dominate the "most contentious
programs" ranking, but that's mostly because they're present in nearly
every transaction (compute budget instructions are almost universal
now), not because they're actually the programs causing the most real
contention. The account-level ranking is more meaningful than the
program-level one for that reason.

What I'd do differently: should have thought about what data the
scheduling algorithm actually needs as input before finalizing the
Part 1 schema, not after a full ingestion was already done. Order
within a block is an obvious requirement in hindsight.

## 2026-08-18, part 3 — OHLCV

Another schema gap right away: price inference needs an "opposing SOL
or wrapped-SOL balance change," but native SOL (preBalances/
postBalances) was never captured, only token balances. Recovering it
would mean a full re-fetch of every block, same cost as the original
ingestion — decided to scope to wrapped-SOL only and document native
SOL as untracked (ADR-0008), rather than pay for another hour-long
re-ingest.

Wrote the matching logic (net delta per mint per tx, require exactly
one non-WSOL mint + opposing WSOL delta, dust floor at 0.0001 SOL) and
candle aggregation, tests passed first try — but ran it against the
real data and got only 68 candles total across all 1000 slots. Way
lower than expected, assumed a bug at first.

Traced it with raw SQL, independent of my Rust code, to make sure it
wasn't just a code bug: 211,584 transactions touch both a WSOL leg and
exactly one other mint, but 211,390 of them (99.9%) net to exactly
zero token change when you sum deltas across all of that mint's
accounts in the tx — real SOL movement, no net token change. Pulled
one specific example: same token mint at two different accounts,
+94,739,442 and -94,739,442, exactly cancelling. That's not noise,
that's some internal routing/vault step netting out while the actual
trade (if there is one) isn't visible from these two rows alone.

Checked the 194 that do net to nonzero too, since I didn't want to
just assume those were all real trades either. Several were exact
round numbers like 1,000,000,000 — pump.fun's standard 1B token supply
on creation, with a small SOL fee attached. Not trades, mint events.
Added a filter for exact multiples of 1,000,000 units and re-ran: 66
real candles, 27 distinct mints.

Had to decide whether to keep digging (tracking per-owner deltas
instead of per-mint, which needs the owner field we never captured
either — another schema+backfill cycle) or just accept and document
the low yield honestly. Went with documenting it, given the time left
today. Wrote up the whole investigation in the README rather than just
quietly reporting the small number — felt important that this reads
as a real, checked finding, not an unexplained gap.

What confused me: my first instinct when I saw "68 candles" was that
I'd broken something, and I almost started rewriting the matching
logic before checking the raw data first. Glad I checked with plain
SQL before touching the code again — the numbers were telling me
something true about the data, not something wrong with my query.

What I'd do differently: same lesson as Part 2 honestly — capture
what a later stage will need (owner field, in this case) during
initial ingestion, not after. Two schema gaps in two parts is a
pattern, not a coincidence; I'm designing tables around "what does
Part N need" instead of thinking about the full pipeline up front.

## 2026-08-18/19, part 4 + part 5 — API, dashboard, load experiments

Part 4 went mostly smoothly, but the /api/contention endpoint forced a
decision I hadn't planned for: the scheduling computation takes 11+
minutes, so it obviously can't run live per request. Added a
build_contention_summary tool that precomputes it once into dedicated
tables, same pattern as the OHLCV candles. Should have seen this
coming given I already knew the compute time from Part 2, but I didn't
think about the API implications until I was actually building the
endpoint.

Two real bugs only showed up once I actually loaded the dashboard in a
browser instead of just reading the code:
- Math.max(...bigArray) with 264k elements blew the JS call stack
  (RangeError). Fixed with reduce().
- Default token selection landed on wrapped-SOL, which never has
  candles (it's always the counter-leg). Chart opened empty by
  default, which looks broken even though it's technically correct
  given the data. Fixed by sorting tokens-with-candles first.

Also missing a viewport meta tag entirely, which I only noticed
because the user pointed out mobile wasn't responsive - should have
caught that myself before hearing about it.

Part 5, given the time crunch, I tried to keep each experiment as
lean as possible while still being real:
- Backpressure: paused the writer 10s, watched channel capacity drain
  in real time. Got a partial result honestly - the writer's direct
  upstream channel filled visibly, but the bound-4 channel further
  upstream never saturated in my 30-slot test window, so I didn't
  observe the full cascade to the fetcher. Reported it as what it is
  rather than cherry-picking a longer run to hide that.
- Async starvation: had to build a separate single-worker-thread
  server for this to actually demonstrate anything, since this dev
  machine has 12 cores and one blocked thread doesn't visibly starve
  anything when there are 11 others free. Once I did that, the effect
  was dramatic and needed no cherry-picking - one blocked request for
  69 seconds vs. 56 successful ones in a similar window.
- Write-path contention: got a confusing result at first (idle reads
  were SLOWER than reads during active writes) and almost reported it
  as "writes don't slow down reads, even faster!" before realizing the
  during-write measurements were averaged over a growing table while
  the idle baseline hit the full, final table size - not a WAL effect
  at all, just different data volumes. Caught it before writing it
  down wrong. The actual signal (no lock errors, no latency spikes) is
  still real, just not the naive average comparison.

What I'd do differently: think about what the API layer needs from
each computed feature (precomputed vs. live, and roughly how long
things take) at the point I build that feature, not as a surprise
later. Same root pattern as the two schema gaps from Parts 2 and 3.

What confused me: the write-path contention numbers, until I stopped
and actually thought about what data was different between the two
measurement windows instead of taking the average at face value.
