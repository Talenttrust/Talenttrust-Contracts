# Signature-Based Approvals

## Overview
The escrow contract supports cryptographically signed approvals for two
actions:

| Action | Description |
|---|---|
| `MilestoneAcceptance` | Client signs to confirm a deliverable was received |
| `DisputeAction` | Either party signs a dispute-resolution step |

## How It Works
1. The caller constructs an `ApprovalRequest` with the correct `nonce`
   (retrieved via `get_nonce`).
2. The caller signs the SHA-256 digest of the XDR-encoded request with
   their Ed25519 key.
3. `submit_approval` is called on-chain; the contract verifies auth,
   nonce, and signature before persisting the record.

## Security Assumptions
- **Auth**: `signer.require_auth()` prevents address spoofing.
- **Replay protection**: monotonic per-signer nonce stored in instance
  storage.
- **Binding**: signature covers the full `ApprovalRequest` struct, so
  parameters cannot be altered after signing.
- **Duplicate guard**: a second approval for the same
  `(contract_id, milestone_id, action)` triple is rejected.

## Entry Points

### `submit_approval(request, signer, signature) → bool`
Verifies and records a signed approval.

### `get_approval(contract_id, milestone_id, action) → Option<SignedApproval>`
Returns the stored approval record, or `None`.

### `get_nonce(signer) → u64`
Returns the next expected nonce for a signer (starts at 0).

## Threat Model
| Threat | Mitigation |
|---|---|
| Replay attack | Nonce check |
| Wrong signer | `require_auth` + ed25519 verify |
| Double approval | Duplicate-key guard |
| Parameter tampering | Signature over full XDR struct |