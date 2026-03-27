# Escrow Access Control Enforcement

This document describes role checks enforced across all state-changing escrow entrypoints.

## Objective

Ensure only valid contract actors (client, freelancer, arbiter) can invoke state mutations, and only in role-appropriate flows.

## Role Model

- Client:
  - creates contract (with freelancer)
  - deposits escrow funds
  - can approve/release milestones depending on `release_auth`
  - issues reputation after completion
- Freelancer:
  - must authorize participation at contract creation
  - is the only valid subject for post-completion reputation on that contract
- Arbiter (optional):
  - can approve/release milestones where selected release mode allows it

## Entry Point Enforcement

### `create_contract`

- Requires `client.require_auth()` and `freelancer.require_auth()`.
- Rejects same-address client/freelancer.
- Validates arbiter distinctness from both client and freelancer.
- Enforces arbiter presence for modes that require arbiter participation.

### `deposit_funds`

- Requires caller auth.
- Caller must equal contract client.
- Contract must be in `Created` status.
- Deposit amount must equal milestone total.

### `approve_milestone_release`

- Requires caller auth.
- Caller role validated against `release_auth` mode.
- Rejects unauthorized roles and duplicate approvals.
- Rejects invalid or already released milestones.

### `release_milestone`

- Requires caller auth.
- Caller role validated against `release_auth` mode.
- Requires sufficient approvals for configured mode.
- Rejects invalid or already released milestones.

### `issue_reputation`

- Requires caller auth.
- Caller must be the contract client.
- Provided freelancer must match contract freelancer.
- Contract must be `Completed` and reputation not previously issued.

## Release Authorization Matrix

- `ClientOnly`: client-only approve and release.
- `ArbiterOnly`: arbiter-only approve and release.
- `ClientAndArbiter`: either client or arbiter can approve/release.
- `MultiSig`: both client and arbiter approvals required before release.

## Latest Test Output

Date: `2026-03-24`

```text
running 32 tests
................................
test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## Coverage Snapshot

- Command: `cargo llvm-cov --workspace --all-features --summary-only`
- `contracts/escrow/src/lib.rs` line coverage: `97.71%`
- Workspace total line coverage: `99.16%`
