# Escrow Contract

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

Latest local verification:

```text
cargo test -p escrow
running 42 tests
test result: ok. 42 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```
