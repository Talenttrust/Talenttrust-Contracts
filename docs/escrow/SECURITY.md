# Security & Threat Model Analysis

## Executive Summary

The Escrow contract enforces **five layered constraints** on reputation issuance to prevent premature or fraudulent credentialing. Each constraint is independently necessary; together they form a complete security gate.

This document outlines the threat scenarios, mitigations provided by the contract, and residual risks within and out of scope.

---

## Trust Model

### Actors

1. **Client** – Party commissioning work; deposits funds and approves milestone payments.
2. **Freelancer** – Party performing work; receives milestone payments and reputation credentials.
3. **Contract** – Soroban smart contract executing on Stellar network; single source of truth.
4. **Indexers / Off-chain Systems** - External services consuming `reputation_issued` events to build freelancer profiles.

### Assumptions

- Clients and freelancers are distinct Stellar accounts (cannot spoof each other cryptographically).
- Soroban SDK's `require_auth()` correctly validates cryptographic signatures.
- Stellar network consensus is live and operates as specified.
- No bugs in Soroban SDK or Stellar Core that would allow contract state escape.

---

## Threat Model & Mitigations

### Threat 1: Premature Reputation Issuance (Severity: HIGH)

**Attack:** Freelancer issues reputation before delivering all work.

**Scenario:**
1. Client creates contract with 3 milestones.
2. Client deposits funds.
3. Client releases milestone 1 (early payment, on good faith).
4. Freelancer immediately calls `issue_reputation` with rating 5, **without completing remaining milestones**.

**Impact:** Freelancer earns reputation for incomplete work, artificially inflating profile.

**Mitigations (Layered):**

1. **Completion Gate (Constraint 2):** Contract must be `Completed` before reputation issuance.
   - Reputation checks: `assert!(status == Completed, ...)`
   - Freelancer cannot influence status transition (only client can call `complete_contract`).

2. **Final Settlement Gate (Constraint 3):** Every milestone must be released.
   - Reputation checks: `for each milestone: assert!(released, ...)`
   - Client must explicitly approve each milestone before `Completed` is reachable.

3. **Complete Contract Precondition:** `complete_contract` requires all milestones released.
   - Without this, client could call `complete_contract` even if only milestone 1 was released.
   - By requiring all milestones released first, we shift the burden onto the client (they must be explicit about approving full work).

**Residual Risk:** If client is colluding with freelancer (e.g., paying for fake work), the contract cannot prevent this, but it does raise the bar because both parties must sign off on each milestone.

---

### Threat 2: Double-Issuance / Reputation Inflation (Severity: CRITICAL)

**Attack:** Same reputation event issued twice, inflating freelancer's credential count.

**Scenario:**
1. Contract is completed.
2. Freelancer calls `issue_reputation(cid, 5)`.
3. Event is emitted and indexed by off-chain aggregators.
4. Freelancer (or attacker with contract-call capabilities) calls `issue_reputation(cid, 5)` again.
5. Second event is emitted; reputation count doubles.

**Impact:** Credential inflation; off-chain indexers might double-count the reputation.

**Mitigations:**

1. **Immutable Single-Issuance Flag (Constraint 4):**
   - Sets `DataKey::ReputationIssued(contract_id) = true` in persistent storage.
   - Check is performed **before** event emission (checks-effects-interactions pattern).
   - On second call, check fails with panic: `assert!(!already_issued, ...)`.

2. **Contract-Level Idempotency:**
   - The flag is scoped to a specific `contract_id`, preventing crosstalk between contracts.
   - The flag is **never reset** (immutable once set).
   - Even if contract state otherwise changed, flag remains set (cannot be exploited by re-funding a contract, etc.).

3. **Panic on Violation:**
   - Transaction reverts atomically; second event is never emitted.
   - Off-chain indexers see either 1 event (success) or 0 events (revert); never 2.

**Residual Risk:** Extremely low. The immutable flag is the cryptographic equivalent of a "used" check in a replay-protected system. No known attack bypasses this pattern in Soroban.

---

### Threat 3: Milestone Released Twice (Severity: MEDIUM)

**Attack:** Client accidentally or maliciously releases the same milestone payment twice.

