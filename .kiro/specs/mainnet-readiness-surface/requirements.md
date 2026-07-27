# Requirements Document

## Introduction

This feature exposes a read-only view function `get_mainnet_readiness_info` on the escrow contract that returns a `MainnetReadinessInfo` snapshot. The snapshot reflects deployment-critical conditions: hard caps, governed protocol parameters, and emergency control state. Deployers and monitoring tools use this function to validate contract readiness post-deploy without mutating state. The checklist fields are updated atomically during existing lifecycle operations (governance initialization, parameter updates, emergency controls, contract initialization) and persisted in instance storage with safe defaults.

## Glossary

- **Escrow**: The Soroban smart contract under `contracts/escrow` that manages milestone-based escrow agreements.
- **MainnetReadinessInfo**: A read-only struct returned by `get_mainnet_readiness_info` that surfaces deployment-critical fields.
- **ReadinessChecklist**: The persistent instance-storage record that tracks whether each deployment-critical condition has been satisfied.
- **Caps**: The hard-coded maximum total escrow value per contract (`MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS`), fixed in WASM.
- **GovernedParams**: Protocol parameters controlled by the governance admin (`ProtocolParameters`: min/max milestone amounts, max milestones, reputation rating bounds).
- **EmergencyControls**: The pause/emergency mechanism that allows an admin to halt contract operations.
- **ProtocolGovernance**: The on-chain governance subsystem that manages `GovernedParams` and admin transfer.
- **InstanceStorage**: Soroban `env.storage().instance()`, used for contract-level singleton state.
- **Deployer**: An operator or script that deploys and validates the escrow contract before directing production traffic to it.
- **ViewFunction**: A contract function that reads state and returns data without modifying storage or emitting events.

---

## Requirements

### Requirement 1: MainnetReadinessInfo Data Structure

**User Story:** As a deployer, I want a stable, versioned data structure that captures all deployment-critical conditions, so that I can programmatically validate contract readiness without parsing raw storage.

#### Acceptance Criteria

1. THE Escrow SHALL expose a `MainnetReadinessInfo` struct with the following boolean fields: `caps_set`, `governed_params_set`, `emergency_controls_enabled`, `initialized`.
2. THE `MainnetReadinessInfo` struct SHALL include a `protocol_version` field of type `u32` that reflects the current WASM protocol version constant.
3. THE `MainnetReadinessInfo` struct SHALL include a `max_escrow_total_stroops` field of type `i128` that reflects the hard-coded cap constant.
4. THE `MainnetReadinessInfo` struct SHALL be annotated with `#[contracttype]` so it is ABI-stable and accessible to Soroban clients.
5. THE `MainnetReadinessInfo` struct SHALL derive `Clone`, `Debug`, `Eq`, and `PartialEq` to support testing and comparison.
6. WHERE future fields are added to `MainnetReadinessInfo`, THE Escrow SHALL remain backward-compatible by providing default values for any new fields when reading older storage entries.

---

### Requirement 2: Persistent ReadinessChecklist Storage

**User Story:** As a deployer, I want the readiness state to be persisted on-chain so that the view function always reflects the true current state of the contract, even across multiple calls and ledger boundaries.

#### Acceptance Criteria

1. THE Escrow SHALL store the `ReadinessChecklist` in `env.storage().instance()` under a dedicated `DataKey` variant.
2. WHEN the `ReadinessChecklist` key is absent from instance storage, THE Escrow SHALL treat all boolean fields as `false` (safe defaults).
3. THE Escrow SHALL update the `ReadinessChecklist` atomically within the same transaction as the lifecycle operation that satisfies each condition.
4. WHEN `initialize_protocol_governance` completes successfully, THE Escrow SHALL set `governed_params_set` to `true` in the `ReadinessChecklist`.
5. WHEN `update_protocol_parameters` completes successfully, THE Escrow SHALL set `governed_params_set` to `true` in the `ReadinessChecklist`.
6. WHEN `activate_emergency_pause` or `resolve_emergency` completes successfully, THE Escrow SHALL set `emergency_controls_enabled` to `true` in the `ReadinessChecklist`.
7. WHEN the contract `initialize` function completes successfully, THE Escrow SHALL set `initialized` to `true` in the `ReadinessChecklist`.
8. WHEN `MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS` is defined as a non-zero constant, THE Escrow SHALL set `caps_set` to `true` in the `ReadinessChecklist` at the time the checklist is first read or initialized.

---

### Requirement 3: Read-Only View Function

**User Story:** As a deployer or monitoring tool, I want a single read-only function that returns the full readiness snapshot, so that I can validate all deployment-critical conditions in one call without side effects.

#### Acceptance Criteria

1. THE Escrow SHALL expose a public function `get_mainnet_readiness_info(env: Env) -> MainnetReadinessInfo`.
2. THE `get_mainnet_readiness_info` function SHALL be a pure read operation and SHALL NOT write to any storage entry.
3. THE `get_mainnet_readiness_info` function SHALL NOT panic under any reachable condition, including absent storage keys.
4. WHEN instance storage contains no `ReadinessChecklist` entry, THE `get_mainnet_readiness_info` function SHALL return a `MainnetReadinessInfo` with all boolean fields set to `false` and constant fields populated from compile-time constants.
5. WHEN instance storage contains a `ReadinessChecklist` entry, THE `get_mainnet_readiness_info` function SHALL return a `MainnetReadinessInfo` whose boolean fields reflect the persisted checklist values.
6. THE `get_mainnet_readiness_info` function SHALL populate `protocol_version` from the `MAINNET_PROTOCOL_VERSION` constant and `max_escrow_total_stroops` from the `MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS` constant regardless of storage state.
7. THE `get_mainnet_readiness_info` function SHALL NOT expose any sensitive data such as admin addresses, private keys, or internal counters beyond the defined `MainnetReadinessInfo` fields.

