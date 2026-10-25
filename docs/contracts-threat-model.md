# Contracts Threat Model

This document defines the threat model, trust assumptions, attacker capabilities, security mitigations, and authorization matrix for the escrow smart contracts in `contracts/escrow/src/`.

---

## 1. Overview & System Scope

The escrow smart contract protocol manages client-freelancer service agreements on the Soroban (Stellar) smart contract platform. It handles milestone-based funding, funds release, dispute resolution, refunds, reputation issuance, governance administration, and client migration.

### System Boundaries

- **In-Scope**: Escrow state transitions, milestone accounting, authorization rules, dispute management, fee calculations, and administrative pause controls in `contracts/escrow/src/`.
- **Out-of-Scope / External**: Off-chain token custody, Stellar Asset Contract (SAC) host calls, front-end user key management, and off-chain indexing services.

---

## 2. Trust Assumptions

| Entity / Component | Trust Level | Scope of Trust & Operational Constraints |
|---|---|---|
| **Governance Admin (`admin`)** | Semi-Trusted | - Authorized to execute operational safety controls: `pause`, `unpause`, `activate_emergency_pause`, `resolve_emergency`, and governance parameter setup.<br>- Can initiate and manage two-step governance admin proposals (`propose_governance_admin`, `accept_governance_admin`).<br>- **Constraint**: Cannot directly drain escrowed milestone funds to an arbitrary address without following standard contract lifecycle or dispute resolution logic. |
| **Arbiter (`arbiter`)** | Semi-Trusted | - Assigned per-contract or governed to resolve disputes (`resolve_dispute`) and approve releases in `ArbiterOnly` or `ClientAndArbiter` modes.<br>- **Constraint**: Dispute resolution is bounded by `client_amount + freelancer_amount <= available_balance`. Cannot award funds beyond the escrowed amount. |
| **Client (`client`)** | Untrusted | - Authorized to create contracts, deposit funds, approve milestone releases (in `ClientOnly`, `ClientAndArbiter`, `MultiSig` modes), request refunds of unreleased milestones on non-terminal contracts, and request client migration.<br>- **Constraint**: Cannot withdraw funds allocated to released milestones or drain other clients' escrow balances. |
| **Freelancer (`freelancer`)** | Untrusted | - Authorized to approve milestone releases (in `MultiSig`), trigger releases post-approval, cancel unfunded contracts, and open disputes.<br>- **Constraint**: Cannot release funds without required authorization/approvals. |
| **Soroban Host Environment & SAC** | Fully Trusted | - Trusted to enforce cryptographic signature verification via `require_auth()`, manage storage isolation, execute atomic SAC token transfers, and manage storage Time-To-Live (TTL). |

---

## 3. Attacker Capabilities & Threat Vectors

### 3.1 Unauthenticated External Attacker
- **Threat Vector**: Submitting transactions to invoke administrative or lifecycle functions without valid key signatures.
- **Attacker Capability**: Can inspect public ledger state, send arbitrary contract invocations, and attempt to call `pause`, `deposit_funds`, `release_milestone`, `refund_unreleased_milestones`, or `resolve_dispute`.
- **Mitigation**: Soroban host cryptographic validation. All mutating functions enforce `require_auth()` on the target address (`admin`, `client`, `freelancer`, `arbiter`, or `caller`), causing unauthenticated calls to revert immediately.

### 3.2 Malicious / Rogue Client
- **Threat Vector**: Reclaiming deposited funds post-release, creating contracts with invalid milestone configurations, overfunding contracts, or issuing duplicate reputation ratings.
- **Attacker Capability**: Has valid client key signatures for contracts they created.
- **Mitigation**:
  - Milestone amount bounds checking (`[1, 1_000_000_0000000]` stroops) and checked summation (`accumulate_amounts`) prevent overflow and invalid contract totals.
  - Strict deposit validation ensures deposits match exact milestone expectations without overfunding.
  - State machine checks prevent refunds after finalization, completion, or cancellation (`AlreadyFinalized`, `ContractCancelled`, `ContractRefunded`).
  - Reputation issuance enforces `Completed` state and single-use `reputation_issued` flags (`AlreadyIssued`).

### 3.3 Malicious / Compromised Freelancer
- **Threat Vector**: Attempting unauthorized milestone releases, draining escrow balances before completing work, or blocking contract cancellation.
- **Attacker Capability**: Has valid freelancer key signatures for assigned contracts.
- **Mitigation**:
  - `release_milestone` enforces mode-specific authorization (`ReleaseAuthorization` matrix) and checks non-expired approval records via `check_approvals`.
  - In `MultiSig` mode, release requires both client and freelancer signed approvals.
  - State machine requires `Funded` status for milestone releases and disputes.

