# Requirements Document

## Introduction

This document specifies the requirements for implementing fee accounting functionality in the TalentTrust escrow smart contract. The feature enables the platform to collect fees from milestone releases, properly split fees between stakeholders, handle fractional amounts through rounding, and maintain accurate treasury totals. This is essential for platform sustainability and transparent fee management.

## Glossary

- **Escrow_Contract**: The Soroban smart contract that holds funds in escrow and manages milestone-based payments between clients and freelancers
- **Fee**: A percentage-based charge deducted from milestone payments before releasing funds to the freelancer
- **Fee_Rate**: The percentage of a milestone amount that is collected as a fee (e.g., 2.5%)
- **Treasury**: The contract's storage location that accumulates all collected fees
- **Milestone_Release**: The process of transferring funds from escrow to the freelancer after approval
- **Rounding**: The process of converting fractional stroops (Stellar's smallest unit) to whole numbers
- **Stroops**: The smallest unit of currency on the Stellar network (1 XLM = 10,000,000 stroops)
- **Fee_Split**: The distribution of collected fees between multiple recipients (e.g., platform treasury, referral rewards)
- **Net_Amount**: The amount paid to the freelancer after fee deduction (milestone_amount - fee)

## Requirements

### Requirement 1: Fee Calculation

**User Story:** As a platform operator, I want fees to be automatically calculated from milestone releases, so that the platform can sustain operations.

#### Acceptance Criteria

1. WHEN a milestone is released, THE Escrow_Contract SHALL calculate the fee as (milestone_amount * Fee_Rate)
2. THE Escrow_Contract SHALL support Fee_Rate values between 0% and 10% inclusive
3. THE Escrow_Contract SHALL calculate the Net_Amount as (milestone_amount - fee)
4. THE Escrow_Contract SHALL ensure that (fee + Net_Amount) equals the original milestone_amount after rounding
5. IF the Fee_Rate is 0%, THEN THE Escrow_Contract SHALL transfer the full milestone_amount to the freelancer with no fee deduction

### Requirement 2: Fee Rounding Behavior

**User Story:** As a smart contract developer, I want fractional stroops to be handled correctly through rounding, so that all amounts are valid integers and no value is lost.

#### Acceptance Criteria

1. WHEN a calculated fee contains fractional stroops, THE Escrow_Contract SHALL round down to the nearest whole stroop
2. WHEN rounding creates a remainder, THE Escrow_Contract SHALL add the remainder to the Net_Amount paid to the freelancer
3. THE Escrow_Contract SHALL ensure that no stroops are lost during rounding (total in = total out)
4. FOR ALL milestone releases, THE Escrow_Contract SHALL verify that (fee + Net_Amount) equals the original milestone_amount exactly
5. THE Escrow_Contract SHALL handle edge cases where the fee calculation results in 0 stroops due to rounding

### Requirement 3: Treasury Accumulation

**User Story:** As a platform operator, I want all collected fees to be tracked in a treasury total, so that I can monitor platform revenue and manage withdrawals.

#### Acceptance Criteria

1. WHEN a milestone is released with a fee, THE Escrow_Contract SHALL add the fee amount to the Treasury total
2. THE Escrow_Contract SHALL initialize the Treasury total to 0 when the contract is first deployed
3. THE Escrow_Contract SHALL maintain the Treasury total across multiple milestone releases and multiple escrow contracts
4. THE Escrow_Contract SHALL provide a read-only function to query the current Treasury total
5. THE Escrow_Contract SHALL ensure the Treasury total never decreases except through authorized withdrawal operations

### Requirement 4: Fee Split Distribution

**User Story:** As a platform operator, I want fees to be split between multiple recipients, so that referral rewards and partner shares can be distributed automatically.

#### Acceptance Criteria

1. WHERE fee splitting is enabled, THE Escrow_Contract SHALL support distributing fees to up to 3 recipients
2. WHEN a fee is split, THE Escrow_Contract SHALL calculate each recipient's share as (fee * recipient_percentage)
3. THE Escrow_Contract SHALL ensure that all recipient percentages sum to exactly 100%
4. WHEN rounding creates remainders in fee splits, THE Escrow_Contract SHALL allocate remainders to the primary treasury recipient
5. THE Escrow_Contract SHALL track each recipient's accumulated fees separately in storage

### Requirement 5: Fee Configuration

**User Story:** As a platform administrator, I want to configure fee rates and split percentages, so that the platform can adjust its fee structure as needed.

#### Acceptance Criteria

1. THE Escrow_Contract SHALL store the Fee_Rate in persistent contract storage
2. THE Escrow_Contract SHALL allow authorized administrators to update the Fee_Rate
3. WHEN the Fee_Rate is updated, THE Escrow_Contract SHALL apply the new rate only to future milestone releases
4. THE Escrow_Contract SHALL validate that Fee_Rate updates are within the allowed range (0% to 10%)
5. THE Escrow_Contract SHALL emit an event when the Fee_Rate is updated, including the old and new values

### Requirement 6: Fee Accounting Tests

**User Story:** As a smart contract developer, I want comprehensive tests for fee accounting, so that I can verify correctness and prevent financial bugs.

#### Acceptance Criteria

1. THE Test_Suite SHALL verify fee calculations for milestone amounts ranging from 1 stroop to 1,000,000 XLM
2. THE Test_Suite SHALL test rounding behavior with Fee_Rate values that produce fractional stroops
3. THE Test_Suite SHALL verify Treasury accumulation across multiple milestone releases
4. THE Test_Suite SHALL test fee splits with various percentage combinations that sum to 100%
5. THE Test_Suite SHALL verify that no stroops are lost or created during fee processing (conservation property)
6. THE Test_Suite SHALL test edge cases including zero fees, maximum fees, and minimum milestone amounts
7. THE Test_Suite SHALL achieve minimum 95% code coverage for all fee accounting functions

### Requirement 7: Fee Accounting Security

**User Story:** As a security auditor, I want fee accounting to be secure against manipulation, so that funds cannot be stolen or misdirected.

#### Acceptance Criteria

1. THE Escrow_Contract SHALL prevent unauthorized addresses from modifying the Fee_Rate
2. THE Escrow_Contract SHALL prevent unauthorized addresses from withdrawing from the Treasury
3. WHEN calculating fees, THE Escrow_Contract SHALL prevent integer overflow for all supported milestone amounts
4. THE Escrow_Contract SHALL prevent fee calculations that would result in negative Net_Amount values
5. THE Escrow_Contract SHALL validate all fee split percentages to prevent total exceeding 100%

### Requirement 8: Fee Accounting Documentation

**User Story:** As a code reviewer, I want clear documentation of fee accounting logic, so that I can efficiently review and verify the implementation.

#### Acceptance Criteria

1. THE Implementation SHALL include Rust doc comments for all fee accounting functions
2. THE Documentation SHALL explain the rounding strategy and remainder handling
3. THE Documentation SHALL provide examples of fee calculations with actual stroop values
4. THE Documentation SHALL document all fee-related storage keys and data structures
5. THE README SHALL include a section explaining the fee accounting system and how to run fee tests
