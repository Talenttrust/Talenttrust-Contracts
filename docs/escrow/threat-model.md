# Escrow Threat Model

This threat model reflects the public API currently implemented in
`contracts/escrow/src/lib.rs`.

## Implemented Security Boundaries

- Contract creation requires client authorization.
- Reputation issuance requires client authorization and a completed contract.
- Cancellation requires client or freelancer authorization.
- Pause and emergency controls require the initialized admin.
- Deposits fail closed on non-positive amounts and overfunding.
- Releases fail closed on invalid milestones, duplicate release, and
  insufficient available balance.
- Arithmetic for deposit/release accounting uses checked helpers for aggregate
  totals.

## Known Live Risks

- `release_milestone` does not authenticate a caller yet. Integrations must not
  claim client-only or arbiter-only release authorization until the contract
  implements it.
- Token custody and token transfers are outside `lib.rs`; state changes must be
  integrated with asset movement carefully and audited separately.
- A single initialized admin controls pause/emergency operations; admin key
  management is operational, not enforced by multi-sig or timelock logic in this
  contract.

## Planned Controls

- Two-step admin transfer:
  [#318](https://github.com/Talenttrust/Talenttrust-Contracts/issues/318)
- Protocol fee accounting and withdrawal:
  [#313](https://github.com/Talenttrust/Talenttrust-Contracts/issues/313),
  [#314](https://github.com/Talenttrust/Talenttrust-Contracts/issues/314)
- Finalization:
  [#320](https://github.com/Talenttrust/Talenttrust-Contracts/issues/320)
- Governed parameter setter:
  [#323](https://github.com/Talenttrust/Talenttrust-Contracts/issues/323)

## Reviewer Checklist

1. Confirm docs do not describe planned entrypoints as implemented.
2. Confirm mutating calls fail while paused/emergency where applicable.
3. Confirm double release, duplicate reputation issuance, and completed-contract
   cancellation fail closed.
4. Confirm token transfer assumptions are handled outside this contract.
