# Implementation Plan: Fee Accounting Tests

## Overview

This plan implements fee accounting functionality for the TalentTrust escrow smart contract. The implementation adds fee calculation, treasury accumulation, fee splitting, and comprehensive testing to ensure correctness and security. All fee accounting logic integrates directly into the existing `release_milestone` function to ensure atomic fee processing.

## Tasks

- [x] 1. Add fee accounting data structures to lib.rs
  - Add `FeeConfig`, `FeeRecipient`, `Treasury`, `FeeCalculation`, and `FeeSplit` structures
  - Add storage key constants for fee configuration and treasury
  - Define validation constraints (rate_bps 0-1000, max 3 recipients, percentages sum to 10000)
  - _Requirements: 5.1, 5.4, 3.2_

- [ ] 2. Implement core fee calculation logic
  - [x] 2.1 Implement `calculate_fee` internal function
    - Calculate fee as `(milestone_amount * rate_bps) / 10000` with floor rounding
    - Calculate net amount as `milestone_amount - fee`
    - Return `FeeCalculation` with fee_amount, net_amount, and splits
    - _Requirements: 1.1, 1.3, 2.1, 2.2_
  
  - [ ]* 2.2 Write property test for fee calculation conservation
    - **Property 1: Conservation property - fee + net_amount = milestone_amount**
    - **Validates: Requirements 1.4, 2.3**
  
  - [ ]* 2.3 Write unit tests for fee calculation edge cases
    - Test zero fee rate (0%)
    - Test maximum fee rate (10%)
    - Test fractional stroops that round down
    - Test minimum amounts where fee rounds to 0
    - _Requirements: 1.5, 2.5, 6.2, 6.6_

- [ ] 3. Implement fee split distribution logic
  - [x] 3.1 Implement `split_fee` internal function
    - Calculate each recipient's share as `(total_fee * percentage_bps) / 10000`
    - Round down each share
    - Calculate remainder and add to primary recipient
    - Return vector of `FeeSplit` entries
    - _Requirements: 4.2, 4.4_
  
  - [ ]* 3.2 Write property test for fee split conservation
    - **Property 2: Split conservation - sum of all splits equals total fee**
    - **Validates: Requirements 4.2, 4.4**
  
  - [ ]* 3.3 Write unit tests for fee split scenarios
    - Test 2-recipient split (70/30)
    - Test 3-recipient split (50/30/20)
    - Test splits with rounding remainders
    - Verify primary recipient receives remainder
    - _Requirements: 4.1, 4.4, 6.4_

- [ ] 4. Implement treasury accumulation
  - [ ] 4.1 Implement `update_treasury` internal function
    - Load current treasury from storage
    - Increment each recipient's balance
    - Increment total treasury amount
    - Save updated treasury to storage
    - _Requirements: 3.1, 3.3_
  
  - [ ]* 4.2 Write property test for treasury accumulation
    - **Property 3: Treasury monotonicity - treasury total never decreases**
    - **Validates: Requirements 3.5**
  
  - [ ]* 4.3 Write unit tests for treasury tracking
    - Test treasury accumulation across multiple releases
    - Test per-recipient balance tracking
    - Verify treasury total equals sum of recipient balances
    - _Requirements: 3.3, 3.4, 4.5, 6.3_

- [ ] 5. Checkpoint - Ensure all tests pass
  - Ensure all tests pass, ask the user if questions arise.

- [ ] 6. Add public fee configuration functions
  - [ ] 6.1 Implement `configure_fees` function
    - Validate caller is admin
    - Validate rate_bps is 0-1000
    - Validate recipients (if provided): max 3, percentages sum to 10000
    - Store FeeConfig in persistent storage
    - _Requirements: 5.1, 5.2, 5.4, 7.1_
  
  - [ ]* 6.2 Write unit tests for fee configuration
    - Test valid configuration updates
    - Test unauthorized caller (should panic)
    - Test invalid rate_bps > 1000 (should panic)
    - Test invalid recipient percentages (should panic)
    - Test more than 3 recipients (should panic)
    - _Requirements: 5.4, 7.1, 7.5_

