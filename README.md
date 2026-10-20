# TalentTrust Contracts

## Live

- Website: https://talenttrust-frontend.vercel.app
- Docs: https://talenttrust-frontend.vercel.app/docs
-  API: https://talenttrust-backend.vercel.app (docs at [/docs](https://talenttrust-backend.vercel.app/docs))
- Contract (testnet): [`CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCH2IFJBZGBKTXM5W`](https://stellar.expert/explorer/testnet/contract/CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCH2IFJBZGBKTXM5W)

Soroban smart contract for the TalentTrust freelancer escrow protocol on Stellar.
Clients lock payment for a job into escrow, release it milestone by milestone as
work is approved, and either party can raise a dispute that the protocol admin settles.
Milestones may optionally carry a due date; once a due date plus a three-day grace
period has passed, the client can reclaim that milestone's funds without cancelling the
rest of the contract.

## Deployments

| Network | Contract | ID | Explorer |
| --- | --- | --- | --- |
| Stellar testnet | escrow | `CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCH2IFJBZGBKTXM5W` | [stellar.expert](https://stellar.expert/explorer/testnet/contract/CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCH2IFJBZGBKTXM5W) |

The contract is initialized with admin `GA7TXEBMEPQCLHIYCWDN7RKAO5IMRD2UV7O7TZXMEW6VZUMEXNR4XBB5`.
Machine-readable details are in [deployments/testnet.json](deployments/testnet.json).

## Contract interface (`contracts/escrow`)

| Function | Auth | Description |
| --- | --- | --- |
| `initialize(admin)` | admin | One-time setup |
| `create_contract(client, freelancer, token, milestone_amounts) -> u64` | client | Create an escrow with up to 20 milestones (no deadlines) |
| `create_contract_with_deadlines(client, freelancer, token, milestone_amounts, deadlines) -> u64` | client | Create an escrow with per-milestone due dates (0 means no deadline) |
| `deposit_funds(id, amount)` | client | Transfer tokens into escrow (cannot exceed total) |
| `release_milestone(id, index)` | client | Pay the freelancer for a milestone |
| `reclaim_overdue(id, index)` | client | Refund an overdue milestone after the grace period without cancelling the contract |
| `open_dispute(id, caller)` | client or freelancer | Freeze the escrow pending resolution |
| `resolve_dispute(id, freelancer_bps)` | admin | Split the held balance (basis points to freelancer) |
| `cancel_contract(id)` | client | Refund unreleased funds and close |
| `issue_reputation(id, rating)` | client | Rate a completed job (1-5), once |
| `set_paused(bool)` / `is_paused()` | admin / - | Circuit breaker for all mutations |
| `get_contract(id)`, `contract_count()`, `get_reputation(addr)`, `get_deadlines(id)`, `get_admin()`, `version()` | - | Reads |

Status flow: `Created -> Funded -> Completed`, with `Disputed -> Resolved` and `Cancelled` side exits.
A contract whose milestones are all reclaimed as overdue (and none were ever released) ends
in `Cancelled`; if some were released and the rest reclaimed, it ends in `Completed`.

## Errors

Error codes are append-only; new variants are added at the end and existing codes are
never renumbered. The new codes for deadlines are:

| Code | Name | Meaning |
| --- | --- | --- |
| 17 | `InvalidDeadline` | Deadline vector length mismatch, past timestamp, or decreasing deadlines |
| 18 | `NotOverdue` | Milestone has no deadline or the grace period has not elapsed |

## Events

| Event | Payload | Description |
| --- | --- | --- |
| `created` | `(client, freelancer, total)` | Contract created |
| `deposit` | `amount` | Funds deposited |
| `release` | `(index, amount)` | Milestone paid to freelancer |
| `overdue` | `(index, refunded)` | Overdue milestone reclaimed by client |
| `dispute` | `caller` | Dispute opened |
| `resolved` | `(to_freelancer, to_client)` | Dispute resolved |
| `cancel` | `refund` | Contract cancelled |
| `repute` | `(freelancer, rating)` | Reputation issued |

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
