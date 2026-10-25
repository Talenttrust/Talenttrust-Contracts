# Milestones & Escrow Error Codes Catalog

This document provides a comprehensive reference for all typed error codes (`Error` / `EscrowError`) defined in the Talenttrust Escrow contract (`contracts/escrow/src/types.rs`). It lists each numerical error code, when it is triggered, how to avoid it, and cross-references the relevant public entrypoints.

---

## Quick Reference Table

| Code | Error Variant | Entrypoint(s) | Trigger Summary |
| :--- | :--- | :--- | :--- |
| **3** | `IndexOutOfBounds` | `approve_milestone_release`, `release_milestone`, `get_milestone_approvals` | Specified milestone index is out of bounds |
| **4** | `AlreadyReleased` | `approve_milestone_release`, `release_milestone` | Milestone is already marked as released |
| **6** | `EmptyRefundRequest` | `refund_milestones` | Refund request vector is empty |
| **7** | `DuplicateMilestoneInRefund` | `refund_milestones` | Duplicate milestone indices provided in refund request |
| **8** | `AlreadyRefunded` | `refund_milestones` | Milestone has already been refunded |
| **9** | `InsufficientFunds` | `deposit_funds`, `release_milestone`, `resolve_dispute`, `withdraw_protocol_fees` | Contract balance or funded amount is insufficient |
| **10** | `ContractNotFound` | `get_contract`, `get_milestones`, `deposit_funds`, `approve_milestone_release`, `release_milestone`, `cancel_contract`, `resolve_dispute`, `issue_reputation` | Contract ID does not exist in storage |
| **11** | `UnauthorizedRole` | `deposit_funds`, `approve_milestone_release`, `release_milestone`, `cancel_contract`, `resolve_dispute`, `issue_reputation`, admin entrypoints | Caller does not possess the required role/authorization |
| **12** | `MissingArbiter` | `resolve_dispute` | Contract has no arbiter assigned |
| **13** | `InvalidArbiter` | `create_contract` | Arbiter address equals client or freelancer address |
| **14** | `InvalidParticipants` | `create_contract` | Client and freelancer addresses are identical or invalid |
| **15** | `AmountMustBePositive` | `create_contract`, `deposit_funds` | Amount parameter is non-positive (`<= 0`) |
| **16** | `InvalidState` | `deposit_funds`, `release_milestone`, `cancel_contract`, `resolve_dispute` | Contract lifecycle status is invalid for operation |
| **17** | `MilestoneAlreadyReleased` | `release_milestone` | Milestone has already been released |
| **18** | `AlreadyApproved` | `approve_milestone_release` | Participant already approved the specified milestone |
| **20** | `InsufficientApprovals` | `release_milestone` | Required approval threshold/policy not met |
| **21** | `FreelancerMismatch` | `approve_milestone_release`, `release_milestone`, `issue_reputation` | Caller is not the registered freelancer |
| **22** | `InvalidRating` | `issue_reputation` | Rating score outside allowed range (1 to 5) |
| **23** | `ReputationAlreadyIssued` | `issue_reputation` | Reputation already issued for contract |
| **25** | `EmptyMilestones` | `create_contract` | Milestone vector is empty |
| **26** | `InvalidMilestoneAmount` | `create_contract` | Milestone amount is non-positive or exceeds max single limit |
| **27** | `ContractIdCollision` | `create_contract` | Contract ID already exists |
| **28** | `ContractIdOverflow` | `create_contract` | Next contract ID exceeds `u32::MAX` |
| **29** | `EmptyComment` | `issue_reputation` | Reputation comment string is empty |
| **30** | `CommentTooLong` | `issue_reputation` | Reputation comment exceeds maximum allowed length |
| **31** | `InvalidParticipant` | `create_contract` | Participant address is invalid or zero |
| **32** | `InvalidDepositAmount` | `deposit_funds` | Deposit amount does not match required milestone funding |
| **33** | `InvalidMilestone` | `create_contract` | Milestone parameters violate validation constraints |
| **34** | `AlreadyInitialized` | `initialize` | Global setup already completed |
| **35** | `InsufficientAccumulatedFees` | `withdraw_protocol_fees` | Fee withdrawal amount exceeds accumulated balance |
| **36** | `NotInitialized` | Core state & admin entrypoints | Global setup has not been executed |
| **37** | `ContractPaused` | State-modifying entrypoints | Contract pause state is active |
| **38** | `EmergencyActive` | State-modifying entrypoints | Emergency controls are active |
| **39** | `SelfRating` | `issue_reputation` | Participant attempting self-rating |
| **40** | `NotCompleted` | `issue_reputation` | Contract status is not `Completed` |
| **41** | `InvalidStatusTransition` | Lifecycle transition entrypoints | State transition is disallowed |
| **42** | `ArbiterRequired` | `resolve_dispute` | Dispute operation attempted without arbiter |
| **43** | `InvalidDisputeSplit` | `resolve_dispute` | Dispute split sum does not match remaining balance |
| **44** | `AccountingInvariantViolated` | Payout / release entrypoints | Balance or accounting invariant check failed |
| **45** | `PotentialOverflow` | Arithmetic & payout helper logic | Checked arithmetic overflow detected |
| **46** | `AlreadyFinalized` | `release_milestone`, `refund_milestones`, `resolve_dispute` | Contract is already finalized/closed |
| **47** | `EvidenceTooLong` | `submit_work_evidence` | Work evidence string exceeds length limit |
| **48** | `TimelockNotElapsed` | `accept_governance_admin` | Governance rotation timelock delay pending |
| **49** | `InvalidProtocolParameters` | `set_governed_parameters`, `set_protocol_fee_bps` | Fee basis points > 10,000 or caps invalid |
| **50** | `AlreadyCancelled` | `cancel_contract` | Contract is already cancelled |
| **51** | `EscrowCapExceeded` | `create_contract` | Contract total escrow amount exceeds protocol cap |
| **52** | `SettlementTokenNotConfigured` | `deposit_funds`, `release_milestone`, `withdraw_protocol_fees` | Settlement token SAC address unconfigured |
| **53** | `MilestoneNotOverdue` | Overdue refund entrypoints | Current ledger timestamp <= milestone deadline |

