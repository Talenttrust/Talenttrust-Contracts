# Implementation Plan: mainnet-readiness-surface

## Overview

Extend the TalentTrust escrow contract with a `get_mainnet_readiness_info` view function that returns a `MainnetReadinessInfo` snapshot. The implementation adds two new types (`ReadinessChecklist` and `MainnetReadinessInfo`), a new `DataKey::ReadinessChecklist` variant, a private helper to update the checklist, and hooks into existing lifecycle functions — all in Rust targeting the Soroban SDK.

## Tasks

- [x] 1. Define types and storage key for the readiness surface
  - Add `DataKey::ReadinessChecklist` variant to the existing `DataKey` enum in `contracts/escrow/src/types.rs`
  - Define `ReadinessChecklist` struct with `#[contracttype]`, `Clone`, `Debug`, `Eq`, `PartialEq`, and a `Default` impl that sets all booleans to `false`
  - Define `MainnetReadinessInfo` struct with `#[contracttype]`, `Clone`, `Debug`, `Eq`, `PartialEq`, fields: `caps_set: bool`, `governed_params_set: bool`, `emergency_controls_enabled: bool`, `initialized: bool`, `protocol_version: u32`, `max_escrow_total_stroops: i128`
  - Add compile-time constants `MAINNET_PROTOCOL_VERSION: u32` and `MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS: i128` to `lib.rs` (or a dedicated `constants.rs`)
  - Re-export `MainnetReadinessInfo` and the constants from `lib.rs` so they are accessible to tests and the generated client
  - _Requirements: 1.1, 1.2, 1.3, 1.4, 1.5, 2.1_

- [x] 2. Implement the `update_readiness_checklist` helper and `get_mainnet_readiness_info` view function
  - [x] 2.1 Implement private `update_readiness_checklist<F>(env: &Env, f: F)` helper in `lib.rs` (or a new `readiness.rs` module)
    - Read `DataKey::ReadinessChecklist` from `env.storage().instance()` using `unwrap_or_default()`
    - Apply the closure `f` to mutate the checklist
    - Write the updated checklist back to instance storage
    - _Requirements: 2.1, 2.2, 2.3_

  - [x] 2.2 Implement `get_mainnet_readiness_info(env: Env) -> MainnetReadinessInfo` on `Escrow`
    - Read `DataKey::ReadinessChecklist` from instance storage using `unwrap_or_default()` — no panics
    - Compute `caps_set` inline as `MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS > 0`
    - Populate `protocol_version` and `max_escrow_total_stroops` from compile-time constants
    - No `require_auth`, no storage writes, no event emissions
    - _Requirements: 3.1, 3.2, 3.3, 3.4, 3.5, 3.6, 3.7, 6.1, 6.2, 6.3_

  - [ ]* 2.3 Write property test for `get_mainnet_readiness_info` idempotence and no-panic (Properties 5 & 6)
    - Add to `contracts/escrow/src/proptest.rs`
    - Tag: `// Feature: mainnet-readiness-surface, Property 5: get_mainnet_readiness_info is idempotent`
    - Tag: `// Feature: mainnet-readiness-surface, Property 6: get_mainnet_readiness_info never panics`
    - Generate random seed `0u32..8u32` to apply any combination of lifecycle ops; call `get_mainnet_readiness_info` twice; assert results are equal and no panic
    - Minimum 100 iterations (`ProptestConfig::with_cases(100)`)
    - **Property 5: `get_mainnet_readiness_info` is idempotent (no writes)**
    - **Property 6: `get_mainnet_readiness_info` never panics**
    - **Validates: Requirements 3.2, 3.3, 7.5**

  - [ ]* 2.4 Write property test for constant fields invariant (Property 7)
    - Add to `contracts/escrow/src/proptest.rs`
    - Tag: `// Feature: mainnet-readiness-surface, Property 7: constant fields always reflect compile-time constants`
    - For any contract state, assert `info.protocol_version == MAINNET_PROTOCOL_VERSION` and `info.max_escrow_total_stroops == MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS`
    - Minimum 100 iterations
    - **Property 7: Constant fields always reflect compile-time constants**
    - **Validates: Requirements 3.6**

- [x] 3. Hook checklist updates into lifecycle functions
  - [x] 3.1 In `initialize` (or equivalent init function): call `update_readiness_checklist` to set `initialized = true` before returning
    - _Requirements: 2.7, 4.1_

  - [x] 3.2 In `initialize_protocol_governance`: call `update_readiness_checklist` to set `governed_params_set = true` before returning
    - _Requirements: 2.4, 4.2_

  - [x] 3.3 In `update_protocol_parameters`: call `update_readiness_checklist` to set `governed_params_set = true` before returning
    - _Requirements: 2.5, 4.3_

  - [x] 3.4 In `activate_emergency_pause`: call `update_readiness_checklist` to set `emergency_controls_enabled = true` before returning
    - _Requirements: 2.6, 4.4_

  - [x] 3.5 In `resolve_emergency`: call `update_readiness_checklist` to set `emergency_controls_enabled = true` before returning
    - _Requirements: 2.6, 4.5_

  - [ ]* 3.6 Write property test for checklist round-trip (Property 8)
    - Add to `contracts/escrow/src/proptest.rs`
    - Tag: `// Feature: mainnet-readiness-surface, Property 8: lifecycle write is visible in read`
    - Generate a random bitmask of lifecycle ops; execute the enabled ones; assert each corresponding boolean field is `true`
    - Minimum 100 iterations
    - **Property 8: Checklist round-trip — lifecycle write is visible in read**
    - **Validates: Requirements 2.3, 3.5**

