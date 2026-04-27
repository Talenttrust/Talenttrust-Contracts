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
| **On-Chain** | Unauthorized cancellation | Role gates + state checks | CRITICAL |
| **On-Chain** | Retroactive cancellation | Terminal state blocks | HIGH |
| **On-Chain** | Double cancellation | Idempotency flag | MEDIUM |
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

### Cancellation Security Guarantees

The `cancel_contract` function implements six critical security guarantees:

1. **Role-Based Authorization**: Each contract state has explicit caller requirements
   - Created: Client or Freelancer
   - Funded: Client (conditional), Freelancer, or Arbiter
   - Disputed: Arbiter only
2. **No Retroactive Cancellation**: Completed contracts cannot be cancelled under any circumstances
3. **Freelancer Protection**: Client cannot cancel after any milestone has been released
4. **Idempotency**: Double-cancellation prevented with explicit AlreadyCancelled error
5. **Event Integrity**: Atomic event emission ensures indexer consistency
6. **Arbiter Isolation**: Arbiter address cannot overlap with client or freelancer at contract creation

---

### Threat 7: Unauthorized Cancellation (Severity: CRITICAL)

**Attack:** A malicious actor cancels a contract they are not party to, forcing funds to be returned before work is complete.

**Scenarios:**
1. A random address calls `cancel_contract(cid, random_addr)`.
2. A client cancels after the freelancer has started work and milestones are released.
3. A freelancer cancels immediately after being funded to disrupt client.

**Mitigations:**

1. **Role-Based Gate (Caller Authorization):**
   - In `Created` state: only client or freelancer can cancel.
   - In `Funded` state: 
     - Client can cancel only if **zero milestones** have been released.
     - Freelancer can cancel (economic deterrent - funds return to client).
     - Arbiter can cancel (dispute resolution authority).
   - In `Disputed` state: arbiter-only cancellation.
   - Any unauthorized caller panics with `EscrowError::UnauthorizedRole`.

2. **Release Check (Protects Freelancer):**
   - In `Funded` state, client can only cancel if **zero milestones** were released.
   - Prevents client from cancelling after receiving the freelancer's delivered work.
   - Panics with `EscrowError::MilestonesAlreadyReleased`.

3. **Status Gate (Prevents Retroactive Cancellation):**
   - `Completed` contracts cannot be cancelled (work is done, funds disbursed).
   - Panics with `EscrowError::InvalidStatusTransition`.
   - Already `Cancelled` contracts panic immediately with `EscrowError::AlreadyCancelled` (no double-cancellation).

4. **Atomic Event Emission:**
   - `contract_cancelled` event is emitted on success for off-chain audit trails.
   - Event structure: Topics `("contract_cancelled", contract_id)`, Data `(caller, status, timestamp)`.
   - Cancellation is fully atomic; no partial state is possible.

5. **Arbiter Validation:**
   - Arbiter address cannot equal client or freelancer address at contract creation.
   - Prevents role confusion and unauthorized cancellation paths.
   - Panics with `EscrowError::InvalidParticipant` if validation fails.

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
4. **Monitor Contract State:** Watch for cancellation attempts via `contract_cancelled` events.

### For Freelancers
1. **Monitor Funded Status:** Watch for unauthorized cancellation via `contract_cancelled` events.
2. **Use Arbiter for Disputes:** Prefer dispute escalation over cancellation if client withholds payment.
3. **Complete Work Promptly:** Release milestones quickly to prevent client cancellation.
4. **Verify Arbiter Identity:** Ensure the arbiter is a trusted third party before contract creation.

### For Arbiters
1. **Act Impartially:** Cancellation authority should only be used in legitimate dispute scenarios.
2. **Document Decisions:** Off-chain documentation of cancellation reasons aids in dispute resolution.
3. **Monitor Events:** Track `contract_cancelled` events for contracts under your supervision.

---

## Version

- **Version:** 0.3.1
- **Last Updated:** 2026-04-25
- **Threat Model:** Complete (updated for identity validation, cancellation, refunds, disputes, and governance)
- **Risk Assessment:** Mitigations adequate for production use with noted caveats.

