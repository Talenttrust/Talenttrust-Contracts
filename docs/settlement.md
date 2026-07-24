# Settlement Model

This document describes the settlement data model of the TalentTrust escrow
contract, the invariants that keep its books consistent, and the entrypoints
that touch settlement state. It is written for auditors and integrators; every
claim is cross-referenced to the source it was verified against.

Scope: `contracts/escrow` only. For the SAC token transfer lifecycle see
[`docs/escrow/settlement.md`](escrow/settlement.md); for fee math see
[`docs/escrow/protocol-fees.md`](escrow/protocol-fees.md); for the token-balance
conservation proof see
[`docs/escrow/balance-conservation-invariant.md`](escrow/balance-conservation-invariant.md).

## Data model

### Persistent state

All settlement state lives in persistent storage under these keys
(`src/types.rs`, `DataKey`):

| Key | Type | Meaning |
|---|---|---|
| `SettlementToken` | `Address` | The write-once bound SAC token all money movement uses. |
| `Contract(u32)` | `Contract` | Per-contract accounting record (below). |
| `(Contract(u32), "milestones")` | `Vec<Milestone>` | The contract's milestone ledger. |
| `AccumulatedProtocolFees` | `i128` | Protocol fees accrued across all contracts, held in-contract until `withdraw_protocol_fees`. |

### `Contract` (`src/types.rs`)

```rust
pub struct Contract {
    pub client: Address,
    pub freelancer: Address,
    pub arbiter: Option<Address>,
    pub status: ContractStatus,
    pub total_deposited: i128,   // lifetime gross deposits (never decreases)
    pub funded_amount: i128,     // current gross amount funded (capped at milestone sum)
    pub released_amount: i128,   // NET amounts paid out (releases + dispute freelancer payouts)
    pub refunded_amount: i128,   // amounts returned to the client (refunds + dispute client payouts)
    pub release_authorization: ReleaseAuthorization,
    pub reputation_issued: bool,
}
```

`ContractStatus`: `Created = 0`, `Accepted = 1`, `Funded = 2`, `Completed = 3`,
`Disputed = 4`, `Cancelled = 5`, `Refunded = 6`, `PartiallyFunded = 7`.

### `Milestone` (`src/types.rs`)

```rust
pub struct Milestone {
    pub amount: i128,            // gross value of the milestone
    pub funded_amount: i128,     // set to `amount` at release time (self-describing record)
    pub released: bool,
    pub refunded: bool,
    pub work_evidence: Option<String>,
    pub refunded_amount: i128,
    pub deadline: Option<u64>,   // Unix seconds; None = never expires
}
```

## Invariants

### I1. Accounting invariant (checked on every release)

```
released_amount + refunded_amount + accumulated_protocol_fees <= funded_amount
```

Enforced in `release_milestone` (`src/lib.rs`): after accruing the fee and
updating `released_amount`, the sum is recomputed and any excess panics with
`AccountingInvariantViolated` (error 27). `accumulated_protocol_fees` is the
global `AccumulatedProtocolFees` balance.

Note the term split: `released_amount` tracks **net** payouts to the
freelancer, while the fee portion of each released milestone stays in the
contract's token balance under `AccumulatedProtocolFees`. The two together
account for the full gross milestone amount.

### I2. Sufficient balance before any payout

`release_milestone` rejects with `InsufficientFunds` unless the available
balance covers the **gross** milestone amount, computed both ways:

- `funded_amount - released_amount - refunded_amount >= milestone.amount`
- `funded_amount - released_amount - refunded_amount - accumulated_fees >= milestone.amount`

so a release can never dip into accrued fees or other milestones' funds.

`refund_unreleased_milestones` (`src/lib.rs`) performs the same
`available_balance >= refund_amount` accounting check before transferring, so
a refund can never dip into accrued fees or other milestones' funds either.

### I3. Deposits are bounded by the milestone schedule

`deposit::validate_deposit` (`src/deposit.rs`) rejects with
`InvalidDepositAmount` if `funded_amount + amount` would exceed the sum of all
milestone amounts, and with `AmountMustBePositive` for `amount <= 0`. All
additions use checked arithmetic (`PotentialOverflow` on overflow). Validation
runs **before** the SAC transfer so a rejected deposit never debits the client.

