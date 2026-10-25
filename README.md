# TalentTrust Contracts

## Live

- Website: https://talenttrust-frontend.vercel.app
- Docs: https://talenttrust-frontend.vercel.app/docs
- API: https://talenttrust-backend.vercel.app (docs at [/docs](https://talenttrust-backend.vercel.app/docs))
- Contract (testnet): [`CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCHHIFJBZGBKTXK5W`](https://stellar.expert/explorer/testnet/contract/CCZMIVUBMG5SBC47SDTC32RWLBJ2HW2DCFVWJEOKCHHIFJBZGBKTXK5W)

Soroban smart contract for the TalentTrust freelancer escrow protocol on Stellar.
Clients lock payment for a job into escrow, release it milestone by milestone as
work is approved, and either party can raise a dispute that the protocol admin settles.
Clients can also pay a tip or bonus that is transferred directly to the freelancer and
recorded for reputation and analytics.

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
| `create_contract(client, freelancer, token, milestone_amounts) -> u64` | client | Create an escrow with up to 20 milestones. Rejects self-referential parties (client, freelancer or token equal to the contract address), the admin as client or freelancer, milestones below `MIN_MILESTONE_AMOUNT` (1 000 000), and totals above `MAX_ESCROW_TOTAL` (10¹⁸). Validation order: parties → milestones → amounts. |
| `create_and_fund(client, freelancer, token, milestone_amounts) -> u64` | client | Validate, create and fully fund an escrow atomically (one signature, one escrow write; same validation as `create_contract`); emits `created` then `deposit` with the two-step payloads |
| `create_contract_sequential(client, freelancer, token, milestone_amounts) -> u64` | client | Same as `create_contract`, but milestones must be released strictly in order (`OutOfOrder` (`28`) otherwise); emits `("seq", id) -> true` |
| `create_stream(client, freelancer, token, amount, start, end) -> u64` | client | One-milestone escrow vesting linearly from `start` to `end` (`end - start >= 1 hour`, else `InvalidSchedule` (`50`)); funded with `deposit_funds`; milestone releases and `cancel_contract` return `InvalidStatus` |
| `withdraw_vested(id) -> i128` | freelancer | Withdraw vested-but-unwithdrawn pay from a `Funded` stream; completes the escrow when fully vested. Emits `("stream_wd", id) -> amount` |
| `stop_stream(id)` | client | Stop the stream: vested-but-unwithdrawn to the freelancer, unvested remainder refunded, escrow `Cancelled`. Emits `("stream_st", id) -> (to_freelancer, to_client)` |
| `vested(id)`, `get_stream(id)` | - | Vested amount (`amount * (min(now, stop_or_end) - start) / (end - start)`, overflow-safe) and the `Stream { start, end, withdrawn, stopped_at }` schedule |
| `deposit_funds(id, amount)` | client | Transfer tokens into escrow (status must be `Created`, cannot exceed total) |
| `deposit_from(id, funder, amount)` | funder | Any address funds the escrow (same checks as `deposit_funds`); contributions are tracked per funder (max 10, else `TooManyFunders` (`34`)) and refunds from `cancel_contract` / the client share of a dispute are split pro-rata among funders, rounding remainder to the largest funder. `deposit` event payload is `(funder, amount)` |
| `set_team_split(id, payees)` | freelancer | Configure 1-5 fixed-bps payee shares (sum 10,000) while `Created` and before any deposit (`InvalidSplitConfig` (`27`) otherwise); emits `("team", id) -> len` |
| `release_milestone(id, index)` | client | Pay the milestone to the team split if set, otherwise the freelancer (or their payout address) |
| `set_payout_address(id, payout)` | freelancer | Route payouts to a separate address; `Created`/`Funded` only, not during dispute |
| `create_contract_with_review(client, freelancer, token, milestone_amounts, review_window_secs) -> u64` | client | `create_contract` with a custom review window (1 hour to 30 days, else `InvalidReviewWindow` (`41`)); other create calls use the 7-day default |
| `submit_milestone(id, index)` | freelancer | Mark a `Pending` milestone of a `Funded` escrow as delivered (its unpaid amount must be covered by the uncommitted balance); emits `("submit", id) -> (index, submitted_at)` |
| `claim_milestone(id, index)` | freelancer | Auto-release a submitted milestone once `now >= submitted_at + review_window` (`ReviewWindowActive` (`43`) before, `MilestoneNotSubmitted` (`42`) if not submitted); not while `Disputed`; also after a cancel. Emits `("claim", id) -> (index, amount)` and a `release` event |
| `get_milestone_status(id, index)`, `get_submitted_at(id, index)`, `get_review_window(id)` | - | `MilestoneStatus { Pending, Submitted, Released }`, submission time and review window |
| `release_partial(id, index, amount)` | client | Pay part of a milestone (`InvalidAmount` if `amount <= 0` or above the remaining amount, `InsufficientFunds` if not enough is held); when fully paid the milestone is released. `release_milestone` / `release_milestones` pay only the remaining amount. Emits `("rel_part", id) -> (index, amount, remaining)` |
| `release_milestones(id, indices) -> i128` | client | Release several milestones in one call (all-or-nothing, one payout, one save); emits a `release` event per milestone plus `("rel_batch", id) -> (count, total)`; `DuplicateIndex` (`29`) on repeated indices |
| `pay_bonus(id, amount)` | client | Transfer a tip/bonus directly client → freelancer (Funded or Completed escrows) |
| `open_dispute(id, caller, reason, evidence)` | client or freelancer | Freeze the escrow and record the dispute reason code and opening evidence hash; posts a refundable bond of `max(DISPUTE_BOND_MIN, total * DISPUTE_BOND_BPS / 10_000)` (`DISPUTE_BOND_BPS = 200`, `DISPUTE_BOND_MIN = 0`) and increments `disputes_opened` |
| `submit_evidence(id, caller, hash)` | client or freelancer | Anchor an evidence hash while the dispute is open (max 10 per party, else `EvidenceLimit` (`17`) — append-only) |
| `resolve_dispute(caller, id, freelancer_bps, bond_to_opener)` | admin or Resolver | Split the held balance; the freelancer share follows the team split (records dispute win/loss outcomes based on 5,000 bps threshold); the resolver cannot be the escrow's client or freelancer; the dispute bond is never part of the split — it returns to the opener when `bond_to_opener`, otherwise goes to the counterparty (panel settlements return it to the opener only if the split favours them). Records the resolution rationale hash |
| `set_panel(members, threshold)` | admin | Replace the arbiter panel (1 <= threshold <= len <= 7, unique members, else `InvalidPanel` (`31`)); emits `("panel",) -> (len, threshold)` |
| `vote_resolution(member, id, freelancer_bps)` | panel member | Vote a split on a disputed escrow; re-votes replace earlier ones, parties cannot vote, only current members are tallied. When `threshold` members agree the escrow settles like `resolve_dispute` and one `("vote", id) -> (member, bps)` event per counted vote is emitted |
| `propose_amendment(id, proposer, new_amounts) -> u32` | client or freelancer | Propose new amounts for the existing milestones (same count, released milestones unchanged else `MilestoneAlreadyReleased` (`12`)); returns the nonce. Emits `("amend_p", id) -> (proposer, nonce)` |
| `accept_amendment(id, acceptor, nonce)` | the other party | Apply it (`SettlementMismatch` (`48`) for a missing/stale nonce): excess funding is refunded to the client, a larger total returns the escrow to `Created` until topped up. Emits `("amended", id) -> (old_total, new_total, refunded)` |
| `get_amendment(id)` | - | Pending `Amendment { proposer, new_amounts, nonce }` |
| `propose_change_order(id, amounts)` | client | Propose extra milestones for a `Created`/`Funded`/`Completed` escrow (`existing + new <= 20`, else `InvalidMilestones`) |
| `accept_change_order(id) -> (u32, i128)` | freelancer | Append the proposed milestones, grow `total_amount` and reopen the escrow as `Created` (needs funding; a fresh funding window starts); emits `("chg_ord", id) -> (added_count, added_total)`. A rating already issued stays issued, so `issue_reputation` remains blocked for that escrow |
| `reject_change_order(id)` / `get_change_order(id)` | freelancer / - | Drop / read the pending proposal (`ChangeOrderNotFound` (`37`) if none) |
| `set_terms(id, terms_hash, milestone_hashes)` | client | Commit a 32-byte terms hash and 0 or one spec hash per milestone (`InvalidMetadata` (`38`) otherwise); only while `Created` with nothing deposited (frozen afterwards, `InvalidStatus`); emits `("terms", id) -> terms_hash` |
| `deliver(id, index, delivery_hash)` | freelancer | Record/overwrite a milestone's delivery hash until it is released; emits `("deliver", id) -> (index, delivery_hash)` |
| `expire_unfunded(id)` | anyone | After the 14-day funding window (`FUNDING_WINDOW_SECS`), cancel an escrow still `Created` with nothing released and refund any partial deposit to its funders; emits `("expired", id) -> refund`. `NotExpired` (`36`) before the deadline |
| `cancel_contract(id)` | client | Refund unreleased funds and close (increments `cancellations` if funds held); funds backing submitted milestones stay in escrow for the freelancer to claim |
| `propose_settlement(id, proposer, freelancer_amount)` / `withdraw_settlement(id, proposer)` | client or freelancer | Propose (or withdraw) a split of the held balance (`0..=funded-released`, else `InvalidAmount`) on a `Created`/`Funded`/`Disputed` escrow; any later deposit or release invalidates it. Emits `("settle_p", id) -> (proposer, freelancer_amount)` |
| `accept_settlement(id, acceptor, freelancer_amount)` | the other party | Accept by echoing the amount (`SettlementMismatch` (`48`) on self-accept, missing proposal or a different amount); pays both sides, returns any dispute bond and closes the escrow as `Resolved` without the admin. Emits `("settled", id) -> (to_freelancer, to_client)` |
| `get_settlement(id)` | - | Pending `SettlementProposal { proposer, freelancer_amount, created_at }` |
| `resign(id)` | freelancer | Resign from a `Created`/`Funded` escrow: unreleased funds return to the client (released milestones stay paid), escrow `Cancelled`; emits `("resign", id) -> refunded` |
| `was_resigned(id)` | - | Whether the escrow ended by freelancer resignation (vs client cancellation) |
| `issue_reputation(id, rating)` | client | Rate a completed or resolved job (1-5), once |
| `rate_client(id, rating)` | freelancer | Rate the client of a completed or resolved job (1-5), once; emits `("rep_cli", id) -> (client, rating)` |
| `void_rating(id, reason)` | admin | Void a fraudulent rating, reverse reputation and emit audit trail |
| `sweep_surplus(token, to) -> i128` | admin | Sweep untracked token surplus above aggregate escrow liabilities (`NoSurplus` (`21`) if none) |
| `freeze_escrow(id, reason)` / `unfreeze_escrow(id)` / `is_frozen(id)` | admin / admin / - | Freeze a single escrow: party mutations return `EscrowFrozen` (`22`); `resolve_dispute` still works |
| `compact_escrow(id)` | anyone | Replace a terminal escrow with a compact `EscrowReceipt` (emits `("compact", id)`). `Cancelled` any time; `Completed`/`Resolved` only after the rating or `THIRTY_DAYS_IN_LEDGERS` after closing, otherwise `NotCompactable` (`25`) |
| `schedule_upgrade(wasm_hash)` / `cancel_upgrade()` / `execute_upgrade()` | admin | Time-locked WASM upgrade: scheduling stores `PendingUpgrade { wasm_hash, eta = now + UPGRADE_DELAY_SECS (48h) }` (re-scheduling restarts the delay); `execute_upgrade` fails with `UpgradeNotReady` (`40`) before `eta` and `NoPendingUpgrade` (`39`) if nothing is scheduled, then installs the code and bumps `version()`. Events: `("upg_sched",) -> (wasm_hash, eta)`, `("upg_cncl",) -> wasm_hash`, `("upgraded",) -> (wasm_hash, new_version)` |
| `get_pending_upgrade()`, `version()` | - | Pending upgrade (if any) and on-chain code version (`1` until the first upgrade) |
| `propose_admin(new_admin)` / `accept_admin()` / `cancel_admin_proposal()` | admin / nominee / admin | Two-step admin rotation (works while paused): propose (`InvalidAdmin` (`45`) for the current admin; overwrites a pending nomination), the nominee accepts, or the admin cancels (`NoPendingAdmin` (`46`) if none). Events: `("adm_prop",) -> (current, new)`, `("adm_acc",) -> (old, new)`, `("adm_cncl",) -> cancelled` |
| `get_pending_admin()` | - | Pending admin nominee, if any |
| `emergency_withdraw(id, caller)` | client or freelancer | After a continuous pause of `EMERGENCY_EXIT_DELAY_SECS` (30 days) unwind a `Created`/`Funded`/`Disputed` escrow: unreleased funds return to the client/funders (submitted milestones go to the freelancer); `Disputed` splits 50/50 with the remainder to the client and returns the dispute bond; ends `Cancelled`. `EmergencyExitNotAvailable` (`47`) before the delay or while unpaused. Emits `("emerg", id) -> (to_client, to_freelancer)`. Re-pausing restarts the clock (the admin can delay, but not block, an exit only by keeping the contract unpaused) |
| `get_paused_at()` | - | Timestamp at which the current pause began |
| `set_paused(caller, paused)` / `is_paused()` | admin or Pauser / - | Circuit breaker for all mutations |
| `set_fee(fee_bps, treasury)` | admin | Protocol fee on freelancer-side payouts (releases, claims, the freelancer share of disputes/settlements), capped at `MAX_FEE_BPS = 500` (`FeeTooHigh` (`49`) above); each escrow snapshots the rate at creation, so later changes never apply to it. Refunds are fee-free. Emits `("fee_set",) -> (fee_bps, treasury)`; `ReleaseEvent.fee` reports the fee per release |
| `get_fee_config()`, `get_escrow_fee(id)` | - | `(fee_bps, Option<treasury>)` and an escrow's snapshotted rate |
| `set_pause_flags(caller, flags)` / `get_pause_flags()` | admin or Pauser / - | Granular pause bitmask: `PAUSE_CREATE = 1` (create/configure), `PAUSE_DEPOSIT = 2`, `PAUSE_RELEASE = 4` (releases, claims, bonuses, deliveries), `PAUSE_DISPUTE = 8`, `PAUSE_CANCEL = 16` (cancel, resign, settlement, expiry), `PAUSE_REPUTATION = 32` (ratings), `PAUSE_ALL = 63`; other bits return `InvalidAmount`. `set_paused(true)` sets `PAUSE_ALL`, `false` clears; `is_paused()` is true iff any flag is set. Emits `("pause",) -> flags` |
| `grant_role(role, account)` / `revoke_role(role, account)` | admin | Grant or revoke a `Pauser` or `Resolver` role (`RoleUnchanged` (`33`) if no change); emits `("role_set", role) -> (account, true\|false)` |
| `has_role(role, account)` | - | Role membership; the admin implicitly holds every role |
| `get_contract(id)`, `get_rating(id)`, `contract_count()`, `get_reputation(addr)`, `get_outcomes(addr)`, `get_admin()`, `version()` | - | Reads |
| `get_bonus(id)`, `get_bonus_total(freelancer)` | - | Reads for cumulative bonus amounts |
| `get_milestone_paid(id, index) -> i128` | - | Amount paid so far on a milestone (its full amount once released) |
| `next_releasable(id) -> Option<u32>` | - | Lowest unreleased milestone index (`None` when all released) |
| `migrate_escrows(ids) -> u32` | anyone | Eagerly move up to 50 legacy `Contract(id)` entries to the versioned `ContractV(id)` envelope (`InvalidAmount` above 50); already-migrated/unknown ids are skipped; returns the number migrated. Legacy entries are otherwise migrated lazily by their first mutation; reads never write |
| `get_client_escrows(client, start, limit)`, `get_freelancer_escrows(freelancer, start, limit)` | - | Escrow ids per party in creation order; `limit` clamped to 50 (`ESCROW_INDEX_PAGE_SIZE`), zero limit or out-of-range start returns empty. Stored in pages of at most 50 ids; escrows created before this index are not backfilled |
| `count_client_escrows(client)`, `count_freelancer_escrows(freelancer)` | - | Number of indexed escrows per party |
| `get_milestone(id, index)` | - | Single milestone; `MilestoneNotFound` when out of range |
| `get_escrow_summary(id)` | - | Status and derived balances (`held`, `remaining_to_fund`, milestone counts, next unreleased index, whether the held balance covers it) |
| `get_escrow_summaries(ids)` | - | Up to 25 summaries; unknown ids skipped, larger batches return `InvalidAmount` |
| `get_team_split(id)` | - | Configured team split (empty if unset) |
| `get_reputation_score(addr) -> Option<u32>` | - | Value-weighted average rating ×100 (e.g. `437` = 4.37), rounded half up; `None` without eligible ratings |
| `get_weighted_reputation(addr)` | - | `WeightedReputation { weighted_rating_sum, weight_total, rated_contracts }` (weight = released amount) |
| `get_client_reputation(addr)` | - | Client reputation accumulated from freelancer ratings |
| `get_terms(id)`, `get_delivery(id, index)` | - | Committed terms/spec hashes and recorded delivery hashes |
| `get_fund_by(id) -> u64` | - | Funding deadline (ledger timestamp; `0` if none) |
| `get_dispute_bond(id) -> Option<(Address, i128)>` | - | Opener and amount of the bond held for a disputed escrow |
| `get_funders(id)` | - | Map of funder -> contribution |
| `get_panel()`, `get_votes(id)` | - | Current arbiter panel and the votes on a disputed escrow (cleared after settlement) |
| `get_receipt(id) -> Option<EscrowReceipt>` | - | Receipt of a compacted escrow; `get_contract` on a compacted id returns `EscrowCompacted` (`26`) |
| `get_payout_address(id)` | - | Payout address for an escrow (falls back to the freelancer) |
| `get_liability(token)` | - | Aggregate amount held for live escrows in `token` |
| `get_dispute(id)` | - | Dispute record including reason code and evidence entries (retained after resolution as an audit trail) |

## Dispute reasons and evidence

Disputes carry a `contracttype` enum `reason` code:

| Variant | Meaning |
| --- | --- |
| `NotDelivered` | Work was never delivered |
| `QualityIssue` | Delivered work does not meet the agreed quality |
| `PaymentWithheld` | Client withholds payment after delivery |
| `ScopeChange` | Dispute over a change in scope |
| `Other` | Any other reason |

Evidence hashes (e.g. IPFS CIDs or SHA-256 digests) are anchored on-chain as
`BytesN<32>`. Each party may submit at most 10 entries per dispute; the content
itself is never stored on-chain. Entries are append-only, so neither party can
delete or overwrite the other's evidence.

### Status flow

Status transitions are centrally validated by a guarded state machine:
- `Created -> Funded`
- `Created -> Completed` (when milestones are released directly from Created)
- `Funded -> Completed`
- `Created | Funded -> Disputed`
- `Disputed -> Resolved`
- `Created | Funded -> Cancelled`
- `Funded | Completed -> Created` only through `accept_change_order` (a dedicated, separately guarded reopen), and `Funded -> Created` through `accept_amendment` when the new total exceeds the funding

Any other status transition returns `EscrowError::InvalidTransition` (`20`). Attempting to `deposit_funds` on an already `Funded` contract returns `EscrowError::InvalidStatus` (`1`) up front.

Resolved jobs allow reputation rating; dispute outcomes and cancellation metrics are tracked in persistent `Outcomes`.

Bonuses are allowed while an escrow is `Funded` or `Completed`; they do not change the escrow status or accounting.

### Storage TTL policy

Persistent entries are extended to `PERSISTENT_TTL_EXTEND` (~30 days of ledgers) whenever they are written and their TTL is below `PERSISTENT_TTL_THRESHOLD` (~1 day). Every mutating entry point also bumps the contract instance (admin, counters, flags), so normal activity on any escrow keeps the instance alive. Reads never extend TTLs. Anyone can pay to keep quiet state alive:

| Function | Auth | Description |
| --- | --- | --- |
| `extend_escrow_ttl(id)` | anyone | Extend an escrow entry (or its receipt) and all of its existing side entries; `ContractNotFound` if missing |
| `extend_reputation_ttl(addr)` | anyone | Extend an address's reputation, weighted reputation, client reputation, outcomes and bonus totals |

### Arithmetic

All amount, id and counter arithmetic is checked and fails with `ArithmeticOverflow` (`17`) instead of panicking. Basis-point splits use the overflow-free `mul_bps(amount, bps) = (amount / D) * bps + ((amount % D) * bps) / D` (`D = 10_000`), which equals `floor(amount * bps / D)` for any `amount`.

### Token transfers

Every transfer into or out of the contract (deposits, releases, refunds, dispute splits and bonds, sweeps) is balance-delta checked: the contract's balance must change by exactly the requested amount, otherwise the call fails with `TransferAmountMismatch` and fully reverts. Fee-on-transfer, rebasing and no-op tokens are therefore rejected rather than silently under-collateralising other escrows of the same token.

### Events

Core lifecycle events are versioned and structured: escrow events use topics
`(EVENT_SCHEMA_V = 1, symbol, id)` and global events `(1, symbol)`, with one
`#[contracttype]` payload struct per event so indexers need not call `get_contract`.

| Event | Topic | Payload | Emitted when |
| --- | --- | --- | --- |
| init | `(1, "init")` | `admin: Address` | `initialize` |
| paused | `(1, "paused")` | `bool` | `set_paused` |
| created | `(1, "created", id)` | `CreatedEvent { client, freelancer, token, total, milestone_count }` | `create_contract`, `create_contract_sequential`, `create_and_fund` |
| deposit | `(1, "deposit", id)` | `DepositEvent { from, amount, funded_amount, status }` | `deposit_funds`, `deposit_from`, `create_and_fund` |
| release | `(1, "release", id)` | `ReleaseEvent { index, amount, to, released_amount, status, fee }` | `release_milestone`, `release_milestones` (one per milestone) |
| complete | `(1, "complete", id)` | `CompletedEvent { released_amount }` | exactly once, when the last milestone is released (single, batch or partial release) |
| dispute | `(1, "dispute", id)` | `DisputeEvent { by, status, bond }` | `open_dispute` |
| resolved | `(1, "resolved", id)` | `ResolvedEvent { to_freelancer, to_client, status, bond_recipient, bond }` | `resolve_dispute`, panel settlement |
| cancel | `(1, "cancel", id)` | `CancelEvent { refund, to, status }` (`to` is the client; refunds go pro-rata to funders) | `cancel_contract` |
| rep | `(1, "rep", id)` | `ReputationEvent { freelancer, rating, completed_contracts, total_rating, weight }` | `issue_reputation` |

Feature-specific events keep their `(symbol, id)` topics, e.g.:

| Event | Topic | Data |
| --- | --- | --- |
| Bonus paid | `("bonus", id)` | `(amount, cumulative)` |
| Partial release | `("rel_part", id)` | `(index, amount, remaining)` |
| Batch release | `("rel_batch", id)` | `(count, total)` |
| Panel vote recorded | `("voted", id)` | `(member, freelancer_bps)` |
| Dispute opened | `("dispute", id)` | `(opener, reason, evidence)` |
| Evidence submitted | `("evidence", id)` | `(caller, hash)` |
| Dispute resolved | `("resolved", id)` | `(freelancer_bps, rationale)` |

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
| 17 | `ArithmeticOverflow` | checked counter / ledger arithmetic; `EvidenceLimit` on the 11th evidence entry per party |
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
| 37 | `ChangeOrderNotFound` | `accept_change_order` / `reject_change_order` without a pending proposal |
| 38 | `InvalidMetadata` | `set_terms` — milestone hash count is neither 0 nor the milestone count |
| 39 | `NoPendingUpgrade` | `cancel_upgrade` / `execute_upgrade` with nothing scheduled |
| 40 | `UpgradeNotReady` | `execute_upgrade` before the scheduled `eta` |
| 41 | `InvalidReviewWindow` | `create_contract_with_review` outside 1 hour..30 days |
| 42 | `MilestoneNotSubmitted` | `claim_milestone` on a milestone that was not submitted |
| 43 | `ReviewWindowActive` | `claim_milestone` before the review window elapsed |
| 44 | `TransferAmountMismatch` | any token transfer into/out of the contract that does not move the contract balance by exactly the requested amount (fee-on-transfer, rebasing or no-op tokens are rejected) |
| 45 | `InvalidAdmin` | `propose_admin` with the current admin |
| 46 | `NoPendingAdmin` | `accept_admin` / `cancel_admin_proposal` without a nomination |
| 47 | `EmergencyExitNotAvailable` | `emergency_withdraw` while unpaused or before the pause delay elapsed |
| 48 | `SettlementMismatch` | `accept_settlement` self-accept, without a proposal, or with a different amount; `accept_amendment` without a pending amendment or with a stale nonce |
| 49 | `FeeTooHigh` | `set_fee` above `MAX_FEE_BPS` |
| 50 | `InvalidSchedule` | `create_stream` with `start >= end` or a duration under 1 hour |

Constants: `MIN_MILESTONE_AMOUNT = 1_000_000` (0.1 token at 7 decimals), `MIN_REPUTATION_VALUE = 10_000_000` (1 token at 7 decimals; ratings are weighted by the released amount and dust jobs cannot be rated — sybil resistance comes from this value threshold, not counterparty graph analysis, so clients who are also freelancers of the same counterparty are not blocked), `MAX_ESCROW_TOTAL = 10^18` stroops, `MAX_MILESTONES = 20`.

Error codes are **append-only** and never renumbered — the frontend and backend map directly on them. The Soroban contract spec allows at most 50 error cases, which `EscrowError` has reached; newer failure modes reuse the closest existing code (e.g. amendments use `MilestoneAlreadyReleased` and `SettlementMismatch`).

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