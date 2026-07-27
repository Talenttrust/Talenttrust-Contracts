# Threat Model: Reputation

Scope: `Escrow::issue_reputation` (contracts/escrow/src/lib.rs) and the
storage it reads/writes — `DataKey::ReputationIssued(contract_id)`,
`DataKey::PendingReputationCredits(freelancer)`, `DataKey::Reputation(freelancer)`,
`Contract::reputation_issued` — plus `grant_pending_reputation_credit`, the
internal function (called from the milestone-release paths) that mints the
pending credit `issue_reputation` later consumes.

## Trust assumptions

- `contract.client` and `contract.freelancer` are trusted values: they were
  fixed at `create_contract` time and are not attacker-writable afterward.
- Soroban's `Address::require_auth()` is trusted to prove the transaction was
  actually authorized by the address it's called on — the contract cannot be
  tricked into treating an unsigned call as authorized.
- `rating`, `comment`, and `caller` are **untrusted, attacker-controlled**
  call arguments. Nothing about them is assumed valid before the checks below
  run.
- A "pending reputation credit" for a freelancer is only trusted to exist if
  it was minted by `grant_pending_reputation_credit`, which itself only runs
  on the milestone-completion paths (release / dispute resolution reaching
  `ContractStatus::Completed`). This is the mechanism that ties a reputation
  event to real, paid-for work rather than to an arbitrary contract record.

## Attacker capabilities

An attacker can call `issue_reputation(env, contract_id, caller, rating, comment)`
directly, with:
- any `contract_id` (including ones they have no relationship to),
- any `caller` address (they do not need to control it to *call* the
  function — only to make `require_auth()` succeed),
- an arbitrary `rating` (any `u32`) and `comment` (any string, any byte length).

What an attacker **cannot** do: make `caller.require_auth()` succeed for an
address they don't control. Soroban's auth framework enforces that
independent of contract logic, so no amount of guessing `caller` values lets
an attacker impersonate `contract.client`.

## Mitigations, mapped to the actual checks (in source order)

1. **Role gating** — `if caller != contract.client { panic UnauthorizedRole }`.
   Only the stored client may issue reputation for a given contract; the
   freelancer or a third party cannot rate themselves in or bypass the client.
2. **Rating bounds** — `if rating < 1 || rating > 5 { panic InvalidRating }`.
   Prevents out-of-range/garbage values from being written to on-chain
   reputation state.
3. **Comment bounds** — `EmptyComment` / `CommentTooLong` (200-byte cap).
   The cap is a direct mitigation against unbounded on-chain storage growth
   from attacker-supplied strings (a storage-cost/DoS concern, not just
   cosmetic).
4. **Lifecycle gating** — `if contract.status != Completed { panic NotCompleted }`.
   Reputation can only be issued once the engagement has actually completed
   (all milestones released or refunded per the status-transition rules in
   `release.rs`/`refund_impl.rs`), not on an open or disputed contract.
5. **Idempotency** — `if contract.reputation_issued { panic ReputationAlreadyIssued }`.
   A one-shot flag prevents the same completed contract from generating
   reputation more than once (blocks reputation-inflation via repeated calls).
6. **Self-rating guard** — `if contract.client == contract.freelancer { panic SelfRating }`.
   Structurally redundant today (contract creation already requires distinct
   client/freelancer addresses — see `InvalidParticipants` in
   `create_contract.rs`), but kept as defense-in-depth in case that invariant
   is ever relaxed.
7. **Signature verification** — `caller.require_auth()`. Confirms the
   transaction was actually signed/authorized by the address that passed the
   role check in step 1. This is what makes step 1 meaningful rather than a
   self-reported claim.
8. **Earned-credit check** — `pending <= 0 { panic InvalidState }` against
   `DataKey::PendingReputationCredits(contract.freelancer)`, decremented by 1
   on success. This is the real anti-farming control: a client cannot rate a
   freelancer for a contract unless `grant_pending_reputation_credit` already
   minted a credit for that freelancer, which only happens on genuine
   milestone completion. Rating cannot be manufactured without underlying
   completed, paid work.

## Known limitation: validation-before-auth ordering

Steps 1–6 above run **before** `caller.require_auth()` (step 7). This means
any caller — without needing to actually control the `caller` address, i.e.
without a valid signature — can invoke `issue_reputation` and, from which
specific error comes back, learn:
- whether `contract_id` exists,
- whether the caller they supplied matches the stored client,
- whether the contract has reached `Completed`,
- whether reputation was already issued for it.

This is a **low-severity information-disclosure oracle**, not a fund- or
reputation-state integrity issue: no state is mutated and no reputation is
recorded unless `require_auth()` (step 7) actually succeeds, so an attacker
cannot forge a rating this way. It's flagged here because reordering
`require_auth()` earlier (immediately after loading the contract) would close
even this limited disclosure, and other entrypoints in this crate follow the
same "role check, then `require_auth()`" order (see cross-references below),
so this is a repo-wide pattern rather than something specific to reputation.

## Cross-reference: auth checks elsewhere in the crate

The same "verify role/state, then `require_auth()`" shape recurs throughout
`lib.rs` and is not unique to reputation:
- `admin.require_auth()` — governance, pause/unpause, emergency controls
  (e.g. `set_protocol_fee_bps`, `pause`, `unpause`).
- `contract.client.require_auth()` — refund and milestone-release paths
  gated to the client (subject to `release_authorization`, see
  `ReleaseAuthorization` in `types.rs` for the client/arbiter/multisig
  variants that can also require freelancer or arbiter auth).
- `arbiter.require_auth()` — dispute-resolution entrypoints.

Reputation follows the identical pattern; the ordering limitation above
applies equally to those call sites and is called out here only because this
note's scope is reputation.