### 3.4 Rogue / Compromised Arbiter
- **Threat Vector**: Arbitrarily resolving non-disputed contracts or allocating more than the total deposited balance.
- **Attacker Capability**: Has valid arbiter key signatures.
- **Mitigation**:
  - `resolve_dispute` is restricted strictly to contracts in the `Disputed` state.
  - Enforces `client_amount + freelancer_amount <= available_balance` via checked arithmetic (`safe_subtract_amounts`).
  - Finalized contracts block dispute resolution (`AlreadyFinalized`).

### 3.5 Reentrancy & Stale Approval Re-use
- **Threat Vector**: Re-using milestone approval signatures or exploiting reentrancy during token transfers.
- **Attacker Capability**: Re-submitting approval signatures or manipulating contract callback order.
- **Mitigation**:
  - Approval records are cleared (`clear_approvals`) immediately upon milestone release.
  - Approvals stored in temporary storage expire automatically after `PENDING_APPROVAL_TTL_LEDGERS` (~7 days). `check_approvals` fails closed (`InsufficientApprovals`) if approvals are missing or expired.
  - Soroban's execution engine prevents traditional EVM-style reentrancy across contract calls.

---

## 4. Security Mitigations & System Guardrails

1. **Authentication & Authorization Gating**: Every mutating function enforces `require_auth()` on the required identity before state changes occur.
2. **State Machine Strictness**: Contracts transition through explicit states: `Created` → `Funded` → (`Completed` | `Disputed` | `Cancelled` | `Refunded`). Terminal states block further value-moving operations.
3. **Checked Arithmetic & Invariant Conservation**:
   - All financial balance additions and subtractions use checked arithmetic (`checked_add`, `checked_sub`, `accumulate_amounts`, `safe_subtract_amounts`).
   - Escrow balance conservation invariant is maintained at all state boundaries:
     $$\text{total\_deposited} == \text{released\_amount} + \text{refunded\_amount} + \text{available\_balance}$$
4. **Emergency & Pause Safeguards**:
   - `pause` and `activate_emergency_pause` immediately halt mutating operations (`ContractPaused`, `EmergencyActive`).
   - Pause checks execute alongside/prior to state mutations.
5. **Fail-Closed Storage & Expiry**:
   - Un-acted temporary approval entries auto-evict via TTL. Missing/evicted entries fail closed (`InsufficientApprovals`).
   - Finalization state is recorded in persistent storage to prevent record loss via TTL eviction.

---

## 5. Public Entrypoint Authorization Cross-Reference

The table below maps every public state-mutating entrypoint in `contracts/escrow/src/` to its required authenticated entity (`require_auth()`), code location, role gating, and state prerequisites.

