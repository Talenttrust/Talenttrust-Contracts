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
| `open_dispute(id, caller)` | client or freelancer | Freeze the escrow pending resolution |
| `resolve_dispute(id, freelancer_bps)` | admin | Split the held balance (basis points to freelancer); high-value escrows must use the panel |
| `set_panel(members, threshold)` | admin | Replace the M-of-N arbiter panel (1 ≤ threshold ≤ members ≤ 7, no duplicates) |
| `vote_resolution(member, id, freelancer_bps)` | panel member | Vote on a disputed split; a member may change their vote; settles once `threshold` current members agree |
| `cancel_contract(id)` | client | Refund unreleased funds and close |
| `issue_reputation(id, rating)` | client | Rate a completed job (1-5), once |
| `set_paused(bool)` / `is_paused()` | admin / - | Circuit breaker for all mutations |
| `get_contract(id)`, `contract_count()`, `get_reputation(addr)`, `get_admin()`, `get_panel()`, `get_votes(id)`, `version()` | - | Reads |

Status flow: `Created -> Funded -> Completed`, with `Disputed -> Resolved` and `Cancelled` side exits.

Escrows with `total_amount >= PANEL_THRESHOLD_AMOUNT` (10^12 base units) settle only by
panel vote: `resolve_dispute` returns `PanelRequired` and `vote_resolution` executes the
same split once `threshold` current panel members agree, then clears the votes. Only
current panel members' votes count — votes from members removed by a later `set_panel`
are ignored, and parties to the escrow cannot vote.

New errors: `InvalidPanel = 17` (bad panel configuration), `PanelRequired = 18`
(high-value escrow resolved without the panel). New events: `("panel",)` →
`(members, threshold)`; when a vote settles an escrow, `("resolved", id)` plus one
`("vote", id)` → `(member, freelancer_bps)` per counted vote.

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
