# Escrow Security Notes

This document summarizes security assumptions and threat handling for escrow storage planning and core flows.

## Controls Implemented

- Authorization:
  - `create_contract` requires client auth.
  - `deposit_funds` requires stored contract client auth.
  - `release_milestone` requires stored contract client auth.
  - `issue_reputation` requires stored contract client auth.
- Input and state validation:
  - participant addresses must differ
  - milestone list must be non-empty
  - milestone amounts must be positive
  - deposit amount must be positive
  - rating must be within `[1, 5]`
  - release requires funded status and unreleased milestone
  - reputation issuance requires completed contract and one-time issuance
- Arithmetic safety:
  - all amount/count updates use checked arithmetic with explicit errors.
- Storage version safety:
  - unknown layout versions are rejected
  - layout metadata is initialized deterministically
  - migration targets are explicit and validated

## Threat Scenarios and Mitigations

- Unauthorized state mutation:
  - Mitigated by `require_auth` on mutating methods.
- Overfunding / accounting drift:
  - Mitigated with total funding cap and release-balance checks.
- Duplicate release attacks:
  - Mitigated with per-milestone release flag and state transitions.
- Cross-version decode risk after upgrades:
  - Mitigated by explicit `LayoutVersion` checks before reads/writes.
- Ambiguous migration execution:
  - Mitigated by explicit `migrate_storage(target_version)` with strict target validation.

## Residual Assumptions

- Token transfer plumbing is out of scope here; accounting is contract-state based.
- Dispute flow (`Disputed`) is reserved for future feature implementation.
- Production fee/resource values should be validated using network simulation tooling.
