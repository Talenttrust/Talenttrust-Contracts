# Contracts Error Reference

This document lists every `EscrowError` code emitted by the TalentTrust escrow smart contract, explains the condition that triggers each code, describes how to avoid it, and cross-references the entrypoints that can return it.

The canonical definition lives in [`contracts/escrow/src/lib.rs`](../contracts/escrow/src/lib.rs) (the `EscrowError` enum).

---

## Quick-reference table

| Code | Name | Value |
|------|------|-------|
| 1 | `InvalidParticipant` | 1 |
| 2 | `EmptyMilestones` | 2 |
| 3 | `InvalidMilestoneAmount` | 3 |
| 4 | `InvalidDepositAmount` | 4 |
| 5 | `InvalidMilestone` | 5 |
| 6 | `ContractNotFound` | 6 |
| 7 | `EmptyRefundRequest` | 7 |
| 8 | `DuplicateMilestoneInRefund` | 8 |
| 9 | `AlreadyReleased` | 9 |
| 10 | `AlreadyRefunded` | 10 |
| 11 | `InsufficientFunds` | 11 |
| 12 | `AlreadyInitialized` | 12 |
| 13 | `InsufficientAccumulatedFees` | 13 |
| 14 | `NotInitialized` | 14 |
| 15 | `UnauthorizedRole` | 15 |
| 16 | `ContractPaused` | 16 |
| 17 | `EmergencyActive` | 17 |
| 18 | `InvalidState` | 18 |
| 19 | `InvalidRating` | 19 |
| 20 | `SelfRating` | 20 |
| 21 | `ReputationAlreadyIssued` | 21 |
| 22 | `NotCompleted` | 22 |
| 23 | `FreelancerMismatch` | 23 |
| 24 | `InvalidStatusTransition` | 24 |
| 25 | `ArbiterRequired` | 25 |
| 26 | `InvalidDisputeSplit` | 26 |
| 27 | `AccountingInvariantViolated` | 27 |
| 28 | `PotentialOverflow` | 28 |
| 29 | `AlreadyFinalized` | 29 |
| 30 | `AmountMustBePositive` | 30 |
| 31 | `SettlementTokenNotConfigured` | 31 |
| 32 | `SettlementTokenAlreadyBound` | 32 |
| 33 | `TotalCapExceeded` | 33 |
| 34 | `TooManyMilestones` | 34 |
| 35 | `MissingArbiter` | 35 |
| 36 | `InvalidArbiter` | 36 |
| 37 | `ContractCancelled` | 37 |
| 38 | `ContractRefunded` | 38 |
| 39 | `InvalidSettlementToken` | 39 |
| 40 | `SettlementTokenIsSelf` | 40 |
| 41 | `SettlementTokenIsAdmin` | 41 |
| 42 | `EmptyComment` | 42 |
| 43 | `CommentTooLong` | 43 |
| 44 | `InvalidProtocolParameters` | 44 |
| 45 | `InvalidWithdrawalAmount` | 45 |

---

## Error details

### `InvalidParticipant` (1)

**When it fires:** `create_contract` is called with `client == freelancer`. The same address cannot hold both roles in an escrow.

**How to avoid:** Supply two distinct, non-equal addresses for `client` and `freelancer`.

**Entrypoints:** `create_contract`

---

### `EmptyMilestones` (2)

**When it fires:** `create_contract` receives an empty milestone vector (`milestones.is_empty()`).

**How to avoid:** Provide at least one milestone with a positive amount.

**Entrypoints:** `create_contract`

---

### `InvalidMilestoneAmount` (3)

**When it fires:** One or more milestone amounts are `≤ 0`, or the sum of all milestone amounts overflows `i128`. Validated in `amount_validation::validate_milestone_amounts`.

**How to avoid:** Every milestone amount must be a positive `i128`. Keep individual amounts within `MAX_SINGLE_AMOUNT_STROOPS` and the total within the configured `max_escrow_total_stroops`.

**Entrypoints:** `create_contract`

---

### `InvalidDepositAmount` (4)

**When it fires:** `deposit_funds` receives an amount that is `≤ 0`, exceeds `MAX_SINGLE_AMOUNT_STROOPS`, or would push `funded_amount` above the contract's total milestone sum.

**How to avoid:** Deposit only positive amounts that do not exceed the remaining unfunded portion of the escrow total.

**Entrypoints:** `deposit_funds`

---

### `InvalidMilestone` (5)

**When it fires:** A milestone-specific operation targets an index that refers to an invalid or structurally inconsistent milestone record.

