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
| `void_rating(id, reason)` | admin | Void a fraudulent rating, reverse reputation and emit audit trail |
| `set_paused(bool)` / `is_paused()` | admin / - | Circuit breaker for all mutations |
| `extend_escrow_ttl(id)` | anyone | Permissionless keep-alive: extend the escrow (and its rating record) TTL |
| `extend_reputation_ttl(freelancer)` | anyone | Permissionless keep-alive: extend a freelancer's reputation TTL |
| `get_contract(id)`, `get_rating(id)`, `contract_count()`, `get_reputation(addr)`, `get_outcomes(addr)`, `get_admin()`, `version()` | - | Reads (never extend TTL) |

Status flow: `Created -> Funded -> Completed`, with `Disputed -> Resolved` and `Cancelled` side exits. Resolved jobs allow reputation rating; dispute outcomes and cancellation metrics are tracked in persistent `Outcomes`.

## Storage TTL policy

All contract state is persistent and must stay live for the protocol to remain usable.

| Constant | Ledgers | Rough duration | Meaning |
| --- | --: | --- | --- |
| `PERSISTENT_TTL_THRESHOLD` | 17,280 | ~1 day | Remaining TTL below which an entry is eligible for extension |
| `PERSISTENT_TTL_EXTEND` | 518,400 | ~30 days | TTL granted whenever an entry is extended |

- Every state-changing entry point (`initialize`, `set_paused`, `create_contract`, `deposit_funds`, `release_milestone`, `open_dispute`, `resolve_dispute`, `cancel_contract`, `issue_reputation`, `void_rating`, `extend_escrow_ttl`, `extend_reputation_ttl`) extends the contract instance TTL, so the instance holding `Admin`, `Paused` and `NextId` is never left to archive while the contract is in use.
- Writes extend the TTL of the entries they touch. Read-only getters never extend TTL.
- `extend_escrow_ttl(id)` and `extend_reputation_ttl(freelancer)` are permissionless keep-alive calls for dormant entries (for example an escrow that has been `Disputed` for a long time). The caller pays the network fee and the storage rent for the extension; the functions change no state other than TTL and emit no events.
- Missing records: `extend_escrow_ttl` returns `ContractNotFound (5)`; `extend_reputation_ttl` returns `ReputationNotFound (20)`.
- If no bump occurs for ~30 days, the entry is archived and must be restored (paying rent) before it can be read or written again. Calling `extend_*` before that point keeps it reachable indefinitely.

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
