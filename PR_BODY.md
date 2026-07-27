## Description
Resolves #1122

Disputes's emitted events weren't asserted, so topic/payload drift could slip through. This PR adds test coverage specifically for the `dispute opened` and `dispute resolved` events, asserting the topic symbols and payload fields. 

## Changes
- **Added `raise_dispute_emits_opened_event`**: Tests the `("dispute", "opened")` event is emitted correctly when a dispute is raised, with the payload `(contract_id, caller)`.
- **Added `resolve_dispute_emits_resolved_event`**: Tests the `("dispute", "resolved")` event is emitted correctly when a dispute is resolved by an arbiter, with the payload `(contract_id, resolution_code)`.

Both tests assert:
1. No topic collisions.
2. The payload fields exactly match what's specified.
3. The event occurs immediately after the emitting call.

## Validation
*Note: Due to lack of permission to execute tests locally on this environment, manual verification of the test output is required via CI.*
Test commands that were meant to be executed:
- `cargo fmt`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test --package escrow`
