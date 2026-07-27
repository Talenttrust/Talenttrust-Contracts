# Milestone Approval and Release Flow

Releasing a milestone is a two-step process. First, authorized parties call
`approve_milestone_release` to record their approval in temporary storage.
Then an authorized caller invokes `release_milestone`, which checks the
approvals, marks the milestone released, and clears the approval record.

**Relevant source:**
- [`contracts/escrow/src/approvals.rs`](../src/approvals.rs) — `approve_milestone`, `check_approvals`, `clear_approvals`
- [`contracts/escrow/src/lib.rs`](../src/lib.rs) — `approve_milestone_release`, `get_milestone_approvals`, `release_milestone`
- [`contracts/escrow/src/test/release.rs`](../src/test/release.rs) — integration tests

---

## State diagram

```
[Funded contract, unreleased milestone]
          │
          ▼
  approve_milestone_release(caller)
  ┌──────────────────────────────────────────────┐
  │ authenticate caller (require_auth)           │
  │ check caller role matches the mode           │
  │ reject duplicate approvals (AlreadyApproved) │
  │ write MilestoneApprovals to temp storage     │
  │ extend TTL → PENDING_APPROVAL_TTL_LEDGERS    │
  └──────────────────────────────────────────────┘
          │  (repeat for each required approver)
          ▼
  release_milestone(caller)
  ┌──────────────────────────────────────────────┐
  │ authenticate caller (require_auth)           │
  │ check contract is Funded, not finalized      │
  │ check caller role matches the mode           │
  │ check_approvals → reads temp storage         │
  │   None (expired or never set) → panic        │
  │   insufficient for mode → panic              │
  │ mark milestone.released = true               │
  │ contract.released_amount += amount           │
  │ accumulate protocol fee if enabled           │
  │ clear_approvals → remove from temp storage   │
  │ if all milestones done → status = Completed  │
  └──────────────────────────────────────────────┘
```

---

## ReleaseAuthorization modes

| Mode               | Who may call `approve_milestone_release` | Approval quorum                           | Who may call `release_milestone` |
|--------------------|------------------------------------------|-------------------------------------------|----------------------------------|
| `ClientOnly`       | client                                   | `client_approved`                         | client                           |
| `ArbiterOnly`      | arbiter                                  | `arbiter_approved`                        | arbiter                          |
| `ClientAndArbiter` | client or arbiter                        | `client_approved \|\| arbiter_approved`   | client or arbiter                |
| `MultiSig`         | client **and** freelancer (both)         | `client_approved && freelancer_approved`  | client or freelancer             |

### MultiSig details

Both client and freelancer must call `approve_milestone_release` before
release is possible. Either of them may then call `release_milestone`.
The arbiter has no role in `MultiSig` — calling `approve_milestone_release`
as arbiter returns `UnauthorizedRole`.

---

## TTL and fail-closed expiry

Approvals are stored in `env.storage().temporary()` under
`DataKey::MilestoneApprovals(contract_id, milestone_index)`.

```
PENDING_APPROVAL_TTL_LEDGERS    = 17_280 × 7 = 120_960 ledgers  (~7 days)
PENDING_APPROVAL_BUMP_THRESHOLD = 17_280 × 1 =  17_280 ledgers  (~1 day)
```

Each `approve_milestone_release` call resets the TTL to `PENDING_APPROVAL_TTL_LEDGERS`.
If the release window closes before `release_milestone` is called, Soroban
evicts the entry automatically.

**Fail-closed:** `check_approvals` reads via `env.storage().temporary().get(...)`.
Soroban returns `None` for both "never written" and "TTL elapsed". Both map to
`InsufficientApprovals` — the call panics without mutating any state. An expired
approval cannot silently authorize a release.

After a successful release, `clear_approvals` explicitly removes the entry
rather than waiting for natural TTL expiry, preventing approval reuse.

---

## Querying live approvals

```rust
get_milestone_approvals(contract_id, milestone_index) -> Option<MilestoneApprovals>
```

Returns `None` when no record exists or the TTL has elapsed. A `Some` value
with all fields `false` and `None` are both insufficient — neither unblocks
`release_milestone`.
