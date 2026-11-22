# SAC Custody and Settlement-Token Lifecycle

This document describes the custody model for the TalentTrust Escrow contract: which
entrypoint moves funds in which direction, the one-token-per-escrow assumption, where
accrued protocol fees reside until withdrawn, and the accounting invariant that
integrators and auditors can rely on.

Cross-check source: [`contracts/escrow/src/lib.rs`](../../contracts/escrow/src/lib.rs)

## One-Token-per-Escrow Assumption

Each deployed escrow instance custodies **exactly one** Stellar Asset Contract (SAC)
token. The token address is stored under `DataKey::SettlementToken` and must be bound
before `create_contract` or any fund-moving entrypoint can execute. The bound token address
is persisted on each contract record at creation. There is no support for multi-token
escrows; all milestone amounts are denominated in stroops of this single token.

## New: Claimable Payout Fallback

When a token transfer fails (e.g., the recipient's address is blocked or the token contract
rejects the transfer), the contract now credits the intended recipient's **claimable balance**
instead of reverting the entire payout operation. This ensures that one blocked recipient
cannot freeze other payouts or the entire escrow lifecycle.

### Data Key

- **`DataKey::Claimable(token, address)`** — Stores a claimable balance for a specific
  recipient of a specific token. The balance is the amount of stroops the recipient can
  claim once the transfer restriction is removed.

### Claimable Balance Accounting

Claimable balances are included in the escrow's liability accounting via `DataKey::Liability`.
When a claimable balance is credited, the liability is increased accordingly. When the
recipient later claims the funds, the liability is decreased.

## Entrypoint Fund-Flow Map

| Entrypoint | Source | Destination | Amount |
|---|---|---|---|
| `bind_settlement_token` | — | — | No transfer; records the SAC address in persistent storage. |
| `deposit_funds` | Client wallet | Escrow contract address | Exact `amount` argument (must equal total unfunded milestones). |
| `release_milestone` | Escrow contract address | Freelancer wallet | `milestone.amount − protocol_fee`; fee is retained inside the escrow contract. |
| `refund_funders` | Escrow contract address | Funders' wallets | Pro-rata share of held funds. |
| `cancel_contract` | Escrow contract address | Client wallet | All funded-but-unreleased amounts returned (except milestone-backed funds). |
| `withdraw_protocol_fees` | Escrow contract address | Protocol treasury | Accumulated fee balance. |
| `try_pay_out_with_fee_fallback` | Escrow contract address | Recipient wallet **or** claimable balance | Net amount after fee; falls back to claimable on transfer failure. |
| `claim_funds` | Claimable balance | Recipient wallet | Recipient claims their outstanding claimable balance. |
| `try_get_claimable` | — | — | Read-only: returns the claimable balance for a given token/address pair. |

## Token-Binding Step (`bind_settlement_token`)

`bind_settlement_token(admin, token)` is the **write-once** configuration step that
records the SAC address under `DataKey::SettlementToken`.