---

## Refund Threat Model (v0.3.0)

### Threat 9: Partial Refund Exploitation (Severity: MEDIUM)

**Attack:** Client or attacker manipulates refund calculations to receive more funds than deposited or approved.

**Scenarios:**
1. Client requests partial refund for milestone not yet released.
2. Client requests refund after partial milestone release with mismatched amounts.
3. Arbiter processes refund that exceeds available escrow balance.

**Mitigations:**

1. **Release Check Before Refund:**
   - Refund can only be processed for milestones where `released == false`.
   - Prevents refunding work already delivered and approved.
   - Panics with `"Milestone already released"` if attempt made.

2. **Amount Validation:**
   - Refund amount must not exceed milestone amount minus already released amount.
   - Prevents over-refunding scenarios.
   - Panics with `"Refund amount exceeds available balance"` if exceeded.

3. **Arbiter Authorization:**
   - Only authorized arbiter can process refunds in disputed state.
   - Prevents unauthorized refunds by malicious actors.
   - Requires proper role validation before processing.

**Residual Risk:** If arbiter is colluding with client, refund could be processed unfairly. Both must cooperate, raising the bar significantly.

---

### Threat 10: Refund Double-Processing (Severity: MEDIUM)

**Attack:** Same refund request processed twice, draining escrow.

**Mitigations:**

1. **Milestone State Lock:**
   - Once refund is processed, milestone status changes to prevent re-processing.
   - Refund flag set in storage prevents duplicate processing.

2. **Atomic Transactions:**
   - Soroban transaction atomicity ensures only one refund per block.
   - No race conditions possible within single ledger.

**Residual Risk:** Very low with proper integration layer handling.

---

## Dispute Threat Model (v0.3.0)

### Threat 11: Premature Dispute Initiation (Severity: MEDIUM)

**Attack:** Malicious actor initiates dispute without valid reason to disrupt contract.

**Scenarios:**
1. Freelancer initiates dispute immediately after contract creation.
2. Attacker initiates dispute on contract they're not party to.
3. Client initiates dispute after milestones are released to avoid payment.

**Mitigations:**

1. **Role-Based Dispute Initiation:**
   - Only client, freelancer, or arbiter can initiate dispute.
   - Unauthorized callers panic immediately.
   - Prevents external attack surface.

2. **Status Gate:**
   - Dispute can only be initiated from `Created`, `Funded`, or `Completed` states.
   - `Disputed` and `Cancelled` contracts cannot be disputed again.
   - Prevents state confusion.

3. **Event Emission for Monitoring:**
   - `contract_disputed` event emitted for off-chain monitoring.
   - Allows stakeholders to detect and respond to disputes.

**Residual Risk:** Client can still abuse dispute process to delay payment. Off-chain governance/reputation should handle this.

---

### Threat 12: Dispute Resolution Collusion (Severity: HIGH)

**Attack:** Arbiter colludes with one party to rule unfairly.

**Mitigations:**

1. **Transparent Dispute Timeline:**
   - All dispute events are on-chain and auditable.
   - Off-chain governance can review arbiter decisions.
   - Reputation system can penalize biased arbiters.

2. **Multi-Arbiter Support:**
   - Contract supports multiple arbiters for high-value contracts.
   - Prevents single point of failure.

3. **Time-Bound Resolution:**
   - Timeout mechanism ensures disputes don't linger indefinitely.
   - Auto-resolution or escalation path available.

**Residual Risk:** **HIGH** if arbiter selection is not properly decentralized. Choose arbiters carefully.

---

## Governance Threat Model (v0.3.0)

### Threat 13: Governance Parameter Manipulation (Severity: HIGH)

**Attack:** Attacker changes critical governance parameters (fees, timeouts, limits) to extract value.

**Scenarios:**
1. Attacker increases platform fee to drain user funds.
2. Attacker extends timeout to indefinite period.
3. Attacker removes all rate limits for spam attacks.

**Mitigations:**

