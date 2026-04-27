# Pull Request: Deterministic Escrow Lifecycle Events

## Summary

This PR standardizes escrow lifecycle event topics and payloads to provide deterministic indexing for create/deposit/approve/release/cancel flows.

## What changed

- `contracts/escrow/src/lib.rs`
  - Added shared `emit_lifecycle_event(...)` helper.
  - Standardized event topic format to:
    - `("escrow", "v1", operation, contract_id)`
  - Standardized event payload format to:
    - `(status, amount, milestone_index, actor, timestamp)`
  - Wired helper into:
    - `create_contract` (`create`)
    - `deposit_funds` (`deposit`)
    - `approve_milestone` (`approve`)
    - `release_milestone` (`release`)
    - `cancel_contract` (`cancel`)

- `contracts/escrow/src/test/cancel_contract.rs`
  - Updated cancellation event test to assert that an event is emitted.

- `docs/escrow/README.md`
  - Added explicit deterministic lifecycle event schema and operation mappings.
  - Added migration note for legacy cancellation topic consumers.

- `docs/escrow/contract.md`
  - Updated cancel event documentation to the v1 deterministic schema.
  - Added lifecycle event schema section and breaking-change note.

## Backward compatibility

- Breaking behavior for event consumers:
  - Legacy cancellation topic `("contract_cancelled", contract_id)` is replaced by the v1 lifecycle topic format.
  - Indexers and monitoring rules must migrate to `("escrow", "v1", "cancel", contract_id)`.

## Test plan

- [ ] `cargo build`
- [ ] `cargo test -p escrow`

> Note: Rust toolchain (`cargo`) is not available in the current execution environment, so build/test commands could not be run here.

## Security notes

- Event emission is now centralized to reduce schema drift between lifecycle endpoints.
- Topics and payload fields are fixed-shape and deterministic for safer downstream parsing.

---

Closes #259
