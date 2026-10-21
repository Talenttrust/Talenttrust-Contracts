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
| `propose_admin(new_admin)` | admin | Nominate a successor admin (cannot be the current admin); overwrites any pending nomination. Works while paused |
| `accept_admin()` | pending admin | Accept a pending nomination, moving the admin role to the caller. Works while paused |
| `cancel_admin_proposal()` | admin | Cancel a pending nomination. Works while paused |
| `set_paused(bool)` / `is_paused()` | admin / - | Circuit breaker for all mutations |
| `get_contract(id)`, `contract_count()`, `get_reputation(addr)`, `get_outcomes(addr)`, `get_admin()`, `get_pending_admin()`, `version()` | - | Reads |

### Admin rotation

The admin controls `set_paused` and is the sole judge in `resolve_dispute`. To let
the admin key be rotated without redeploying (which would abandon every live
escrow), control moves in two steps: the current admin calls `propose_admin` to
nominate a successor, and the nominee must call `accept_admin` to take control. A
pending nomination can be replaced by another `propose_admin`, or cleared with
`cancel_admin_proposal`. All three functions are intentionally callable while the
contract is paused, since rotation is a recovery action.

Events: `adm_prop` `(current, new_admin)`, `adm_acc` `(old, new)`, `adm_cncl`
`(cancelled_address)`.

Status flow: `Created -> Funded -> Completed`, with `Disputed -> Resolved` and `Cancelled` side exits. Resolved jobs allow reputation rating; dispute outcomes and cancellation metrics are tracked in persistent `Outcomes`.

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
| 18 | `InvalidAdmin` | `propose_admin` nominated the current admin |
| 19 | `NoPendingAdmin` | No pending admin nomination to accept or cancel |

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
| `adm_prop` | `(current, new_admin)` | `propose_admin` |
| `adm_acc` | `(old, new)` | `accept_admin` |
| `adm_cncl` | `cancelled_address` | `cancel_admin_proposal` |


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