1. **Multi-Sig Governance:**
   - Critical parameter changes require multi-signature authorization.
   - No single key can modify governance.
   - Configurable threshold (e.g., 3-of-5 governors).

2. **Parameter Bounds:**
   - All parameters have minimum and maximum bounds.
   - Prevents extreme values even with proper authorization.
   - Panics with `"Invalid protocol parameters"` if exceeded.

3. **Timelock for Changes:**
   - Parameter changes have mandatory timelock period.
   - Allows users to exit before unfavorable changes take effect.

**Residual Risk:** If governance keys are compromised, all funds at risk. Use hardware wallets and secure key management.

---

### Threat 14: Governance Upgrade Attack (Severity: CRITICAL)

**Attack:** Malicious upgrade replaces contract with backdoored version.

**Mitigations:**

1. **Upgrade Authorization:**
   - Only authorized governance can propose upgrades.
   - Upgrade requires multi-sig approval.
   - Immediate upgrades blocked by timelock.

2. **Integrity Verification:**
   - Contract hash verified before upgrade.
   - Upgrade fails if hash mismatch detected.

3. **Emergency Pause:**
   - Upgrade can be paused in case of detected attack.
   - Emergency stop functionality for critical situations.

**Residual Risk:** **CRITICAL** if governance is centralized. Ensure diverse, distributed governance for production.

---

## Cancellation Threat Model Updates (v0.3.0)

### Threat 15: Cancellation After Significant Work (Severity: MEDIUM)

**Attack:** Client cancels contract after freelancer has completed substantial work.

**Scenarios:**
1. Client cancels after 2 of 3 milestones released.
2. Freelancer disputes cancellation claiming completed work.
3. Arbiter must decide fair outcome.

**Mitigations:**

1. **Milestone-Based Cancellation Limits:**
   - In `Funded` state, client can only cancel if zero milestones released.
   - After releases, cancellation requires mutual consent or arbiter decision.

2. **Refund Calculation:**
   - Refund limited to unreleased milestone amounts.
   - Released work is compensated appropriately.

3. **Dispute Path:**
   - Freelancer can initiate dispute if unfair cancellation.
   - Arbiter adjudicates based on evidence.

**Residual Risk:** Dispute resolution quality depends on arbiter. Choose arbiters with conflict resolution experience.

---

## Security Recommendations for Cancellation, Refunds, Disputes, and Governance

### For Clients
1. **Deposit Only When Ready:** Confirm milestones and terms off-chain before funding.
2. **Release Milestones Actively:** Track work progress and release milestones as work is completed.
3. **Use Refund Judiciously:** Request refunds only for valid reasons with documentation.
4. **Participate in Governance:** Monitor governance proposals and vote on parameter changes.

### For Freelancers
1. **Document Work:** Keep evidence of completed milestones before requesting payment.
2. **Monitor Contract State:** Watch for unauthorized cancellation via `contract_cancelled` events.
3. **Use Dispute Process:** Initiate dispute if cancellation is unfair after work delivery.
4. **Understand Governance:** Stay informed about platform fee and parameter changes.

### For Arbiters
1. **Remain Neutral:** Evaluate disputes based on evidence, not party pressure.
2. **Document Decisions:** Record reasoning for dispute resolutions.
3. **Escalate When Needed:** Flag complex cases for community review.

### For Governance Participants
1. **Verify Proposals:** Review all governance proposals thoroughly before voting.
2. **Consider Timelock:** Use timelock period to alert users of upcoming changes.
3. **Monitor Emergency Actions:** Track emergency pause usage and governance decisions.
4. **Diversify Keys:** Use multi-sig for all governance operations.

---

## Off-Chain Integration Notes

### Cancellation Off-Chain
- Off-chain systems must listen for `contract_cancelled` events.
- Payment processors should halt automated payments on cancellation.
- Reputation impact depends on cancellation reason and context.

### Refund Off-Chain
- Payment processor must verify refund amount against on-chain escrow balance.
- Refund should only be processed after on-chain confirmation.
- Idempotency key should include `(contract_id, milestone_id)` to prevent duplicates.

