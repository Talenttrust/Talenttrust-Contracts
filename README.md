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
| `release_milestone(id, index)` | client | Pay the freelancer for a milestone |
| `open_dispute(id, caller)` | client or freelancer | Freeze the escrow pending resolution (increments `disputes_opened`) |
| `resolve_dispute(id, freelancer_bps)` | admin | Split the held balance (records dispute win/loss outcomes based on 5,000 bps threshold) |
| `cancel_contract(id)` | client | Refund unreleased funds and close (increments `cancellations` if funds held) |
| `issue_reputation(id, rating)` | client | Rate a completed or resolved job (1-5), once |
| `set_paused(bool)` | admin | Circuit breaker for all mutations (`true` = pause every category, `false` = clear) |
| `set_pause_flags(flags)` | admin | Pause only the categories given by the flag bitmask (see below) |
| `is_paused()` / `get_pause_flags()` | - | Reads: whether any category is paused, and the current flag bitmask |
| `get_contract(id)`, `contract_count()`, `get_reputation(addr)`, `get_outcomes(addr)`, `get_admin()`, `version()` | - | Reads |

Status flow: `Created -> Funded -> Completed`, with `Disputed -> Resolved` and `Cancelled` side exits. Resolved jobs allow reputation rating; dispute outcomes and cancellation metrics are tracked in persistent `Outcomes`.

### Pause flags

`set_pause_flags` takes a bitmask of operation categories, so an incident freeze
can stop new inflows while users keep their exit paths:

| Flag | Value | Gates |
| --- | --- | --- |
| `PAUSE_CREATE` | 1 | `create_contract` |
| `PAUSE_DEPOSIT` | 2 | `deposit_funds` |
| `PAUSE_RELEASE` | 4 | `release_milestone` |
| `PAUSE_DISPUTE` | 8 | `open_dispute` |
| `PAUSE_CANCEL` | 16 | `cancel_contract` |
| `PAUSE_REPUTATION` | 32 | `issue_reputation` |
| `PAUSE_ALL` | 63 | all of the above |

- Bits outside `PAUSE_ALL` are rejected with `InvalidAmount`.
- `set_paused(true)` sets `PAUSE_ALL` and `set_paused(false)` sets `0`.
- `is_paused()` is `true` when **any** flag is set; `get_pause_flags()` returns the bitmask.
- A blocked call fails with the `Paused` error before any auth, state read or transfer.
- `resolve_dispute` stays available to the admin so disputes can always be settled.
- Deployments created before the flags existed keep working: a stored legacy `Paused == true` is treated as `PAUSE_ALL`.

Events: `("paused", paused)` from `set_paused` and `("pause", flags)` from `set_pause_flags`.

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
