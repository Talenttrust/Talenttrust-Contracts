# Escrow Contract Fee Model

This module adds configurable protocol fee settings for the escrow milestone model.

## New features

- `protocol_fee_bps` configurable in `create_contract` (0-10000 basis points).
- `protocol_fee_account` set at creation time; only this account can withdraw fees and update fee rate.
- Per-milestone fee accounting via `Milestone.protocol_fee` and `EscrowContract.protocol_fee_accrued`.
- `get_protocol_fee_accrued` to query current fee balance.
- `withdraw_protocol_fees` for controlled withdrawal.
- `set_protocol_fee_bps` to update protocol fee rate with authorization.

## Security controls

- Only the `protocol_fee_account` can adjust fee rate or withdraw accrued fees.
- Fee account is authenticated with `caller.require_auth()`.
- Fee bounds enforced at 0..=10000.
- All protocol fee operations use persisted state and safe integer arithmetic.

## Behaviour on release

On each milestone release:
- Compute fee: `milestone.amount * protocol_fee_bps / 10000`.
- Save fee to milestone object.
- Increment `protocol_fee_accrued`.
- Mark milestone released and contract status completed when all milestones done.
# Escrow Contract Documentation

**Mainnet readiness (limits, events, risks):** [mainnet-readiness.md](mainnet-readiness.md)

This document summarizes the reviewer-facing architecture for `contracts/escrow`.

## Scope

The contract persists:

- escrow lifecycle state for each contract
- participant metadata for the client and freelancer
- milestone release state
- funded and released accounting
- pending and issued reputation aggregates
- protocol governance parameters
- pause and emergency flags

## Public Flows

Core escrow endpoints:

- `create_contract(client, freelancer, milestone_amounts) -> u32`
- `deposit_funds(contract_id, amount) -> bool`
- `release_milestone(contract_id, milestone_id) -> bool`
- `issue_reputation(contract_id, rating) -> bool`
- `get_contract(contract_id) -> EscrowContractData`
- `get_reputation(freelancer) -> Option<ReputationRecord>`
- `get_pending_reputation_credits(freelancer) -> u32`

Operational controls:

- `initialize(admin) -> bool`
- `pause() -> bool`
- `unpause() -> bool`
- `activate_emergency_pause() -> bool`
- `resolve_emergency() -> bool`
- `is_paused() -> bool`
- `is_emergency() -> bool`

Governance:

- `initialize_protocol_governance(admin, min_milestone_amount, max_milestones, min_reputation_rating, max_reputation_rating) -> bool`
- `update_protocol_parameters(...) -> bool`
- `propose_governance_admin(next_admin) -> bool`
- `accept_governance_admin() -> bool`
- `get_protocol_parameters() -> ProtocolParameters`
- `get_governance_admin() -> Option<Address>`
- `get_pending_governance_admin() -> Option<Address>`

The escrow tests are grouped into dedicated modules:

To prevent out-of-gas or infinite-loop denial of service attacks, the escrow contract enforces creation limits:

- maximum milestone count is capped by `ProtocolParameters.max_milestones` (defaults to 16)
- total escrow amount is bounded by the immutable mainnet cap (`MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS`)

## Lifecycle Model

Supported lifecycle transitions:

- `Created -> Funded` after any positive deposit
- `Funded -> Completed` after the final unreleased milestone is released

Operational invariants:

- client and freelancer addresses are immutable after creation
- milestone amounts are immutable after creation
- each milestone can transition from `released = false` to `released = true` exactly once
- `released_amount` is the sum of released milestone amounts
- `released_milestones` matches the number of released milestone flags
- `reputation_issued` can only become `true` after `Completed`

## Incident Response

### Emergency Response

1. Detect incident and call `activate_emergency_pause`.
2. Investigate and remediate root cause.
3. Validate mitigations in test/staging.
4. Call `resolve_emergency` to restore service.
5. Publish incident summary for ecosystem transparency.

## Persistence Notes

Each `EscrowContractData` record stores:

- participant addresses
- milestone vector and cached milestone count
- total escrow amount
- funded and released balances
- released milestone count
- contract status
- reputation issuance flag
- creation and update timestamps

Detailed storage-key coverage is documented in [state-persistence.md](state-persistence.md).

## Test Coverage

The escrow regression suite is split by concern:

- `flows.rs`: happy-path lifecycle and reputation aggregation
- `lifecycle.rs`: state transition persistence
- `persistence.rs`: storage round-trip assertions
- `security.rs`: failure paths and validation checks
- `governance.rs`: admin and parameter persistence
- `pause_controls.rs` and `emergency_controls.rs`: operational safety controls
- `performance.rs`: resource regression ceilings
