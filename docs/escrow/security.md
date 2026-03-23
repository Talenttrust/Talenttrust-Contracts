# Escrow Security Notes

This document describes assumptions, controls, and threat scenarios for `contracts/escrow`.

## Security Controls Implemented

- Authorization checks on state-mutating flows:
  - `create_contract`: requires `client` auth.
  - `deposit_funds`: requires stored contract client auth.
  - `release_milestone`: requires stored contract client auth.
  - `issue_reputation`: requires stored contract client auth.
- Input validation:
  - participant addresses must differ.
  - milestone list must be non-empty.
  - milestone amounts must be strictly positive.
  - deposit amount must be strictly positive.
  - rating must be in `[1, 5]`.
- State-machine validation:
  - release requires funded state.
  - issue reputation requires completed state.
  - reputation can be issued once per contract.
- Funds integrity checks:
  - funded amount cannot exceed milestone total.
  - milestone release requires sufficient funded-minus-released balance.
  - double-release for the same milestone is rejected.
- Arithmetic safety:
  - all accumulators use checked arithmetic with explicit overflow error handling.

## Threat Scenarios and Mitigations

- Unauthorized state changes:
  - Mitigated by `require_auth` on all mutating entrypoints.
- Over-funding / balance drift:
  - Mitigated by explicit cap `funded_amount <= total_amount`.
- Premature or duplicate payouts:
  - Mitigated by contract status checks and per-milestone `released` flag.
- Out-of-range reputation manipulation:
  - Mitigated by rating bounds and one-issuance-per-contract enforcement.
- DoS via pathological milestone release scanning:
  - Mitigated by storing `released_milestones` and `milestone_count`, allowing O(1) completion check.

## Remaining Assumptions / Out-of-Scope

- Token transfer integration is not part of this module yet (balance accounting is logical contract state).
- Dispute workflow (`ContractStatus::Disputed`) is reserved for future implementation.
- Final transaction-fee accuracy should be validated with network simulation tooling before production deployment.

## Test Coverage Areas

Tests are organized in:

- `contracts/escrow/src/test/flows.rs` for happy paths and state persistence.
- `contracts/escrow/src/test/security.rs` for edge cases, invalid inputs, and failure paths.
- `contracts/escrow/src/test/performance.rs` for resource and fee regression baselines.
