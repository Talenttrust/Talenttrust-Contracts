# WASM Upgrade and Redeploy Runbook

This document describes the operational sequence for deploying a new WASM binary
to a live escrow contract instance with in-flight contracts. It covers
pre-upgrade checks, pausing, the upgrade itself, post-upgrade verification, and
rollback.

---

## Scope

- **Repository**: Talenttrust/Talenttrust-Contracts
- **Contract**: `contracts/escrow`
- **Applies to**: Any Soroban deployer-based upgrade of the escrow WASM binary on
  an existing contract instance that already holds on-ledger state (contracts,
  reputation, governance parameters, settlement token binding, etc.)

---

## Prerequisites

- The admin address (stored under `DataKey::Admin`) must be accessible and
  funded for Soroban transaction fees.
- The new WASM binary must be built, optimised, and its hash recorded
  (`sha256` of the `.wasm` file). This hash is used for deployment verification.
- A Soroban deployer contract must be available (if using the deployer-based
  upgrade pattern) or the network must support direct WASM replacement.
- The operator must have the admin's signing keys (multi-sig cold storage or
  equivalent).

---

## 1. Pre-Upgrade Checks

Before initiating any upgrade, capture a baseline snapshot of the contract state.
These values are immutable across a plain WASM code swap (no storage migration
required) and serve as the post-upgrade verification target.

### 1.1 Snapshot Current State

Query and record the following read-only values:

```bash
# Admin address (immutable across code swaps)
soroban contract invoke \
  --id <ESCROW_CONTRACT_ID> \
  -- \
  get_admin

# Settlement token address (immutable across code swaps)
soroban contract invoke \
  --id <ESCROW_CONTRACT_ID> \
  -- \
  get_settlement_token

# Protocol fee in basis points (immutable across code swaps)
soroban contract invoke \
  --id <ESCROW_CONTRACT_ID> \
  -- \
  get_protocol_fee_bps

# Next contract ID high-water mark (monotonic; may only increase after upgrade)
soroban contract invoke \
  --id <ESCROW_CONTRACT_ID> \
  -- \
  get_next_contract_id

# Readiness checklist (should show all flags true for a live contract)
soroban contract invoke \
  --id <ESCROW_CONTRACT_ID> \
  -- \
  get_mainnet_readiness_info

# Storage layout version
soroban contract invoke \
  --id <ESCROW_CONTRACT_ID> \
  -- \
  storage_layout_plan
```

### 1.2 Record Baseline

Document the exact values returned above. After the upgrade, these values must
be identical (for immutable fields) or monotonically increasing (for
`get_next_contract_id`).

| Field | Expected Behaviour Post-Upgrade |
|---|---|
| `get_admin()` | Unchanged |
| `get_settlement_token()` | Unchanged |
| `get_protocol_fee_bps()` | Unchanged |
| `get_next_contract_id()` | >= pre-upgrade value |
| `get_mainnet_readiness_info()` | All flags unchanged |
| `storage_layout_plan()` | Same or newer version |

### 1.3 In-Flight Contracts Audit

Check for contracts in non-terminal states:

```bash
# Query each active contract by ID from the pre-upgrade snapshot
soroban contract invoke \
  --id <ESCROW_CONTRACT_ID> \
  -- \
  get_contract --contract_id <N>
```

Contracts in `Created`, `Funded`, `PartiallyFunded`, or `Disputed` status are
"live" and could be affected by a code upgrade. Ensure the new WASM handles
these states correctly.

---

## 2. Activate Emergency Pause

The emergency pause must be activated before the upgrade to freeze all
state-changing operations. This prevents in-flight contracts from mutating while
the WASM binary is being replaced.

```bash
soroban contract invoke \
  --id <ESCROW_CONTRACT_ID> \
  -- \
  activate_emergency_pause
```

### 2.1 Verify Pause State

```bash
soroban contract invoke \
  --id <ESCROW_CONTRACT_ID> \
  -- \
  is_paused
# Expected: true

soroban contract invoke \
  --id <ESCROW_CONTRACT_ID> \
  -- \
  is_emergency
# Expected: true
```

### 2.2 Confirm Mutating Operations Are Blocked

Verify that at least one mutating operation fails with `ContractPaused` or
`EmergencyActive`:

