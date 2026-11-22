# Events authorization and access rules

Source of truth: `contracts/escrow/src/lib.rs`. `Lnnnn` links to that file at line `nnnn`. Structured events are emitted by the `events` module and use topic prefix `EVENT_SCHEMA_V = 1`; inline events publish directly from entrypoints/helpers and have no prefix.

Payloads are intentionally not duplicated here. See [indexer-schema.md](escrow/indexer-schema.md#event-topics-and-payloads) for every topic's emitted data.

## Reading the tables

- **Signer** is the exact `require_auth` subject. `stored Admin` means `require_admin` reads `DataKey::Admin` and calls `require_auth` on it. `none` means the entrypoint has no `require_auth`.
- `init` — stored `Admin` exists, else `NotInitialized`.
- `NP(x)` — `require_not_paused(PAUSE_x)`, else `Paused`. `L(id)` — `load(id)` succeeds, else `ContractNotFound`. `NF(id)` — `require_not_frozen(id)`, else `EscrowFrozen`. `S(...)` — current status must be one of the listed statuses, else `InvalidStatus`.
- `CREATE` — `init`; `NP(CREATE)`; `build_escrow` checks parties are distinct and neither the contract nor the admin, 1..=20 milestones, each amount >= `MIN_MILESTONE_AMOUNT`, total <= `MAX_ESCROW_TOTAL`.
- `DEPOSIT` — `NP(DEPOSIT)`; `L(id)`; `NF(id)`; `S(Created)`; deadline not passed; amount > 0; no overfund; <= 10 funders; exact token transfer.
- `REL` — `NP(RELEASE)`; `L(id)`; `NF(id)`; not a stream escrow.

There is no finalization gate in the current contract. Terminal/compacted checks are listed per row.

## Structured events (`events` module)

| Event topic | Entrypoint (source) | Signer | Preconditions |
|---|---|---|---|
| `(1, "init")` | `initialize` ([L1624](../contracts/escrow/src/lib.rs#L1624) → [L1633](../contracts/escrow/src/lib.rs#L1633)) | `admin` argument | No stored `Admin` yet, else `AlreadyInitialized`. |
| `(1, "paused")` | `set_paused` ([L1712](../contracts/escrow/src/lib.rs#L1712) → [L1721](../contracts/escrow/src/lib.rs#L1721)) | `caller` argument | `holds_role(Pauser, caller)` (admin or granted member); `true` sets all pause bits, `false` clears them. |
| `(1, "created", id)` | `create_contract` ([L1985](../contracts/escrow/src/lib.rs#L1985) → [L1008](../contracts/escrow/src/lib.rs#L1008)) | `client` argument | `CREATE`. |
| `(1, "created", id)` | `create_contract_with_review` ([L1998](../contracts/escrow/src/lib.rs#L1998) → [L1008](../contracts/escrow/src/lib.rs#L1008)) | `client` argument | `CREATE`; review window 1 hour..30 days. |
| `(1, "created", id)` | `create_and_fund` ([L2023](../contracts/escrow/src/lib.rs#L2023) → [L1008](../contracts/escrow/src/lib.rs#L1008)) | `client` argument | `CREATE`; `NP(DEPOSIT)`; exact full-total transfer; status becomes `Funded`. |
| `(1, "created", id)` | `create_contract_sequential` ([L2035](../contracts/escrow/src/lib.rs#L2035) → [L1008](../contracts/escrow/src/lib.rs#L1008)) | `client` argument | `CREATE`; sequential flag stored after creation. |
| `(1, "created", id)` | `create_stream` ([L3628](../contracts/escrow/src/lib.rs#L3628) → [L1008](../contracts/escrow/src/lib.rs#L1008)) | `client` argument | `CREATE`; `start < end` and duration >= 1 hour. |
| `(1, "deposit", id)` | `deposit_funds` ([L2121](../contracts/escrow/src/lib.rs#L2121) → [L1519](../contracts/escrow/src/lib.rs#L1519)) | stored client (`c.client` in `apply_deposit`) | `DEPOSIT`. |
| `(1, "deposit", id)` | `deposit_from` ([L2132](../contracts/escrow/src/lib.rs#L2132) → [L1519](../contracts/escrow/src/lib.rs#L1519)) | `funder` argument | `DEPOSIT`. |
| `(1, "deposit", id)` | `create_and_fund` ([L2023](../contracts/escrow/src/lib.rs#L2023) → [L1010](../contracts/escrow/src/lib.rs#L1010)) | `client` argument | `CREATE`; `NP(DEPOSIT)`; exact full-total transfer; status becomes `Funded`. |
| `(1, "release", id)` | `claim_milestone` ([L2194](../contracts/escrow/src/lib.rs#L2194) → [L2249](../contracts/escrow/src/lib.rs#L2249)) | stored freelancer (`c.freelancer`) | `REL`; `S(Funded\|Cancelled)`; milestone exists, unreleased, submitted; review window elapsed; funds available. |
| `(1, "release", id)` | `release_milestone` ([L2285](../contracts/escrow/src/lib.rs#L2285) → [L2334](../contracts/escrow/src/lib.rs#L2334)) | stored client (`c.client`) | `REL`; `S(Created\|Funded)`; milestone exists/unreleased; sequential order if enabled; funds available. |
| `(1, "release", id)` | `release_milestones` ([L2427](../contracts/escrow/src/lib.rs#L2427) → [L2530](../contracts/escrow/src/lib.rs#L2530)) | stored client (`c.client`) | `REL`; `S(Created\|Funded)`; 1..=20 unique, existing, unreleased indices; sequential order if enabled; funds available. |
| `(1, "complete", id)` | `claim_milestone` ([L2194](../contracts/escrow/src/lib.rs#L2194) → [L2251](../contracts/escrow/src/lib.rs#L2251)) | stored freelancer (`c.freelancer`) | Same as its `(1, "release", id)`, plus all milestones released while status was `Funded`. |
| `(1, "complete", id)` | `release_milestone` ([L2285](../contracts/escrow/src/lib.rs#L2285) → [L2336](../contracts/escrow/src/lib.rs#L2336)) | stored client (`c.client`) | Same as its `(1, "release", id)`, plus all milestones released. |
| `(1, "complete", id)` | `release_partial` ([L2345](../contracts/escrow/src/lib.rs#L2345) → [L2406](../contracts/escrow/src/lib.rs#L2406)) | stored client (`c.client`) | `REL`; `S(Created\|Funded)`; unreleased index; `0 < amount <= remaining`; funds available; final part leaves all milestones released. |
| `(1, "complete", id)` | `release_milestones` ([L2427](../contracts/escrow/src/lib.rs#L2427) → [L2535](../contracts/escrow/src/lib.rs#L2535)) | stored client (`c.client`) | Same as its `(1, "release", id)`, plus all milestones released. |
| `(1, "complete", id)` | `withdraw_vested` ([L3678](../contracts/escrow/src/lib.rs#L3678) → [L3717](../contracts/escrow/src/lib.rs#L3717)) | stored freelancer (`c.freelancer`) | `NP(RELEASE)`; `L(id)`; `NF(id)`; stream exists; `S(Funded)`; not stopped; vested amount > withdrawn and reaches total. |
| `(1, "dispute", id)` | `open_dispute` ([L2608](../contracts/escrow/src/lib.rs#L2608) → [L2653](../contracts/escrow/src/lib.rs#L2653)) | `caller` argument | `NP(DISPUTE)`; `L(id)`; `NF(id)`; caller is client or freelancer; `S(Created\|Funded)`; dispute bond transferred. |
| `(1, "resolved", id)` | `resolve_dispute` ([L2670](../contracts/escrow/src/lib.rs#L2670) → [L1452](../contracts/escrow/src/lib.rs#L1452)) | `caller` argument | `holds_role(Resolver, caller)`; caller not a party; `S(Disputed)`; `total_amount < PANEL_THRESHOLD_AMOUNT`; `freelancer_bps <= 10000`. Frozen escrows are allowed. |
| `(1, "resolved", id)` | `vote_resolution` ([L2729](../contracts/escrow/src/lib.rs#L2729) → [L1452](../contracts/escrow/src/lib.rs#L1452)) | `member` argument | `S(Disputed)`; caller is current panel member and not a party; `freelancer_bps <= 10000`; current matching votes >= `threshold`. |
| `(1, "cancel", id)` | `cancel_contract` ([L3244](../contracts/escrow/src/lib.rs#L3244) → [L3295](../contracts/escrow/src/lib.rs#L3295)) | stored client (`c.client`) | `NP(CANCEL)`; `L(id)`; `NF(id)`; not a stream; `S(Created\|Funded)`. |
| `(1, "rep", id)` | `issue_reputation` ([L3300](../contracts/escrow/src/lib.rs#L3300) → [L3379](../contracts/escrow/src/lib.rs#L3379)) | stored client (`c.client`) | `NP(REPUTATION)`; not compacted; `L(id)`; `NF(id)`; `S(Completed\|Resolved)`; not already issued; rating 1..=5; released amount >= `MIN_REPUTATION_VALUE`. |

## Inline events

| Event topic | Entrypoint (source) | Signer | Preconditions |
|---|---|---|---|
| `("role_set", role)` | `grant_role` / `revoke_role` ([L4044](../contracts/escrow/src/lib.rs#L4044) → [L538](../contracts/escrow/src/lib.rs#L538)) | stored Admin | `init`; requested membership differs from current (`RoleUnchanged` otherwise). |
| `("adm_prop",)` | `propose_admin` ([L1656](../contracts/escrow/src/lib.rs#L1656)) | stored Admin | `init`; `new_admin != current`. Allowed while paused. |
| `("adm_acc",)` | `accept_admin` ([L1674](../contracts/escrow/src/lib.rs#L1674)) | `PendingAdmin` address | Pending nomination exists (`NoPendingAdmin`); stored Admin exists. Allowed while paused. |
| `("adm_cncl",)` | `cancel_admin_proposal` ([L1693](../contracts/escrow/src/lib.rs#L1693)) | stored Admin | Pending nomination exists. Allowed while paused. |
| `("pause",)` | `set_pause_flags` ([L1730](../contracts/escrow/src/lib.rs#L1730)) | `caller` argument | `holds_role(Pauser, caller)`; flags contain only bits in `PAUSE_ALL`. |
| `("fee_set",)` | `set_fee` ([L1747](../contracts/escrow/src/lib.rs#L1747)) | stored Admin | `init`; `fee_bps <= MAX_FEE_BPS`; treasury is not the contract. |
| `("emerg", id)` | `emergency_withdraw` ([L1797](../contracts/escrow/src/lib.rs#L1797)) | `caller` argument | `L(id)`; caller is client or freelancer; paused continuously >= 30 days; `S(Created\|Funded\|Disputed)`. Works while paused/frozen. |
| `("sweep", token)` | `sweep_surplus` ([L1888](../contracts/escrow/src/lib.rs#L1888)) | stored Admin | `init`; contract token balance exceeds aggregate liability (`NoSurplus` otherwise). |
| `("payout", id)` | `set_payout_address` ([L1912](../contracts/escrow/src/lib.rs#L1912)) | stored freelancer (`c.freelancer`) | `NP(CREATE)`; `L(id)`; `NF(id)`; `S(Created\|Funded)`; payout is neither contract nor client. |
| `("frozen", id)` | `freeze_escrow` ([L1944](../contracts/escrow/src/lib.rs#L1944)) | stored Admin | `init`; escrow exists; not already frozen. |
| `("unfrozen", id)` | `unfreeze_escrow` ([L1962](../contracts/escrow/src/lib.rs#L1962)) | stored Admin | `init`; escrow exists; currently frozen. |
| `("seq", id)` | `create_contract_sequential` ([L2035](../contracts/escrow/src/lib.rs#L2035)) | `client` argument | `CREATE`; sequential flag written after `build_escrow`. |
| `("team", id)` | `set_team_split` ([L2070](../contracts/escrow/src/lib.rs#L2070)) | stored freelancer (`c.freelancer`) | `NP(CREATE)`; `L(id)`; `NF(id)`; `S(Created)`; no deposit; 1..=5 unique non-zero payees excluding client/contract; shares sum to 10000 bps. |
| `("submit", id)` | `submit_milestone` ([L2151](../contracts/escrow/src/lib.rs#L2151)) | stored freelancer (`c.freelancer`) | `REL`; `S(Funded)`; non-stream; milestone exists, unreleased, not already submitted; unpaid amount covered by held balance. |
| `("claim", id)` | `claim_milestone` ([L2194](../contracts/escrow/src/lib.rs#L2194)) | stored freelancer (`c.freelancer`) | `REL`; `S(Funded\|Cancelled)`; submitted, unreleased milestone; review window elapsed; funds available. |
| `("rel_part", id)` | `release_partial` ([L2345](../contracts/escrow/src/lib.rs#L2345)) | stored client (`c.client`) | `REL`; `S(Created\|Funded)`; unreleased index; `0 < amount <= remaining`; sequential order if enabled; funds available. |
| `("rel_batch", id)` | `release_milestones` ([L2427](../contracts/escrow/src/lib.rs#L2427)) | stored client (`c.client`) | `REL`; `S(Created\|Funded)`; 1..=20 unique, existing, unreleased indices; sequential order if enabled; funds available. |
| `("bonus", id)` | `pay_bonus` ([L2545](../contracts/escrow/src/lib.rs#L2545)) | stored client (`c.client`) | `NP(RELEASE)`; `L(id)`; `NF(id)`; `S(Funded\|Completed)`; `amount > 0`; direct transfer succeeds. |
| `("panel",)` | `set_panel` ([L2701](../contracts/escrow/src/lib.rs#L2701)) | stored Admin | `init`; `1 <= threshold <= len <= 7`; no duplicate members. |
| `("voted", id)` | `vote_resolution` ([L2729](../contracts/escrow/src/lib.rs#L2729)) | `member` argument | `S(Disputed)`; member is on current panel and not a party; `freelancer_bps <= 10000`. |
| `("vote", id)` | `vote_resolution` ([L2792](../contracts/escrow/src/lib.rs#L2792)) | `member` argument | Same as `("voted", id)`, plus current matching votes reach `threshold`; emitted per counted vote after settlement. |
| `("chg_prop", id)` | `propose_change_order` ([L2806](../contracts/escrow/src/lib.rs#L2806)) | stored client (`c.client`) | `NP(CREATE)`; `L(id)`; `NF(id)`; `S(Created\|Funded\|Completed)`; added milestones fit cap and pass amount checks. |
| `("chg_ord", id)` | `accept_change_order` ([L2844](../contracts/escrow/src/lib.rs#L2844)) | stored freelancer (`c.freelancer`) | `NP(CREATE)`; `L(id)`; `NF(id)`; pending proposal; `S(Created\|Funded\|Completed)`; amounts revalidated; reopens escrow as `Created`. |
| `("chg_rej", id)` | `reject_change_order` ([L2905](../contracts/escrow/src/lib.rs#L2905)) | stored freelancer (`c.freelancer`) | `L(id)`; pending change order exists. No pause/freeze gate. |
| `("terms", id)` | `set_terms` ([L2927](../contracts/escrow/src/lib.rs#L2927)) | stored client (`c.client`) | `NP(CREATE)`; `L(id)`; `NF(id)`; `S(Created)`; no deposit; zero or one spec hash per milestone. |
| `("deliver", id)` | `deliver` ([L2961](../contracts/escrow/src/lib.rs#L2961)) | stored freelancer (`c.freelancer`) | `NP(RELEASE)`; `L(id)`; `NF(id)`; `S(Created\|Funded\|Disputed)`; milestone exists and is unreleased. |
| `("expired", id)` | `expire_unfunded` ([L3014](../contracts/escrow/src/lib.rs#L3014)) | none | `NP(CANCEL)`; `L(id)`; `NF(id)`; `S(Created)`; nothing released; funding deadline recorded and passed. |
| `("settle_p", id)` | `propose_settlement` ([L3056](../contracts/escrow/src/lib.rs#L3056)) | `proposer` argument | `NP(CANCEL)`; `L(id)`; `NF(id)`; proposer is client or freelancer; `S(Created\|Funded\|Disputed)`; `0 <= freelancer_amount <= held`. |
| `("settled", id)` | `accept_settlement` ([L3105](../contracts/escrow/src/lib.rs#L3105)) | `acceptor` argument | `NP(CANCEL)`; `L(id)`; `NF(id)`; acceptor is the other party; pending proposal amount matches; `S(Created\|Funded\|Disputed)`; amount <= held. |
| `("settle_w", id)` | `withdraw_settlement` ([L3175](../contracts/escrow/src/lib.rs#L3175)) | `proposer` argument | `L(id)`; pending proposal belongs to signer. No pause/freeze gate. |
| `("resign", id)` | `resign` ([L3202](../contracts/escrow/src/lib.rs#L3202)) | stored freelancer (`c.freelancer`) | `NP(CANCEL)`; `L(id)`; `NF(id)`; `S(Created\|Funded)`. |
| `("rep_cli", id)` | `rate_client` ([L3384](../contracts/escrow/src/lib.rs#L3384)) | stored freelancer (`c.freelancer`) | `NP(REPUTATION)`; `L(id)`; `NF(id)`; `S(Completed\|Resolved)`; client not already rated; rating 1..=5. |
| `("rep_void", id)` | `void_rating` ([L3428](../contracts/escrow/src/lib.rs#L3428)) | stored Admin | `init`; `NP(REPUTATION)`; rating record exists and is not already voided. |
| `("migrated",)` | `migrate_escrows` ([L3500](../contracts/escrow/src/lib.rs#L3500)) | none | <= 50 ids; at least one legacy entry migrated. |
| `("stream_wd", id)` | `withdraw_vested` ([L3678](../contracts/escrow/src/lib.rs#L3678)) | stored freelancer (`c.freelancer`) | `NP(RELEASE)`; `L(id)`; `NF(id)`; stream exists; `S(Funded)`; not stopped; vested amount > withdrawn. |
| `("stream_st", id)` | `stop_stream` ([L3726](../contracts/escrow/src/lib.rs#L3726)) | stored client (`c.client`) | `NP(CANCEL)`; `L(id)`; `NF(id)`; stream exists; `S(Funded)`; not already stopped. |
| `("amend_p", id)` | `propose_amendment` ([L3778](../contracts/escrow/src/lib.rs#L3778)) | `proposer` argument | `NP(CREATE)`; `L(id)`; `NF(id)`; non-stream; proposer is client or freelancer; `S(Created\|Funded)`; amounts pass amendment checks. |
| `("amended", id)` | `accept_amendment` ([L3830](../contracts/escrow/src/lib.rs#L3830)) | `acceptor` argument | `NP(CREATE)`; `L(id)`; `NF(id)`; acceptor is the other party; pending nonce matches; `S(Created\|Funded)`; amounts revalidated. |
| `("compact", id)` | `compact_escrow` ([L3949](../contracts/escrow/src/lib.rs#L3949)) | none | `L(id)`; `NF(id)`; not already compacted; `Cancelled` with no held funds, or `Completed`/`Resolved` already rated or past the 30-day grace period. |
| `("upg_sched",)` | `schedule_upgrade` ([L4069](../contracts/escrow/src/lib.rs#L4069)) | stored Admin | `init`; timestamp addition succeeds; rescheduling resets the 48-hour delay. Allowed while paused. |
| `("upg_cncl",)` | `cancel_upgrade` ([L4093](../contracts/escrow/src/lib.rs#L4093)) | stored Admin | `init`; pending upgrade exists. |
| `("upgraded",)` | `execute_upgrade` ([L4120](../contracts/escrow/src/lib.rs#L4120)) | stored Admin | `init`; pending upgrade exists; current time >= `eta`; version addition succeeds. Allowed while paused. |

## Role shorthand

`stored Admin` signs all admin entrypoints. `Pauser` and `Resolver` are delegated roles; the admin implicitly holds both. Party-scoped events require the stored `client` or `freelancer`, never an arbitrary caller, except where the table names an explicit argument that is checked against the stored party.

## Related documentation

- [indexer-schema.md](escrow/indexer-schema.md#event-topics-and-payloads) — event topics and payloads.
- [ABI reference](escrow/abi-reference.md) — public entrypoint signatures.