**Scenario:**
1. Client calls `release_milestone(cid, 0)`.
2. Client (or attacker with client's auth) calls `release_milestone(cid, 0)` again.
3. Milestone is marked released a second time (or payment attempt is made twice if off-chain integration exists).

**Impact:** If asset transfer is triggered by the event, freelancer receives double payment.

**Mitigations:**

1. **Released Flag Per Milestone:**
   - Each milestone has a `released: bool` flag.
   - On first release: `milestone.released = false -> true`.
   - On second release: check fails with `assert!(!milestone.released, ...)`.

2. **On-Chain Prevention (This Contract):**
   - `release_milestone` panics on second attempt.
   - No asset transfer happens in the contract itself (out of scope for this spec).

3. **Off-Chain Prevention:**
   - If asset transfer is triggered by `MilestoneReleased` events, indexers should deduplicate by `(contract_id, milestone_id)` pair.
   - Only the first release event should trigger a transfer.

**Residual Risk:** If off-chain asset transfer logic is not idempotent, freelancer could receive double payment. This is **outside the contract** but critical to the integration layer.

---

### Threat 4: Unauthorized Fund Release (Severity: CRITICAL)

**Attack:** Non-client releases funds or marks milestones as released.

**Scenario:**
1. Attacker calls `deposit_funds(cid, amount)` pretending to be the client.
2. Attacker calls `release_milestone(cid, 0)` to approve payments without client consent.

**Impact:** Attacker drains client's escrow or approves payments the client didn't authorize.

**Mitigations:**

1. **Stellar Cryptographic Auth:**
   - `deposit_funds` and `release_milestone` require `client.require_auth()`.
   - Soroban SDK verifies the function invocation is signed by the client's private key.
   - Only the holder of `client`'s private key can pass this check.

2. **Principle of Least Privilege:**
   - No other functions (e.g., `issue_reputation`) require client auth.
   - Freelancer cannot be coerced into authorizing anything.

3. **Per-Function Granularity:**
   - Different functions have different auth requirements (client-only for funds, no-auth for reputation issuance).

**Residual Risk:** Very low. Soroban's auth model is battle-tested on Stellar. Risk is only if private keys are compromised.

---

### Threat 5: Contract State Mutation During Verification (Severity: MEDIUM)

**Attack:** Concurrent calls to `issue_reputation` both pass all checks, both set the flag, both emit events.

**Scenario:**
1. Two threads/processes both invoke `issue_reputation(cid, 5)` nearly simultaneously.
2. Both load contract, both see status `Completed`, both see all milestones released, both see flag `false`.
3. Both set flag to `true` concurrently.
4. Both emit events.

**Impact:** Two reputation events for one contract.

**Mitigations:**

1. **Soroban Transaction Atomicity:**
   - Soroban transactions are **fully atomic**. Only one invocation's effects are committed at a time.
   - No true concurrency exists; Stellar network consensus serializes all transactions.
   - If two `issue_reputation` calls are submitted in the same block/ledger:
     - First transaction commits: `ReputationIssued(cid) = true`.
     - Second transaction executes: sees `ReputationIssued(cid) = true`, panics, reverts.

2. **Mempool Ordering:**
   - Stellar/Soroban network ensures strict serial execution of transactions.
   - No race condition window exists.

3. **Flag is Set Before Event:**
   - Even if a bug existed, the checks-effects-interactions pattern ensures the flag is immutable before the event is visible.

**Residual Risk:** Extremely low. Assumes Soroban consensus is live and correctly implemented (reasonable for Stellar).

---

### Threat 6: Insufficient Funds in Escrow (Severity: MEDIUM)

**Attack:** Client creates a contract for $1000 worth of milestones but only deposits $100.

**Scenario:**
1. Client calls `create_contract(client, freelancer, [500, 500])` (1000 total).
2. Client calls `deposit_funds(cid, 100)` (only 100).
3. Freelancer completes work.
4. Client calls `release_milestone(cid, 0)` (first 500).
5. Asset transfer fails (insufficient funds) **outside the contract**.

**Impact:** Freelancer does not receive promised payment; reputation is issued for unpaid work.

**Mitigations (In-Scope):**

1. **Contract Does Not Track Amounts:**
   - This contract does **not** track or verify deposits against milestones.
   - Assumption: integration layer enforces deposit ≥ sum(milestones) atomically.

2. **Contract Does Not Transfer Assets:**
   - The contract merely approves releases; actual transfers happen off-chain or in a separate asset contract.
   - If transfer fails, the contract's state remains (milestone marked released).

**Mitigations (Out-of-Scope):**

1. **Atomic Asset Transfer + Escrow State Update:**
   - In production, a higher-level orchestration contract or transaction should atomically:
     - Transfer funds from client to freelancer.
     - Update escrow state.
   - Soroban supports multi-contract invocation within one transaction (enabling this).

2. **Off-Chain Verification:**
   - Client and/or payment processor verifies sufficient balance before allowing milestone release.

**Residual Risk:** **HIGH** if integration is not properly designed. The escrow contract itself is not responsible for asset custody; it only manages approvals.

---

### Threat 7: Freelancer Impersonation (Severity: MEDIUM)

**Attack:** Attacker uses the freelancer's address without their consent.

**Scenario:**
1. Client creates contract with attacker-controlled `freelancer` address.
2. Client and attacker conspire to approve all milestones.
3. Reputation is issued to the attacker's address.
4. Real freelancer's profile is unaffected.

**Impact:** Attacker's profile is artificially inflated, not the real freelancer's.

**Mitigations:**

1. **Off-Chain Verification:**
   - Client and freelancer (both) sign a contract agreement off-chain before calling `create_contract`.
   - This is a social/business process, not enforced by the smart contract.

2. **Event Transparency:**
   - `reputation_issued` events include the freelancer address; anyone can audit that field.
   - If address doesn't match known freelancer, flag as fraudulent off-chain.

3. **No Impersonation Inside Contract:**
   - Contract does not authenticate the freelancer.
   - But contract also never takes actions on the freelancer's behalf (freelancer never calls contract).

**Residual Risk:** **MEDIUM**. The real protection is off-chain (client-freelancer agreement). Smart contract can only record; it cannot verify identity.

---

### Threat 8: Rating Manipulation (Severity: LOW)

**Attack:** Attacker calls `issue_reputation` with an out-of-range rating to bypass logic downstream.

**Scenario:**
1. Contract issues reputation with rating 10 (invalid).
2. Off-chain indexer stores rating 10 instead of rejecting.
3. Freelancer's average reputation is skewed.

**Impact:** Reputation score is invalid downstream.

**Mitigations:**

1. **Rating Validation (Constraint 5):**
   - Contract checks `assert!(rating >= 1 && rating <= 5, ...)`
   - Invalid ratings are rejected before event emission.

2. **Panic on Violation:**
   - Transaction reverts; event is never emitted; on-chain history is clean.

3. **Off-Chain Redundancy:**
   - Indexers should still validate rating ∈ [1, 5] before storing (defense in depth).

**Residual Risk:** Very low. Contract prevents bad data from entering the blockchain.

---

## Defense-in-Depth Summary

| Layer | Threat | Defense | Severity |
|-------|--------|---------|----------|
| **On-Chain** | Premature issuance | Completion gate + Final settlement | HIGH |
| **On-Chain** | Double-issuance | Immutable flag | CRITICAL |
| **On-Chain** | Double release | Milestone flag + panic | MEDIUM |
| **On-Chain** | Unauthorized fund ops | Cryptographic auth | CRITICAL |
| **On-Chain** | Concurrent mutations | Atomic transactions | MEDIUM |
| **On-Chain** | Invalid ratings | Rating validation | LOW |
| **Off-Chain** | Insufficient funds | Asset amount verification | MEDIUM |
| **Off-Chain** | Freelancer impersonation | Social verification | MEDIUM |

---

## Assumptions & Limitations

### Within Scope (Contract Guarantees)

[OK] Only clients can approve milestone releases.
[OK] Reputation can only be issued after all milestones are released.
[OK] Each contract can have at most one reputation issuance.
[OK] Ratings are validated to [1, 5].
[OK] Contract state is immutable after critical operations (flags, status).

### Out of Scope (Not Guaranteed by Contract)

[OUT] Sufficient funds in escrow to pay all milestones.
[OUT] Actual asset transfer to freelancer (off-chain integration).
[OUT] Freelancer identity verification.
[OUT] Dispute resolution and contract reversal.
[OUT] Client or freelancer solvency/creditworthiness.

### External Dependencies

- **Soroban SDK & Stellar Network:** Must operate correctly per specification.
- **Asset Contract (if used):** Must handle atomic transfer + state update sequences.
- **Off-Chain Indexers:** Must deduplicate events and validate data.
- **Client & Freelancer:** Must hold private keys securely.

---

## Security Recommendations for Deployers

### For Client Systems

1. **Verify Freelancer Identity** before creating a contract.
2. **Use a Threshold Multi-Sig** for high-value deposits (e.g., 2-of-3).
3. **Inspect Off-Chain Integration** to ensure asset transfers are atomic with contract state updates.
4. **Implement Rate Limits** on contract creation / deposit to prevent spam attacks.

### For Reputation Indexers

1. **Deduplicate Events:** Only the first `reputation_issued(contract_id)` is valid; ignore subsequent attempts.
2. **Validate Ratings:** Reject any event with `rating > 5 || rating < 1` (defense in depth).
3. **Cross-Check Contract State:** Before displaying reputation, verify the contract is indeed in `Completed` status on-chain.
4. **Audit Trails:** Log all events and state changes for forensic analysis.

### For Freelancers

1. **Verify Contract Terms** before work begins (off-chain).
2. **Request Milestones Progress Updates** from client to confirm releases are on track.
3. **Monitor for Disputes:** If client marks contract `Disputed`, review the reason and escalate if necessary.

---

## Audit Trail & Forensics

All key state transitions are visible on-chain:

- `create_contract` invocation -> contract ID assigned
- `deposit_funds` invocation -> status changes to `Funded`
- `release_milestone` invocation -> milestone marked released
- `complete_contract` invocation -> status changes to `Completed`
- `cancel_contract` invocation -> status changes to `Cancelled` + event emitted
- `issue_reputation` invocation -> `ReputationIssued` flag set + event emitted

Off-chain observers can reconstruct the exact timeline and verify no constraints were violated.

---

## Cancellation Threat Model (v0.2.0)

### Threat 7: Unauthorized Cancellation (Severity: CRITICAL)

**Attack:** A malicious actor cancels a contract they are not party to, forcing funds to be returned before work is complete.

**Scenarios:**
1. A random address calls `cancel_contract(cid, random_addr)`.
2. A client cancels after the freelancer has started work and milestones are released.
3. A freelancer cancels immediately after being funded to disrupt client.

**Mitigations:**

1. **Role-Based Gate (Caller Authorization):**
   - In `Created` state: only client or freelancer can cancel.
   - In `Funded` state: only client (before releases), freelancer (mutual agreement), or arbiter allowed.
   - In `Disputed` state: arbiter-only cancellation.
   - Any unauthorized caller panics immediately.

2. **Release Check (Protects Freelancer):**
   - In `Funded` state, client can only cancel if **zero milestones** were released.
   - Prevents client from cancelling after receiving the freelancer's delivered work.
   - Panics with `"Client cannot cancel after milestones have been released"`.

3. **Status Gate (Prevents Retroactive Cancellation):**
   - `Completed` contracts cannot be cancelled (work is done, funds disbursed).
   - Already `Cancelled` contracts panicis immediately (no double-cancellation).

4. **Atomic Event Emission:**
   - `contract_cancelled` event is emitted on success for off-chain audit trails.
   - Cancellation is fully atomic; no partial state is possible.

**Residual Risk:** Client and arbiter collusion can cancel a funded contract even when milestones remain. Both must cooperate, raising the bar significantly.

---

### Threat 8: Griefing via Premature Freelancer Cancellation (Severity: MEDIUM)

**Attack:** Freelancer cancels immediately after funding to disrupt client operations.

**Mitigation:**
- **Economic deterrent:** Freelancer gains nothing from cancellation (funds go back to client).
- **Off-chain monitoring:** Client can detect cancellation via the `contract_cancelled` event.
- **Arbiter Role:** Client can request arbiter oversight to prevent unilateral freelancer cancellation.

**Residual Risk:** The contract does not prevent freelancer griefing (cancel-then-create-loop), but the cost is borne entirely by the freelancer (gas), not by clients.

---

## Security Recommendations for Cancellation

### For Clients
1. **Deposit Only When Ready:** Confirm milestones and terms off-chain before funding.
2. **Nominate an Arbiter:** Always include an arbiter in high-value contracts for third-party cancellation rights.
3. **Track Release Events:** Once milestones are released, unilateral cancellation is blocked.

### For Freelancers
1. **Monitor Funded Status:** Watch for unauthorized cancellation via `contract_cancelled` events.
2. **Use Arbiter for Disputes:** Prefer dispute escalation over cancellation if client withholds payment.

---

## Version

- **Version:** 0.2.0
- **Last Updated:** 2026-03-24
- **Threat Model:** Complete (updated for cancellation path)
- **Risk Assessment:** Mitigations adequate for production use with noted caveats.