- Requires `initialize` to have been called (`NotInitialized` error otherwise).
- Requires the caller to be the stored admin (`UnauthorizedRole` otherwise).
- Rejects a second call when a token is already bound (`SettlementTokenAlreadyBound`).
- **Pre-bind probe (issue #723):** Before persisting the token address, the
  entrypoint performs a read-only probe to verify the supplied address is a live
  SAC token contract:
  1. Calls `token::Client::balance(env.current_contract_address())` against the
     candidate address. If the address does not implement the SAC token
     interface, the call panics and the bind is rejected.
  2. Rejects `env.current_contract_address()` (the escrow contract itself) with
     `SettlementTokenIsSelf` — binding self creates a circular custody reference.
  3. Rejects the stored admin address with `SettlementTokenIsAdmin` — conflating
     governance authority with the settlement token role is a privilege-separation
     violation.
- `set_settlement_token` is a deprecated alias retained for backward compatibility;
  new code must call `bind_settlement_token`.

### Reentrancy Mitigation

All downstream money-flow entrypoints (`deposit_funds`, `release_milestone`,
`cancel_contract`, `refund_funders`, `try_pay_out_with_fee_fallback`) follow strict
**state-before-transfer** (Checks-Effects-Interactions) ordering: contract state
is finalized *before* any `token::Client::transfer` call. A malicious token
contract that re-enters the escrow during a transfer will observe the
already-mutated state and cannot double-spend or front-run the operation.

After binding, every fund-moving entrypoint reads this key via
`Escrow::read_settlement_token`. If the key is absent at call time the contract panics
with `SettlementTokenNotConfigured`.

## Deposit (`deposit_funds`)

```
Client wallet ──[SAC transfer]──► Escrow contract address
```

1. Validates `amount > 0`, contract exists, caller is the client, and contract is in
   `Created` status.
2. Calls `token::Client::transfer(from: client, to: escrow_address, amount)` — the SAC
   deducts from the client's on-chain balance and credits the escrow contract address.
3. Updates `contract.funded_amount += amount` and `contract.total_deposited += amount`.
4. Transitions status to `Funded` once `funded_amount >= sum(milestones)`.

The contract address itself holds the balance; there is no separate vault or sub-account.

## Milestone Release (`release_milestone`)

```
Escrow contract address ──[SAC transfer, net of fee]──► Freelancer wallet
```

1. Validates: contract is `Funded`, milestone not already released or refunded,
   caller has the required `ReleaseAuthorization` role, approvals are satisfied.
2. Computes `fee = milestone.amount × fee_bps / 10 000`.
3. Transfers `milestone.amount − fee` from the escrow address to the freelancer.
4. Updates `contract.released_amount += amount paid`.
5. Emits `release` event.

### Fallback: Failed Transfer → Claimable Balance

If the token transfer in step 3 fails (the recipient's token contract rejects the transfer),
instead of reverting the entire operation:

1. The contract **credits the freelancer's claimable balance** with `milestone.amount − fee`.
2. The liability is increased by the claimable amount.
3. The event `claimable` is emitted with `(id, token, freelancer, amount)`.
4. The escrow **continues operating** — other milestones can still be released, and the
   escrow is not stuck in an inconsistent state.

The recipient can later claim their funds by calling `claim_funds(token, recipient)` once
the transfer restriction is removed (e.g., after updating their address or the token
contract being updated).

## Failed Transfer Behavior (`try_pay_out_with_fee_fallback`)

This entrypoint replaces the normal `pay_out_with_fee` for scenarios where a recipient
might have a blocked or rejecting token contract.

1. Attempts the token transfer using `token::Client::try_transfer`.
2. **On success**: Preserves normal-token behavior and event semantics (same as `pay_out_with_fee`).
3. **On failure**: Credits the intended recipient's claimable balance instead of reverting.
   The liability is adjusted to include the claimable amount.
4. **Duplicate protection**: A claimable balance is only credited once per payout; subsequent
   calls either reduce the existing balance or return an error.

## Refund Funders (`refund_funders`)

```
Escrow contract address ──[SAC transfer]──► Funders' wallets
```

1. Validates `amount > 0` and that the escrow has sufficient held funds.
2. Refunds funders their pro-rata share of the held balance.
3. Updates `contract.refunded_amount` and `contract.funded_amount` accordingly.
4. If a funder's transfer fails, the contract attempts to credit their claimable balance
   instead of reverting the entire refund operation.

The contract address holds the funded balance; refunds are sent directly to each funder's
address.

## Claim Funds (`claim_funds`)

```
Recipient calls: claim_funds(token, caller)
```

1. Authenticates the recipient (`caller.require_auth()`).
2. Reads their claimable balance from `DataKey::Claimable(token, caller)`.
3. Rejects if the balance is zero (no funds to claim).
4. Attempts the token transfer from the contract to the recipient.
5. **On success**: Reduces or clears the claimable balance; liability is decreased.
6. **On failure**: Preserves the claimable balance; liability remains unchanged.
7. Emits `claimable` event with updated state.

**Key behaviors:**
- Prevents reentrancy: the claim is executed in a single VM step; no intermediate state
  that a malicious token could re-enter.
- Duplicate claims are handled: if the balance is already zero, the call returns an error
  rather than crediting again.
- Liability accounting is correct: `get_liability` includes claimable balances, and
  `sweep_surplus` never takes funds reserved for claimable recipients.

## Get Liability (`get_liability`)

Returns the total amount the contract owes across all live escrows, including:

- Held milestone balances (released + unreleased).
- Claimable balances for all recipients.
- Protocol fee accumulations.

**Formula**: `liability = sum(held per token) + sum(claimable per token)`

This ensures that auditors and integrators can verify the contract's solvency at any time.

## Sweep Surplus (`sweep_surplus`)

Never takes funds reserved for claimable recipients. The surplus sweep only operates on
funds that are not subject to any claimable balance. If a claimable balance exists for a
given token/address, the sweep skips that portion of the balance.

**Invariant**: `sweep_surplus` never reduces a claimable balance below its current stored value.

## Normal Token Transfer Behavior

All existing token transfer logic (`release_milestone`, `refund_funders`, `cancel_contract`)
continues to behave as before when transfers succeed. The claimable fallback is only
activated when `try_transfer` returns a failure (revert).

## Events

New events introduced for claimable balances:

- `claimable(id, token, address, amount)` — Emitted when a claimable balance is credited
  or claimed.

---

## Failure Modes

| Scenario | Error |
|---|---|
| `deposit_funds` / `release_milestone` called before `bind_settlement_token` | `SettlementTokenNotConfigured` |
| `bind_settlement_token` called a second time | `SettlementTokenAlreadyBound` |
| Any fund-moving call before `initialize` | `NotInitialized` |
| Caller not the expected role (client / admin) | `UnauthorizedRole` |
| Contract is paused | `ContractPaused` |
| Deposit while status is not `Created` | `InvalidState` |
| Release while status is not `Funded` | `InvalidState` |
| Available balance less than requested amount | `InsufficientFunds` |

---

## Security Notes for Integrators

1. **Single-token deployment**: Deploy one escrow contract per currency.  Binding a
   different SAC after the first binding is rejected; migration requires deploying a new
   contract.
2. **Commingled protocol fees**: Accrued fees sit in the escrow contract's own SAC
   balance.  The balance invariant above accounts for them.  Draining `withdraw_protocol_fees`
   regularly avoids confusion when auditing the contract balance.
3. **Client auth on deposit**: The SAC `transfer` in `deposit_funds` requires the
   client's signature (`require_auth`).  An escrow contract cannot pull funds without
   explicit client authorisation per transaction.
4. **Atomicity**: State updates and SAC transfers occur in the same Soroban transaction.
   A failed SAC transfer (e.g., insufficient client balance) leaves contract state
   unchanged.
5. **Overflow protection**: All arithmetic uses `safe_add_amounts` /
   `safe_subtract_amounts` wrappers; overflow panics with `PotentialOverflow` before any
   state or balance is modified.

---

## Admin Token Allowlist

To prevent non-token contracts or malicious tokens from being used in production
deployments, the escrow contract provides an opt-in, admin-controlled token allowlist:

- **`set_token_allowed(token, allowed)`** (admin-only): Adds or removes `token` in
  persistent storage (`DataKey::AllowedToken(Address)`) with TTL extension. Rejects
  allowlisting the escrow contract's own address (`InvalidParties`). Emits
  `("tok_allow",) -> (token, allowed)`.
- **`is_token_allowed(token)`**: Read-only check returning whether `token` is currently
  allowlisted (`false` if unset).
- **`set_allowlist_enabled(enabled)`** (admin-only): Enables or disables allowlist
  enforcement (`DataKey::AllowlistEnabled`, `false` by default). Emits
  `("allowlist",) -> enabled`.
- **`is_allowlist_enabled()`**: Read-only check returning whether allowlist enforcement
  is active.

### Enforcement & Existing Escrows

- While disabled (default), `build_escrow` preserves existing behavior and accepts any
  token address other than the escrow contract itself.
- Once enabled, `build_escrow` — and therefore all creation paths (`create_contract`,
  `create_contract_with_review`, `create_contract_sequential`, `create_and_fund`, and
  `create_stream`) — rejects unlisted tokens with `EscrowError::InvalidParties`.
- Removing a token from the allowlist only blocks *new* escrow creation; existing
  escrows using that token continue to deposit, release, cancel, and settle normally.
