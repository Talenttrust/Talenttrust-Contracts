# TalentTrust Contracts

## Live

- Website: https://talenttrust-frontend.vercel.app
- Docs: https://talenttrust-frontend.vercel.app/docs
-  API: https://talenttrust-backend.vercel.app (docs at [/docs](https://talenttrust-backend.vercel.app/docs))
- Contract (testnet): [`CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCHHIFJBZGBKTXKXW5`](https://stellarexpert.io/explorer/testnet/contract/CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCHHIFJBZGBKTXKXW5)

Soroban smart contract for the TalentTrust freelancer escrow protocol on Stellar.
Clients lock payment for a job into escrow, release it milestone by milestone as
work is approved, and either party can raise a dispute that the protocol admin settles.
A freelancer can also resign from an escrow, returning all unreleased funds to the client.

## Deployments

| Network | Contract | ID | Explorer |
| --- | --- | --- | --- |
| Stellar testnet | escrow | `CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCHHIFJBZGBKXXKW5` | [stellarexpert](https://stellarexpert.io/explorer/testnet/contract/CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCHHIFJBZGBKTXKXW5) |

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
| `resolve_dispute(id, freelancer_bps)` | admin | Split the held balance (records dispute win/loss outcomes based on 5,000 bps threshold) |
| `cancel_contract(id)` | client | Refund unreleased funds and close (increments `cancellations` if funds held) |
| `resign(id)` | freelancer | Resign from a `Created` or `Funded` escrow, refunding unreleased funds to the client and closing as `Cancelled`; records `Resigned(id)` |
| `was_resigned(id) -> bool` | - | Returns true if the escrow was closed by freelancer resignation |
| `issue_reputation(id, rating)` | client | Rate a completed or resolved job (1-5), once |
| `void_rating(id, reason)` | admin | Void a fraudulent rating, reverse reputation and emit audit trail |
| `set_paused(bool)` / `is_paused()` | admin / - | Circuit breaker for all mutations |
| `get_contract(id)`, `get_rating(id)`, `contract_count()`, `get_reputation(addr)`, `get_outcomes(addr)`, `get_admin()`, `version()` | - | Reads |

Status flow: `Created -> Funded -> Completed`, with `Disputed -> Resolved` and `Cancelled` side exits. A freelancer resignation is a `Cancelled` exit from `Created` or `Funded` that refunds all unreleased funds to the client and is flagged via `Resigned(id)`. Resolved jobs allow reputation rating; dispute outcomes and cancellation metrics are tracked in persistent `Outcomes`.

## Events

| Event | Payload | Description |
| --- | --- | --- |
| `cancell` | `id` | Escrow cancelled by the client |
| `resign` | `id, refunded` | Escrow closed by freelander resignation; `refunded` is the amount returned to the client |

## Errors

Errors are returned as typed contract errors. Resignation reuses the existing status and authorization errors:

| Error | Description |
| --- | --- |
| `Unauthorized` | Caller is not the escrow freelancer (or otherwise not allowed) |
| `InvalidStatus` | Escrow is not in `Created` or `Funded` (e.g. already `Disputed`, `Resolved`, `Completed`, or `Cancelled`) |
| `Paused` | Mutations are disabled by the admin circuit breaker |

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