---

## Detailed Error Code Definitions

### Code 3: `IndexOutOfBounds`
- **When it fires**: Raised when referencing a milestone index `milestone_index` that is greater than or equal to the total number of milestones in the contract.
- **Entrypoint(s)**: `approve_milestone_release`, `release_milestone`, `get_milestone_approvals`, `refund_milestones`.
- **How to avoid**: Query `get_milestones` first and verify that `milestone_index < milestones.len()`.

### Code 4: `AlreadyReleased`
- **When it fires**: Raised when invoking release or approval on a milestone that has already been marked as released (`milestone.released == true`).
- **Entrypoint(s)**: `approve_milestone_release`, `release_milestone`.
- **How to avoid**: Check the milestone list via `get_milestones` and ensure `released == false` prior to calling release.

### Code 6: `EmptyRefundRequest`
- **When it fires**: Raised when the requested vector of milestone indices for refund is empty.
- **Entrypoint(s)**: `refund_milestones`.
- **How to avoid**: Ensure the input vector contains at least one milestone index.

### Code 7: `DuplicateMilestoneInRefund`
- **When it fires**: Raised when the input vector for refunding milestones contains duplicate indices.
- **Entrypoint(s)**: `refund_milestones`.
- **How to avoid**: Deduplicate milestone index lists before passing them to the entrypoint.

### Code 8: `AlreadyRefunded`
- **When it fires**: Raised when requesting a refund for a milestone that has already been refunded (`milestone.refunded == true`).
- **Entrypoint(s)**: `refund_milestones`.
- **How to avoid**: Inspect milestone status and exclude already refunded milestones from refund requests.

### Code 9: `InsufficientFunds`
- **When it fires**: Raised when contract or custody balance is insufficient to complete a milestone release, dispute resolution payout, or fee withdrawal.
- **Entrypoint(s)**: `deposit_funds`, `release_milestone`, `resolve_dispute`, `withdraw_protocol_fees`.
- **How to avoid**: Verify funded balance (`funded_amount`, `get_refundable_balance`) before performing payout transactions.

