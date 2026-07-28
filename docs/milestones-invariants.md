# Milestone Invariants

This document lists the invariants that hold for the `Milestone` and
per-milestone lifecycle logic in the TalentTrust escrow contract — properties
that are always true, and the exact code location that enforces each one.

Scope: `Talenttrust/Talenttrust-Contracts` only. Source of truth for this
document is `contracts/escrow/src/milestones.rs` (all invariants below are
verified directly against that file, not inferred from other docs).

Related docs (auth roles, storage layout, threat model — read these for
broader context, not invariants):
- [`docs/milestones-auth.md`](milestones-auth.md)
- [`docs/milestones-storage.md`](milestones-storage.md)
- [`docs/milestones-threat-model.md`](milestones-threat-model.md)
- [`docs/milestones-errors.md`](milestones-errors.md)

---

## 1. Settlement flags are one-way and mutually exclusive

`Milestone.released` and `Milestone.refunded` each transition `false → true`
exactly once and are never reset to `false`. The two flags can never both be
`true` for the same milestone.

**Enforced by:**
- `release_milestone_impl` — rejects if `milestone.released` is already
  `true` (`Error::MilestoneAlreadyReleased`) or if `milestone.refunded` is
  `true` (`EscrowError::AlreadyRefunded`), checked **before** any state
  mutation, and checked a second time after the milestone vector is
  re-loaded from storage (defense-in-depth double-check).
- `refund_unreleased_milestones_impl` — rejects if `milestone.released` is
  `true` (`Error::AlreadyReleased`) or `milestone.refunded` is already `true`
  (`EscrowError::AlreadyRefunded`).

## 2. Milestone index must be in bounds

`milestone_index` (or every index in a refund batch) must satisfy
`milestone_index < milestones.len()`.

**Enforced by:**
- `release_milestone_impl` — `Error::IndexOutOfBounds` panic, checked twice
  (once before the approvals check, once after milestone re-load).
- `refund_unreleased_milestones_impl` — `Error::IndexOutOfBounds` panic per
  index in the batch.
- `submit_work_evidence_impl` — `Error::IndexOutOfBounds` panic.
- `get_milestone_impl` / `get_work_evidence_impl` — return `None` rather than
  panicking for an out-of-range index (read-only paths).

## 3. Refund batches are non-empty and index-unique

A call to `refund_unreleased_milestones_impl` must include at least one
index, and no index may repeat within the same call.

**Enforced by:**
- Empty check: `EscrowError::EmptyRefundRequest`.
- Duplicate check: pairwise comparison over `milestone_indices`,
  `EscrowError::DuplicateMilestoneInRefund`.

## 4. Release requires the contract to be exactly `Funded`

`release_milestone_impl` only proceeds when `contract.status ==
ContractStatus::Funded`. Any other status → `Error::InvalidState`.

Refund is permitted in a wider set of states: `Created`, `Funded`, or
`Disputed`. Any other status → `EscrowError::InvalidState`.

## 5. Release caller authorization is mode-dependent

The caller of `release_milestone_impl` must satisfy `contract
.release_authorization`:

| Mode | Authorized releasers |
|---|---|
| `ClientOnly` | client |
| `ArbiterOnly` | arbiter |
| `ClientAndArbiter` | client **or** arbiter |
| `MultiSig` | client **or** freelancer (approval step, separately, requires both) |

Violated → `EscrowError::UnauthorizedRole`.

`refund_unreleased_milestones_impl` requires `contract.client.require_auth()`
— refund is client-only regardless of release mode.

## 6. A milestone with a deadline can only be refunded once overdue

If `milestone.deadline` is `Some(t)`, `refund_unreleased_milestones_impl`
requires `now_seconds(env) > t` (checked via `is_milestone_overdue_impl`)
before that milestone may be included in a refund. If `deadline` is `None`,
the milestone may be refunded at any time — no overdue check applies.

**Enforced by:** `Error::MilestoneNotOverdue` panic when a dated milestone is
refunded before its deadline.

## 7. Pause and finalization guards run before any mutation

Both `release_milestone_impl` and `refund_unreleased_milestones_impl` call
`Self::require_not_paused` at entry. `release_milestone_impl` additionally
calls `Self::require_not_finalized` before any milestone state is touched.

## 8. Available balance must cover the requested amount

- **Release:** `contract.funded_amount - contract.released_amount -
  contract.refunded_amount - accumulated_protocol_fees` (the accumulated-fees
  term reads the **global** `DataKey::AccumulatedProtocolFees` value, not a
  per-contract figure) must be `>= gross milestone amount`, else
  `EscrowError::InsufficientFunds`.
