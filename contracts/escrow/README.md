# Escrow Contract

This is a Rust smart contract for the **Soroban** platform that implements a simple escrow system for freelancers and clients. Payments are handled via **milestones**, ensuring that funds are only released when work is completed and verified. 

---

## Features

- Create an escrow contract between a **client** and a **freelancer**.  
- Define multiple **milestones**, each with a set payment amount.  
- Deposit funds into escrow (only the client can do this).  
- Release milestone payments to the freelancer once verified.  
- Issue a reputation score or credential to the freelancer after the contract is completed.  
- Fully tested with unit tests to ensure contract correctness.

---

## Security Notes

- Only the **client** is allowed to deposit funds.  
- Only the **freelancer** can receive payments for milestones.  
- Milestone amounts are validated to be **greater than zero**.  
- Non-existent contracts are safely handled to prevent panics.  
- Token transfers are skipped during testing to avoid unnecessary errors.  
- Always verify the addresses used when calling contract methods.  


