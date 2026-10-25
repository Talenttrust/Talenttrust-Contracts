# Refund serialization and recovery

`refund_unreleased_milestones` delegates to `src/refund.rs` without changing its
arguments, return type, storage keys or error ABI. Soroban commits conflicting
ledger transactions serially; clients should resubmit against current ledger
state rather than assume an earlier simulation guarantees a payout. Overlapping
refunds and release/refund races have one winner for each milestone. A retry of
an already refunded milestone is rejected (no second token transfer); disjoint
refunds can succeed in either order.

All indices, deadlines and available funds are validated before marking any
milestone refunded. The available-funds check reserves released milestones at
gross value, including retained protocol fees, and uses checked arithmetic. It
does not subtract a global fee counter belonging to other escrows. This also
fails closed for legacy or inconsistent partially funded post-release records;
current release callers require full funding.

Milestone flags, accounting, terminal status, dispute rollback invalidation and
events are written before token transfer. Soroban invocation rollback restores
those effects when a token transfer fails, permitting a safe retry after the
underlying token problem is corrected. The existing client authorization,
pause/finalization guards, strict `now > deadline` timeout boundary and mixed
release/refund reputation behavior remain in force.

Run focused regression tests with `cargo test -p escrow --lib test::refund`.
The suite exercises both serialization orders instead of multithreading a shared
Soroban test environment; this is not a live-network load test.