**How to avoid:** Only reference milestone indexes that exist in the contract's milestone vector. Use `get_milestones` to enumerate valid indexes before operating on them.

**Entrypoints:** `release_milestone`, `refund_unreleased_milestones`

---

### `ContractNotFound` (6)

**When it fires:** Any entrypoint that looks up a contract by ID finds no record under `DataKey::Contract(id)`. Also fires when the milestone vector for a contract is missing.

**How to avoid:** Only pass contract IDs returned by `create_contract` or confirmed present via `contract_exists`. Verify the ID range with `get_next_contract_id`.

**Entrypoints:** `get_contract`, `get_contract_summary`, `get_milestones`, `get_milestone`, `deposit_funds`, `release_milestone`, `refund_unreleased_milestones`, `cancel_contract`, `finalize_contract`, `issue_reputation`, `set_arbiter`

---

### `EmptyRefundRequest` (7)

**When it fires:** `refund_unreleased_milestones` is called with an empty index list.

**How to avoid:** Pass at least one milestone index in the refund request.

**Entrypoints:** `refund_unreleased_milestones`

---

### `DuplicateMilestoneInRefund` (8)

**When it fires:** `refund_unreleased_milestones` receives the same milestone index more than once in the input list.

**How to avoid:** Deduplicate the index list before calling `refund_unreleased_milestones`.

**Entrypoints:** `refund_unreleased_milestones`

---

### `AlreadyReleased` (9)

**When it fires:** An operation attempts to release a milestone that has already been released (`milestone.released == true`).

**How to avoid:** Check `get_milestone` or `get_contract_summary` first. Only release milestones whose `released` flag is `false`.

**Entrypoints:** `release_milestone`

---

### `AlreadyRefunded` (10)

**When it fires:** An operation attempts to refund a milestone that has already been refunded (`milestone.refunded == true`).

**How to avoid:** Check `get_milestone` before refunding. Only refund milestones whose `refunded` flag is `false`.

**Entrypoints:** `refund_unreleased_milestones`

---

### `InsufficientFunds` (11)

**When it fires:** `release_milestone` determines that the contract's available balance (`funded_amount - released_amount - refunded_amount`) is less than the milestone amount to be paid out.

**How to avoid:** Ensure the escrow is fully funded before releasing milestones. Deposits must cover the milestone amount being released.

**Entrypoints:** `release_milestone`

---

### `AlreadyInitialized` (12)

**When it fires:** `initialize` is called on a contract instance that already has `DataKey::Initialized == true`.

**How to avoid:** Call `initialize` exactly once during contract deployment. Use `is_initialized` or `get_admin` to check the initialization state before calling.

**Entrypoints:** `initialize`

---

### `InsufficientAccumulatedFees` (13)

**When it fires:** `withdraw_protocol_fees` is called with an amount that exceeds the value stored under `DataKey::AccumulatedProtocolFees`.

**How to avoid:** Read the current accumulated fee balance before requesting a withdrawal. Never request more than is available.

**Entrypoints:** `withdraw_protocol_fees`

---

### `NotInitialized` (14)

**When it fires:** Any lifecycle or money-flow entrypoint is called before `initialize` has been executed. All state-changing operations require initialization so that admin-controlled safety rails (pause, emergency controls, protocol fees) are always active before funds move.

**How to avoid:** Call `initialize(admin)` once during contract setup before invoking any other entrypoint.

**Entrypoints:** All state-changing entrypoints: `create_contract`, `deposit_funds`, `release_milestone`, `refund_unreleased_milestones`, `cancel_contract`, `issue_reputation`, `finalize_contract`, `set_arbiter`, `set_protocol_fee_bps`, `set_governed_params`, `set_contracts_parameters`, `set_max_settlement`, `withdraw_protocol_fees`, `pause`, `unpause`, `activate_emergency_pause`, `resolve_emergency`

---

### `UnauthorizedRole` (15)

**When it fires:** The caller's address does not match the role required by the entrypoint. For example: a non-client calls `deposit_funds`, a non-admin calls `set_protocol_fee_bps`, or an incorrect admin is supplied to `set_arbiter`.

**How to avoid:** Ensure the caller's address matches the stored role. Read `get_admin` for admin-gated operations and `get_contract` for client/freelancer/arbiter roles.

**Entrypoints:** `deposit_funds`, `release_milestone`, `set_arbiter`, `set_protocol_fee_bps`, `set_governed_params`, `set_contracts_parameters`, `set_max_settlement`, `pause`, `unpause`, `activate_emergency_pause`, `resolve_emergency`, `propose_governance_admin`, `bind_settlement_token`, `withdraw_protocol_fees`

