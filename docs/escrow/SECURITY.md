# Escrow Security Notes

This document reflects the escrow API currently implemented in
`contracts/escrow/src/lib.rs`.

## Implemented Controls

- `initialize(admin)` is single-use and requires `admin.require_auth()`.
- Pause and emergency controls require the stored admin's authorization.
- Mutating lifecycle calls fail while paused or in emergency mode.
- `create_contract` requires client authorization, rejects identical
  client/freelancer addresses, rejects empty or non-positive milestones, caps
  milestone count, and caps total escrow value.
- `deposit_funds` rejects non-positive amounts, repeat exact-total deposits,
  exact-total mismatches, and incremental overfunding.
- `release_milestone` rejects missing contracts, invalid milestone indexes,
  duplicate release, paused/emergency state, and insufficient available balance.
- `issue_reputation` requires the stored client as caller, matching freelancer,
  completed status, rating in `1..=5`, and no prior reputation issuance for the
  contract.
- `cancel_contract` requires client or freelancer authorization and rejects
  completed or already-cancelled contracts.
- `finalize_contract` requires client, freelancer, or assigned arbiter
  authorization, is allowed only from `Completed` or `Disputed`, and locks
  future contract-specific mutations with `AlreadyFinalized`.
- Aggregate amount math uses checked helpers where totals are accumulated.
- Balance-changing operations verify the core accounting invariant:
  `total_deposited == released_amount + refunded_amount + available_balance`.
- Finalization summaries use checked arithmetic and persistent storage. They do
  not expire through TTL and do not create, deduct, or withdraw protocol fees.

## Known Live Gaps

- `release_milestone` does not authenticate a caller. Integrators must not claim
  client-only, arbiter-only, or approval-based release authorization until that
  entrypoint is implemented.
- The contract records escrow accounting only. Token custody, token transfers,
  and atomic asset movement are outside `lib.rs` and must be handled by a
  separate audited integration.
- Admin transfer, protocol fees, refunds, approval expiry, and storage migration
  are not implemented public entrypoints.
- `ReadinessChecklist.governed_params_set` exists, but no live governance
  parameter entrypoint sets it to `true`.

## Planned Security Work

- Two-step admin transfer:
  [#318](https://github.com/Talenttrust/Talenttrust-Contracts/issues/318)
- Protocol fee accounting and withdrawal:
  [#313](https://github.com/Talenttrust/Talenttrust-Contracts/issues/313),
  [#314](https://github.com/Talenttrust/Talenttrust-Contracts/issues/314)
- Immutable finalization:
  [#320](https://github.com/Talenttrust/Talenttrust-Contracts/issues/320)
- Governed parameter setter/readiness wiring:
  [#323](https://github.com/Talenttrust/Talenttrust-Contracts/issues/323)
- Structured deposit and fee events:
  [#336](https://github.com/Talenttrust/Talenttrust-Contracts/issues/336)
- Canonical storage-key reference:
  [#342](https://github.com/Talenttrust/Talenttrust-Contracts/issues/342)

## Reviewer Checklist

1. Verify no integration guide treats planned entrypoints as live API.
2. Verify pause/emergency blocks every mutating lifecycle call.
3. Verify duplicate release, duplicate reputation issuance, overfunding, and
   invalid amount paths fail closed.
4. Verify off-chain token transfer integrations are atomic or idempotent with
   respect to escrow state changes.