### Dispute Off-Chain
- Dispute notification should be sent to all contract parties.
- Time-bound response windows should be enforced off-chain.
- Resolution must be atomic with on-chain state update.

### Governance Off-Chain
- Governance dashboard should display all proposals and voting status.
- Notification system for timelock activations.
- Emergency contact protocol for critical security events.

---

## Identity Validation Threat Model (v0.3.1)

### Threat 16: Role Overlap and Identity Confusion (Severity: HIGH)

**Attack:** Attacker or colluding parties create a contract where the same address holds multiple roles, enabling unauthorized actions.

**Scenarios:**
1. Client creates contract with `client == freelancer` (same address).
   - Client can self-approve milestone releases without freelancer consent.
   - Client can self-issue reputation without delivering work.
2. Client creates contract with `arbiter == client`.
   - Client can unilaterally cancel contract in `Disputed` state.
   - Client can resolve disputes in their own favor.
3. Freelancer creates contract with `arbiter == freelancer`.
   - Freelancer can cancel contract in `Disputed` state.
   - Freelancer can resolve disputes in their own favor.

**Impact:** Complete bypass of multi-party authorization model. Single actor controls all contract decisions.

**Mitigations:**

1. **Fail-Closed Identity Validation (Constraint 1):**
   - `validate_participant_identities()` function checks all identity rules before contract creation.
   - Validation happens **before any storage writes** (fail-closed principle).
   - Panics with specific error codes if any rule is violated.

2. **Client ≠ Freelancer Rule (Constraint 2):**
   - Contract panics with `EscrowError::ClientEqualsFreelancer` (error code 17) if `client == freelancer`.
   - Prevents self-approval of milestone releases and self-collection of funds.
   - Enforced at contract creation time; cannot be bypassed later.

3. **Arbiter Independence Rule (Constraint 3):**
   - Contract panics with `EscrowError::ArbiterRoleOverlap` (error code 18) if:
     - `arbiter == client`, OR
     - `arbiter == freelancer`
   - Ensures arbiter is a fully independent third party.
   - Prevents arbiter from unilaterally cancelling or resolving disputes in their favor.

4. **Optional Arbiter Support (Constraint 4):**
   - Arbiter can be `None` (no third-party dispute resolution).
   - If `None`, no arbiter-specific checks are performed.
   - Allows two-party contracts without requiring a third party.

5. **Atomic Validation (Constraint 5):**
   - All identity checks are performed in a single atomic operation.
   - No partial state is possible; either all checks pass or entire transaction reverts.
   - Prevents race conditions or concurrent validation bypasses.

**Residual Risk:** Very low. Identity validation is cryptographic (address equality) and cannot be spoofed. Assumes Soroban SDK's address comparison is correct.

---

### Threat 17: Arbiter Collusion (Severity: MEDIUM)

**Attack:** Arbiter colludes with one party to unfairly resolve disputes or cancel contracts.

**Mitigations:**

1. **Transparent Audit Trail:**
   - All arbiter actions (cancellation, dispute resolution) are on-chain and auditable.
   - Off-chain governance can review arbiter decisions and penalize bias.

2. **Reputation System:**
   - Arbiters with poor dispute resolution records can be de-listed.
   - Freelancers and clients can choose arbiters based on reputation.

3. **Multi-Arbiter Support (Future):**
   - High-value contracts can require multiple arbiters for consensus.
   - Prevents single arbiter collusion.

**Residual Risk:** **MEDIUM** if arbiter selection is not properly decentralized. Choose arbiters with conflict resolution experience and good reputation.

---

### Threat 18: Identity Spoofing via Contract Reuse (Severity: LOW)

**Attack:** Attacker creates multiple contracts with overlapping identities to confuse off-chain systems.

**Scenarios:**
1. Attacker creates contract A: `(alice, bob, charlie)`.
2. Attacker creates contract B: `(alice, bob, diana)`.
3. Off-chain system confuses the two contracts and attributes reputation incorrectly.

**Mitigations:**

1. **Unique Contract IDs:**
   - Each contract has a unique `contract_id` assigned at creation.
   - Off-chain systems must use `(contract_id, event_type)` as the deduplication key.

