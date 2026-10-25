# Milestone Threat Model

This document covers the trust assumptions, attacker capabilities, and
mitigations specific to the milestone subsystem of the TalentTrust escrow
contract. It complements the broader escrow threat model at
[`docs/escrow/threat-model.md`](escrow/threat-model.md) and the authorization
reference at [`docs/escrow/authorization.md`](escrow/authorization.md).

Implementation references:

- `contracts/escrow/src/types.rs` — `Milestone`, `MilestoneApprovals`,
  `ReleaseAuthorization`
- `contracts/escrow/src/approvals.rs` — `approve_milestone`,
  `check_approvals`, `clear_approvals`
- `contracts/escrow/src/release.rs` — `release_milestone_impl`
- `contracts/escrow/src/create_contract.rs` — milestone construction and
  amount validation
- `contracts/escrow/src/refund_impl.rs` — `refund_unreleased_milestones`

---

## Scope

A **milestone** is a single payment unit in an escrow contract. It carries an
`amount` (i128 stroops), release/refund flags, optional `work_evidence`, and
an optional `deadline`. The contract stores a `Vec<Milestone>` in persistent
storage keyed by `(DataKey::Contract(contract_id), "milestones")`.

The threat model covers:

1. Milestone creation and amount validation
2. Approval recording (`approve_milestone_release`)
3. Milestone release (`release_milestone`)
4. Milestone refund (`refund_unreleased_milestones`)
5. Approval TTL and expiry behavior
6. Schedule metadata (`set_milestone_schedule`)

---

## Trust Assumptions

### Trusted parties

| Party | Assumption |
|---|---|
| **Client** | Funded the escrow; authorized to approve (ClientOnly, ClientAndArbiter), co-approve (MultiSig), and refund unreleased milestones |
| **Freelancer** | Recipient of released funds; authorized to co-approve (MultiSig) and trigger release after both approvals exist |
| **Arbiter** | Neutral third party; authorized to approve (ArbiterOnly, ClientAndArbiter); must be a different address from both client and freelancer |
| **Contract admin** | Controls pause/emergency flags only; has no special milestone privileges |

### Untrusted inputs

- All arguments to every entrypoint (`contract_id`, `milestone_index`, amounts,
  addresses, strings) are treated as attacker-controlled until validated.
- Ledger timestamps (`env.ledger().timestamp()`) are set by the Stellar network
  and cannot be spoofed by a single caller, but they are not secret.
- Off-chain work evidence strings are caller-supplied and unverified on-chain.

### Out-of-scope assumptions

- SAC (Stellar Asset Contract) token behavior is assumed correct. The escrow
  contract calls `token::Client::transfer`; a malicious or buggy SAC could
  misdeliver funds. See [`docs/escrow/sac-custody.md`](escrow/sac-custody.md).
- Admin key management (single admin, no multi-sig or hardware signing) is an
  operational concern documented in
  [`docs/escrow/governance-security.md`](escrow/governance-security.md).

---

## Attacker Capabilities

The attacker model considers adversaries that can:

1. **Submit arbitrary transactions** — call any public entrypoint with any
   arguments.
2. **Impersonate addresses** — attempt to pass a crafted `caller` argument for
   an address they do not control (mitigated by `require_auth()`).
3. **Race concurrent transactions** — submit multiple calls in the same or
   adjacent ledgers.
4. **Front-run** — observe pending transactions and submit higher-fee
   transactions before them (constrained by Soroban's atomic per-transaction
   execution model).
5. **Read all on-chain state** — all persistent and temporary storage is
   publicly visible.
6. **Control the freelancer account** — a malicious freelancer may attempt to
   release funds early or bypass multi-sig requirements.
7. **Control one party in a multi-sig pair** — a single compromised key cannot
   unilaterally release in MultiSig mode.
8. **Observe approval TTL** — an adversary can wait for an approval to expire
   and attempt a replay after re-approval.

---

## Attack Surface and Mitigations

### 1. Unauthorized milestone release

**Goal:** Release a milestone without the required approval(s).

**Mitigated by:**

- `caller.require_auth()` in `release_milestone_impl` — Soroban's native auth
  ensures only the holder of the private key for `caller` can sign the
  invocation. Passing a forged address fails at the host level.
- Role check against `contract.release_authorization` before any state change
  (`UnauthorizedRole` on failure). See
  [`docs/escrow/authorization.md`](escrow/authorization.md) for the full
  authorization matrix.
- `check_approvals` must return `Ok(true)` before funds move
  (`InsufficientApprovals` otherwise). Approvals live in temporary storage;
  absent or expired records fail closed.

