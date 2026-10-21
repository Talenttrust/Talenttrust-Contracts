# TalentTrust Contracts

## Live

- Website: https://talenttrust-frontend.vercel.app
- Docs: https://talenttrust-frontend.vercel.app/docs
-  API: https://talenttrust-backend.vercel.app (docs at [/docs](https://talenttrust-backend.vercel.app/docs))
- Contract (testnet): [`CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCH2IFJBZGBKTXKX5I`](https://stellarexpert.com/explorer/testnet/contract/CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCH2IFJBJGBKXXK5Y)

Soroban smart contract for the TalentTrust freelancer escrow protocol on Stellar.
Clients lock payment for a job into escrow, release it milestone by milestone as
work is approved, and either party can raise a dispute that the protocol admin settles.

## Deployments

| Network | Contract | ID | Explorer |
| --- | --- | --- | --- |
| Stellar testnet | escrow | `CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCH2IFJBZGBKTXKX5I` | [stellarexpert](https://stellarexpert.com/explorer/testnet/contract/CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCH2IFJBZGBKTXKX5I) |

The contract is initialized with admin `GA7TXEBMEPQCLHIYCWDN7RKAO5IMRD2UV7O7TZXMEW6VZUMEXNR4XBB5`.
Machine-readable details are in [deployments/testnet.json](deployments/testnet.json).

The admin is now set atomically at deploy time via the contract constructor, so there is no
separate initialization step and no front-running window.

## Contract interface (`contracts/escrow`)

| Function | Auth | Description |
| --- | --- | --- |
| `__constructor(admin)` | deployer | Atomic deploy-time setup; sets the admin and initial state |
| `initialize(admin)` | - | Deprecated; always returns `AlreadyInitialized` on new deployments |
| `create_contract(client, freelancer, token, milestone_amounts) -> u64` | client | Create an escrow with up to 20 milestones |
| `deposit_funds(id, amount)` | client | Transfer tokens into escrow (cannot exceed total) |
| `release_milestone(id, index)` | client | Pay the freelancer for a milestone |
| `open_dispute(id, caller)` | client or freelancer | Freeze the escrow pending resolution (increments `disputes_opened`) |
| `resolve_dispute(id, freelancer_bps)` | admin | Split the held balance (records dispute win/loss outcomes based on 5,000 bps threshold) |
| `cancel_contract(id)` | client | Refund unreleased funds and close (increments `cancellations` if funds held) |
| `issue_reputation(id, rating)` | client | Rate a completed or resolved job (1-5), once |
| `set_paused(bool)` / `is_paused()` | admin / - | Circuit breaker for all mutations |
| `get_contract(id)`, `contract_count()`, `get_reputation(addr)`, `get_outcomes(addr)`, `get_admin()`, `version()` | - | Reads |

Status flow: `Created -> Funded -> Completed`, with `Disputed -> Resolved` and `Cancelled` side exits. Resolved jobs allow reputation rating; dispute outcomes and cancellation metrics are tracked in persistent `Outcomes`.

Events:

| Event | Topic | Payload |
| --- | --- | --- |
| `init` | `init` | `admin: Address` |
| `paused` | `paused` | `paused: bool` |
| `created` | `created`, id | `(client, freelancer, total)` |
| `deposit` | `deposit`, id | `amount` |
| `release` | `release`, id | `(index, amount)` |
| `dispute` | `dispute`, id | `caller` |
| `resolved` | `resolved`, id | `(to_freelancer, to_client)` |
| `cancel` | `cancel`, id | `refund` |
| `repute` | `repute`, id | `rating` |

Errors:

| Code | Name | Description |
| --- | --- | --- |
| 1 | `AlreadyInitialized` | Constructor already ran; `initialize` cannot be used |
| 2 | `NotInitialized` | Admin not set (legacy instances) |
| 3 | `Unauthorized` | Caller is not a party to the escrow |
| 4 | `Paused` | Contract is paused |
| 5 | `ContractNotFound` | Unknown escrow id |
| 6 | `InvalidParties` | Client and freelancer are the same address |
| 7 | `InvalidMilestones` | Zero or too many milestones |
| 8 | `InvalidAmount` | Non-positive or overflowing amount |
| 9 | `OverFunded` | Deposit exceeds total amount |
| 10 | `InvalidStatus` | Operation not allowed in current status |
| 11 | `MilestoneNotFound` | Index out of range |
| 12 | `MilestoneAlreadyReleased` | Milestone already paid |
| 13 | `InsufficientFunds` | Not enough funded to release |
| 14 | `InvalidSplit` | Dispute split exceeds 10000 bps |
| 15 | `InvalidRating` | Rating outside 1-5 |
| 16 | `ReputationAlreadyIssued` | Rating already recorded |

## Build and test

```bash
rustup target add wasm32v1-none      # toolchain pinned in rust-toolchain.toml
cargo build --target wasm32v1-none --release
cargo test
```

The wasm is written to `target/wasm32v1-none/release/escrow.wasm`.

## Deploy

The admin is passed to the constructor in the same transaction as the deploy, so there is
no window for a third party to set themselves as admin.

```bash
stellar contract deploy --wasm target/wasm32v1-none/release/escrow.wasm \
  --source <key> --network testnet -- --admin <ADMIN_ADDRESS>
```

## License

MIT