- [x] 4. Write unit tests in `contracts/escrow/src/test/mainnet_readiness.rs`
  - [x] 4.1 Test: fresh contract returns all-false boolean fields (except `caps_set` which reflects the constant)
    - Register contract, call `get_mainnet_readiness_info`, assert `initialized == false`, `governed_params_set == false`, `emergency_controls_enabled == false`, `caps_set == (MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS > 0)`
    - _Requirements: 2.2, 3.3, 3.4, 5.1, 7.1_

  - [x] 4.2 Test: `initialize` sets `initialized` to `true`
    - Call `initialize`, then assert `get_mainnet_readiness_info().initialized == true`
    - _Requirements: 2.7, 4.1, 7.2_

  - [x] 4.3 Test: `initialize_protocol_governance` sets `governed_params_set` to `true`
    - Call `initialize_protocol_governance`, then assert `get_mainnet_readiness_info().governed_params_set == true`
    - _Requirements: 2.4, 4.2, 7.3_

  - [x] 4.4 Test: `update_protocol_parameters` also sets `governed_params_set` to `true`
    - Call `initialize_protocol_governance` then `update_protocol_parameters`, assert `governed_params_set == true`
    - _Requirements: 2.5, 4.3_

  - [x] 4.5 Test: `activate_emergency_pause` sets `emergency_controls_enabled` to `true`
    - Call `initialize` then `activate_emergency_pause`, assert `get_mainnet_readiness_info().emergency_controls_enabled == true`
    - _Requirements: 2.6, 4.4, 7.4_

  - [x] 4.6 Test: `resolve_emergency` also sets `emergency_controls_enabled` to `true`
    - Call `initialize`, `activate_emergency_pause`, `resolve_emergency`, assert `emergency_controls_enabled == true`
    - _Requirements: 2.6, 4.5_

  - [x] 4.7 Test: `caps_set` reflects the compile-time constant
    - Assert `get_mainnet_readiness_info().caps_set == (MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS > 0)`
    - _Requirements: 2.8, 7.7_

  - [x] 4.8 Test: `get_mainnet_readiness_info` requires no auth and emits no events
    - Call without mocking any auth; assert success and `env.events().all().is_empty()`
    - _Requirements: 6.1, 6.2_

  - [x] 4.9 Test: `get_mainnet_readiness_info` is idempotent across multiple calls
    - Call twice on the same contract state; assert both results are equal
    - _Requirements: 3.2, 7.5_

  - [x] 4.10 Test: missing storage returns safe defaults (backward compatibility)
    - Register a fresh contract (no lifecycle ops), call `get_mainnet_readiness_info`, assert no panic and all mutable boolean fields are `false`
    - _Requirements: 2.2, 5.1, 5.3, 7.6_

  - [x] 4.11 Test: failed lifecycle operation does not update checklist
    - Attempt a double-`initialize` (which should panic); catch the panic; assert `initialized` is still `false` on a fresh contract
    - _Requirements: 4.6_

- [x] 5. Checkpoint — ensure all tests pass
  - Run `cargo test -p escrow` and confirm all existing and new tests pass. Ask the user if any questions arise.

- [x] 6. Update documentation at `docs/escrow/mainnet-readiness.md`
  - Describe each field of `MainnetReadinessInfo` with its type, meaning, and the lifecycle event that sets it
  - Include an example JSON-like output for a fully-ready contract
  - Include a deployer validation guide listing conditions to verify before directing production traffic
  - Note that `get_mainnet_readiness_info` is read-only and safe to call from preflight scripts
  - _Requirements: 8.1, 8.2, 8.3, 8.4_

- [x] 7. Final checkpoint — ensure all tests pass
  - Run `cargo test -p escrow` and confirm the full test suite is green. Ask the user if any questions arise.

## Notes

- Tasks marked with `*` are optional and can be skipped for a faster MVP
- Each task references specific requirements for traceability
- Property tests use the `proptest` crate already present in the workspace (`contracts/escrow/src/proptest.rs`)
- Soroban transaction atomicity guarantees that a panic before the checklist write rolls back the entire write — no partial updates are possible (Requirement 4.6)
- `caps_set` is computed inline from the compile-time constant rather than persisted, keeping `ReadinessChecklist` minimal