2. **Event Transparency:**
   - All events include the full contract ID and participant addresses.
   - Off-chain systems can verify participant consistency across events.

3. **Audit Trail:**
   - Complete on-chain history of all contracts and their participants.
   - Off-chain systems can reconstruct and verify contract lineage.

**Residual Risk:** Very low. Assumes off-chain systems properly deduplicate by contract ID.

---

### Identity Validation Test Coverage

The test suite in `test/input_sanitization_identities.rs` covers:

1. **Client ≠ Freelancer Rule:**
   - ✓ Rejects `client == freelancer`
   - ✓ Accepts distinct client and freelancer
   - ✓ Multiple contracts with different participants

2. **Arbiter Independence Rule:**
   - ✓ Rejects `arbiter == client`
   - ✓ Rejects `arbiter == freelancer`
   - ✓ Accepts distinct arbiter (different from both)
   - ✓ Rejects partial arbiter overlap

3. **Optional Arbiter:**
   - ✓ Accepts `None` arbiter
   - ✓ Accepts `Some(arbiter)` with distinct address

4. **Fail-Closed Validation:**
   - ✓ Validation happens before storage writes
   - ✓ No partial state on validation failure

5. **Edge Cases:**
   - ✓ Three-way distinct addresses
   - ✓ Multiple distinct contracts
   - ✓ Non-contiguous participant sets

**Test Count:** 13 comprehensive tests covering all rules and edge cases.

---

### Security Recommendations for Identity Validation

### For Clients
1. **Verify Freelancer Identity:** Confirm freelancer address off-chain before contract creation.
2. **Choose Independent Arbiter:** Select an arbiter with no financial interest in the outcome.
3. **Monitor Contract Creation:** Verify contract was created with correct participants via `get_contract()`.

### For Freelancers
1. **Verify Client Identity:** Confirm client address off-chain before contract creation.
2. **Verify Arbiter Independence:** Ensure arbiter is not affiliated with client.
3. **Monitor Contract State:** Watch for unauthorized contract modifications.

### For Arbiters
1. **Remain Neutral:** Do not create contracts where you are a participant.
2. **Disclose Conflicts:** If you have a financial interest, recuse yourself.
3. **Document Decisions:** Record reasoning for all dispute resolutions.

### For Off-Chain Integration
1. **Deduplicate by Contract ID:** Use `(contract_id, event_type)` as deduplication key.
2. **Verify Participants:** Cross-check participant addresses in events against contract creation.
3. **Audit Trail:** Maintain complete history of all contracts and their participants.

---

## Coverage Matrix

| Lifecycle Operation | On-Chain Security | Off-Chain Requirement | Test Coverage |
|---------------------|------------------|----------------------|---------------|
| Create Contract | Participant validation, milestone validation | Off-chain agreement | **HIGH** |
| Deposit Funds | Amount validation, authorization | Atomic transfer verification | **HIGH** |
| Release Milestone | Released flag, authorization | Idempotent event processing | **HIGH** |
| Complete Contract | All milestones released check | - | **HIGH** |
| Issue Reputation | Completion gate, rating validation, single-issuance | Deduplication | **HIGH** |
| Cancel Contract | Role-based, status check, release check | Event monitoring | **MEDIUM** |
| Request Refund | Release check, amount bounds, arbiter auth | Idempotent processing | **MEDIUM** |
| Initiate Dispute | Role-based, status gate | Notification, response window | **MEDIUM** |
| Resolve Dispute | Arbiter authorization | Documentation, evidence | **MEDIUM** |
| Governance Update | Multi-sig, bounds, timelock | Dashboard, notifications | **MEDIUM** |

---

## Assumptions for Off-Chain Integration

1. **Asset Transfers:** Actual asset transfers (deposits, releases, refunds) happen off-chain and must be atomic with contract state updates.
2. **Event Indexing:** All events are indexed by `(contract_id, event_type)` to prevent duplicate processing.
3. **Arbiter Selection:** Arbiter selection happens off-chain with agreement from both parties.
4. **Governance Security:** Governance keys are stored securely with multi-sig requirements.
5. **Timeout Handling:** Off-chain timeout monitoring triggers escalation or auto-resolution.
6. **Dispute Evidence:** Evidence submission happens off-chain; on-chain stores only resolution.

