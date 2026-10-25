# TalentTrust Contracts

## Live

- Website: https://talenttrust-frontend.vercel.app
- Docs: https://talenttrust-frontend.vercel.app/docs
- API: https://talenttrust-backend.vercel.app (docs at [/docs](https://talenttrust-backend.vercel.app/docs))
- Contract (testnet): [`CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCH2IFJBZGBKTXK5W`](https://stellar.expert/explorer/testnet/contract/CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCH2IFJBZGBKTXK5W)

Soroban smart contract for the TalentTrust freelancer escrow protocol on Stellar.
Clients lock payment for a job into escrow, release it milestone by milestone as
work is approved, and either party can raise a dispute that the protocol admin settles.
Clients can also pay a tip or bonus that is transferred directly to the freelancer and
recorded for reputation and analytics.

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
| `create_contract(client, freelancer, token, milestone_amounts) -> u64` | client | Create an escrow with up to 20 milestones. Rejects self-referential parties (client, freelancer or token equal to the contract address), the admin as client or freelancer, milestones below `MIN_MILESTONE_AMOUNT` (1 000 000), and totals above `MAX_ESCROW_TOTAL` (10¹⁸). Validation order: parties → milestones → amounts. |
| `create_contract_sequential(client, freelancer, token, milestone_amounts) -> u64` | client | Same as `create_contract`, but milestones must be released strictly in order (`OutOfOrder` (`28`) otherwise); emits `("seq", id) -> true` |
| `deposit_funds(id, amount)` | client | Transfer tokens into escrow (status must be `Created`, cannot exceed total) |
| `deposit_from(id, funder, amount)` | funder | Any address funds the escrow (same checks as `deposit_funds`); contributions are tracked per funder (max 10, else `TooManyFunders` (`34`)) and refunds from `cancel_contract` / the client share of a dispute are split pro-rata among funders, rounding remainder to the largest funder. `deposit` event payload is `(funder, amount)` |
| `set_team_split(id, payees)` | freelancer | Configure 1-5 fixed-bps payee shares (sum 10,000) while `Created` and before any deposit (`InvalidSplitConfig` (`27`) otherwise); emits `("team", id) -> len` |
| `release_milestone(id, index)` | client | Pay the milestone to the team split if set, otherwise the freelancer (or their payout address) |
| `set_payout_address(id, payout)` | freelancer | Route payouts to a separate address; `Created`/`Funded` only, not during dispute |
| `release_milestones(id, indices) -> i128` | client | Release several milestones in one call (all-or-nothing, one payout, one save); emits a `release` event per milestone plus `("rel_batch", id) -> (count, total)`; `DuplicateIndex` (`29`) on repeated indices |
| `pay_bonus(id, amount)` | client | Transfer a tip/bonus directly client → freelancer (Funded or Completed escrows) |
| `open_dispute(id, caller)` | client or freelancer | Freeze the escrow pending resolution (increments `disputes_opened`) |
| `resolve_dispute(caller, id, freelancer_bps)` | admin or Resolver | Split the held balance; the freelancer share follows the team split (records dispute win/loss outcomes based on 5,000 bps threshold); the resolver cannot be the escrow's client or freelancer |
| `set_panel(members, threshold)` | admin | Replace the arbiter panel (1 <= threshold <= len <= 7, unique members, else `InvalidPanel` (`31`)); emits `("panel",) -> (len, threshold)` |
| `vote_resolution(member, id, freelancer_bps)` | panel member | Vote a split on a disputed escrow; re-votes replace earlier ones, parties cannot vote, only current members are tallied. When `threshold` members agree the escrow settles like `resolve_dispute` and one `("vote", id) -> (member, bps)` event per counted vote is emitted |
| `expire_unfunded(id)` | anyone | After the 14-day funding window (`FUNDING_WINDOW_SECS`), cancel an escrow still `Created` with nothing released and refund any partial deposit to its funders; emits `("expired", id) -> refund`. `NotExpired` (`36`) before the deadline |
| `cancel_contract(id)` | client | Refund unreleased funds and close (increments `cancellations` if funds held) |
| `issue_reputation(id, rating)` | client | Rate a completed or resolved job (1-5), once |
| `rate_client(id, rating)` | freelancer | Rate the client of a completed or resolved job (1-5), once; emits `("rep_cli", id) -> (client, rating)` |
| `void_rating(id, reason)` | admin | Void a fraudulent rating, reverse reputation and emit audit trail |
| `sweep_surplus(token, to) -> i128` | admin | Sweep untracked token surplus above aggregate escrow liabilities (`NoSurplus` (`21`) if none) |
| `freeze_escrow(id, reason)` / `unfreeze_escrow(id)` / `is_frozen(id)` | admin / admin / - | Freeze a single escrow: party mutations return `EscrowFrozen` (`22`); `resolve_dispute` still works |
| `compact_escrow(id)` | anyone | Replace a terminal escrow with a compact `EscrowReceipt` (emits `("compact", id)`). `Cancelled` any time; `Completed`/`Resolved` only after the rating or `THIRTY_DAYS_IN_LEDGERS` after closing, otherwise `NotCompactable` (`25`) |
| `set_paused(caller, paused)` / `is_paused()` | admin or Pauser / - | Circuit breaker for all mutations |
| `grant_role(role, account)` / `revoke_role(role, account)` | admin | Grant or revoke a `Pauser` or `Resolver` role (`RoleUnchanged` (`33`) if no change); emits `("role_set", role) -> (account, true\|false)` |
| `has_role(role, account)` | - | Role membership; the admin implicitly holds every role |
| `get_contract(id)`, `get_rating(id)`, `contract_count()`, `get_reputation(addr)`, `get_outcomes(addr)`, `get_admin()`, `version()` | - | Reads |
| `get_bonus(id)`, `get_bonus_total(freelancer)` | - | Reads for cumulative bonus amounts |
| `next_releasable(id) -> Option<u32>` | - | Lowest unreleased milestone index (`None` when all released) |
| `get_milestone(id, index)` | - | Single milestone; `MilestoneNotFound` when out of range |
| `get_escrow_summary(id)` | - | Status and derived balances (`held`, `remaining_to_fund`, milestone counts, next unreleased index, whether the held balance covers it) |
| `get_escrow_summaries(ids)` | - | Up to 25 summaries; unknown ids skipped, larger batches return `InvalidAmount` |
| `get_team_split(id)` | - | Configured team split (empty if unset) |
| `get_reputation_score(addr) -> Option<u32>` | - | Value-weighted average rating ×100 (e.g. `437` = 4.37), rounded half up; `None` without eligible ratings |
| `get_weighted_reputation(addr)` | - | `WeightedReputation { weighted_rating_sum, weight_total, rated_contracts }` (weight = released amount) |
| `get_client_reputation(addr)` | - | Client reputation accumulated from freelancer ratings |
| `get_fund_by(id) -> u64` | - | Funding deadline (ledger timestamp; `0` if none) |
| `get_funders(id)` | - | Map of funder -> contribution |
| `get_panel()`, `get_votes(id)` | - | Current arbiter panel and the votes on a disputed escrow (cleared after settlement) |
| `get_receipt(id) -> Option<EscrowReceipt>` | - | Receipt of a compacted escrow; `get_contract` on a compacted id returns `EscrowCompacted` (`26`) |
| `get_payout_address(id)` | - | Payout address for an escrow (falls back to the freelancer) |
| `get_liability(token)` | - | Aggregate amount held for live escrows in `token` |

### Status flow

Status transitions are centrally validated by a guarded state machine:
- `Created -> Funded`
- `Created -> Completed` (when milestones are released directly from Created)
- `Funded -> Completed`
- `Created | Funded -> Disputed`
- `Disputed -> Resolved`
- `Created | Funded -> Cancelled`

Any other status transition returns `EscrowError::InvalidTransition` (`20`). Attempting to `deposit_funds` on an already `Funded` contract returns `EscrowError::InvalidStatus` (`1`) up front.

Resolved jobs allow reputation rating; dispute outcomes and cancellation metrics are tracked in persistent `Outcomes`.

Bonuses are allowed while an escrow is `Funded` or `Completed`; they do not change the escrow status or accounting.

### Events

| Event | Topic | Data |
| --- | --- | --- |
| Bonus paid | `("bonus", id)` | `(amount, cumulative)` |

Bonus payments reuse the escrow's existing validation errors: a missing escrow `id`, an escrow in a disallowed state (`Disputed`, `Cancelled`, `Resolved`), a non-positive `amount`, an unauthorized caller (client auth required), and the pause gate all return the corresponding contract error.

### Reputation Governance & Audit Trail
- Ratings are recorded per escrow as `RatingRecord { freelancer, rating, issued_at, voided }`.
- Protocol admin can void fraudulent or collusive ratings via `void_rating(id, reason)`, which subtracts the rating and 1 contract from the freelancer's reputation and emits `("rep_void", id) -> (freelancer, rating, reason)`.
- Voided escrows remain flagged so they cannot be re-rated (`ReputationAlreadyIssued`). Attempting to void twice returns `EscrowError::RatingAlreadyVoided (19)`.
- Ratings issued before this audit trail upgrade have no `RatingRecord` and return `EscrowError::RatingNotFound (18)`.

## Error catalog

| Code | Variant | Raised by |
| ---: | --- | --- |
| 1 | `AlreadyInitialized` | `initialize` |
| 2 | `NotInitialized` | any call before `initialize` |
| 3 | `Unauthorized` | `open_dispute` when caller is neither party |
| 4 | `Paused` | any mutating function while paused |
| 5 | `ContractNotFound` | any function with an unknown escrow id |
| 6 | `InvalidParties` | `create_contract` — client, freelancer or token equals the contract address, or client == freelancer; `set_payout_address` |
| 7 | `InvalidMilestones` | `create_contract` — zero milestones or more than 20 |
| 8 | `InvalidAmount` | non-positive amounts; `create_contract` total > `MAX_ESCROW_TOTAL` |
| 9 | `OverFunded` | `deposit_funds` — would exceed the escrow total |
| 10 | `InvalidStatus` | operation not allowed in the current status |
| 11 | `MilestoneNotFound` | `release_milestone` |
| 12 | `MilestoneAlreadyReleased` | `release_milestone` |
| 13 | `InsufficientFunds` | `release_milestone` — not enough deposited yet |
| 14 | `InvalidSplit` | `resolve_dispute` — basis points > 10 000 |
| 15 | `InvalidRating` | `issue_reputation` — rating outside 1–5 |
| 16 | `ReputationAlreadyIssued` | `issue_reputation` — already rated this contract |
| 17 | `ArithmeticOverflow` | checked counter / ledger arithmetic |
| 18 | `RatingNotFound` | `void_rating` |
| 19 | `RatingAlreadyVoided` | `void_rating` |
| 20 | `InvalidTransition` | guarded status machine |
| 21 | `NoSurplus` | `sweep_surplus` |
| 22 | `EscrowFrozen` | party mutations on a frozen escrow; `freeze_escrow` twice |
| 23 | `AdminCannotBeParty` | `create_contract` — client or freelancer is the admin address |
| 24 | `MilestoneTooSmall` | `create_contract` — milestone amount < `MIN_MILESTONE_AMOUNT` (1 000 000) |
| 25 | `NotCompactable` | `compact_escrow` — escrow live, already compacted, or unrated within the grace period |
| 26 | `EscrowCompacted` | `get_contract` / `issue_reputation` on a compacted escrow |
| 27 | `InvalidSplitConfig` | `set_team_split` — bad length, zero/duplicate share, client/contract payee, or sum != 10 000 |
| 28 | `OutOfOrder` | `release_milestone` / `release_milestones` on a sequential escrow before all earlier milestones are released |
| 29 | `DuplicateIndex` | `release_milestones` with a repeated index |
| 30 | `BelowReputationThreshold` | `issue_reputation` when `released_amount < MIN_REPUTATION_VALUE` |
| 31 | `InvalidPanel` | `set_panel` — bad threshold/size or duplicate members |
| 32 | `PanelRequired` | `resolve_dispute` on an escrow with `total_amount >= PANEL_THRESHOLD_AMOUNT` (10^12) — settle via `vote_resolution` |
| 33 | `RoleUnchanged` | `grant_role` on an existing member / `revoke_role` on a non-member |
| 34 | `TooManyFunders` | `deposit_funds` / `deposit_from` by an 11th distinct funder |
| 35 | `FundingExpired` | deposit into a `Created` escrow after its funding deadline |
| 36 | `NotExpired` | `expire_unfunded` at or before the funding deadline |

Constants: `MIN_MILESTONE_AMOUNT = 1_000_000` (0.1 token at 7 decimals), `MIN_REPUTATION_VALUE = 10_000_000` (1 token at 7 decimals; ratings are weighted by the released amount and dust jobs cannot be rated — sybil resistance comes from this value threshold, not counterparty graph analysis, so clients who are also freelancers of the same counterparty are not blocked), `MAX_ESCROW_TOTAL = 10^18` stroops, `MAX_MILESTONES = 20`.

Error codes are **append-only** and never renumbered — the frontend and backend map directly on them.

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
