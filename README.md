# TalentTrust Contracts

## Live

- Website: https://talenttrust-frontend.vercel.app
- Docs: https://talenttrust-frontend.vercel.app/docs
- API: https://talenttrust-backend.vercel.app (docs at [/docs](https://talenttrust-backend.vercel.app/docs))
- Contract (testnet): [`CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCH2IFJBZGBKTXK5W`](https://stellar.expert/explorer/testnet/contract/CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCH2IFJBZGBKTXK5W)

Soroban smart contract for the TalentTrust freelancer escrow protocol on Stellar.
Clients lock payment for a job into escrow, release it milestone by milestone as
work is approved, and either party can raise a dispute that the protocol admin settles.

## Deployments

| Network | Contract | ID | Explorer |
| --- | --- | --- | --- |
| Stellar testnet | escrow | `CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCH2IFJBZGBKTXK5W` | [stellar.expert](https://stellar.expert/explorer/testnet/contract/CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCH2IFJBZGBKTXK5W) |

The contract is initialized with admin `GA7TXEBMEPQCLHIYCWDN7RKAO5IMRD2UV7O7TZXMEW6VZUMEXNR4XBB5`.
Machine-readable details are in [deployments/testnet.json](deployments/testnet.json).

## Contract interface (`contracts/escrow`)

| Function | Auth | Description |
| --- | --- | --- |
| `initialize(admin)` | admin | One-time setup |
| `create_contract(client, freelancer, token, milestone_amounts) -> u64` | client | Create an escrow with up to 20 milestones |
| `deposit_funds(id, amount)` | client | Transfer tokens into escrow (cannot exceed total) |
| `deposit_from(id, funder, amount)` | funder | Transfer tokens from any funder into escrow; records the funder's contribution (max 10 funders) |
| `release_milestone(id, index)` | client | Pay the freelancer for a milestone |
| `open_dispute(id, caller)` | client or freelancer | Freeze the escrow pending resolution (increments `disputes_opened`) |
| `resolve_dispute(id, freelancer_bps)` | admin | Split the held balance (records dispute win/loss outcomes based on 5,000 bps threshold); the client share is refunded pro-rata to funders |
| `cancel_contract(id)` | client | Refund unreleased funds pro-rata to funders and close (increments `cancellations` if funds held) |
| `issue_reputation(id, rating)` | client | Rate a completed or resolved job (1-5), once |
| `set_paused(bool)` / `is_paused()` | admin / - | Circuit breaker for all mutations |
| `get_contract(id)`, `contract_count()`, `get_reputation(addr)`, `get_outcomes(addr)`, `get_admin()`, `version()`, `get_funders(id)` | - | Reads |

Status flow: `Created -> Funded -> Completed`, with `Disputed -> Resolved` and `Cancelled` side exits. Resolved jobs allow reputation rating; dispute outcomes and cancellation metrics are tracked in persistent `Outcomes`.

### Errors

Append-only error codes (never renumbered or reused; frontend and backend map on them):

| Error | Code | Description |
| --- | --- | --- |
| `AlreadyInitialized` | 1 | initialize called twice |
| `NotInitialized` | 2 | contract not initialized |
| `Unauthorized` | 3 | caller not authorized |
| `Paused` | 4 | contract paused |
| `ContractNotFound` | 5 | escrow id does not exist |
| `InvalidParties` | 6 | client equals freelancer |
| `InvalidMilestones` | 7 | milestone count out of range |
| `InvalidAmount` | 8 | non-positive or overflowing amount |
| `OverFunded` | 9 | deposit would exceed total amount |
| `InvalidStatus` | 10 | contract not in a mutable state |
| `MilestoneNotFound` | 11 | milestone index out of range |
| `MilestoneAlreadyReleased` | 12 | milestone already paid |
| `InsufficientFunds` | 13 | not enough held funds for milestone |
| `InvalidSplit` | 14 | dispute bps out of range |
| `InvalidRating` | 15 | rating outside 1..=5 |
| `ReputationAlreadyIssued` | 16 | reputation already issued for this contract |
| `TooManyFunders` | 17 | more than 10 distinct funders |

### Events

| Event | Payload | Description |
| --- | --- | --- |
| `deposit` | `(id, funder, amount)` | tokens deposited by `funder` |
| `created` | `(id, client, freelancer, total)` | contract created |
| `release` | `(id, index, amount)` | milestone released |
| `dispute` | `(id, caller)` | dispute opened |
| `resolved` | `(id, to_freelancer, to_client)` | dispute resolved |
| `cancel` | `(id, refund)` | contract cancelled and refunded |
| `rep` | `(id, freelancer, rating)` | reputation issued |
| `init` | `(admin)` | contract initialized |
| `paused` | `(paused)` | pause state changed |

Refunds (`cancel_contract` and the client share of `resolve_dispute`) are distributed pro-rata to funders by contribution, with the rounding remainder going to the largest funder, so no refund unit is lost.

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