### I4. Milestone terminal flags are mutually exclusive

A milestone cannot be both released and refunded:

- `release_milestone` panics `AlreadyRefunded` on a refunded milestone and
  `MilestoneAlreadyReleased` on a released one.
- `refund_unreleased_milestones` panics `AlreadyReleased` / `AlreadyRefunded`
  symmetrically, rejects duplicate indices in one request
  (`DuplicateMilestoneInRefund`), and rejects empty requests
  (`EmptyRefundRequest`).

### I5. Fee math rounds down and never exceeds the gross amount

`calculate_protocol_fee(amount, fee_bps) = floor(amount * fee_bps / 10_000)`,
short-circuiting to `0` when `fee_bps == 0` (`src/lib.rs`). The freelancer
always receives at least `amount - fee`; the protocol receives at most the
floored fee. With `fee_bps` in `[0, 10_000]`, `fee <= amount` always holds.

### I6. Dispute resolutions conserve the available balance

`dispute::resolution_payouts` (`src/dispute.rs`) computes
`(client_payout, freelancer_payout)` over
`available = funded_amount - released_amount - refunded_amount`:

- `FullRefund` → `(available, 0)`
- `PartialRefund` → freelancer gets `floor(available * 30 / 100)`, client the remainder
- `FullPayout` → `(0, available)`
- `Split(s)` → rejected (`InvalidDisputeSplit`) if either leg is negative,
  exceeds `available`, or the two legs do not sum to `available` **exactly**
  (`src/dispute.rs`)

`resolve_dispute` then adds the client leg to `refunded_amount` and the
freelancer leg to `released_amount`, so I1 is preserved by construction.

### I7. Status machine reflects settlement progress

- Deposit completes funding: `Created`/`PartiallyFunded` → `Funded`.
- Release: when **all** milestones are `released || refunded` → `Completed`
  (pending reputation credit is granted to the freelancer).
- Refund: all refunded → `Refunded`; all released-or-refunded (mixed) →
  `Completed`; otherwise stays `Funded` (status logic inline in `src/lib.rs`).
- Dispute: `Funded | PartiallyFunded` → `Disputed` (`raise_dispute`);
  resolution → `Refunded` if `refunded_amount == funded_amount`, else
  `Completed` (`final_status_after_resolution`).
- `Cancelled` and `Refunded` are terminal for value movement: deposits are
  rejected (`ContractCancelled` / `ContractRefunded`, `src/deposit.rs`) and
  refunds are rejected (`InvalidState`, `src/lib.rs`).
- `finalize_contract` writes immutable close metadata; afterwards any
  contract-specific mutation fails with `AlreadyFinalized`.

### I8. Transfers and state mutations are atomic

Stellar/Soroban executions are atomic: if any step of an entrypoint panics —
including the SAC `token::Client::transfer` call — the whole transaction
reverts. The books can therefore never record a movement that did not happen,
and no partial state is observable even if a malicious token contract re-enters
during a transfer. Note that ordering differs per entrypoint (e.g.
`deposit_funds` and `release_milestone` transfer before finalizing their state
updates), so the guarantee comes from atomicity, not from checks-effects-
interactions ordering; `bind_settlement_token` performs no transfer at all,
only a read-only `balance` probe.

## Entrypoints that touch settlement state

