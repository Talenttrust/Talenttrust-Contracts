# Escrow Contract Architecture

## Overview
The TalentTrust Escrow protocol facilitates secure, milestone-based compensation for freelancers on the Stellar network using Soroban smart contracts.

## Lifecycle State Machine
The contract relies on a rigid physical state machine governed by `ContractStatus`:
1. **Created**: Initialized with parameters (Client, Freelancer, Arbiter, Milestones). No funds hold yet.
2. **Funded**: Client deposits the exact `i128` sum of the underlying milestones. Authorization checks bind here.
3. **Completed**: All milestones have been marked `released: true`.
4. **Disputed**: Placeholder state for active arbitration interventions.

## Authorization
Funds cannot be released until explicit `ReleaseAuthorization` requirements are met:
- `ClientOnly`: Client must approve.
- `ClientAndArbiter`: Either Client or Arbiter can approve.
- `ArbiterOnly`: Only Arbiter can approve.
- `MultiSig`: Both must approve.

## Storage
Contract data is stored persistently using `env.storage().persistent()`. This requires the contract's storage footprint to be actively managed, renewing TTL for long-running escrow agreements.
