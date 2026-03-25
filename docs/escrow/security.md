# Escrow State Persistence Threat Model

## Scope

This model covers persisted escrow state, participant metadata, governance parameters, and pause controls in `contracts/escrow/src/lib.rs`.

## Security Assumptions

- Soroban address authentication is enforced correctly by `require_auth()`.
- Persistent storage writes are atomic within a transaction.
- Governance and pause admin keys are controlled by trusted operators.
- Off-chain payment execution uses the on-chain persisted state as the source of truth.

## Protected Invariants

- escrow participant identities do not change after contract creation
- milestone amounts and milestone count do not change after contract creation
- `funded_amount` never exceeds `total_amount`
- a milestone cannot be released twice
- `released_amount` never exceeds `funded_amount`
- only completed contracts can produce reputation credits
- a contract can issue reputation once
- pause and emergency flags fail closed for every mutating escrow entry point

## Threat Scenarios and Mitigations

1. Invalid participant spoofing at contract creation.
Mitigation: `create_contract` requires authenticated client consent and rejects `client == freelancer`.

2. Storage corruption through overfunding.
Mitigation: `deposit_funds` rejects any mutation that would push `funded_amount` above `total_amount`.

3. Nested or repeated milestone release attempts.
Mitigation: each milestone has an irreversible `released` flag and releases also require sufficient funded-but-unreleased balance.

4. Reputation inflation through repeated issuance.
Mitigation: contracts accrue one pending credit on completion and consume that credit exactly once when `issue_reputation` succeeds.

5. Governance parameter takeover.
Mitigation: governance initialization is single-use and admin transfer is two-step (`propose` then `accept`).

6. Operations proceeding during incident response.
Mitigation: `create_contract`, `deposit_funds`, `release_milestone`, and `issue_reputation` all check the persisted pause flag before mutating state.

7. Emergency recovery bypass.
Mitigation: `unpause` rejects while the emergency flag is still active; only `resolve_emergency` clears both flags.

## Residual Risks

- there is no on-chain token transfer integration yet, so escrow accounting remains ledger-state bookkeeping rather than asset settlement
- governance and pause roles are single-key controls in this version
- the contract does not emit reviewer-facing events for every state transition
- dispute handling remains outside the scope of this persistence implementation

## Recommended Hardening

1. Move governance and pause authorities behind multisig accounts.
2. Add event emission for create, fund, release, rating, and admin transitions.
3. Introduce dispute-specific persisted state if adversarial settlement paths are required.
4. Add property-based fuzzing for milestone vectors and cumulative funding edge cases.
