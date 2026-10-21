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
| `create_contract(client, freelancer, token, milestone_amounts) -> u64` | client | Create an escrow with up to 20 milestones. Rejects self-referential parties (client, freelancer or token equal to the contract address), admin as client or freelancer, milestones below `MIN_MILESTONE_AMOUNT` (1 000 000), and totals above `MAX_ESCROW_TOTAL` (10¹⁸). Validation order: parties → milestones → amounts. |
| `deposit_funds(id, amount)` | client | Transfer tokens into escrow (cannot exceed total) |
| `release_milestone(id, index)` | client | Pay the freelancer for a milestone |
| `open_dispute(id, caller)` | client or freelancer | Freeze the escrow pending resolution |
| `resolve_dispute(id, freelancer_bps)` | admin | Split the held balance (basis points to freelancer) |
| `cancel_contract(id)` | client | Refund unreleased funds and close |
| `issue_reputation(id, rating)` | client | Rate a completed job (1-5), once |
| `set_paused(bool)` / `is_paused()` | admin / - | Circuit breaker for all mutations |
| `get_contract(id)`, `contract_count()`, `get_reputation(addr)`, `get_admin()`, `version()` | - | Reads |

Status flow: `Created -> Funded -> Completed`, with `Disputed -> Resolved` and `Cancelled` side exits.

## Error catalog

| Code | Variant | Raised by |
| ---: | --- | --- |
| 1 | `AlreadyInitialized` | `initialize` |
| 2 | `NotInitialized` | any call before `initialize` |
| 3 | `Unauthorized` | `open_dispute` when caller is neither party |
| 4 | `Paused` | any mutating function while paused |
| 5 | `ContractNotFound` | any function with an unknown escrow id |
| 6 | `InvalidParties` | `create_contract` — client, freelancer, or token equals the contract address; or client == freelancer |
| 7 | `InvalidMilestones` | `create_contract` — zero milestones or more than 20 |
| 8 | `InvalidAmount` | `create_contract` — non-positive milestone amount or total > `MAX_ESCROW_TOTAL` (10¹⁸); also `deposit_funds` |
| 9 | `OverFunded` | `deposit_funds` — would exceed the escrow total |
| 10 | `InvalidStatus` | status machine violation |
| 11 | `MilestoneNotFound` | `release_milestone` |
| 12 | `MilestoneAlreadyReleased` | `release_milestone` |
| 13 | `InsufficientFunds` | `release_milestone` — not enough deposited yet |
| 14 | `InvalidSplit` | `resolve_dispute` — basis points > 10 000 |
| 15 | `InvalidRating` | `issue_reputation` — rating outside 1–5 |
| 16 | `ReputationAlreadyIssued` | `issue_reputation` — already rated this contract |
| 17 | `AdminCannotBeParty` | `create_contract` — client or freelancer is the admin address |
| 18 | `MilestoneTooSmall` | `create_contract` — milestone amount < `MIN_MILESTONE_AMOUNT` (1 000 000) |

Constants: `MIN_MILESTONE_AMOUNT = 1_000_000` (0.1 token at 7 decimals), `MAX_ESCROW_TOTAL = 10^18` stroops, `MAX_MILESTONES = 20`.

Error codes are **append-only** and never renumbered — the frontend and backend map directly on them.

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
