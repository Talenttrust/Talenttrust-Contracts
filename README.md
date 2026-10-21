# TalentTrust Contracts

## Live

- Website: https://talenttrust-frontend.vercel.app
- Docs: https://talenttrust-frontend.vercel.app/docs
-  API: https://talenttrust-backend.vercel.app (docs at [/docs](https://talenttrust-backend.vercel.app/docs))
- Contract (testnet): [`CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCHIIFJBZGBKTXKX5F`](https://stellarexpert.com/explorer/testnet/contract/CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCHIIFJBBGBKXXKX5F)

Soroban smart contract for the TalentTrust freelancer escrow protocol on Stellar.
Clients lock payment for a job into escrow, release it milestone by milestone as
work is approved, and either party can raise a dispute that the protocol admin settles.

## Deployments

| Network | Contract | ID | Explorer |
| --- | --- | --- | --- |
| Stellar testnet | escrow | `CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCHIIFJBBGBKXXKX5F` | [stellar.expert](https://stellar.expert/explorer/testnet/contract/CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCHIIFJBZGBKTXKX5F) |

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
| `cancel_contract(id)` | client | Instant refund for `Created` escrows; requires notice for `Funded` |
| `request_cancel(id)` | client | Start a 3-day cancellation notice on a `Funded` escrow |
| `finalize_cancel(id)` | client | Refund unreleased funds after the notice period elapses |
| `withdraw_cancel(id)` | client | Withdraw a pending cancellation request |
| `issue_reputation(id, rating)` | client | Rate a completed or resolved job (1-5), once |
| `set_paused(bool)` / `is_paused()` | admin / - | Circuit breaker for all mutations |
| `get_contract(id)`, `contract_count()`, `get_reputation(addr)`, `get_outcomes(addr)`, `get_admin()`, `version()` | - | Reads |

Status flow: `Created -> Funded -> Completed`, with `Disputed -> Resolved` and `Cancelled` side exits.
Cancellation of a `Funded` escrow goes through `request_cancel` -> 3-day notice -> `finalize_cancel`; the freelancer may dispute during the notice period (which clears the request), and the client may still release milestones or `withdraw_cancel`. Resolved jobs allow reputation rating; dispute outcomes and cancellation metrics are tracked in persistent `Outcomes`.

## Errors

Error codes are append-only; new variants are added at the end and existing codes are never renumbered.

| Code | Name | Meaning |
| --- | --- | --- |
| 17 | `CancelAlreadyRequested` | A cancellation request is already pending for this escrow |
| 18 | `NoticePeriodActive` | `_finalize_cancel` called before the notice period elapsed |
| 19 | `NoticeRequired` | `cancel_contract` called on a `Funded` escrow; use `request_cancel` instead |

## Events

| Topic | Data | Emitted by |
| --- | --- | --- |
| `("cncl_req", id)` | effective_at (timestamp after which cancel can finalize) | `request_cancel` |
| `("cncl_wd", id)` | - | `withdraw_cancel` |

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