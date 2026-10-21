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
| `deposit_funds(id, amount)` | client | Transfer tokens into escrow and credit exactly what the contract received (cannot exceed total; rejects tokens that deliver less than `amount`) |
| `release_milestone(id, index)` | client | Pay the freelancer for a milestone (verifies the escrow balance actually falls by the milestone amount) |
| `open_dispute(id, caller)` | client or freelancer | Freeze the escrow pending resolution (increments `disputes_opened`) |
| `resolve_dispute(id, freelancer_bps)` | admin | Split the held balance (records dispute win/loss outcomes based on 5,000 bps threshold; both payouts are balance-checked) |
| `cancel_contract(id)` | client | Refund unreleased funds and close (increments `cancellations` if funds held; refund is balance-checked) |
| `issue_reputation(id, rating)` | client | Rate a completed or resolved job (1-5), once |
| `set_paused(bool)` / `is_paused()` | admin / - | Circuit breaker for all mutations |
| `get_contract(id)`, `contract_count()`, `get_reputation(addr)`, `get_outcomes(addr)`, `get_admin()`, `version()` | - | Reads |

Status flow: `Created -> Funded -> Completed`, with `Disputed -> Resolved` and `Cancelled` side exits. Resolved jobs allow reputation rating; dispute outcomes and cancellation metrics are tracked in persistent `Outcomes`.

### Balance-delta accounting

Every transfer in which the escrow contract is the sender or the recipient is
verified against the contract's own token balance: the balance must move by
exactly the requested amount. Tokens that take a transfer fee, rebase, or
misreport a successful transfer (delivering less than requested) are rejected
with `TransferAmountMismatch` (error code `18`), which reverts the transaction
so the client loses nothing. This keeps `funded_amount` equal to what the
contract actually holds, so one escrow can never pay out using another escrow's
tokens in the same token contract. Fee-on-transfer tokens are not supported.

## Errors

| Code | Variant | Meaning |
| --- | --- | --- |
| 1 | `AlreadyInitialized` | `initialize` called twice |
| 2 | `NotInitialized` | Contract not initialized |
| 3 | `Unauthorized` | Caller is not permitted for this action |
| 4 | `Paused` | Mutations blocked by the circuit breaker |
| 5 | `ContractNotFound` | No escrow with that id |
| 6 | `InvalidParties` | Client and freelancer are the same address |
| 7 | `InvalidMilestones` | Empty milestone list or more than 20 milestones |
| 8 | `InvalidAmount` | Non-positive amount, or a funding total that overflows |
| 9 | `OverFunded` | Deposit would exceed the escrow total |
| 10 | `InvalidStatus` | Operation not allowed in the escrow's current status |
| 11 | `MilestoneNotFound` | No milestone at that index |
| 12 | `MilestoneAlreadyReleased` | Milestone already paid |
| 13 | `InsufficientFunds` | Not enough unreleased funds for this milestone |
| 14 | `InvalidSplit` | Dispute split above 10,000 bps |
| 15 | `InvalidRating` | Rating outside 1-5 |
| 16 | `ReputationAlreadyIssued` | Reputation already issued for this escrow |
| 17 | `ArithmeticOverflow` | Checked arithmetic overflowed |
| 18 | `TransferAmountMismatch` | Token moved a different amount than requested (fee-on-transfer / rebasing / misreporting token) |

> Error codes are append-only: variants are added at the end and never
> renumbered or reused, because the frontend and backend map on the numbers.

## Events

| Topic | Data | Emitted by |
| --- | --- | --- |
| `init` | `admin` | `initialize` |
| `created` | `(client, freelancer, total)` | `create_contract` |
| `deposit` | `amount` | `deposit_funds` |
| `release` | `(index, amount)` | `release_milestone` |
| `dispute` | `caller` | `open_dispute` |
| `resolved` | `(to_freelancer, to_client)` | `resolve_dispute` |
| `cancel` | `refund` | `cancel_contract` |
| `rep` | `(freelancer, rating)` | `issue_reputation` |
| `paused` | `paused` | `set_paused` |


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