---

### Requirement 4: Lifecycle Integration

**User Story:** As a contract operator, I want the readiness checklist to be updated automatically during normal lifecycle operations, so that the readiness view always reflects the actual contract state without requiring a separate update call.

#### Acceptance Criteria

1. WHEN `initialize` is called and succeeds, THE Escrow SHALL atomically update `initialized` to `true` in the `ReadinessChecklist` before returning.
2. WHEN `initialize_protocol_governance` is called and succeeds, THE Escrow SHALL atomically update `governed_params_set` to `true` in the `ReadinessChecklist` before returning.
3. WHEN `update_protocol_parameters` is called and succeeds, THE Escrow SHALL atomically update `governed_params_set` to `true` in the `ReadinessChecklist` before returning.
4. WHEN `activate_emergency_pause` is called and succeeds, THE Escrow SHALL atomically update `emergency_controls_enabled` to `true` in the `ReadinessChecklist` before returning.
5. WHEN `resolve_emergency` is called and succeeds, THE Escrow SHALL atomically update `emergency_controls_enabled` to `true` in the `ReadinessChecklist` before returning.
6. IF a lifecycle operation fails or panics before completion, THEN THE Escrow SHALL NOT update the `ReadinessChecklist` for that operation (atomicity guarantee via Soroban transaction rollback).

---

### Requirement 5: Backward Compatibility

**User Story:** As a deployer upgrading an existing contract, I want the readiness view function to return safe defaults for any missing storage fields, so that a WASM upgrade does not break existing monitoring scripts.

#### Acceptance Criteria

1. WHEN `get_mainnet_readiness_info` is called on a contract instance that has no `ReadinessChecklist` in storage, THE Escrow SHALL return a `MainnetReadinessInfo` with all boolean fields set to `false`.
2. WHEN `get_mainnet_readiness_info` is called after a WASM upgrade that adds new fields to `MainnetReadinessInfo`, THE Escrow SHALL return `false` for any new boolean fields not present in the stored checklist.
3. THE Escrow SHALL NOT panic or return an error when reading a `ReadinessChecklist` entry that was written by an older version of the contract.

---

### Requirement 6: Security Constraints

**User Story:** As a security auditor, I want the readiness surface to be strictly read-only and free of information leakage, so that it cannot be exploited to mutate state or extract sensitive data.

#### Acceptance Criteria

1. THE `get_mainnet_readiness_info` function SHALL require no authorization (no `require_auth` calls) since it is a public read-only view.
2. THE `get_mainnet_readiness_info` function SHALL NOT emit any contract events.
3. THE `MainnetReadinessInfo` struct SHALL NOT include admin addresses, pending admin addresses, private keys, or any data that could aid an attacker in targeting the contract.
4. THE Escrow SHALL NOT provide any function that allows external callers to directly write to the `ReadinessChecklist` storage key outside of the defined lifecycle operations.

---

### Requirement 7: Testing Coverage

**User Story:** As a developer, I want comprehensive tests for the readiness surface, so that regressions in readiness reporting are caught before deployment.

#### Acceptance Criteria

1. THE test suite SHALL include a test that calls `get_mainnet_readiness_info` on a freshly registered contract and asserts all boolean fields are `false`.
2. THE test suite SHALL include a test that calls `initialize`, then `get_mainnet_readiness_info`, and asserts `initialized` is `true`.
3. THE test suite SHALL include a test that calls `initialize_protocol_governance`, then `get_mainnet_readiness_info`, and asserts `governed_params_set` is `true`.
4. THE test suite SHALL include a test that calls `activate_emergency_pause`, then `get_mainnet_readiness_info`, and asserts `emergency_controls_enabled` is `true`.
5. THE test suite SHALL include a test that calls `get_mainnet_readiness_info` multiple times and asserts the returned value is identical across calls (idempotence).
6. THE test suite SHALL include a test that simulates missing storage (backward compatibility) and asserts `get_mainnet_readiness_info` returns all-false boolean fields without panicking.
7. THE test suite SHALL include a test that verifies `caps_set` is `true` whenever `MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS` is non-zero.

---

### Requirement 8: Documentation

**User Story:** As a deployer, I want up-to-date documentation that explains each readiness field and provides a validation guide, so that I can confidently use the readiness surface in deployment scripts.

#### Acceptance Criteria

1. THE documentation at `docs/escrow/mainnet-readiness.md` SHALL describe each field of `MainnetReadinessInfo` with its type, meaning, and the lifecycle event that sets it.
2. THE documentation SHALL include an example JSON-like output of `get_mainnet_readiness_info` for a fully-ready contract.
3. THE documentation SHALL include a deployer validation guide that lists the conditions a deployer MUST verify before directing production traffic to the contract.
4. THE documentation SHALL note that `get_mainnet_readiness_info` is read-only and safe to call from preflight scripts without side effects.