| Entrypoint | Primary Authenticated Entity (`require_auth`) | Source File Cross-Reference | Role Gating & Policy Rules | Required Contract State |
|---|---|---|---|---|
| `initialize` | `admin` | [`lib.rs:376`](file:///c:/Users/godzi/Documents/Talenttrust-Contracts/contracts/escrow/src/lib.rs#L376) | Single-use setup; sets global Admin address | System uninitialized (`NotInitialized`) |
| `set_governance_admin` | `admin` | [`governance.rs:39`](file:///c:/Users/godzi/Documents/Talenttrust-Contracts/contracts/escrow/src/governance.rs#L39) | Caller must match stored Admin | System initialized |
| `propose_governance_admin` | `admin` | [`governance.rs:83`](file:///c:/Users/godzi/Documents/Talenttrust-Contracts/contracts/escrow/src/governance.rs#L83) | Stored Admin initiates 2-step transfer | System initialized |
| `accept_governance_admin` | `pending_admin` | [`governance.rs:122`](file:///c:/Users/godzi/Documents/Talenttrust-Contracts/contracts/escrow/src/governance.rs#L122) | Stored Pending Admin accepts transfer | Proposal exists & active |
| `cancel_governance_admin_proposal` | `admin` | [`governance.rs:164`](file:///c:/Users/godzi/Documents/Talenttrust-Contracts/contracts/escrow/src/governance.rs#L164) | Stored Admin cancels proposal | Proposal exists |
| `pause` | `admin` | [`lib.rs:1431`](file:///c:/Users/godzi/Documents/Talenttrust-Contracts/contracts/escrow/src/lib.rs#L1431) | Stored Admin | System unpaused |
| `unpause` | `admin` | [`lib.rs:1457`](file:///c:/Users/godzi/Documents/Talenttrust-Contracts/contracts/escrow/src/lib.rs#L1457) | Stored Admin | System paused & Emergency inactive |
| `activate_emergency_pause` | `admin` | [`lib.rs:1499`](file:///c:/Users/godzi/Documents/Talenttrust-Contracts/contracts/escrow/src/lib.rs#L1499) | Stored Admin | Emergency inactive |
| `resolve_emergency` | `admin` | [`lib.rs:1545`](file:///c:/Users/godzi/Documents/Talenttrust-Contracts/contracts/escrow/src/lib.rs#L1545) | Stored Admin | Emergency active |
| `create_contract` | `client` | [`create_contract.rs:54`](file:///c:/Users/godzi/Documents/Talenttrust-Contracts/contracts/escrow/src/create_contract.rs#L54) | `client` address parameter; `client != freelancer` | System initialized, not paused |
| `deposit_funds` | `caller` | [`deposit.rs:125`](file:///c:/Users/godzi/Documents/Talenttrust-Contracts/contracts/escrow/src/deposit.rs#L125) | `caller` signature verified | Contract state `Created`, not paused |
| `approve_milestone_release` | `caller` | [`lib.rs:698`](file:///c:/Users/godzi/Documents/Talenttrust-Contracts/contracts/escrow/src/lib.rs#L698), [`approvals.rs:42`](file:///c:/Users/godzi/Documents/Talenttrust-Contracts/contracts/escrow/src/approvals.rs#L42) | Role checked per `ReleaseAuthorization` | Contract state `Funded`, milestone unreleased |
| `release_milestone` | `caller` | [`release.rs:19`](file:///c:/Users/godzi/Documents/Talenttrust-Contracts/contracts/escrow/src/release.rs#L19), [`lib.rs:1864`](file:///c:/Users/godzi/Documents/Talenttrust-Contracts/contracts/escrow/src/lib.rs#L1864) | Role checked per `ReleaseAuthorization` + `check_approvals` | Contract state `Funded`, milestone unreleased/unrefunded |
| `refund_unreleased_milestones` | `contract.client` | [`refund_impl.rs:88`](file:///c:/Users/godzi/Documents/Talenttrust-Contracts/contracts/escrow/src/refund_impl.rs#L88), [`lib.rs:1059`](file:///c:/Users/godzi/Documents/Talenttrust-Contracts/contracts/escrow/src/lib.rs#L1059) | Stored `contract.client` | State `Created`, `Funded`, or `Disputed`, not finalized |
| `cancel_contract` | `caller` | [`lib.rs:1620`](file:///c:/Users/godzi/Documents/Talenttrust-Contracts/contracts/escrow/src/lib.rs#L1620) | `caller == client \|\| caller == freelancer` | State `Created` or `Funded`, zero released amount |
| `raise_dispute` | `caller` | [`lib.rs:1723`](file:///c:/Users/godzi/Documents/Talenttrust-Contracts/contracts/escrow/src/lib.rs#L1723) | `caller == client \|\| caller == freelancer` | State `Funded`, arbiter assigned, not finalized |
| `resolve_dispute` | `caller` | [`lib.rs:2189`](file:///c:/Users/godzi/Documents/Talenttrust-Contracts/contracts/escrow/src/lib.rs#L2189) | `caller == arbiter \|\| caller == admin` | State `Disputed`, not finalized |
| `finalize_contract` | `finalizer` | [`finalize.rs:142`](file:///c:/Users/godzi/Documents/Talenttrust-Contracts/contracts/escrow/src/finalize.rs#L142) | `finalizer` is Client, Freelancer, or Arbiter | State `Completed` or `Disputed`, not finalized |
| `issue_reputation` | `client` | [`lib.rs:2030`](file:///c:/Users/godzi/Documents/Talenttrust-Contracts/contracts/escrow/src/lib.rs#L2030) | `client == contract.client` | State `Completed`, `reputation_issued == false` |
| `submit_migration_request` | `current_client` | [`migration.rs:55`](file:///c:/Users/godzi/Documents/Talenttrust-Contracts/contracts/escrow/src/migration.rs#L55) | Stored `contract.client` | Contract not finalized |
| `approve_migration_request` | `new_client` | [`migration.rs:99`](file:///c:/Users/godzi/Documents/Talenttrust-Contracts/contracts/escrow/src/migration.rs#L99) | `new_client` target address | Pending migration proposal exists |
| `cancel_migration_request` | `current_client` | [`migration.rs:133`](file:///c:/Users/godzi/Documents/Talenttrust-Contracts/contracts/escrow/src/migration.rs#L133) | Stored `contract.client` | Pending migration proposal exists |

---

## 6. Verification & Auditing Checklist

When auditing contract changes or reviewing Pull Requests:

1. **Auth Placement**: Confirm `require_auth()` is invoked *before* any state modification or external token transfers.
2. **Pause/Emergency Enforcement**: Verify mutating entrypoints check initialization, pause, and emergency flags.
3. **State Transition Guards**: Ensure operations check contract state and reject execution on terminal states (`Cancelled`, `Refunded`, `AlreadyFinalized`).
4. **Checked Arithmetic**: Confirm all additions, subtractions, and balance updates use checked arithmetic to prevent panics or wraparound.
5. **Fail-Closed Approvals**: Confirm approval checks enforce non-expired status and clear records post-release.
