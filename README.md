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
| `expire_unfunded(id)` | - | Permissionlessly cancel expired unfunded escrow and refund client |
| `issue_reputation(id, rating)` | client | Rate a completed or resolved job (1-5), once |
| `rate_client(id, rating)` | freelancer | Rate a completed or resolved job (1-5), once |
| `void_rating(id, reason)` | admin | Void a fraudulent rating, reverse reputation and emit audit trail |
| `set_paused(bool)` / `is_paused()` | admin / - | Circuit breaker for all mutations |
| `get_contract(id)`, `get_rating(id)`, `contract_count()`, `get_reputation(addr)`, `get_client_reputation(addr)`, `get_fund_by(id)`, `get_outcomes(addr)`, `get_admin()`, `version()` | - | Reads |

Status flow: `Created -> Funded -> Completed`, with `Disputed -> Resolved` and `Cancelled` side exits (an eligible `Created` escrow can transition to `Cancelled` after its funding deadline via `expire_unfunded`). Resolved jobs allow reputation rating; dispute outcomes and cancellation metrics are tracked in persistent `Outcomes`.

### Funding Deadlines & Expiration

- **Funding deadline**: Escrows are created with a 14-day funding window (`FUNDING_WINDOW_SECS = 14 * 86_400`). The stored deadline is readable via `get_fund_by(id)`.
- **Deposit restriction**: When an escrow is in `Created` status, deposits are rejected with `FundingExpired` if `now > fund_by`.
- **Permissionless expiration**: Any caller may invoke `expire_unfunded(id)` once `now > fund_by` if the escrow is in `Created` status with `released_amount == 0`. The entire `funded_amount` is refunded exclusively to the original client, and the escrow transitions to `Cancelled`.
- **Event**: Emits event topics `("expired", id)` with the refunded amount as the event data.
- **Errors**: `FundingExpired` when depositing past the deadline for a `Created` escrow; `NotExpired` when calling `expire_unfunded` before `now > fund_by`; `InvalidStatus` if the escrow is not in `Created` status or has `released_amount > 0`.

### Reputation & Ratings

- **Freelancer reputation**: Clients rate freelancers via `issue_reputation(id, rating)` after `Completed` or `Resolved`. Emits event topics `("rep", id)` with data `(freelancer, rating)`.
- **Client reputation**: Freelancers rate clients via `rate_client(id, rating)` after `Completed` or `Resolved`. Emits event topics `("rep_cli", id)` with data `(client, rating)`.
- **Errors**: Ratings must be in the `1..=5` range (`InvalidRating`), can only be submitted once per escrow (`ReputationAlreadyIssued`), require appropriate status (`InvalidStatus`), and respect the circuit breaker (`Paused`).

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
