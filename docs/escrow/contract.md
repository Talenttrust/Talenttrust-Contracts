# Escrow Contract Documentation

## Overview
The Escrow Contract is a Rust smart contract built for the Soroban platform. It provides a secure way for clients and freelancers to handle payments with milestones, ensuring that funds are released only when work is verified.

This contract includes:

- Contract creation between a client and freelancer
- Milestone-based payments
- Secure fund deposit and release
- Reputation issuance for freelancers
- Automated unit tests to verify correctness

## Contract Structure
### Types
ContractStatus: Represents the state of an escrow contract. Values:
- Created – Contract created but not funded
- Funded – Client has deposited funds
- Completed – All milestones completed
- Disputed – Issue flagged for dispute

Milestone: Defines a payment milestone:

amount: i128 – payment amount  
released: bool – whether the milestone has been paid

EscrowContract: Holds the full contract data:

client: Address – client address  
freelancer: Address – freelancer address  
milestones: Vec<Milestone> – milestone payments  
status: ContractStatus – current state

## Functions
### create_contract(env, client, freelancer, milestone_amounts) -> u32
- Creates a new escrow contract.
- Stores the client and freelancer addresses.
- Sets up milestones with specified amounts.
- Returns a contract_id.

### deposit_funds(env, contract_id, token, client, amount) -> bool
- Deposits funds into escrow.
- Only the client can call this.
- Updates contract status to Funded after success.
- Returns true if successful.

### release_milestone(env, contract_id, token, freelancer, amount) -> bool
- Releases a milestone payment to the freelancer.
- Only the freelancer can receive payments.
- Updates contract status to Completed after success.
- Returns true if successful.

### issue_reputation(env, freelancer, rating) -> bool
- Issues a reputation score for the freelancer after contract completion.
- Returns true.

### hello(env, to) -> Symbol
- Simple test function to verify contract interaction.
- Returns the same symbol passed in.

## Security Considerations
- Only the client can deposit funds.
- Only the freelancer can receive milestone payments.
- Milestone amounts must be greater than zero.
- Handles non-existent contracts safely using Option.
- Skips token transfers during unit tests to prevent errors.
- Always validate addresses before calling contract functions.

## Testing
All core functions are covered with unit tests.
Tests include:
- Contract creation
- Fund deposit
- Milestone release
- Invalid deposit handling
- Hello-world function check