```bash
# This should fail — contract is paused
soroban contract invoke \
  --id <ESCROW_CONTRACT_ID> \
  -- \
  create_contract \
  --client <SOME_ADDR> --freelancer <SOME_ADDR> \
  --milestones '[1000000]'
```

### 2.3 Confirm Read-Only Queries Remain Available

```bash
# These should all succeed
soroban contract invoke --id <ESCROW_CONTRACT_ID> -- get_admin
soroban contract invoke --id <ESCROW_CONTRACT_ID> -- get_settlement_token
soroban contract invoke --id <ESCROW_CONTRACT_ID> -- get_protocol_fee_bps
soroban contract invoke --id <ESCROW_CONTRACT_ID> -- get_mainnet_readiness_info
soroban contract invoke --id <ESCROW_CONTRACT_ID> -- is_paused
```

---

## 3. WASM Install and Upgrade

### 3.1 Build the New WASM

```bash
# From the repository root
stellar contract build --path contracts/escrow
# Produces: target/wasm32-unknown-unknown/release/escrow.wasm

# Record the hash for verification
sha256sum target/wasm32-unknown-unknown/release/escrow.wasm
```

### 3.2 Upload the New WASM

```bash
stellar contract install \
  --network mainnet \
  --source <ADMIN_SECRET_KEY> \
  --wasm target/wasm32-unknown-unknown/release/escrow.wasm
```

Note the returned WASM hash (contract hash). This is the new binary that will be
bound to the existing contract instance.

### 3.3 Upgrade the Contract

Using the Soroban deployer or the network's upgrade mechanism:

```bash
# Option A: Using soroban contract upgrade (if supported by the network)
stellar contract upgrade \
  --network mainnet \
  --source <ADMIN_SECRET_KEY> \
  --contract-id <ESCROW_CONTRACT_ID> \
  --wasm target/wasm32-unknown-unknown/release/escrow.wasm

# Option B: Using a deployer contract
soroban contract invoke \
  --id <DEPLOYER_CONTRACT_ID> \
  -- \
  upgrade \
  --contract_id <ESCROW_CONTRACT_ID> \
  --new_wasm_hash <NEW_WASM_HASH>
```

### 3.4 Verify Binary Hash (Optional but Recommended)

If the network exposes the WASM hash of a deployed contract, verify it matches:

```bash
# The exact command depends on the network tooling
stellar contract inspect <ESCROW_CONTRACT_ID> --wasm-hash
```

---

## 4. Post-Upgrade Verification

Immediately after the upgrade, verify that all state is intact and the new
binary is functional.

### 4.1 Identity Verification Checklist

Assert that the following values are **unchanged** from the pre-upgrade
snapshot:

```bash
# Admin must be unchanged
ADMIN=$(soroban contract invoke --id <ESCROW_CONTRACT_ID> -- get_admin)
# Compare with pre-upgrade value

# Settlement token must be unchanged
TOKEN=$(soroban contract invoke --id <ESCROW_CONTRACT_ID> -- get_settlement_token)
# Compare with pre-upgrade value

# Protocol fee must be unchanged
FEE=$(soroban contract invoke --id <ESCROW_CONTRACT_ID> -- get_protocol_fee_bps)
# Compare with pre-upgrade value

# Next contract ID must be >= pre-upgrade value
NEXT_ID=$(soroban contract invoke --id <ESCROW_CONTRACT_ID> -- get_next_contract_id)
# Compare with pre-upgrade value (should be identical unless a contract was created during upgrade)
```

### 4.2 Readiness Checklist Verification

```bash
soroban contract invoke \
  --id <ESCROW_CONTRACT_ID> \
  -- \
  get_mainnet_readiness_info
```

Expected: all three flags (`initialized`, `governed_params_set`,
`emergency_controls_enabled`) remain `true`.

### 4.3 Live Contract State Verification

For each in-flight contract identified in step 1.3, verify the state is
unchanged:

```bash
soroban contract invoke \
  --id <ESCROW_CONTRACT_ID> \
  -- \
  get_contract --contract_id <N>
```

Compare status, funded_amount, released_amount, and refunded_amount against
pre-upgrade records.

### 4.4 Functional Smoke Test

Perform a minimal read-only operation using the new binary:

```bash
soroban contract invoke \
  --id <ESCROW_CONTRACT_ID> \
  -- \
  get_bounds
```

