# Mainnet Readiness Runbook and Checklist

This document explains what "mainnet ready" means for the Talenttrust Escrow contract, details the verification flags tracked in the `ReadinessChecklist`, and outlines the step-by-step workflow that operators must follow to prepare and verify a fresh contract instance for production deployment on Stellar Mainnet.

---

## What is Mainnet Readiness?

Before opening the Escrow contract to clients and freelancers on the mainnet, the contract instance must be properly initialized, governed with realistic fee parameters and limits, and verified to ensure that the admin retains operational emergency pause controls. 

The contract tracks these stages using the `ReadinessChecklist` struct, which is persisted in instance storage and returned by the `get_mainnet_readiness_info` view function.

---

## ReadinessChecklist Fields

The checklist consists of three critical mutable flags:

### 1. `initialized`
* **Purpose**: Verifies that the contract has been bound to an admin identity.
* **Where it is set**: Flips to `true` inside the `initialize` method upon successful completion.
* **Why it gates readiness**: An uninitialized contract has no admin security context. Anyone could call `initialize` and take ownership if left unconfigured in production.

### 2. `governed_params_set`
* **Purpose**: Verifies that realistic protocol parameters have been configured by the admin.
* **Where it is set**: Flips to `true` inside the `set_governed_params` method upon successful completion.
* **Why it gates readiness**: Default parameters are unset or set to zero/safe placeholders. Governance parameters (such as the protocol fee in basis points and max escrow total stroops limits) must be set to values matching the business requirements.

### 3. `emergency_controls_enabled`
* **Purpose**: Verifies that the admin can successfully invoke emergency security procedures (e.g. pausing the contract in case of a bug or exploit).
* **Where it is set**: Flips to `true` inside the `activate_emergency_pause` method.
* **Why it gates readiness**: Ensures that the admin's multi-sig or cold storage keys are correctly authorized to trigger emergency procedures, avoiding lockouts when an actual emergency occurs.

---

## Operator Runbook & Checklist

Operators must execute the following sequential workflow on a newly deployed contract instance to satisfy all gates:

### Step 1: Initialize the Contract
Call `initialize` with the designated admin address:
```rust
client.initialize(&admin);
```
* **Result**: `initialized` becomes `true`.

### Step 2: Configure Governed Parameters
Call `set_governed_params` with the desired protocol fee (in bps) and limit caps:
```rust
client.set_governed_params(&admin, &protocol_fee_bps, &max_escrow_total_stroops);
```
* **Result**: `governed_params_set` becomes `true`.

### Step 3: Exercise Emergency Controls
Verify emergency pause functionality by calling `activate_emergency_pause`:
```rust
client.activate_emergency_pause();
```
* **Result**: `emergency_controls_enabled` becomes `true`.
* **Important Note**: This leaves the contract in a **paused** state.

### Step 4: Verify Readiness
Query the readiness checklist to verify that all flags are set to `true`:
```rust
let info = client.get_mainnet_readiness_info();
assert!(info.initialized);
assert!(info.governed_params_set);
assert!(info.emergency_controls_enabled);
```

### Step 5: Resolve the Emergency (Resume Normal Operations)
Because Step 3 pauses the contract, you must resolve the pause to make the contract usable by the public:
```rust
client.resolve_emergency();
```
* **Result**: The contract returns to normal operational status (unpaused).

---

## Summary of State Transitions

| Action | `initialized` | `governed_params_set` | `emergency_controls_enabled` | Contract Status |
| :--- | :---: | :---: | :---: | :---: |
| *Deploy* | `false` | `false` | `false` | Uninitialized |
| `initialize()` | `true` | `false` | `false` | Active |
| `set_governed_params()` | `true` | `true` | `false` | Active |
| `activate_emergency_pause()` | `true` | `true` | `true` | **Paused (Emergency)** |
| `resolve_emergency()` | `true` | `true` | `true` | **Active (Mainnet Ready)** |
