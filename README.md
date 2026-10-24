# TalentTrust Contracts

## Live

- Website: https://talenttrust-frontend.vercel.app
- Docs: https://talenttrust-frontend.vercel.app/docs
- $API: https://talenttrust-backend.vercel.app (docs at [/docs](https://talenttrust-backend.vercel.app/docs))
- Contract (testnet): [`CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCHHIFJBZGBKTXKW5`](https://stellar.expert/explorer/testnet/contract/CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCHHIFJBZGBKTXKW5)

Soroban smart contract for the TalentTrust freelancer escrow protocol on Stellar.
Clients lock payment for a job into escrow, release it milestone by milestone as
work is approved, and either party can raise a dispute that the protocol admin settles.

## Deployments

| Network | Contract | ID | Explorer |
| --- | --- | --- | --- |
| Stellar testnet | escrow | `CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCHHIFJBZGBKTXKW5` | [stellar.expert](https://stellar.expert/explorer/testnet/contract/CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCHHIFJBZGBKTXKW5) |

The contract is initialized with admin `GA7TXEBMEPQCLHIYCWDN7RKAO5IMRD2UV7O7TZXMEW6VZUMEXNR4XBB5`.
Machine-readable details are in [deployments/testnet.json](deployments/testnet.json).

## Contract interface (`contracts/escrow`)

| Function | Auth | Description |
| --- | --- | --- |
| `initialize(admin)` | admin | One-time setup |
| `create_contract(client, freelancer, token, milestone_amounts) -> u64` | client | Create an escrow with up to 20 milestones |
| `deposit_funds(id, amount)` | client | Transfer tokens into escrow (cannot exceed total) |
| `release_milestone(id, index)` | client | Pay the freelancer for a milestone |
| `open_dispute(id, caller)` | client or freelancer | Freeze the escrow pending resolution (increments `disputes_opened`) |
| `resolve_dispute(id, freelancer_bps)` | admin | Split the held balance using overflow-safe checked math (records dispute win/loss outcomes based on 5,000 bps threshold) |
| `cancel_contract(id)` | client | Refund unreleased funds and close (increments `cancellations` if funds held) |
| `issue_reputation(id, rating)` | client | Rate a completed or resolved job (1-5), once |
| `void_rating(id, reason)` | admin | Void a fraudulent rating, reverse reputation and emit audit trail |
| `set_paused(bool)` / `is_paused()` | admin / - | Circuit breaker for all mutations |
| `get_contract(id)`, `get_rating(id)`, `contract_count()`, `get_reputation(addr)`, `get_outcomes(addr)`, `get_admin()`, `version()` | - | Reads |

Status flow: `Created -> Funded -> Completed`, with `Disputed -> Resolved` and `Cancelled` side exits. Resolved jobs allow reputation rating; dispute outcomes and cancellation metrics are tracked in persistent `Outcomes`.

## Errors (`EscrowError`)

Error codes are append-only: existing codes never change meaning, and new variants are added at the end of the enum. The frontend and backend map on these numeric codes.

| Code | Variant | Meaning |
| --- | --- | --- |
| 1 | `NotInitialized` | Contract has not been initialized |
| 2 | `AlreadyInitialized` | `administrator` has already been set |
| 3 | `Unauthorized` | Caller is not allowed to perform this action |
| 4 | `NotFound` | Escrow id does not exist |
| 5 | `InvalidState` | Operation is not valid in the current escrow state |
| 6 | `InvalidAmount` | Amount is zero, negative, or otherwise invalid |
| 7 | `TooManyMilestones` | More than 20 milestones were provided |
| 8 | `OverDeposit` | Deposit would exceed the escrow total |
| 9 | `InsufficientFunds` | Escrow holds fewer tokens than requested |
| 10 | `InvalidBps` | Dispute split basis points are outside 0-10,000 |
| 11 | `InvalidRating` | Rating is outside 1-5 |
| 12 | `AlreadyReleased` | Milestone has already been paid out |
| 13 | `Paused` | Contract is paused via the circuit breaker |
| 14 | `NothingToRelease` | No funds are available to release |
| 15 | `ReputationNotAllowed` | Reputation is not allowed in the current state |
| 16 | `ReputationAlreadyIssued` | Reputation has already been issued for this escrow |
| 17 | `ArithmeticOverflow` | A checked arithmetic operation overflowed (amounts, ids, or counters) |
| 18 | `RatingNotFound` | No rating record exists for this escrow (e.g. issued before the audit trail upgrade) |
| 19 | `RatingAlreadyVoided` | Rating has already been voided |

## Events

Events are emitted with the same payloads for non-overflowing inputs as before the checked-arithmetic change.

| Event | Data | Description |
| --- | --- | --- |
| `init` | admin | Contract initialized |
| `created` | id, client, freelancer, total | Escrow created |
| `deposit` | id, amount | Funds deposited |
| `released` | id, index, amount | Milestone released to freelancer |
| `dispute_opened` | id, caller | Dispute opened |
| `dispute_resolved` | id, to_freelancer, to_client | Dispute resolved with overflow-safe split |
| `cancelled` | id, refunded | Escrow cancelled and unreleased funds refunded |
| `reputation_issued` | id, freelancer, rating | Reputation issued for a completed or resolved job |
| `rep_void` | id, freelancer, rating, reason | Fraudulent rating voided by admin |
| `paused` | paused | Circuit breaker toggled |

## Overflow-safe split

The dispute split is computed without multiplying `held * bps` directly, which would overflow for large `escrows. Instead it uses the algebraically equivalent form

```
to_freelancer = (held / D) * bps + ((held % D) * bps) / D
```

where `D = BPS_DENOMINATOR = 10_000`. This equals `floor(held * bps / D)` and conserves value exactly (`to_freelancer + to_client == held`) for any `held` up to `i128::MAX`.

### Reputation Governance & Audit Trail
- Ratings are recorded per escrow as `RatingRecord { freelancer, rating, issued_at, voided }`.
- Protocol admin can void fraudulent or collusive ratings via `void_rating(id, reason)`, which subtracts the rating and 1 contract from the freelancer's reputation and emits `("rep_void", id) -> (freelancer, rating, reason)`.
- Voided escrows remain flagged so they cannot be re-rated (`ReputationAlreadyIssued`). Attempting to void twice returns `EscrowError::RatingAlreadyVoided (19)`.
- Ratings issued before this audit trail upgrade have no `RatingRecord` and return `EscrowError::RatingNotFound (18)`.

## Build and test

```bash
rustup target add wasm32v1-none      # toolchain pinned in rust-toolchain.toml
cargo build --target wasm32v1-none --release
cargo test
```

The wasm is written to `target/wasm32v1-none/release/escrow.wasm`.

## Deploy

```bash
stellar contract deploy --wasm target/wasm32v1-none/release/escrow.wasm \
  --source <key> --network testnet
stellar contract invoke --id <CONTRACT_ID> --source <key> --network testnet \
  -- initialize --admin <ADMIN_ADDRESS>
```

## License

MIT