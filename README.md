# TalentTrust Contracts

## Live

- Website: https://talenttrust-frontend.vercel.app
- Docs: https://talenttrust-frontend.vercel.app/docs
-  API: https://talenttrust-backend.vercel.app (docs at [/docs](https://talenttrust-backend.vercel.app/docs))
- Contract (testnet): [`CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCH2IFJBZGBKTXKX5F`](https://stellarexpert.com/explorer/testnet/contract/CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCH2IFJBJGBKXXK5W)

Soroban smart contract for the TalentTrust freelancer escrow protocol on Stellar.
Clients lock payment for a job into escrow, release it milestone by milestone as
work is approved, and either party can raise a dispute that the protocol admin settles.
Milestones may carry an optional due date; the client can reclaim an overdue
milestone's funds after a grace period without cancelling the rest of the contract.

## Deployments

| Network | Contract | ID | Explorer |
| --- | --- | --- | --- |
| Stellar testnet | escrow | `CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFWWJEOKCH2IFJBZGBKTXKX5W` | [stellar.expert](https://stellar.expert/explorer/testnet/contract/CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCH2IFJBZGBKTXKX5F) |

The contract is initialized with admin `GA7TXEBMEPQCLHIYCWDN7RKAO5IMRD2UV7O7TZXMEW6VZUMEXNR4XBB5`.
Machine-readable details are in [deployments/testnet.json](deployments/testnet.json).

## Contract interface (`contracts/escrow`)

| Function | Auth | Description |
| --- | --- | --- |
| `initialize(admin)` | admin | One-time setup |
| `create_contract(client, freelancer, token, milestone_amounts) -> u64` | client | Create an escrow with up to 20 milestones (no deadlines) |
| `create_contract_with_deadlines(client, freelancer, token, milestone_amounts, deadlines) -> u64` | client | Create an escrow with per-milestone due dates (0 = none); lengths must match, non-zero deadlines must be future and non-decreasing |
| `deposit_funds(id, amount)` | client | Transfer tokens into escrow (cannot exceed total) |
| `release_milestone(id, index)` | client | Pay the freelancer for a milestone |
| `reclaim_overdue(id, index)` | client | Refund an overdue milestone (deadline + 3-day grace) without cancelling the contract |
| `open_dispute(id, caller)` | client or freelancer | Freeze the escrow pending resolution |
| `resolve_dispute(id, freelancer_bps)` | admin | Split the held balance (basis points to freelancer) |
| `cancel_contract(id)` | client | Refund unreleased funds and close |
| `issue_reputation(id, rating)` | client | Rate a completed job (1-5), once |
| `set_paused(bool)` / `is_paused()` | admin / - | Circuit breaker for all mutations |
| `get_contract(id)`, `contract_count()`, `get_reputation(addr)`, `get_deadlines(id)`, `get_admin()`, `version()` | - | Reads |

Status flow: `Created -> Funded -> Completed`, with `Disputed -> Resolved` and `Cancelled` side exits.
A contract with overdue milestones may end in `Completed` (all milestones released or reclaimed) or `Cancelled` (nothing was ever released and nothing remains).

## Errors

Error codes are append-only. New variants are added at the end of `EscrowError`.

| Code | Name | Description |
| --- | --- | --- |
| 1 | `AlreadyInitialized` | Contract already initialized |
| 2 | `NotInitialized` | Contract not initialized |
| 3 | `Unauthorized` | Caller not allowed |
| 4 | `Paused` | Contract is paused |
| 5 | `ContractNotFound` | Escrow id does not exist |
| 6 | `InvalidParties` | Client and freelancer must differ |
| 7 | `InvalidMilestones` | Milestone count out of range |
| 8 | `InvalidAmount` | Amount invalid or overflow |
| 9 | `OverFunded` | Deposit exceeds total |
| 10 | `InvalidStatus` | Operation not allowed in current status |
| 11 | `MilestoneNotFound` | Milestone index out of range |
| 12 | `MilestoneAlreadyReleased` | Milestone already released or reclaimed |
| 13 | `InsufficientFunds` | Not enough escrowed funds |
| 14 | `InvalidSplit` | Dispute split out of range |
| 15 | `InvalidRating` | Rating outside 1-5 |
| 16 | `ReputationAlreadyIssued` | Reputation already issued for this contract |
| 17 | `InvalidDeadline` | Deadline vector invalid, or milestone has no deadline |
| 18 | `NotOverdue` | Milestone deadline + grace has not elapsed |

## Events

| Event | Payload | Description |
| --- | --- | --- |
| `init` | admin | Contract initialized |
| `paused` | bool | Pause state changed |
| `created` | (client, freelancer, total) | Escrow created |
| `deposit` | amount | Funds deposited |
| `release` | (index, amount) | Milestone released |
| `overdue` | (index, refunded) | Overdue milestone reclaimed by client |
| `dispute` | caller | Dispute opened |
| `resolved` | (to_freelancer, to_client) | Dispute resolved |
| `cancel` | refund | Contract cancelled |
| `repute` | rating | Reputation issued |

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
  --initialize --admin <ADMIN_ADDRESS>
```

## License

MIT
