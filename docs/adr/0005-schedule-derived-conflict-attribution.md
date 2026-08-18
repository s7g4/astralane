# ADR-0005: Conflicts are counted per scheduling rejection, not pairwise

Status: Accepted

## Context

The spec asks for "the accounts responsible for conflicts and the
programs associated with those transactions" but doesn't define what
counts as one conflict. Two reasonable readings: (A) every time the
greedy scheduler tries to place a transaction into a step and rejects
it because of a specific account lock, that's one conflict; or (B) an
independent pairwise count — every pair of transactions in a block
that share a conflicting lock on an account, regardless of scheduling.

## Decision

Went with (A), schedule-derived counting. Implemented as a side effect
of `schedule_block` itself: each rejected placement attempt records a
`ConflictEvent` (account, lock type, the rejected transaction's
`program_ids`), aggregated by account and by program across the range.

Rejected (B): a separate pairwise pass over all transaction pairs in a
block is an independent computation with its own complexity, and its
numbers wouldn't correspond to anything in the actual reconstructed
schedule being presented — (A) ties the reported conflict counts
directly to the schedule depth/width numbers in the same report.

## Consequences

Conflict counts scale with how many steps a heavily-contended
transaction gets rejected from before finding a home, not just with
how many transactions exist — a transaction that conflicts with 500
earlier steps generates 500 conflict events for the same account, not
one. This produces large raw numbers (tens of millions on the real
1,000-slot range) that should be read as "scheduling friction
attributable to this account," not literally "this many transaction
pairs touched this account." Also: since a conflict is attributed to
*all* of a transaction's `program_ids`, near-ubiquitous programs
(ComputeBudget, System Program) dominate the program ranking simply by
being present in almost every transaction, not because they're
inherently more contentious — worth calling out explicitly in
FINDINGS.md rather than letting the numbers imply something they
don't.