---

### `ContractPaused` (16)

**When it fires:** Any mutating escrow operation is attempted while the admin has set the pause flag via `pause()`.

**How to avoid:** Check `is_paused()` before calling state-changing entrypoints. Wait for the admin to call `unpause()`.

**Entrypoints:** `create_contract`, `deposit_funds`, `release_milestone`, `refund_unreleased_milestones`, `cancel_contract`, `issue_reputation`, `set_arbiter`

---

### `EmergencyActive` (17)

**When it fires:** Any mutating escrow operation is attempted while the admin has set the emergency flag via `activate_emergency_pause()`.

**How to avoid:** Check `is_emergency()` before calling state-changing entrypoints. Wait for the admin to call `resolve_emergency()`.

**Entrypoints:** `create_contract`, `deposit_funds`, `release_milestone`, `refund_unreleased_milestones`, `cancel_contract`, `issue_reputation`

---

### `InvalidState` (18)

**When it fires:** A lifecycle operation is called on a contract that is not in the expected status. For example: `release_milestone` requires `Funded` status; `deposit_funds` requires `Created` or `PartiallyFunded`.

**How to avoid:** Read the contract's `status` field via `get_contract` before calling state-changing entrypoints. Follow the status machine: `Created → PartiallyFunded → Funded → Completed`.

**Entrypoints:** `deposit_funds`, `release_milestone`, `cancel_contract`, `issue_reputation`, `finalize_contract`, `accept_governance_admin`

---

### `InvalidRating` (19)

**When it fires:** `issue_reputation` receives a `rating` value outside the configured `[min_rating, max_rating]` range (default 1–5).

**How to avoid:** Keep ratings within bounds. Read the current config with `get_reputation_config` to know the allowed range.

**Entrypoints:** `issue_reputation`

---

### `SelfRating` (20)

**When it fires:** `issue_reputation` is called with the rater's address equal to the freelancer's address — self-rating is disallowed.

**How to avoid:** The caller of `issue_reputation` must be the client, not the freelancer of that contract.

**Entrypoints:** `issue_reputation`

---

### `ReputationAlreadyIssued` (21)

**When it fires:** `issue_reputation` is called for a contract that already has `DataKey::ReputationIssued(contract_id) == true`.

**How to avoid:** Check `get_contract_summary.reputation_issued` before calling. Reputation can only be issued once per completed contract.

**Entrypoints:** `issue_reputation`

---

### `NotCompleted` (22)

**When it fires:** `issue_reputation` or `finalize_contract` is called on a contract that has not reached `Completed` or `Disputed` status (for finalization) or `Completed` status (for reputation).

**How to avoid:** Only call `issue_reputation` after all milestones are released and the contract transitions to `Completed`. Only call `finalize_contract` on contracts in `Completed` or `Disputed` state.

**Entrypoints:** `issue_reputation`, `finalize_contract`

---

### `FreelancerMismatch` (23)

**When it fires:** `issue_reputation` is called with a `freelancer` argument that does not match the address stored in the contract.

**How to avoid:** Read `get_contract.freelancer` first and pass that exact address.

**Entrypoints:** `issue_reputation`

---

### `InvalidStatusTransition` (24)

**When it fires:** An operation attempts a status change that violates the contract's state machine (e.g., cancelling an already-completed contract).

**How to avoid:** Read the contract status before attempting transitions. Only valid status transitions are permitted.

**Entrypoints:** `cancel_contract`, `resolve_dispute`

---

### `ArbiterRequired` (25)

**When it fires:** `resolve_dispute` is called on a contract whose `release_authorization` is `ArbiterOnly` or `ClientAndArbiter` but no arbiter has been set.

**How to avoid:** Ensure an arbiter is assigned (via `create_contract` or `set_arbiter`) before initiating dispute resolution that requires arbiter involvement.

**Entrypoints:** `resolve_dispute`, `open_dispute`

---

### `InvalidDisputeSplit` (26)

**When it fires:** `resolve_dispute` is called with a `DisputeResolution::Split` where the client and freelancer amounts do not sum to the available balance.

**How to avoid:** Compute the available balance (`funded_amount - released_amount - refunded_amount`) from `get_refundable_balance` and ensure both payout amounts are non-negative and sum exactly to that value.

**Entrypoints:** `resolve_dispute`

---

### `AccountingInvariantViolated` (27)