This verifies the new WASM compiles and executes correctly on the host.

---

## 5. Resolve Emergency (Unpause)

After all post-upgrade verifications pass, resume normal operations:

```bash
soroban contract invoke \
  --id <ESCROW_CONTRACT_ID> \
  -- \
  resolve_emergency
```

### 5.1 Verify Normal Operations

```bash
soroban contract invoke \
  --id <ESCROW_CONTRACT_ID> \
  -- \
  is_paused
# Expected: false

soroban contract invoke \
  --id <ESCROW_CONTRACT_ID> \
  -- \
  is_emergency
# Expected: false
```

### 5.2 Confirm Mutating Operations Resume

Test with a low-risk read-write operation or verify that `create_contract` no
longer returns `ContractPaused`:

```bash
# This should succeed (or fail with a non-pause error like InvalidParticipants)
soroban contract invoke \
  --id <ESCROW_CONTRACT_ID> \
  -- \
  create_contract \
  --client <TEST_ADDR> --freelancer <TEST_ADDR> \
  --milestones '[1000000]' \
  --release_authorization ClientOnly
```

---

## 6. Rollback Procedure

If the post-upgrade verification fails (step 4), the operator must roll back to
the previous WASM binary.

### 6.1 Rollback Steps

1. **Do NOT unpause** — the contract should remain in emergency pause state.
2. **Re-install the previous WASM binary** using the same upload and upgrade
   procedure from step 3, but with the original `.wasm` file.
3. **Re-run the post-upgrade verification checklist** (step 4) against the
   rolled-back binary.
4. If verification passes, proceed to unpause (step 5).
5. If verification still fails, **keep the contract paused** and investigate
   the storage state manually. Contact the protocol team.

### 6.2 Rollback Timeline

- The emergency pause prevents all state changes, so there is no urgency to
  complete the rollback within a specific timeframe.
- However, in-flight contracts with deadlines may be affected. Monitor for
  deadline-based refunds (`claim_timeout_refund`) that clients may initiate once
  operations resume.

---

## 7. Storage Layout: Migration vs Plain Code Swap

### 7.1 Plain Code Swap (No Migration Required)

The current escrow contract (V1 layout) uses a **plain code swap** for
upgrades. The following storage entries are unaffected by a WASM binary
replacement:

| Storage Key | Namespace | Affected by Code Swap? |
|---|---|---|
| `DataKey::Initialized` | persistent | No — persists across swaps |
| `DataKey::Admin` | persistent | No |
| `DataKey::Paused` | persistent | No |
| `DataKey::Emergency` | persistent | No |
| `DataKey::Contract(id)` | persistent | No |
| `DataKey::NextContractId` | persistent | No |
| `DataKey::SettlementToken` | persistent | No |
| `DataKey::ProtocolFeeBps` | persistent | No |
| `DataKey::GovernedParameters` | persistent | No |
| `DataKey::ReadinessChecklist` | persistent | No |
| `DataKey::AccumulatedProtocolFees` | persistent | No |
| `DataKey::Reputation(addr)` | persistent | No |
| `DataKey::PendingReputationCredits(addr)` | persistent | No |
| `DataKey::MilestoneApprovals(id, idx)` | temporary | No — auto-evicted by host |
| `DataKey::PendingClientMigration(id)` | temporary | No — auto-evicted by host |

**Key insight**: All live contract state is stored in Soroban persistent or
temporary storage keyed by stable `DataKey` variants. Replacing the WASM binary
does not clear or alter on-ledger storage entries. The new binary reads the same
keys and interprets them identically.

### 7.2 When a Storage Migration IS Required

A storage migration step is required when:

1. **New `DataKey` variants are added** — if the new WASM introduces a new
   variant (e.g. `DataKey::V2Metadata`), existing storage entries under V1 keys
   are unaffected, but any new feature that reads from the V2 key will find
   nothing. A migration function can initialise V2 defaults.

2. **Existing key value layouts change** — if the serialised shape of
   `Contract(id)` or `Milestone` changes (e.g. adding a field), the new WASM
   must either:
   - Add a backward-compatible default for the missing field, or
   - Provide an explicit `migrate_storage(target_version)` entrypoint that
     re-encodes existing entries.

3. **Layout version bumps** — the `LayoutVersion` metadata (checked by
   `storage_layout_plan()`) must be bumped when value layouts change. The
   contract's internal version guard rejects operations if the on-ledger version
   is unsupported.

