# Arbiter Model & Invariants

This document describes the arbiter data model used by the TalentTrust escrow contract, the invariants that must always hold, and the public entrypoints that interact with arbiter state.

## Data Model

The arbiter is an optional Address stored inside each Contract. It is set when the contract is created (or later via client migration) and is only used when a dispute is raised.

### Relevant Types

- ContractStatus: Disputed, Completed, Refunded
- DisputeResolution: FullRefund, PartialRefund, FullPayout, Split(DisputeSplit)
- DisputeSplit: client_amount (i128), freelancer_amount (i128)

### Storage

The arbiter address is stored as part of the Contract value under DataKey::Contract(contract_id).

## Invariants

1. Arbiter is optional until a dispute is opened. Once a dispute is raised, an arbiter must be present.
2. Only the stored arbiter can resolve a dispute.
3. For DisputeResolution::Split, both amounts must be >= 0 and their sum must equal the available balance exactly.
4. After resolution, funded_amount = released_amount + refunded_amount (accounting must stay consistent).
5. A contract can be finalized only once.
6. Arbiter cannot be the contract itself or the admin.

## Entrypoints that touch the arbiter

- create_contract (Client) → Optionally sets the arbiter address
- raise_dispute (Client or Freelancer) → Requires arbiter to be set
- resolve_dispute (Arbiter only) → Uses arbiter for authorization
- finalize (Admin / parties) → Locks the final resolution

## Worked Example

1. Create a contract that includes an arbiter
2. Raise a dispute
3. Arbiter resolves the dispute using DisputeResolution::Split

## Related Error Codes

- ArbiterRequired → Dispute opened but no arbiter is set
- MissingArbiter → Arbiter address is missing
- InvalidArbiter → Caller is not the stored arbiter
- InvalidDisputeSplit → Split amounts do not equal available balance

This document is the single source of truth for the arbiter model.