**When it fires:** An internal consistency check detects that `released_amount + refunded_amount > funded_amount`. This indicates a serious bug and should never fire under normal operation.

**How to avoid:** This is a defense-in-depth guard. It cannot be triggered by correct client usage; it indicates an unexpected internal accounting error.

**Entrypoints:** Internal guard used in `release_milestone`, `refund_unreleased_milestones`

---

### `PotentialOverflow` (28)

**When it fires:** A checked arithmetic operation (`checked_add`, `checked_sub`, `checked_mul`) would overflow `i128`. Fired when accumulating milestone amounts or computing funded/released totals.

**How to avoid:** Keep milestone amounts and totals within safe `i128` bounds. The contract enforces a per-milestone cap (`MAX_SINGLE_AMOUNT_STROOPS`) and a per-contract total cap (`max_escrow_total_stroops`) in `create_contract` to prevent this in practice.

**Entrypoints:** `create_contract`, `deposit_funds`, `release_milestone`, `get_contract_summary`

---

### `AlreadyFinalized` (29)

**When it fires:** Any mutating, contract-specific operation (deposit, release, refund, cancel) is attempted after `finalize_contract` has been called for that contract ID.

**How to avoid:** Check `get_contract_summary` for finalization state. Once finalized, only read-only operations are permitted on a contract.

**Entrypoints:** `deposit_funds`, `release_milestone`, `refund_unreleased_milestones`, `cancel_contract`, `set_arbiter`

---

### `AmountMustBePositive` (30)

**When it fires:** A storage or event helper validates an amount and finds it is negative (`< 0`). Used in `validate_event_amounts` and `storage_validation::validate_stroop_amount`.

**How to avoid:** All amounts passed to money-flow entrypoints and event helpers must be `≥ 0`.

**Entrypoints:** `deposit_funds`, `emit_contract_indexed_event` (internal event helper)

---

### `SettlementTokenNotConfigured` (31)

**When it fires:** `deposit_funds`, `release_milestone`, `refund_unreleased_milestones`, `cancel_contract`, or `withdraw_protocol_fees` is called before `bind_settlement_token` has been called.

**How to avoid:** Call `bind_settlement_token(admin, token)` after `initialize` and before any money-flow operation. Use `is_settlement_token_bound()` as a pre-flight check.

**Entrypoints:** `deposit_funds`, `release_milestone`, `refund_unreleased_milestones`, `cancel_contract`, `withdraw_protocol_fees`

---

### `SettlementTokenAlreadyBound` (32)

**When it fires:** `bind_settlement_token` is called a second time. The settlement token is a write-once field.

**How to avoid:** Call `bind_settlement_token` exactly once. Use `get_settlement_token` to read the currently bound token.

**Entrypoints:** `bind_settlement_token`

---

### `TotalCapExceeded` (33)

**When it fires:** The sum of all milestone amounts in `create_contract` exceeds the configured `max_escrow_total_stroops` (from `GovernedParameters` or the default cap).

**How to avoid:** Keep the total escrow value below the configured cap. Read `get_governed_parameters` or `get_bounds` to learn the current cap.

**Entrypoints:** `create_contract`

---

### `TooManyMilestones` (34)

**When it fires:** `create_contract` receives more milestones than the configured maximum (`MAX_MILESTONES`, default 10, adjustable via `set_max_milestones`).

**How to avoid:** Keep the number of milestones at or below `get_max_milestones()`.

**Entrypoints:** `create_contract`

---

### `MissingArbiter` (35)

**When it fires:** `create_contract` is called with `release_authorization` set to `ArbiterOnly` or `ClientAndArbiter` but `arbiter` is `None`. Also fires in `set_arbiter` if trying to remove an arbiter from a contract that requires one.

**How to avoid:** Provide a non-`None` arbiter when using `ArbiterOnly` or `ClientAndArbiter` release modes.

**Entrypoints:** `create_contract`, `set_arbiter`

---

### `InvalidArbiter` (36)

**When it fires:** The supplied arbiter address is the same as `client` or `freelancer`. An arbiter must be a neutral third party.

**How to avoid:** Supply an arbiter address that is distinct from both `client` and `freelancer`.

**Entrypoints:** `create_contract`, `set_arbiter`

---

### `ContractCancelled` (37)

**When it fires:** A value-moving operation (`deposit_funds`, `release_milestone`, `refund_unreleased_milestones`) is attempted on a contract already in `Cancelled` status.

**How to avoid:** Check `get_contract.status` before attempting operations. Cancelled contracts are terminal — no further value operations are permitted.