- [ ] 7. Add public treasury query and withdrawal functions
  - [ ] 7.1 Implement `get_treasury_total` function
    - Load treasury from storage
    - Return total accumulated fees
    - _Requirements: 3.4_
  
  - [ ] 7.2 Implement `get_recipient_balance` function
    - Load treasury from storage
    - Return balance for specified recipient
    - _Requirements: 4.5_
  
  - [ ] 7.3 Implement `withdraw_fees` function
    - Validate caller is admin
    - Validate amount is positive
    - Validate amount doesn't exceed recipient balance
    - Decrement recipient balance and treasury total
    - Transfer funds to recipient
    - _Requirements: 3.5, 7.2_
  
  - [ ]* 7.4 Write unit tests for treasury queries and withdrawals
    - Test get_treasury_total returns correct value
    - Test get_recipient_balance for each recipient
    - Test successful withdrawal
    - Test unauthorized withdrawal (should panic)
    - Test withdrawal exceeding balance (should panic)
    - Test withdrawal with negative amount (should panic)
    - _Requirements: 3.4, 7.2, 7.3_

- [ ] 8. Integrate fee accounting into release_milestone
  - [ ] 8.1 Modify `release_milestone` function
    - Load fee configuration from storage
    - Call `calculate_fee` to compute fee and net amount
    - If fee > 0, call `update_treasury` with splits
    - Transfer net_amount to freelancer (instead of full milestone amount)
    - Maintain all existing validation and authorization logic
    - _Requirements: 1.1, 1.3, 3.1_
  
  - [ ]* 8.2 Write integration tests for release_milestone with fees
    - Test milestone release with 2.5% fee
    - Test milestone release with 5% fee
    - Test milestone release with 0% fee (backward compatibility)
    - Test milestone release with fee splitting enabled
    - Verify net amount transferred to freelancer
    - Verify treasury accumulation after release
    - _Requirements: 1.1, 1.3, 1.5, 3.1, 3.3_

- [ ] 9. Checkpoint - Ensure all tests pass
  - Ensure all tests pass, ask the user if questions arise.

- [ ] 10. Add comprehensive security tests
  - [ ]* 10.1 Write security tests for overflow protection
    - Test fee calculation with maximum milestone amounts (1,000,000 XLM)
    - Test fee calculation with maximum fee rate (10%)
    - Verify no integer overflow occurs
    - _Requirements: 6.1, 7.3_
  
  - [ ]* 10.2 Write security tests for authorization
    - Test configure_fees with unauthorized caller
    - Test withdraw_fees with unauthorized caller
    - Verify only admin can modify fee configuration
    - _Requirements: 7.1, 7.2_
  
  - [ ]* 10.3 Write security tests for validation
    - Test fee calculation doesn't produce negative net amounts
    - Test fee split percentages validation
    - Test rate_bps boundary validation
    - _Requirements: 7.4, 7.5_

- [ ] 11. Add Rust doc comments to all fee accounting functions
  - Add doc comments to `FeeConfig`, `FeeRecipient`, `Treasury` structures
  - Add doc comments to `calculate_fee`, `split_fee`, `update_treasury` functions
  - Add doc comments to `configure_fees`, `get_treasury_total`, `get_recipient_balance`, `withdraw_fees` functions
  - Include examples with actual stroop values
  - Document rounding strategy and remainder handling
  - _Requirements: 8.1, 8.2, 8.3, 8.4_

- [ ] 12. Update README.md with fee accounting documentation
  - Add "Fee Accounting System" section explaining the feature
  - Document fee rate configuration (basis points representation)
  - Explain rounding strategy and conservation property
  - Document fee splitting functionality
  - Provide examples of fee calculations
  - Add instructions for running fee accounting tests
  - _Requirements: 8.5_

- [ ] 13. Final checkpoint - Verify test coverage and run all tests
  - Run all tests to ensure 95%+ coverage of fee accounting functions
  - Verify all property tests pass
  - Verify all unit tests pass
  - Verify all integration tests pass
  - Verify all security tests pass
  - _Requirements: 6.7_

## Notes

- Tasks marked with `*` are optional and can be skipped for faster MVP
- Each task references specific requirements for traceability
- Checkpoints ensure incremental validation
- Property tests validate universal correctness properties (conservation, monotonicity)
- Unit tests validate specific examples and edge cases
- Integration tests verify fee accounting works correctly with existing milestone release logic
- Security tests ensure the implementation is safe against manipulation and overflow