### 7.3 Current V1 Storage Rules

Per `docs/escrow/upgradeable-storage.md`:

- V1 keys and value layouts are **immutable once deployed**.
- Future upgrades must add new version key variants (e.g. `V2(...)`) rather than
  mutating V1 key/value formats.
- `LayoutVersion` is checked before all state reads/writes.
- Unknown versions are rejected with `UnsupportedStorageVersion`.
- The `migrate_storage(target_version)` entrypoint is explicit and rejects
  unsupported targets.

### 7.4 Decision Matrix

| Upgrade Scenario | Migration Step Required? |
|---|---|
| Bug fix in existing logic (no storage changes) | No — plain code swap |
| New read-only query (no new storage keys) | No — plain code swap |
| New mutating entrypoint (no new storage keys) | No — plain code swap |
| New `DataKey` variant for a new feature | Optional — new keys default to empty |
| Changed serialisation of `Contract(id)` | **Yes** — `migrate_storage` required |
| Changed serialisation of `Milestone` | **Yes** — `migrate_storage` required |
| New `LayoutVersion` value | **Yes** — `migrate_storage` required |

---

## 8. Post-Upgrade Monitoring

After unpausing, monitor the following for at least 24 hours:

1. **Event stream**: watch for `("emergency", "activated")` events that might
   indicate the operator triggered an emergency pause in response to an
   unexpected issue.
2. **Contract creation**: verify new `("created", contract_id)` events are
   emitted correctly.
3. **Deposits and releases**: verify `("deposited", contract_id)` and
   `("mlstn_rls", contract_id)` events are emitted with correct payloads.
4. **Error rates**: monitor for unexpected `ContractNotFound`,
   `InvalidState`, or `AccountingInvariantViolated` errors that might indicate
   a regression.

---

## 9. Checklist Summary

| Step | Action | Expected Result |
|---|---|---|
| 1.1 | Snapshot current state | Values recorded |
| 1.2 | Record baseline | All fields documented |
| 1.3 | Audit in-flight contracts | List of live contract IDs |
| 2 | `activate_emergency_pause` | `is_paused() == true`, `is_emergency() == true` |
| 2.2 | Verify mutations blocked | Mutating calls fail with `ContractPaused` |
| 2.3 | Verify reads still work | Read-only queries succeed |
| 3.1 | Build new WASM | `escrow.wasm` produced, hash recorded |
| 3.2 | Upload new WASM | WASM hash returned |
| 3.3 | Upgrade contract | Upgrade transaction succeeds |
| 4.1 | Verify identity fields | Admin, token, fee unchanged |
| 4.2 | Verify readiness checklist | All flags still `true` |
| 4.3 | Verify live contract state | Status/amounts unchanged |
| 4.4 | Functional smoke test | `get_bounds()` succeeds |
| 5 | `resolve_emergency` | `is_paused() == false`, `is_emergency() == false` |
| 5.2 | Confirm operations resume | Mutating calls no longer blocked |
| 9 | Post-upgrade monitoring | 24h watch for anomalies |

---

## 10. Test Coverage

The post-upgrade verification checklist assertions are covered by tests in
`contracts/escrow/src/test/mainnet_readiness.rs`:

- `upgrade_snapshot_admin_unchanged` — asserts `get_admin()` survives a
  code swap
- `upgrade_snapshot_settlement_token_unchanged` — asserts
  `get_settlement_token()` survives a code swap
- `upgrade_snapshot_protocol_fee_unchanged` — asserts
  `get_protocol_fee_bps()` survives a code swap
- `upgrade_snapshot_next_contract_id_unchanged` — asserts
  `get_next_contract_id()` survives a code swap
- `upgrade_snapshot_readiness_checklist_unchanged` — asserts
  `get_mainnet_readiness_info()` survives a code swap
- `post_upgrade_pause_unpause_cycle` — exercises the full
  pause → upgrade → verify → unpause cycle
- `post_upgrade_in_flight_contract_integrity` — creates a funded contract,
  pauses, performs a code swap (simulated by re-registering), and verifies
  the contract state is unchanged
- `emergency_pause_blocks_mutations_during_upgrade` — verifies all mutating
  entrypoints are blocked while the contract is paused for upgrade