### Code 10: `ContractNotFound`
- **When it fires**: Raised when supplying a `contract_id` that does not exist in persistent contract storage.
- **Entrypoint(s)**: `get_contract`, `get_milestones`, `deposit_funds`, `approve_milestone_release`, `release_milestone`, `cancel_contract`, `resolve_dispute`, `issue_reputation`.
- **How to avoid**: Use a valid `contract_id` returned by a successful `create_contract` call.

### Code 11: `UnauthorizedRole`
- **When it fires**: Raised when the caller address fails authentication or does not possess the requisite role (client, freelancer, arbiter, or admin).
- **Entrypoint(s)**: `deposit_funds`, `approve_milestone_release`, `release_milestone`, `cancel_contract`, `resolve_dispute`, `issue_reputation`, governance entrypoints.
- **How to avoid**: Sign transactions with the appropriate address corresponding to the required contract role.

### Code 12: `MissingArbiter`
- **When it fires**: Raised when attempting dispute resolution on a contract that was created without an assigned arbiter (`arbiter: None`).
- **Entrypoint(s)**: `resolve_dispute`.
- **How to avoid**: Specify an arbiter address during contract creation if dispute resolution capabilities are required.

### Code 13: `InvalidArbiter`
- **When it fires**: Raised during contract creation if the designated arbiter address is identical to either the client or freelancer address.
- **Entrypoint(s)**: `create_contract`.
- **How to avoid**: Provide a neutral, distinct address for the arbiter.

### Code 14: `InvalidParticipants`
- **When it fires**: Raised during contract creation if client and freelancer addresses are identical or invalid.
- **Entrypoint(s)**: `create_contract`.
- **How to avoid**: Ensure client and freelancer are two distinct, valid Soroban addresses.

### Code 15: `AmountMustBePositive`
- **When it fires**: Raised when a financial amount parameter (deposit or milestone amount) is less than or equal to zero.
- **Entrypoint(s)**: `create_contract`, `deposit_funds`.
- **How to avoid**: Ensure all financial amount arguments are strictly positive integer values (> 0 stroops).

### Code 16: `InvalidState`
- **When it fires**: Raised when invoking an operation while the contract lifecycle status is incompatible (e.g. attempting to fund a completed or cancelled contract).
- **Entrypoint(s)**: `deposit_funds`, `release_milestone`, `cancel_contract`, `resolve_dispute`.
- **How to avoid**: Query `get_contract` and check `status` before executing state-dependent entrypoints.

### Code 17: `MilestoneAlreadyReleased`
- **When it fires**: Raised when attempting to release a milestone that was previously released.
- **Entrypoint(s)**: `release_milestone`.
- **How to avoid**: Verify `milestone.released == false` before invoking `release_milestone`.

### Code 18: `AlreadyApproved`
- **When it fires**: Raised when a participant (client, freelancer, or arbiter) submits an approval for a milestone they have already approved.
- **Entrypoint(s)**: `approve_milestone_release`.
- **How to avoid**: Check `get_milestone_approvals` to confirm current participant approval state.

### Code 20: `InsufficientApprovals`
- **When it fires**: Raised when attempting to release a milestone before the required approval policy (ClientOnly, FreelancerOnly, or MultiSig) is fulfilled.
- **Entrypoint(s)**: `release_milestone`.
- **How to avoid**: Collect required approvals via `approve_milestone_release` prior to triggering milestone release.

### Code 21: `FreelancerMismatch`
- **When it fires**: Raised when an entrypoint restricted to the registered freelancer is called by a different address.
- **Entrypoint(s)**: `approve_milestone_release`, `release_milestone`, `issue_reputation`.
- **How to avoid**: Authorize the invocation using the exact freelancer address bound to the contract.

### Code 22: `InvalidRating`
- **When it fires**: Raised when submitting a reputation rating numerical score outside the range `1..=5`.
- **Entrypoint(s)**: `issue_reputation`.
- **How to avoid**: Pass an integer rating between 1 and 5 inclusive.

### Code 23: `ReputationAlreadyIssued`
- **When it fires**: Raised when attempting to issue reputation for a contract where reputation has already been recorded.
- **Entrypoint(s)**: `issue_reputation`.
- **How to avoid**: Ensure `reputation_issued` flag in contract summary is `false`.