---

## Milestone-Level Partial Refund Security (PR #213)

### Overview

`refund_milestone` allows the client to return unused escrow funds on a per-milestone basis.
The function is designed with defence-in-depth to prevent every known class of refund abuse.

### Security Properties

#### 1. No double-refund

Each `Milestone` carries a `refunded: bool` flag that is set atomically when the refund is
applied.  Any subsequent call that includes the same milestone index panics with
`EscrowError::MilestoneAlreadyRefunded`.  The flag is stored in persistent contract storage
and is never reset.

#### 2. No refund-after-release

A milestone that has already been released to the freelancer (`released == true`) cannot be
refunded.  The check is performed before any state mutation, so the transaction reverts
cleanly with `EscrowError::MilestoneAlreadyReleased`.

#### 3. Balance cap — refund ≤ available escrow balance

Before any state is mutated the function computes:

```
available = total_deposited − released_amount − refunded_amount
```

If `available < sum(requested milestone amounts)` the call panics with
`EscrowError::InsufficientEscrowBalance`.  This prevents the contract from ever entering a
state where `refunded_amount > total_deposited − released_amount`.

#### 4. Accounting invariant

The invariant `total_deposited == released_amount + refunded_amount + available_balance` is
maintained after every operation.  `get_refundable_balance` exposes the available balance so
off-chain systems can verify it at any time.

#### 5. Duplicate-index guard

A single `refund_milestone` call that lists the same milestone index more than once is
rejected with `EscrowError::DuplicateMilestoneInRefund` before any state is touched.

#### 6. Atomic all-or-nothing semantics

All validation (bounds check, released check, refunded check, balance check) is performed
in a read-only pass over the milestone list before any writes occur.  If any check fails the
entire transaction reverts; no partial refund is ever committed.

#### 7. Status transition to `Refunded`

When every milestone is either released or refunded the contract status transitions to
`ContractStatus::Refunded`.  This is a terminal state: no further deposits, releases, or
refunds are possible.

#### 8. Event emission for indexers

Two events are emitted on a successful call:

| Event | Topics | Data |
|-------|--------|------|
| `milestone_refunded` | `(contract_id,)` | `(milestone_idx, amount, timestamp)` |
| `contract_refunded`  | `(contract_id,)` | `(total_refund, cumulative_refunded, timestamp)` |

Per-milestone events allow indexers to track individual refunds; the contract-level event
provides a summary for dashboards.

### Threat Analysis

| Threat | Mitigation | Residual Risk |
|--------|-----------|---------------|
| Double-refund same milestone | `refunded` flag, panics on second attempt | Negligible |
| Refund after release | `released` check before any mutation | Negligible |
| Refund exceeds balance | Balance cap check before mutation | Negligible |
| Duplicate indices in one call | O(n²) dedup check (n ≤ 10) | Negligible |
| Partial state on validation failure | All-or-nothing validation pass | Negligible |
| Indexer double-counting | Per-milestone events keyed by `(contract_id, milestone_idx)` | Low (indexer must deduplicate) |

### Coverage Matrix Update

| Operation | On-Chain Security | Test Coverage |
|-----------|------------------|---------------|
| `refund_milestone` — single milestone | Balance cap, released/refunded flags | **HIGH** |
| `refund_milestone` — multiple milestones | Duplicate check, all-or-nothing | **HIGH** |
| `refund_milestone` — all milestones | Status → Refunded | **HIGH** |
| Mixed release + refund | Invariant verification | **HIGH** |
| Double-refund guard | `MilestoneAlreadyRefunded` error | **HIGH** |
| Refund-after-release guard | `MilestoneAlreadyReleased` error | **HIGH** |
| Insufficient balance guard | `InsufficientEscrowBalance` error | **HIGH** |
| Release-after-refund guard | `MilestoneAlreadyRefunded` error | **HIGH** |
