# Mainnet Readiness Runbook and Checklist

This runbook is the operator checklist for deploying, configuring, and verifying a TalentTrust Escrow contract instance (`contracts/escrow/src/lib.rs`) before opening it to production traffic on Stellar Mainnet.

> **Related document:** See [Release Readiness Checklist](./release-readiness-checklist.md) for the per-agreement release lifecycle and milestones verification model.  
> **How they differ:** `release-readiness-checklist.md` covers agreement-level gates for an individual escrow (creation, funding, milestone release, and reputation), whereas this runbook covers **contract-instance deployment, admin/governance setup, artifact recording, and post-deploy verification** performed by operators.

---

## Pre-Deployment Artifact Check

- [ ] Build the release Wasm binary for `contracts/escrow` and compute its SHA-256 hash.
- [ ] Record the deployed `contractId`, `wasmSha256`, `admin`, `explorer`, and `deployedAt` metadata in [`deployments/testnet.json`](../../deployments/testnet.json) for testnet rehearsals (and in the corresponding mainnet deployment manifest once promoted).
- [ ] Confirm the Wasm hash deployed on-chain matches `wasmSha256` recorded in `deployments/testnet.json`.

---

## Operator Configuration Checklist

Execute the following steps in order against the freshly deployed contract instance:

### 1. Initialize the Contract

- [ ] Call `initialize(admin)` to bind the contract to the designated admin address (recommended: multi-sig or hardware-backed admin account).
  ```rust
  client.initialize(&admin);
  ```

### 2. Configure Protocol Fees (Optional)

- [ ] If protocol fees are enabled, call `set_fee(fee_bps, treasury)` as `admin` to set the fee in basis points and the treasury destination address.
  ```rust
  client.set_fee(&fee_bps, &treasury);
  ```

### 3. Configure Arbiter Panel (Optional)

- [ ] If M-of-N panel dispute resolution is used, call `set_panel(members, threshold)` as `admin` with unique member addresses and a valid threshold (`1 <= threshold <= members.len()`).
  ```rust
  client.set_panel(&members, &threshold);
  ```

### 4. Assign Operational Roles

- [ ] Delegate operational permissions with `grant_role(role, account)` as `admin` so day-to-day actions do not require the root admin key:
  - `Role::Pauser` — authorized to call `set_paused(caller, paused)` during incidents.
  - `Role::Resolver` — authorized to resolve disputes.
  ```rust
  client.grant_role(&Role::Pauser, &pauser_account);
  client.grant_role(&Role::Resolver, &resolver_account);
  ```
- [ ] Revoke any temporary or rotated accounts with `revoke_role(role, account)`.

---

## Post-Deploy Verification

Run these read-only checks immediately after configuration and record the outputs:

- [ ] **Admin check:** Call `get_admin()` and verify it returns the expected admin address.
- [ ] **Version check:** Call `version()` and verify it matches the expected contract schema/release version.
- [ ] **Fee configuration check:** Call `get_fee_config()` and verify `(fee_bps, treasury)` matches the intended fee settings (or `(0, None)` if fees are disabled).
- [ ] **Panel check (if configured):** Call `get_panel()` and verify the member list and threshold.
- [ ] **Role check:** Call `has_role(Role::Pauser, pauser_account)` and `has_role(Role::Resolver, resolver_account)` and confirm each returns `true`.
- [ ] **Pause state check:** Call `is_paused()` and confirm it returns `false`.

---

## Testnet Smoke Test Sequence

Before promoting the same Wasm hash to Mainnet, run a full end-to-end smoke test against the testnet deployment recorded in `deployments/testnet.json`:

1. **Create an escrow:** Call `create_contract` with distinct test `client` and `freelancer` addresses, a testnet token, and at least one positive milestone amount.
2. **Verify creation:** Call `get_contract(id)` and `contract_count()` to confirm the escrow is persisted.
3. **Fund the escrow:** Call `deposit_funds(id, amount)` from the test `client`.
4. **Release a milestone:** Call `release_milestone(id, index)` and verify the milestone payout settles as expected.
5. **Test pause/unpause (non-production only):** Call `set_paused(pauser, true)`, confirm `is_paused()` returns `true`, then call `set_paused(pauser, false)` and confirm `is_paused()` returns `false`.