### Code 25: `EmptyMilestones`
- **When it fires**: Raised when creating a contract with an empty list of milestones.
- **Entrypoint(s)**: `create_contract`.
- **How to avoid**: Supply a vector containing at least one milestone specification.

### Code 26: `InvalidMilestoneAmount`
- **When it fires**: Raised when a milestone amount is non-positive or exceeds `MAX_SINGLE_AMOUNT_STROOPS`.
- **Entrypoint(s)**: `create_contract`.
- **How to avoid**: Verify that each milestone amount is positive and within single milestone protocol limits.

### Code 27: `ContractIdCollision`
- **When it fires**: Raised when explicitly specifying a contract ID that is already present in persistent storage.
- **Entrypoint(s)**: `create_contract`.
- **How to avoid**: Rely on automatic contract ID generation or supply unique contract IDs.

### Code 28: `ContractIdOverflow`
- **When it fires**: Raised when contract ID generation reaches maximum `u32::MAX` capacity.
- **Entrypoint(s)**: `create_contract`.
- **How to avoid**: Operational boundary check; monitor total created contract count off-chain.

### Code 29: `EmptyComment`
- **When it fires**: Raised when passing an empty string as a reputation review comment.
- **Entrypoint(s)**: `issue_reputation`.
- **How to avoid**: Pass a non-empty comment string.

### Code 30: `CommentTooLong`
- **When it fires**: Raised when a reputation comment exceeds the maximum allowed character count limit.
- **Entrypoint(s)**: `issue_reputation`.
- **How to avoid**: Truncate or validate comment string length client-side before submission.

### Code 31: `InvalidParticipant`
- **When it fires**: Raised when a participant address parameter is malformed or zero.
- **Entrypoint(s)**: `create_contract`.
- **How to avoid**: Provide valid Soroban `Address` objects.

### Code 32: `InvalidDepositAmount`
- **When it fires**: Raised when a deposit amount does not match required milestone funding calculations or exceeds requirements.
- **Entrypoint(s)**: `deposit_funds`.
- **How to avoid**: Calculate expected deposit amount based on contract deposit mode and milestone requirements.

### Code 33: `InvalidMilestone`
- **When it fires**: Raised when milestone parameters (e.g. deadline timestamp or structure) fail validation checks.
- **Entrypoint(s)**: `create_contract`.
- **How to avoid**: Validate milestone schedule deadlines and parameters before contract creation.

### Code 34: `AlreadyInitialized`
- **When it fires**: Raised when invoking `initialize` on an escrow contract instance that has already completed setup.
- **Entrypoint(s)**: `initialize`.
- **How to avoid**: Check `is_initialized()` or `ReadinessChecklist` state prior to calling `initialize`.

### Code 35: `InsufficientAccumulatedFees`
- **When it fires**: Raised when attempting to withdraw more protocol fees than the stored accumulated fee balance.
- **Entrypoint(s)**: `withdraw_protocol_fees`.
- **How to avoid**: Query `get_accumulated_protocol_fees()` to determine available withdrawable fee balance.

### Code 36: `NotInitialized`
- **When it fires**: Raised when attempting to invoke stateful contract functions before global contract initialization.
- **Entrypoint(s)**: All operational contract entrypoints.
- **How to avoid**: Execute contract initialization during deployment before opening client entrypoints.

### Code 37: `ContractPaused`
- **When it fires**: Raised when invoking state-modifying functions while global pause state is enabled by admin.
- **Entrypoint(s)**: State-modifying entrypoints (`deposit_funds`, `release_milestone`, etc.).
- **How to avoid**: Wait for contract unpause or check `is_paused()` status off-chain.

### Code 38: `EmergencyActive`
- **When it fires**: Raised when invoking standard state modifications while emergency control mode is active.
- **Entrypoint(s)**: Standard state-modifying entrypoints.
- **How to avoid**: Wait for emergency conditions to resolve and controls to reset.

### Code 39: `SelfRating`
- **When it fires**: Raised if a user attempts to issue reputation rating to their own address.
- **Entrypoint(s)**: `issue_reputation`.
- **How to avoid**: Ensure client rates freelancer and freelancer rates client.

