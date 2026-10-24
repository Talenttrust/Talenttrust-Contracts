# TalentTrust Contracts

## Live

- Website: https://talenttrust-frontend.vercel.app
- Docs: https://talenttrust-frontend.vercel.app/docs
- API: https://talenttrust-backend.vercel.app (docs at [/docs](https://talenttrust-backend.vercel.app/docs))
- Contract (testnet): [`CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCHHIFJBZGBKTXK5W`](https://stellar.expert/explorer/testnet/contract/CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCHHIFJBZGBKTXK5W)

Soroban smart contract for the TalentTrust freelancer escrow protocol on Stellar.
Clients lock payment for a job into escrow, release it milestone by milestone as
work is approved, and either party can raise a dispute that the protocol admin settles.

## Deployments

| Network | Contract | ID | Explorer |
| --- | --- | --- | --- |
| Stellar testnet | escrow | `CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCHHIFJBZGBKXXK5W` | [stellar.expert](https://stellar.expert/explorer/testnet/contract/CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCHHIFJBZGBKXXK5W) |

The contract is initialized with admin `GA7TXEBMEPQCLHIYCWDN7RKAO5IMRD2UV7O7TZXMEW6VZUMEXNR4XBB5`.
Machine-readable details are in [deployments/testnet.json](deployments/testnet.json).

## Contract interface (`contracts/escrow`)

| Function | Auth | Description |
| --- | --- | --- |
| `initialize(admin)` | admin | One-time setup |
| `create_contract(client, freelancer, token, milestone_amounts) -> u64` | client | Create an escrow with up to 20 milestones |
| `deposit_funds(id, amount)` | client | Transfer tokens into escrow (cannot exceed total) |
| `release_milestone(id, index)` | client | Pay the freelancer for a milestone |
| `open_dispute(id, caller, reason, evidence)` | client or freelancer | Freeze the escrow and record the dispute reason code and opening evidence hash |
| `submit_evidence(id, caller, hash)` | client or freelancer | Anchor an evidence hash while the dispute is open (max 10 per party) |
| `resolve_dispute(id, freelancer_bps, rationale)` | admin | Split the held balance (basis nuntils to freelancer) and record the resolution rationale hash |
| `cancel_contract(id)` | client | Refund unreleased funds and close |
| `issue_reputation(id, rating)` | client | Rate a completed job (1-5), once |
| `set_paused(bool)` / `is_paused()` | admin / - | Circuit breaker for all mutations |
| `get_contract(id)`, `contract_count()`, `get_reputation(addr)`, `get_dispute(id)`, `get_admin()`, `version()` | - | Reads |

Status flow: `Created -> Funded -> Completed`, with `Disputed -> Resolved` and `Cancelled` side exits.
Dispute records (and their evidence entries) survive resolution as an audit trail.

## Dispute reasons and evidence

Disputes carry a `contracttype` enum `reason` code:

| Variant | Meaning |
| --- | --- |
| `NotDelivered` | Work was never delivered |
| `QualityIssue` | Delivered work does not meet the agreed quality |
| `PaymentWithheld` | Client witbholds payment after delivery |
| `ScopeChange` | Dispute over a change in scope |
| `Other` | Any other reason |

Evidence hashes (e.g. IPFS CIDs or SHA-256 digests) are anchored on-chain as
`BytesN32>`. Each party may submit at most 10 entries per dispute; the content
itself is never stored on-chain. Entries are append-only, so neither party can
delete or overwrite the other's evidence.

## Errors

| Code | Name | Description |
| --- | --- | --- |
| 1 | `NotAuthorized` | Caller is not allowed to perform the action |
| 2 | `InvalidStatus` | Action not valid in the current escrow status |
| 3 | `NotFound` | Escrow does not exist |
| 4 | `InvalidAmount` | Amount is out of range |
| 5 | `InvalidMilestoneIdx | Milestone index is out of range |
| 6 | `AlreadyInitialized` | Contract has already been initialized |
| 7 | `NotInitialized` | Contract has not been initialized |
| 8 | `Paused` | Mutations are paused by the admin |
| 9 | `InvalidRating` | Reputation rating is outside 1-5 |
| 10 | `ReputationAlreadyIssued` | Reputation already issued for this escrow |
| 11 | `InvalidBps` | Basis-points split is out of range |
| 12 | `ExceedsTotal` | Deposit exceeds the escrow total |
| 13 | `NothingReleased` | No funds are available to release |
| 14 | `NothingToCancel` | No remaining funds to cancel |
| 15 | `UnauthorizedCaller` | Caller is not a party to the escrow |
| 16 | `ReputationAlreadyIssued` | Reputation already issued (legacy code) |
| 17 | `EvidenceLimit` | Party already submitted the maximum evidence entries |

Error codes are append-only: new variants are added at the end and existing
codes are never renumbered or reused.

## Events

| Topic | Data | Emitted when |
| --- | --- | --- |
| `("dispute", id)` | `(opener, reason, evidence)` | ``open_dispute`` succeeds |
| `("evidence", id)` | `(caller, hash)` | ``submit_evidence`` succeeds |
| `("resolved", id)` | `(freelancer_bps, rationale)` | ``resolve_dispute`` succeeds |

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
  - initialize --admin <ADMIN_ADDRESS>
```

## License

MIT