**Entrypoints:** `deposit_funds`, `release_milestone`, `refund_unreleased_milestones`

---

### `ContractRefunded` (38)

**When it fires:** A value-moving operation is attempted on a contract already in `Refunded` status.

**How to avoid:** Check `get_contract.status` before attempting operations. Refunded contracts are terminal.

**Entrypoints:** `deposit_funds`

---

### `InvalidSettlementToken` (39)

**When it fires:** `bind_settlement_token` performs a read-only probe (`token::Client::balance`) against the candidate address and the call panics — the address does not implement the SAC token interface.

**How to avoid:** Only bind a valid, deployed Stellar Asset Contract (SAC) address. Verify the token contract is live before calling `bind_settlement_token`.

**Entrypoints:** `bind_settlement_token`

---

### `SettlementTokenIsSelf` (40)

**When it fires:** `bind_settlement_token` is called with `token == env.current_contract_address()`. Binding the escrow contract as its own settlement token creates a circular custody reference.

**How to avoid:** Never pass the escrow contract's own address as the settlement token.

**Entrypoints:** `bind_settlement_token`

---

### `SettlementTokenIsAdmin` (41)

**When it fires:** `bind_settlement_token` is called with `token == stored_admin`. Conflating governance authority with the settlement token role is a privilege-separation violation.

**How to avoid:** Never pass the admin address as the settlement token.

**Entrypoints:** `bind_settlement_token`

---

### `EmptyComment` (42)

**When it fires:** `issue_reputation` receives an empty string for the `comment` field.

**How to avoid:** Provide a non-empty, non-whitespace comment when issuing reputation feedback.

**Entrypoints:** `issue_reputation`

---

### `CommentTooLong` (43)

**When it fires:** `issue_reputation` receives a `comment` that exceeds the configured `max_comment_bytes` limit (default 200 bytes).

**How to avoid:** Keep comments within the byte limit. Read `get_reputation_config.max_comment_bytes` to learn the current cap.

**Entrypoints:** `issue_reputation`

---

### `InvalidProtocolParameters` (44)

**When it fires:** `set_protocol_fee_bps` or `set_governed_params` receives a `protocol_fee_bps` value greater than `10_000` (100%). Also fires in `set_max_milestones` if the value is outside `[MIN_MAX_MILESTONES, MAX_MAX_MILESTONES]`, and in `set_arbiter_config` if the basis-point split does not sum to 10 000.

**How to avoid:**
- Protocol fee: keep `new_bps ≤ 10_000`.
- Milestone cap: keep value within `[1, 100]`.
- Arbiter split: ensure `freelancer_bps + client_bps == 10_000`.

**Entrypoints:** `set_protocol_fee_bps`, `set_governed_params`, `set_max_milestones`, `set_arbiter_config`

---

### `InvalidWithdrawalAmount` (45)

**When it fires:** `withdraw_protocol_fees` receives a withdrawal amount that is `≤ 0` or exceeds the maximum allowed per-operation withdrawal.

**How to avoid:** Only withdraw positive amounts at or below any per-operation cap. Check accumulated fees with `get_accumulated_fees` first.

**Entrypoints:** `withdraw_protocol_fees`

---

## Integration guidance

### Pre-flight checks

Before calling a money-flow entrypoint, use these read-only probes to avoid the most common errors:

```rust
// 1. Confirm initialization
assert!(client.get_admin().is_some(), "not initialized");

// 2. Confirm not paused / emergency
assert!(!client.is_paused(), "contract paused");
assert!(!client.is_emergency(), "emergency active");

// 3. Confirm settlement token is bound before deposits/releases
assert!(client.is_settlement_token_bound(), "no settlement token");

// 4. Confirm contract exists and is in the right state
let contract = client.get_contract(&contract_id);
assert_eq!(contract.status, ContractStatus::Funded, "not funded");

// 5. Confirm milestone is actionable
let milestone = client.get_milestone(&contract_id, &index).unwrap();
assert!(!milestone.released, "already released");
assert!(!milestone.refunded, "already refunded");
```

### Error numeric codes

All `EscrowError` variants are `#[repr(u32)]` and are transmitted as their numeric discriminant in Soroban error values. Off-chain SDKs should map the received `u32` code to the enum name using the table above.

### See also

- [ABI reference](escrow/abi-reference.md) — full entrypoint signatures
- [Authorization model](contracts-auth.md) — who can call what
- [Storage model](contracts-storage.md) — what state each error touches
- [Emergency controls](escrow/emergency-controls.md) — pause and emergency flag semantics