### Code 40: `NotCompleted`
- **When it fires**: Raised when calling `issue_reputation` on a contract whose status is not yet `Completed`.
- **Entrypoint(s)**: `issue_reputation`.
- **How to avoid**: Wait until all milestones are released and contract transitions to `Completed`.

### Code 41: `InvalidStatusTransition`
- **When it fires**: Raised when an operation attempts an unsupported status transition (e.g. `Cancelled -> Funded`).
- **Entrypoint(s)**: Contract state transition handlers.
- **How to avoid**: Adhere to documented state lifecycle transitions.

### Code 42: `ArbiterRequired`
- **When it fires**: Raised when invoking dispute operations on a contract that lacks an assigned arbiter.
- **Entrypoint(s)**: `resolve_dispute`.
- **How to avoid**: Ensure the target contract was initialized with an arbiter address.

### Code 43: `InvalidDisputeSplit`
- **When it fires**: Raised during dispute resolution if the sum of client and freelancer split amounts does not equal remaining refundable balance.
- **Entrypoint(s)**: `resolve_dispute`.
- **How to avoid**: Ensure `client_amount + freelancer_amount == remaining_refundable_balance`.

### Code 44: `AccountingInvariantViolated`
- **When it fires**: Raised if internal accounting checks detect a mismatch between total deposits, released funds, and refundable balances.
- **Entrypoint(s)**: Financial settlement functions.
- **How to avoid**: Ensure valid state handling; indicates a core accounting safety protection.

### Code 45: `PotentialOverflow`
- **When it fires**: Raised when safe checked math detects an arithmetic overflow condition.
- **Entrypoint(s)**: Math accumulation and payout calculations.
- **How to avoid**: Keep financial amounts within valid `i128` ranges.

### Code 46: `AlreadyFinalized`
- **When it fires**: Raised when executing operations on a contract that is already in a finalized lifecycle state.
- **Entrypoint(s)**: `release_milestone`, `refund_milestones`, `resolve_dispute`.
- **How to avoid**: Check contract status prior to sending settlement transactions.

### Code 47: `EvidenceTooLong`
- **When it fires**: Raised when work evidence description/URL string exceeds maximum length limits.
- **Entrypoint(s)**: `submit_work_evidence`.
- **How to avoid**: Ensure evidence string byte length is within allowed maximum bounds.

### Code 48: `TimelockNotElapsed`
- **When it fires**: Raised when attempting to finalize governance admin rotation before the timelock delay has elapsed.
- **Entrypoint(s)**: `accept_governance_admin`.
- **How to avoid**: Wait for `ADMIN_ROTATION_MIN_DELAY_LEDGERS` ledgers to pass before completing transfer.

### Code 49: `InvalidProtocolParameters`
- **When it fires**: Raised when setting protocol parameters with invalid fee basis points (> 10,000) or negative caps.
- **Entrypoint(s)**: `set_governed_parameters`, `set_protocol_fee_bps`.
- **How to avoid**: Specify protocol fee basis points `<= 10000` and positive protocol caps.

### Code 50: `AlreadyCancelled`
- **When it fires**: Raised when requesting cancellation of a contract that has already been cancelled.
- **Entrypoint(s)**: `cancel_contract`.
- **How to avoid**: Check contract status before calling `cancel_contract`.

### Code 51: `EscrowCapExceeded`
- **When it fires**: Raised during contract creation if total escrow amount exceeds `max_escrow_total_stroops`.
- **Entrypoint(s)**: `create_contract`.
- **How to avoid**: Ensure contract total amount does not exceed protocol escrow cap limit.

### Code 52: `SettlementTokenNotConfigured`
- **When it fires**: Raised when attempting SAC token custody transfers before a settlement token address is set.
- **Entrypoint(s)**: `deposit_funds`, `release_milestone`, `withdraw_protocol_fees`.
- **How to avoid**: Set settlement token address via governance prior to executing money movement.

### Code 53: `MilestoneNotOverdue`
- **When it fires**: Raised when attempting overdue milestone cancellation before the milestone deadline timestamp has passed.
- **Entrypoint(s)**: Overdue refund functions.
- **How to avoid**: Ensure `env.ledger().timestamp() > milestone.deadline`.