**Residual risk:** None within the contract boundary. Token delivery is
handled by the SAC; see the SAC custody section.

---

### 2. Approval replay / stale approval reuse

**Goal:** Reuse an old approval (e.g., from a previous negotiation round) to
release a milestone without fresh consent.

**Mitigated by:**

- Approvals are stored in Soroban **temporary storage** with a TTL of
  `PENDING_APPROVAL_TTL_LEDGERS` (120,960 ledgers ≈ 7 days). Expired records
  are automatically evicted by the host and treated as absent.
- `clear_approvals` removes the `MilestoneApprovals` entry immediately after a
  successful release. A released milestone cannot be approved or released again
  (`MilestoneAlreadyReleased`).
- Approvals are scoped to `(contract_id, milestone_index)`. An approval for
  milestone 0 cannot satisfy milestone 1.

**Residual risk:** If the approval window (7 days) is long relative to the
intended review period, a party could grant approval and then change their mind
but be unable to revoke it before the other party calls `release_milestone`.
Approval revocation is not currently implemented; see
[Future Improvements](#future-improvements).

---

### 3. Double release (release the same milestone twice)

**Goal:** Transfer the milestone amount to the freelancer more than once.

**Mitigated by:**

- `milestone.released` flag is checked before any state change
  (`MilestoneAlreadyReleased`).
- The flag is written atomically with the `released_amount` increment in the
  same `env.storage().persistent().set()` call.
- A finalized contract rejects all further mutations (`AlreadyFinalized`).

---

### 4. Release a refunded milestone

**Goal:** Extract funds from a milestone already returned to the client.

**Mitigated by:**

- `milestone.refunded` flag is checked at the start of
  `release_milestone_impl` (`AlreadyRefunded`).

---

### 5. Over-release (extract more than the available balance)

**Goal:** Release milestones totaling more than the funded balance.

**Mitigated by:**

- `available_balance = contract.funded_amount - contract.released_amount - contract.refunded_amount`
  is computed and compared to `milestone.amount` before the transfer
  (`InsufficientFunds`).
- The accounting invariant
  `total_deposited == released_amount + refunded_amount + available_balance`
  is enforced on every balance-changing operation. See
  [`docs/escrow/balance-conservation-invariant.md`](escrow/balance-conservation-invariant.md).

---

### 6. Milestone amount manipulation at creation

**Goal:** Create a milestone with a zero, negative, or overflow amount to break
accounting later.

**Mitigated by:**

- `amount_validation::validate_milestone_amounts` in `create_contract` enforces:
  - Each amount is strictly positive (≥ 1 stroop).
  - Each amount does not exceed `MAX_SINGLE_MILESTONE_STROOPS`
    (1 × 10¹³ stroops).
  - The total of all amounts does not exceed the governed
    `max_escrow_total_stroops` cap (falls back to `i128::MAX` when unset).
  - Accumulation uses `checked_add`, returning `PotentialOverflow` instead of
    panicking.
- Milestone amounts are immutable after `create_contract`; no entrypoint
  modifies them.

---

### 7. Index out-of-bounds / invalid milestone index

**Goal:** Reference a non-existent milestone to trigger a panic or access
unintended state.

**Mitigated by:**

- Both `approve_milestone` and `release_milestone_impl` compare
  `milestone_index` against `milestones.len()` and panic with
  `IndexOutOfBounds` if out of range.

---

### 8. Role confusion (arbiter is client or freelancer)

**Goal:** Register a participant as their own arbiter to gain elevated release
authority.

**Mitigated by:**

- `create_contract` rejects any arbiter address equal to `client` or
  `freelancer` with `InvalidArbiter`.
- `ArbiterOnly` and `ClientAndArbiter` modes additionally require `arbiter` to
  be `Some(...)` at creation time; `MissingArbiter` is returned otherwise.

---

### 9. MultiSig bypass (release with only one signature)

**Goal:** In MultiSig mode, trigger release with only client or freelancer
approval.

**Mitigated by:**

- `check_approvals` for MultiSig mode requires
  `approvals.client_approved && approvals.freelancer_approved` — both flags
  must be `true`.
- `approve_milestone` rejects duplicate approvals from the same party
  (`AlreadyApproved`), so a single key cannot set both flags.

---

### 10. Duplicate approval from the same party

**Goal:** Set both the client and freelancer approval flags using the same key
(e.g., by calling `approve_milestone_release` twice with different role claims).

**Mitigated by:**

- Approval identity is determined by comparing the `caller` address against
  the stored `contract.client`, `contract.freelancer`, and `contract.arbiter`
  fields — not by a caller-supplied role parameter.
- `AlreadyApproved` is returned if the same party's flag is already `true`.

---

### 11. Release while paused or in emergency mode

**Goal:** Push a release through during an incident response window.

**Mitigated by:**

- `Self::require_not_paused` is called at the start of
  `release_milestone_impl` (and again after TTL extension as defense in
  depth). `ContractPaused` or `EmergencyActive` is returned while the flag is
  set.

---

### 12. Release on a finalized contract

**Goal:** Mutate milestone state after the contract has been closed.

**Mitigated by:**

- `Self::require_not_finalized` is called at the start of
  `release_milestone_impl` and again after TTL extension. `AlreadyFinalized`
  is returned if a finalization record exists.

---

### 13. Release on an incorrect contract status

**Goal:** Release a milestone on a contract that is `Created`, `Cancelled`,
`Completed`, etc.

**Mitigated by:**

- `release_milestone_impl` checks `contract.status == ContractStatus::Funded`
  and returns `InvalidState` otherwise.

---

### 14. Milestone deadline manipulation

**Goal:** Manipulate `deadline` or `updated_at` fields to fake schedule
compliance or exploit timeout logic.

**Context:** Schedule metadata (`due_date`, `title`, `description`) is
informational only; the on-chain contract does not automatically release or
refund based on deadlines. `updated_at` is set from `env.ledger().timestamp()`
by the contract — callers cannot supply it.

**Mitigated by:**

- The `deadline` field on `Milestone` is optional and does not gate any
  value-moving operation in the current implementation.
- `set_milestone_schedule` is restricted to the client
  (`contract.client.require_auth()`) and rejects past `due_date` values
  (`ScheduleDueDateInPast`).
- Once a milestone is released, its schedule entry is immutable
  (`ScheduleImmutableAfterRelease`).

**Residual risk:** Deadline enforcement is the responsibility of the calling
application; on-chain, overdue milestones cannot self-trigger a refund without
a client-initiated call.

---

### 15. Work evidence injection

**Goal:** Supply a crafted `work_evidence` string to trigger unexpected
contract behavior.

**Mitigated by:**

- `work_evidence` is a free-form `Option<String>` stored as-is. The contract
  does not parse or act on its contents; it is solely for off-chain
  consumption.
- Maximum length is enforced by `EvidenceTooLong` (see `Error` enum).

---

## Auth Check Cross-Reference

The following table maps each milestone-relevant entrypoint to its auth
enforcement points in the source code.

| Entrypoint | `require_auth()` call site | Role check | Approval check |
|---|---|---|---|
| `create_contract` | `client.require_auth()` in `create_contract.rs` | Validates arbiter distinctness | N/A |
| `approve_milestone_release` | `caller.require_auth()` in `lib.rs` | `approvals::approve_milestone` role match | N/A (writes approval) |
| `release_milestone` | `caller.require_auth()` in `release.rs` | `release_authorization` match in `release.rs` | `approvals::check_approvals` must return `Ok(true)` |
| `refund_unreleased_milestones` | `contract.client.require_auth()` in `refund_impl.rs` | Client only | N/A |
| `set_milestone_schedule` | `contract.client.require_auth()` | Client only | N/A |

---

## Known Gaps and Planned Work

| Gap | Status | Tracking |
|---|---|---|
| Approval revocation | Not implemented — a party cannot retract an approval once recorded before TTL expires | Untracked |
| Protocol fee withdrawal | Accumulation is implemented; withdrawal entrypoint is planned | [#314](https://github.com/Talenttrust/Talenttrust-Contracts/issues/314) |
| Two-step admin transfer | Single admin controls pause/emergency; no key rotation with timelock | [#318](https://github.com/Talenttrust/Talenttrust-Contracts/issues/318) |
| SAC token custody audit | Token transfer correctness is outside this contract's scope | See [`docs/escrow/sac-custody.md`](escrow/sac-custody.md) |
| On-chain deadline enforcement | Deadlines are metadata only; timeout refunds require a client-initiated call | Informational |

---

## Future Improvements

- **Approval revocation** — allow a party to retract a recorded approval before
  the milestone is released, subject to the same role restrictions as approval.
- **Approval events** — emit structured events when approvals are recorded or
  cleared to improve off-chain auditability.
- **Minimum approval window** — enforce a minimum elapsed ledgers between the
  first approval and the release call to reduce front-running risk in
  ClientOnly mode.