| Entrypoint | Effect on the model |
|---|---|
| `create_contract` | Writes the `Contract` record and the milestone ledger (`src/create_contract.rs`). |
| `bind_settlement_token` / `set_settlement_token` | Write-once binding of the SAC token; second call rejected with `SettlementTokenAlreadyBound`. Probes the candidate with a read-only `balance` call (`InvalidSettlementToken`), rejects self (`SettlementTokenIsSelf`) and admin (`SettlementTokenIsAdmin`). |
| `deposit_funds` | `funded_amount += amount`, `total_deposited += amount`; pulls `amount` from client to contract; transitions to `Funded` when the milestone sum is reached. |
| `approve_milestone_release` | Stages approvals (temporary storage, TTL ~7 days); no money movement, but gates `release_milestone`. |
| `release_milestone` | Transfers `net = gross - fee` to the freelancer; accrues `fee` into `AccumulatedProtocolFees`; sets `milestone.released`, `milestone.funded_amount = gross`; `released_amount += net`; checks I1/I2; may complete the contract. Emits `mlstn_rls`, and `ctrct_cmp` on completion. |
| `refund_unreleased_milestones` | Transfers the summed gross amounts back to the client; sets `refunded` flags; `refunded_amount += total`; updates status per I7. |
| `raise_dispute` / `resolve_dispute` | Freezes releases; arbiter resolution updates the accounting per I6 (client leg → `refunded_amount`, freelancer leg → `released_amount`) and sets the final status. No token transfers occur at resolution; the recorded legs remain in the contract balance. Emits `dispute`/`resolved`. |
| `cancel_contract` | Client-only cancellation with refund of remaining balance; status → `Cancelled` (terminal). |
| `finalize_contract` | Writes immutable close metadata; freezes further mutation (`AlreadyFinalized`). |
| `withdraw_protocol_fees` | Moves accrued fees out of `AccumulatedProtocolFees` to a treasury address; reduces the on-chain balance by exactly the withdrawn amount. |
| `accept_client_migration` | Replaces the client address on the contract record. |
| `submit_work_evidence` | Attaches evidence to a milestone (mutates the ledger). |
| `issue_reputation` | Sets `reputation_issued` once the contract completes. |
| `is_milestone_overdue` | Read-only helper for deadline-based timeout refunds (`now > deadline`, strictly). |

Read-only views: `get_contract`, `get_contract_summary`, `get_milestones`,
`get_milestone`, `get_refundable_balance`, `get_accumulated_protocol_fees`,
`get_settlement_token`, `is_settlement_token_bound`, `contract_exists`,
`get_work_evidence`.

## Worked example

Setup: admin bound a USDC SAC token; `fee_bps = 250` (2.5%). Client creates a
contract with milestones `[1_000_000, 2_000_000, 3_000_000]` stroops
(total `6_000_000`), `ReleaseAuthorization::ClientOnly`, no arbiter.

1. **Deposit.** `deposit_funds(id, client, 6_000_000)`:
   `funded_amount = total_deposited = 6_000_000`; I3 holds
   (`6_000_000 <= 6_000_000`); status `Funded`.
2. **Release milestone 0.** `fee = floor(1_000_000 * 250 / 10_000) = 25_000`,
   `net = 975_000` transferred to the freelancer.
   `released_amount = 975_000`, `AccumulatedProtocolFees = 25_000`.
   I1: `975_000 + 0 + 25_000 = 1_000_000 <= 6_000_000`.
3. **Refund milestone 2.** `refund_unreleased_milestones(id, [2])` transfers
   `3_000_000` to the client. `refunded_amount = 3_000_000`.
   I1: `975_000 + 3_000_000 + 25_000 = 4_000_000 <= 6_000_000`.
   Status stays `Funded` (milestone 1 still open).
4. **Dispute on milestone 1.** With an arbiter assigned, `raise_dispute` moves
   status to `Disputed`. `available = 6_000_000 - 975_000 - 3_000_000 = 2_025_000`
   (note: `available` does **not** subtract the 25_000 accrued fee).
   The arbiter resolves `Split { client_amount: 1_000_000, freelancer_amount: 1_025_000 }`
   — the legs must sum to `available` exactly or the resolution is rejected:
   `refunded_amount = 4_000_000`, `released_amount = 2_000_000`, final status
   `Completed` (since `refunded_amount != funded_amount`).
   Quirk worth knowing: after this, `released + refunded + fees = 6_025_000`,
   i.e. `funded_amount` plus the accrued fee. I1 is enforced only in
   `release_milestone`, not at resolution, so the sum can sit over
   `funded_amount` by the reserved fee until fees are withdrawn.
5. **Treasury withdrawal.** `withdraw_protocol_fees(25_000, treasury)` moves
   the accrued fee out. The token balance goes from `2_025_000` to
   `2_000_000`. Because `resolve_dispute` performs no token transfers, the
   recorded dispute legs (`2_025_000`) are still held in-contract; the
   `25_000` difference against the remaining balance is exactly the
   reserved-fee quirk above.

Every transition above is reproducible against the test suites in
`contracts/escrow/src/test/` (see `accounting_invariants.rs`,
`resolution_payouts_prop.rs`, `deposit.rs`, `release.rs`).