- **Refund:** `contract.funded_amount - contract.released_amount -
  contract.refunded_amount` must be `>= sum(refund batch amounts)`, else
  `EscrowError::InsufficientFunds`.

## 9. Post-release accounting invariant

After a release is applied in memory (before it is committed to storage),
the contract enforces:

```
contract.released_amount + contract.refunded_amount + accumulated_protocol_fees <= contract.funded_amount
```

Violated → `EscrowError::AccountingInvariantViolated` panic, and the write is
never committed (the check happens before `ttl::store_milestones` /
`env.storage().persistent().set`).

Note: `contract.released_amount` accumulates the **net** amount (gross minus
protocol fee) paid to the freelancer, not the gross milestone amount.

## 10. Arithmetic uses checked addition, never silent overflow

- `contract.released_amount` is updated via `checked_add`, panicking with
  `EscrowError::PotentialOverflow` on overflow.
- `contract.refunded_amount` is updated via `checked_add` in the refund path,
  but its overflow fallback is `Error::InsufficientFunds` rather than
  `PotentialOverflow` — worth knowing since the error code differs from the
  release path for what is conceptually the same class of failure.

## 11. Settlement token must be configured before any transfer

Both release and refund read the settlement token via
`Self::read_settlement_token`. If unset, `Error::SettlementTokenNotConfigured`
panics before any `token::Client::transfer` call.

## 12. Contract-level completion follows milestone completion

- **Release path:** once every milestone in the vector is `released ||
  refunded`, `contract.status` is set to `ContractStatus::Completed` and a
  pending reputation credit is granted to the freelancer
  (`grant_pending_reputation_credit`).
- **Refund path:** once every milestone is `released || refunded`:
  - if *all* are `refunded` → `ContractStatus::Refunded` (no reputation
    credit — no work was accepted).
  - if it's a mix of released and refunded → `ContractStatus::Completed`,
    and a reputation credit is granted to the freelancer.

## 13. Approvals are cleared immediately after a successful release

`approvals::clear_approvals` runs unconditionally on every successful
`release_milestone_impl` call, before the milestone vector is persisted. A
released milestone therefore never carries a stale approval record forward
(moot for re-release, since Invariant 1 already blocks that, but relevant for
approval-record hygiene / TTL accounting — see
[`docs/milestones-storage.md`](milestones-storage.md)).

## 14. Work evidence is bounded and freelancer-gated

`submit_work_evidence_impl` requires `contract.freelancer.require_auth()`,
requires `contract.status == Funded`, and rejects evidence longer than 1000
bytes (`Error::EvidenceTooLong`). It may overwrite prior evidence for the
same milestone (no append-only guarantee), and is rejected if the milestone
is already `released` or `refunded`.

---

## Known documentation discrepancies found while writing this note

These were discovered by reading `milestones.rs` directly rather than relying
on other docs, and are recorded here rather than silently corrected elsewhere
(out of scope for this issue):

1. **Evidence length limit.** [`docs/milestones-auth.md`](milestones-auth.md)
   states the evidence cap is "> 256 bytes → `EvidenceTooLong`". The actual
   check in `submit_work_evidence_impl` is `evidence.len() > 1000`. The limit
   is **1000 bytes**, not 256.
2. **Per-milestone funded amount.**
   [`docs/escrow/PER_MILESTONE_FUNDING.md`](escrow/PER_MILESTONE_FUNDING.md)
   states there is no per-milestone funded-amount tracking ("There is no
   `set_milestone_funded` or `get_milestone_funded` entrypoint... and release
   does not transfer tokens to the freelancer"). This does not match
   `milestones.rs`: `Milestone.funded_amount` is set to the gross milestone
   amount on release (`milestone.funded_amount = gross_amount`), and
   `release_milestone_impl` does transfer tokens to the freelancer via
   `token_client.transfer`. That doc may predate the current implementation.

---

## Entrypoint cross-reference

| Invariant(s) | Entrypoint | Source |
|---|---|---|
| 1, 2, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13 | `release_milestone` | `contracts/escrow/src/milestones.rs::release_milestone_impl` |
| 1, 2, 3, 4, 6, 7, 8, 10, 11, 12 | `refund_unreleased_milestones` | `contracts/escrow/src/milestones.rs::refund_unreleased_milestones_impl` |
| 14 | `submit_work_evidence` | `contracts/escrow/src/milestones.rs::submit_work_evidence_impl` |
| 2 (read-only) | `get_milestones`, `get_milestone`, `get_work_evidence` | `contracts/escrow/src/milestones.rs` |
| 6 | `is_milestone_overdue` | `contracts/escrow/src/milestones.rs::is_milestone_overdue_impl` |
| — (protocol limits referenced above) | `MAX_MILESTONES`, fee bounds | `contracts/escrow/src/milestones_consts.rs` |
