# Escrow Dispute Workflow

## Opening a dispute

Either the client or the freelancer may call `open_dispute(id, caller)` to
transition a `Created` or `Funded` escrow into the `Disputed` state.

**Requirement (issue #1800):** the escrow must hold a positive net balance at
the time the dispute is opened:

```
held = funded_amount - released_amount > 0
```

If `held == 0` the call returns `EscrowError::InsufficientFunds`. This
prevents disputes on unfunded `Created` escrows (which would block
`expire_unfunded` and `cancel_contract`) and on `Funded` escrows whose entire
balance has already been released to the freelancer.

The opener also posts a refundable bond proportional to `total_amount`.

## Submitting evidence

While an escrow is `Disputed`, either party may submit a `BytesN<32>` evidence
hash with `submit_evidence(id, caller, evidence_hash)`. The caller must
authorize the call and be the escrow's client or freelancer. Each party may
submit up to `MAX_EVIDENCE_PER_PARTY` hashes (currently five); submissions are
returned in order by `get_evidence(id)` with the submitter and ledger timestamp.
The contract emits a versioned `evidence` event for each accepted hash.

Evidence submission observes the `PAUSE_DISPUTE` pause and is blocked while
the escrow is frozen. Evidence storage receives the normal persistent TTL
extension when submitted and when `extend_escrow_ttl` is called. Evidence is
deleted when the dispute is settled, including timeout, resolver, panel, and
mutual-settlement paths.

## Resolving a dispute

The admin (or a panel holder above `PANEL_THRESHOLD`) calls
`resolve_dispute(admin, id, freelancer_bps, refund_bond)`:

- `freelancer_bps` (0–10 000) determines the split of the held balance.
- If `refund_bond` is true, the bond is returned to the opener; otherwise it
  is kept in the contract as a protocol fee.

## Timeout resolution

If a `Disputed` escrow is left unresolved for `DISPUTE_TIMEOUT_SECS` (30 days),
anyone may call `resolve_expired_dispute(id)` to split the balance 50/50
(`DISPUTE_TIMEOUT_SPLIT_BPS = 5 000`).

## expire_unfunded is unaffected

Because `open_dispute` now requires held funds, an unfunded `Created` escrow
can never enter `Disputed`. `expire_unfunded` therefore remains available on
any escrow that was never funded past its `FUNDING_WINDOW_SECS` deadline.
