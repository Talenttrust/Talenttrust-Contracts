# Contracts Storage Layout & TTL Policy

This document defines the on-chain storage architecture, layout, and time-to-live (TTL) policy for the TalentTrust escrow contract on Soroban. It provides a comprehensive specification and reference for every storage key declared in the contract's private `DataKey` enum ([`contracts/escrow/src/lib.rs`](../contracts/escrow/src/lib.rs)), including stored value types, storage tiers (`instance` vs `persistent`), writer entrypoints, TTL maintenance, and coverage under permissionless keep-alive helpers.

---

## 1. Storage Architecture & Tiers

Soroban provides three storage tiers with distinct persistence, capacity, and cost profiles. The TalentTrust escrow contract strictly partitions state between `instance` and `persistent` storage:

| Storage Tier | Scope & Capacity | Eviction & Rent Model | Contract Usage |
|---|---|---|---|
| `env.storage().instance()` | Contract-level singleton state; shared across all contract calls. Co-located with the contract executable instance. | Extended as a single unit via `bump_instance(env)`. Stays hot as long as admin or mutating entrypoints execute. | Global settings, operational admin address, protocol fee rate and treasury destination, granular pause bitmask, dispute panel configuration, and scheduled upgrade proposals. |
| `env.storage().persistent()` | Key-value records allocated independently. Scalable to arbitrary numbers of escrow contracts, milestones, and participants. | Evicted by the host if remaining TTL drops to 0 without access or renewal. Renewed via `extend_ttl` to `PERSISTENT_TTL_EXTEND` when remaining TTL falls below `PERSISTENT_TTL_THRESHOLD`. | Escrow contracts (`ContractV`), historical receipts (`Receipt`), milestones, delivery hashes, dispute records, settlement proposals, amendments, reputation ledgers, and paginated participant indexes. |
| `env.storage().temporary()` | Ephemeral storage auto-evicted by the host upon TTL expiry. | Host auto-eviction without archival. | **Not used.** The escrow contract deliberately avoids temporary storage to prevent unexpected state drop on unfinalized counterparties. |

> **Key Takeaway:** The escrow contract **does not** use `env.storage().temporary()`. All state is strictly modeled across `instance()` or `persistent()` storage to guarantee persistence and auditability across business workflows.

---

## 2. Invariants: Append-Only Enum & Storage Versioning

### 2.1 The Append-Only `DataKey` Enum Requirement

In Soroban (`soroban-sdk`), `#[contracttype]` enums are serialized by their variant discriminant (their 0-based index in the declaration order).

- Reordering existing variants alters their assigned numeric discriminants.
- Inserting a new variant before or between existing variants alters the discriminants of all subsequent variants.
- Removing an existing variant shifts all subsequent discriminants.

If the order of variants in `enum DataKey` is changed, existing on-chain data written with the original discriminants will fail deserialization or deserialize into incorrect variants, resulting in permanent contract corruption.

> [!CRITICAL]
> **Append-Only Requirement:** The `DataKey` enum in [`contracts/escrow/src/lib.rs`](../contracts/escrow/src/lib.rs) **MUST be strictly append-only**. Any newly added key variants must be appended exclusively at the very end of the enum definition.

### 2.2 Legacy Keys and Versioning Migrations

To maintain compatibility with earlier deployments and on-chain records, two variants are marked as legacy keys:

1. **`DataKey::Contract(u64)` (Index 3):**
   - **Legacy Format:** Pre-versioning unadorned escrow contract record (`EscrowContract`).
   - **Current Versioned Format:** `DataKey::ContractV(u64)` (Index 33), which wraps contract state in a versioned envelope `StoredEscrow::V1(EscrowContract)`.
   - **Migration Strategy:**
     - *Lazy Migration:* The internal `save(env, c)` function writes the updated record to `DataKey::ContractV(c.id)` and drops `DataKey::Contract(c.id)` if present. Read operations via `load(env, id)` check `ContractV` first and fall back to `Contract(id)` transparently without performing writes.
     - *Eager Migration:* The permissionless public function `migrate_escrows(env, ids)` allows batch migration of legacy entries to `ContractV` in batches of up to `MAX_MIGRATE_BATCH = 50`.
     - *Compaction:* When an escrow is finalized, `compact_escrow(env, id)` purges both `ContractV(id)` and `Contract(id)` from persistent storage and writes an immutable `Receipt(id)`.
2. **`DataKey::Paused` (Index 1):**
   - **Legacy Format:** Simple `bool` indicating whether the entire contract is paused.
   - **Current Format:** `DataKey::PauseFlags` (Index 40), a 32-bit bitmask governing granular operational categories (`PAUSE_CREATE`, `PAUSE_DEPOSIT`, `PAUSE_RELEASE`, `PAUSE_DISPUTE`, `PAUSE_CANCEL`, `PAUSE_REPUTATION`, or `PAUSE_ALL = 63`).
   - **Synchronization:** Whenever pause flags are modified via `write_pause_flags`, the contract updates `PauseFlags` and simultaneously sets `Paused` to `flags != 0`. This ensures legacy tools and queries inspecting `DataKey::Paused` receive accurate status.

---

## 3. TTL Constants & Maintenance Policy

All storage TTL durations and thresholds in Soroban are denominated in **ledgers** (~5 seconds per ledger on Stellar mainnet).

### 3.1 TTL Constants

Defined in [`contracts/escrow/src/lib.rs`](../contracts/escrow/src/lib.rs):

| Constant | Value | Approximate Wall-Clock | Description |
|---|---:|---:|---|
| `PERSISTENT_TTL_THRESHOLD` | 17 280 ledgers | ~1 day | Rent renewal threshold. If an accessed entry's remaining TTL is below this threshold, Soroban extends it to `PERSISTENT_TTL_EXTEND`. |
| `PERSISTENT_TTL_EXTEND` | 518 400 ledgers | ~30 days | Standard extension target. Persistent entries are renewed to approximately 30 days of lifetime. |
| `THIRTY_DAYS_IN_LEDGERS` | 518 400 ledgers | ~30 days | Canonical 30-day duration constant in ledgers. |

### 3.2 Timelock Constants (Wall-Clock Seconds)

The contract enforces several time-based business rules using ledger timestamp seconds. These are protocol timelocks, distinct from Soroban storage TTL:

| Constant | Value | Approximate Wall-Clock | Purpose |
|---|---:|---:|---|
| `UPGRADE_DELAY_SECS` | 172 800 s | 2 days | Minimum timelock delay between scheduling a WASM code upgrade and executing it. |
| `EMERGENCY_EXIT_DELAY_SECS` | 2 592 000 s | 30 days | Minimum continuous pause duration required before counterparties can execute unilateral emergency withdrawals. |
| `FUNDING_WINDOW_SECS` | 1 209 600 s | 14 days | Time allowed for counterparties to fully fund an escrow before unfunded deposits become refundable. |
| `MIN_REVIEW_WINDOW_SECS` | 3 600 s | 1 hour | Minimum permissible review window for milestone submissions. |
| `DEFAULT_REVIEW_WINDOW_SECS` | 604 800 s | 7 days | Default review window duration before freelancer may trigger milestone auto-approval. |
| `MAX_REVIEW_WINDOW_SECS` | 2 592 000 s | 30 days | Maximum permissible review window duration. |
| `MIN_STREAM_SECS` | 3 600 s | 1 hour | Minimum allowed duration for streaming escrow vesting schedules. |

### 3.3 Storage Extension Helpers

The contract manages TTL extensions through dedicated internal helpers and public entrypoints:

1. **`bump_instance(env: &Env)`**
   ```rust
   fn bump_instance(env: &Env) {
       env.storage()
           .instance()
           .extend_ttl(PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
   }
   ```
   Invoked by `require_admin`, `initialize`, `build_escrow`, `save`, and keep-alive entrypoints to ensure contract instance metadata remains live.

2. **`extend_if_present(env: &Env, key: &DataKey) -> bool`**
   ```rust
   fn extend_if_present(env: &Env, key: &DataKey) -> bool {
       if env.storage().persistent().has(key) {
           env.storage()
               .persistent()
               .extend_ttl(key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
           true
       } else {
           false
       }
   }
   ```
   Probes whether a key exists without mutating its data; if present, extends its TTL to 30 days.

3. **`extend_escrow_ttl(env: Env, id: u64) -> Result<(), EscrowError>`**
   Permissionless public entrypoint. Anyone can pay the gas/rent to keep an escrow alive.
   - Verifies the escrow exists by extending `ContractV(id)`, `Contract(id)`, or `Receipt(id)`. Returns `ContractNotFound` if none exist.
   - Refreshes all 23 associated per-escrow persistent keys (if present).
   - Iterates through `0..milestones.len()` and extends `Submitted`, `Delivery`, and `MilestonePaid` keys for each milestone.
   - Executes `bump_instance(&env)` to renew instance storage.

4. **`extend_reputation_ttl(env: Env, freelancer: Address)`**
   Permissionless public entrypoint for an address's reputation records.
   - Refreshes `Reputation(freelancer)`, `WeightedRep(freelancer)`, `ClientReputation(freelancer)`, `Outcomes(freelancer)`, and `BonusTotal(freelancer)`.
   - Executes `bump_instance(&env)` to renew instance storage.

---

## 4. Complete DataKey Reference Table

The table below enumerates all 51 variants in the private `DataKey` enum ([`contracts/escrow/src/lib.rs`](../contracts/escrow/src/lib.rs)), preserving their exact definition order (indices 0 through 50):

| Index | Variant Signature | Value Type | Storage Tier | Writer Function(s) | TTL Handling | Covered by `extend_escrow_ttl`? | Description & Invariants |
|---|---|---|---|---|---|:---:|---|
| 0 | `Admin` | `Address` | Instance | `initialize`, `accept_admin` | Bumped via `bump_instance` on admin calls and init | No (instance bumped at end) | Operational admin address. Gated access for pause, fee config, panel settings, role assignments, and upgrade proposals. |
| 1 | `Paused` | `bool` | Instance | `initialize`, `write_pause_flags` | Bumped via `bump_instance` | No (instance bumped at end) | **Legacy Key**. Kept synchronized with `PauseFlags != 0` for backwards compatibility with legacy readers. |
| 2 | `NextId` | `u64` | Instance | `initialize`, `build_escrow` | Bumped via `bump_instance` on escrow creation | No (instance bumped at end) | Monotonic counter for allocating unique escrow IDs. Starts at 1; 0 is never allocated. |
| 3 | `Contract(u64)` | `EscrowContract` | Persistent | Legacy writer; removed by `save` (lazy migration to `ContractV`) and `compact_escrow` | Explicitly extended by `extend_escrow_ttl` via `extend_if_present` | **Yes** | **Legacy Key**. Unversioned escrow record. Preserved so pre-versioning entries remain addressable until migrated lazily or eagerly. |
| 4 | `Reputation(Address)` | `Reputation` | Persistent | `issue_reputation`, `void_rating` | Extended with `PERSISTENT_TTL_EXTEND` on write; refreshed via `extend_reputation_ttl` | No | Cumulative reputation score (`total_score: i128, count: i128`) for a freelancer address. |
| 5 | `Outcomes(Address)` | `Outcomes` | Persistent | `settle`, `open_dispute`, `cancel_contract` | Extended with `PERSISTENT_TTL_EXTEND` on write; refreshed via `extend_reputation_ttl` | No | Historical dispute and resolution statistics (`completed`, `disputed_resolved`, `disputed_lost`, `cancelled`) for an address. |
| 6 | `Rating(u64)` | `RatingRecord` | Persistent | `issue_reputation`, `void_rating` | Extended with `PERSISTENT_TTL_EXTEND` on write; refreshed via `extend_escrow_ttl` | **Yes** | Client rating record for an escrow (`freelancer`, `rating: u32`, `issued_at: u64`, `voided: bool`). |
| 7 | `Bonus(u64)` | `i128` | Persistent | `pay_bonus` | Extended with `PERSISTENT_TTL_EXTEND` on write; refreshed via `extend_escrow_ttl` | **Yes** | Discretionary bonus amount paid by client to freelancer for an escrow. |
| 8 | `BonusTotal(Address)` | `i128` | Persistent | `pay_bonus` | Extended with `PERSISTENT_TTL_EXTEND` on write; refreshed via `extend_reputation_ttl` | No | Total lifetime discretionary bonus received by a freelancer across all escrows. |
| 9 | `Liability(Address)` | `i128` | Persistent | `set_liability` | Extended with `PERSISTENT_TTL_EXTEND` on write | No | Aggregate locked token balance held by the contract per token address to ensure solvency. |
| 10 | `Payout(u64)` | `Address` | Persistent | `set_payout_address` | Extended with `PERSISTENT_TTL_EXTEND` on write; refreshed via `extend_escrow_ttl` | **Yes** | Optional alternative payee address specified by the freelancer for milestone releases. |
| 11 | `Frozen(u64)` | `bool` | Persistent | `freeze_escrow`, `unfreeze_escrow` (sets `false`) | Extended with `PERSISTENT_TTL_EXTEND` on write; refreshed via `extend_escrow_ttl` | **Yes** | Freeze flag preventing mutations during ongoing administrative or dispute investigations. |
| 12 | `ClosedAt(u64)` | `u64` | Persistent | `record_closed_at`; removed by `accept_change_order` (reopen), `compact_escrow` | Extended with `PERSISTENT_TTL_EXTEND` on write; refreshed via `extend_escrow_ttl` | **Yes** | Ledger timestamp recording when an escrow reached a terminal state (`Completed`, `Refunded`, etc.). |
| 13 | `Receipt(u64)` | `EscrowReceipt` | Persistent | `compact_escrow` | Extended with `PERSISTENT_TTL_EXTEND` on write; refreshed via `extend_escrow_ttl` | **Yes** | Compacted immutable archival receipt of a finalized escrow, replacing full contract state. |
| 14 | `Team(u64)` | `Vec<(Address, u32)>` | Persistent | `set_team_split` | Extended with `PERSISTENT_TTL_EXTEND` on write; refreshed via `extend_escrow_ttl` | **Yes** | Multi-party split recipients and basis point shares for freelancer team milestone distribution. |
| 15 | `ClientReputation(Address)` | `Reputation` | Persistent | `rate_client` | Extended with `PERSISTENT_TTL_EXTEND` on write; refreshed via `extend_reputation_ttl` | No | Cumulative counter-party reputation score for clients, rated by freelancers upon contract completion. |
| 16 | `ClientRated(u64)` | `bool` | Persistent | `rate_client` | Extended with `PERSISTENT_TTL_EXTEND` on write; refreshed via `extend_escrow_ttl` | **Yes** | Boolean guard preventing duplicate ratings submitted by a freelancer for the client on an escrow. |
| 17 | `Sequential(u64)` | `bool` | Persistent | `create_contract_sequential` | Extended with `PERSISTENT_TTL_EXTEND` on write; refreshed via `extend_escrow_ttl` | **Yes** | Mode flag requiring milestones to be completed and released in strict sequential order. |
| 18 | `WeightedRep(Address)` | `WeightedReputation` | Persistent | `issue_reputation`, `void_rating` | Extended with `PERSISTENT_TTL_EXTEND` on write; refreshed via `extend_reputation_ttl` | No | Volume-weighted reputation record (`weighted_rating_sum: i128, total_weight: i128`) scaled by escrow value. |
| 19 | `RatingWeight(u64)` | `i128` | Persistent | `issue_reputation` | Extended with `PERSISTENT_TTL_EXTEND` on write; refreshed via `extend_escrow_ttl` | **Yes** | Snapshot of the weight attributed to an escrow's rating, retained for potential voiding adjustments. |
| 20 | `Panel` | `Panel` | Instance | `set_panel` | Bumped via `bump_instance` on write and vote | No (instance bumped at end) | Multi-member dispute arbitration panel configuration (`members: Vec<Address>, threshold: u32`). |
| 21 | `Votes(u64)` | `Map<Address, u32>` | Persistent | `vote_resolution`; removed upon reaching consensus threshold | Extended with `PERSISTENT_TTL_EXTEND` on write; refreshed via `extend_escrow_ttl` | **Yes** | Panel member votes for split resolution on a disputed escrow (`member -> freelancer_bps`). |
| 22 | `RoleMember(Role, Address)` | `bool` | Persistent | `set_role` | Extended with `PERSISTENT_TTL_EXTEND` on write | No | Access control list mapping role (`Role::EmergencyAdmin`, etc.) and account to membership boolean. |
| 23 | `Funders(u64)` | `Map<Address, i128>` | Persistent | `build_escrow`, `deposit_funds` | Extended with `PERSISTENT_TTL_EXTEND` on write; refreshed via `extend_escrow_ttl` | **Yes** | Per-funder deposited amounts supporting multi-party funding and pro-rata unfunded refunds. |
| 24 | `FundBy(u64)` | `u64` | Persistent | `build_escrow`, `accept_change_order`, `extend_funding_deadline`, `accept_amendment` | Extended with `PERSISTENT_TTL_EXTEND` on write; refreshed via `extend_escrow_ttl` | **Yes** | Ledger timestamp deadline by which the escrow must be fully funded before deposit refunds open. |
| 25 | `ChangeOrder(u64)` | `Vec<i128>` | Persistent | `propose_change_order`; removed by `accept_change_order`, `cancel_change_order` | Extended with `PERSISTENT_TTL_EXTEND` on write; refreshed via `extend_escrow_ttl` | **Yes** | Proposed additional milestone amounts awaiting acceptance by counterparty. |
| 26 | `MilestonePaid(u64, u32)` | `bool` | Persistent | `mark_milestone_paid`; removed by `reset_milestone_paid` | Extended with `PERSISTENT_TTL_EXTEND` on write; refreshed via `extend_escrow_ttl` | **Yes** | Tracks off-chain or manual payment confirmation status for a specific milestone. |
| 27 | `Terms(u64)` | `BytesN<32>` | Persistent | `set_terms` | Extended with `PERSISTENT_TTL_EXTEND` on write; refreshed via `extend_escrow_ttl` | **Yes** | SHA-256 hash digest of the off-chain contractual terms and agreement. |
| 28 | `MilestoneSpecs(u64)` | `Vec<BytesN<32>>` | Persistent | `set_terms` | Extended with `PERSISTENT_TTL_EXTEND` on write; refreshed via `extend_escrow_ttl` | **Yes** | SHA-256 hash digests of detailed specifications for each milestone in the escrow. |
| 29 | `Delivery(u64, u32)` | `BytesN<32>` | Persistent | `submit_delivery` | Extended with `PERSISTENT_TTL_EXTEND` on write; refreshed via `extend_escrow_ttl` | **Yes** | SHA-256 hash digest of the deliverable submitted for a milestone review. |
| 30 | `DisputeBond(u64)` | `(Address, i128)` | Persistent | `open_dispute`; removed in `settle` | Extended with `PERSISTENT_TTL_EXTEND` on write; refreshed via `extend_escrow_ttl` | **Yes** | Record of the dispute opener address and collateral bond amount posted to initiate dispute. |
| 31 | `Version` | `u32` | Instance | `execute_upgrade` | Bumped via `bump_instance` | No (instance bumped at end) | Contract logic version number, incremented on contract WASM upgrades. |
| 32 | `PendingUpgrade` | `PendingUpgrade` | Instance | `schedule_upgrade`; removed by `cancel_upgrade`, `execute_upgrade` | Bumped via `bump_instance` | No (instance bumped at end) | Scheduled contract upgrade proposal (`new_wasm_hash: BytesN<32>, executable_after: u64`). |
| 33 | `ContractV(u64)` | `StoredEscrow` | Persistent | `save`; removed by `compact_escrow` | Extended with `PERSISTENT_TTL_EXTEND` on `save`; explicitly checked and extended in `extend_escrow_ttl` | **Yes** | Canonical versioned escrow envelope (`StoredEscrow::V1(EscrowContract)`). |
| 34 | `Submitted(u64, u32)` | `u64` | Persistent | `submit_milestone`; removed by `clear_submitted` | Extended with `PERSISTENT_TTL_EXTEND` on write; refreshed via `extend_escrow_ttl` | **Yes** | Ledger timestamp when freelancer submitted a milestone, initiating review window. |
| 35 | `ReviewWindow(u64)` | `u64` | Persistent | `build_escrow` | Extended with `PERSISTENT_TTL_EXTEND` on write; refreshed via `extend_escrow_ttl` | **Yes** | Configured review period duration in seconds before auto-approval becomes eligible. |
| 36 | `PendingAdmin` | `Address` | Instance | `propose_admin`; removed by `accept_admin`, `cancel_admin_proposal` | Bumped via `bump_instance` | No (instance bumped at end) | Candidate new admin address during 2-step administrative rotation. |
| 37 | `PausedAt` | `u64` | Instance | `write_pause_flags` (set on pause; removed on unpause) | Bumped via `bump_instance` | No (instance bumped at end) | Ledger timestamp marking the start of the current pause period, used for emergency timelock. |
| 38 | `Resigned(u64)` | `bool` | Persistent | `resign` | Extended with `PERSISTENT_TTL_EXTEND` on write; refreshed via `extend_escrow_ttl` | **Yes** | Flag indicating the freelancer voluntarily resigned from the contract. |
| 39 | `Settlement(u64)` | `SettlementProposal` | Persistent | `propose_settlement`; removed by `clear_settlement` | Extended with `PERSISTENT_TTL_EXTEND` on write; refreshed via `extend_escrow_ttl` | **Yes** | Pending mutual settlement proposal (`proposer: Address, freelancer_amount: i128`). |
| 40 | `PauseFlags` | `u32` | Instance | `write_pause_flags` | Bumped via `bump_instance` | No (instance bumped at end) | Fine-grained pause category bitmask (`PAUSE_CREATE`, `PAUSE_DEPOSIT`, etc.). |
| 41 | `FeeBps` | `u32` | Instance | `set_fee` | Bumped via `bump_instance` | No (instance bumped at end) | Global protocol release fee rate in basis points (max 500 bps = 5%). |
| 42 | `Treasury` | `Address` | Instance | `set_fee` | Bumped via `bump_instance` | No (instance bumped at end) | Recipient address for collected protocol fees. |
| 43 | `EscrowFee(u64)` | `u32` | Persistent | `build_escrow` | Extended with `PERSISTENT_TTL_EXTEND` on write; refreshed via `extend_escrow_ttl` | **Yes** | Protocol fee rate (basis points) snapshotted and locked for a specific escrow at creation. |
| 44 | `ClientEscrows(Address)` | `u32` | Persistent | `append_party_index` | Extended with `PERSISTENT_TTL_EXTEND` on write | No | Counter of escrows created by client address, indexing into paginated ID lists. |
| 45 | `FreelancerEscrows(Address)` | `u32` | Persistent | `append_party_index` | Extended with `PERSISTENT_TTL_EXTEND` on write | No | Counter of escrows created for freelancer address, indexing into paginated ID lists. |
| 46 | `ClientPage(Address, u32)` | `Vec<u64>` | Persistent | `append_party_index` | Extended with `PERSISTENT_TTL_EXTEND` on write | No | Paginated list of escrow IDs (page size 50) created by client address. |
| 47 | `FreelancerPage(Address, u32)` | `Vec<u64>` | Persistent | `append_party_index` | Extended with `PERSISTENT_TTL_EXTEND` on write | No | Paginated list of escrow IDs (page size 50) created for freelancer address. |
| 48 | `Stream(u64)` | `Stream` | Persistent | `save_stream` | Extended with `PERSISTENT_TTL_EXTEND` on write; refreshed via `extend_escrow_ttl` | **Yes** | Linear vesting schedule details (`start: u64, duration: u64, total_amount: i128, claimed_amount: i128`). |
| 49 | `Amendment(u64)` | `Amendment` | Persistent | `propose_amendment`; removed by `accept_amendment` | Extended with `PERSISTENT_TTL_EXTEND` on write; refreshed via `extend_escrow_ttl` | **Yes** | Proposed milestone amount realignment awaiting counterparty acceptance (`proposer, new_amounts, nonce`). |
| 50 | `AmendNonce(u64)` | `u32` | Persistent | `propose_amendment` | Extended with `PERSISTENT_TTL_EXTEND` on write; refreshed via `extend_escrow_ttl` | **Yes** | Monotonically incrementing replay-protection nonce for contract amendments. |

---

## 5. Functional Key Domain Specifications

### 5.1 Global Instance Storage (Singleton Config)

These keys reside in `env.storage().instance()` and are bumped together whenever `bump_instance(env)` is called:

- **`Admin`**: Initialized once via `initialize(env, admin)`. Rotation is protected by a two-step handshake: `propose_admin` stores candidate in `PendingAdmin`; `accept_admin` verifies caller equals `PendingAdmin`, overwrites `Admin`, and clears `PendingAdmin`.
- **`Paused`** *(Legacy)*: Legacy global pause toggle. Maintained alongside `PauseFlags` so legacy clients reading `Paused` see `true` if any pause flag is active.
- **`NextId`**: Monotonic `u64` counter allocating escrow IDs starting at 1. Read and incremented inside `build_escrow`.
- **`Panel`**: Stores the dispute arbitration panel (`members: Vec<Address>, threshold: u32`). Configured by admin through `set_panel`. Members cannot exceed `PANEL_MAX_MEMBERS = 7`, and threshold must be `>= 1` and `<= members.len()`.
- **`Version`**: Current contract implementation version `u32`, updated when executing code upgrades.
- **`PendingUpgrade`**: Holds `PendingUpgrade` struct (`new_wasm_hash: BytesN<32>, executable_after: u64`). Enforces a 2-day timelock (`UPGRADE_DELAY_SECS`) before `execute_upgrade` can apply the migration.
- **`PendingAdmin`**: Stores the proposed new administrator address awaiting acceptance.
- **`PausedAt`**: Stores the ledger timestamp when the contract transitioned from unpaused to paused. Used to compute eligibility for emergency exits after `EMERGENCY_EXIT_DELAY_SECS`.
- **`PauseFlags`**: 32-bit bitmask governing fine-grained operational pause states:
  - `PAUSE_CREATE = 1`
  - `PAUSE_DEPOSIT = 2`
  - `PAUSE_RELEASE = 4`
  - `PAUSE_DISPUTE = 8`
  - `PAUSE_CANCEL = 16`
  - `PAUSE_REPUTATION = 32`
  - `PAUSE_ALL = 63`
- **`FeeBps`**: Protocol release fee in basis points (0..=500, representing 0% to 5%).
- **`Treasury`**: Destination `Address` where protocol fees are transferred upon milestone release.

### 5.2 Core Escrow State & Archival Lifecycle

- **`Contract(u64)`** *(Legacy)*: Legacy unversioned storage key storing `EscrowContract`. Replaced lazily upon mutation or eagerly via `migrate_escrows`.
- **`ContractV(u64)`**: Canonical versioned envelope storing `StoredEscrow::V1(EscrowContract)`. Encapsulates counterparties (`client`, `freelancer`), token address, milestone vector, total/funded/released/refunded balances, and lifecycle status (`Created`, `Funded`, `Completed`, `Disputed`, `Cancelled`, `Refunded`).
- **`ClosedAt(u64)`**: Ledger sequence number recording when an escrow entered a terminal state.
- **`Receipt(u64)`**: Written by `compact_escrow` after terminal state. Removes `ContractV`, `Contract`, and `ClosedAt` from storage to reclaim space, leaving a lightweight immutable `EscrowReceipt` (`id`, `client`, `freelancer`, `token`, `status`, `total_amount`, `released_amount`, `refunded_amount`, `closed_at`).
- **`EscrowFee(u64)`**: Snapshots the protocol fee rate (bps) at escrow creation time. Ensures future changes to the global `FeeBps` never affect active escrows.
- **`Sequential(u64)`**: Stored boolean flag enforcing strict sequential completion of milestones.
- **`Stream(u64)`**: Stores linear streaming schedule details (`start`, `duration`, `total_amount`, `claimed_amount`) for continuous release escrows.
- **`Payout(u64)`**: Freelancer-specified redirected payout address for milestone disbursements.
- **`Frozen(u64)`**: Administrative lock preventing any mutations during investigation.
- **`Resigned(u64)`**: Flag indicating freelancer voluntary withdrawal from the contract.

### 5.3 Milestone & Deliverable Lifecycle

- **`Submitted(u64, u32)`**: Ledger timestamp recording when the freelancer submitted milestone deliverables for review.
- **`Delivery(u64, u32)`**: SHA-256 hash digest `BytesN<32>` of the deliverable submitted for review.
- **`MilestonePaid(u64, u32)`**: Tracks milestone settlement status.
- **`ReviewWindow(u64)`**: Custom duration (seconds) after milestone submission during which the client can review work before auto-approval becomes eligible.
- **`Terms(u64)`**: SHA-256 hash digest `BytesN<32>` of the agreed contract terms.
- **`MilestoneSpecs(u64)`**: Vector of SHA-256 hash digests `Vec<BytesN<32>>` specifying technical requirements for each milestone.

### 5.4 Financial Accounting & Multi-Funder Solvency

- **`Funders(u64)`**: `Map<Address, i128>` tracking deposits made by individual funding addresses. Allows up to `MAX_FUNDERS = 10` participants to contribute to an escrow and guarantees pro-rata refunds if the contract is cancelled or unfunded.
- **`FundBy(u64)`**: Ledger timestamp deadline by which total deposits must reach the contract amount. Once expired, any participant can trigger `refund_unfunded`.
- **`Liability(Address)`**: Global accumulator tracking the total active token balance owed by the contract for a given token address. Checked on every financial mutation via `assert_solvent` to guarantee reserves match liabilities.
- **`Bonus(u64)`**: Discretionary bonus amount `i128` paid by client to freelancer for an escrow.
- **`BonusTotal(Address)`**: Cumulative bonuses `i128` paid to a freelancer address.

### 5.5 Dispute Arbitration & Mutual Settlement

- **`DisputeBond(u64)`**: Stores `(Address, i128)` recording the dispute opener and posted collateral bond (calculated as `DISPUTE_BOND_BPS = 200` bps = 2%). Returned if the dispute is resolved in the opener's favor; forfeited otherwise.
- **`Settlement(u64)`**: Stores active `SettlementProposal` (`proposer: Address, freelancer_amount: i128`). Either counterparty can propose a settlement split; acceptance by the other counterparty settles the escrow immediately.
- **`Votes(u64)`**: `Map<Address, u32>` recording panel member votes on resolution splits. Once `threshold` matching votes are recorded, the contract settles the dispute and removes `Votes(id)`.

### 5.6 Contract Negotiation & Governance

- **`ChangeOrder(u64)`**: `Vec<i128>` proposed additional milestone amounts submitted by the client. Must be accepted by the freelancer to become active.
- **`Amendment(u64)`**: `Amendment` proposal (`proposer: Address, new_amounts: Vec<i128>, nonce: u32`) allowing reallocation of remaining milestone amounts.
- **`AmendNonce(u64)`**: Monotonic counter `u32` for amendment replay protection.
- **`RoleMember(Role, Address)`**: Access control mapping role enum to account membership boolean.

### 5.7 Reputation & Rating System

- **`Reputation(Address)`**: `Reputation` struct (`total_score: i128, count: i128`) for freelancers.
- **`Outcomes(Address)`**: `Outcomes` tracking historical contract completions, dispute resolutions, and cancellations.
- **`Rating(u64)`**: `RatingRecord` storing client rating (1..=5) and void status.
- **`WeightedRep(Address)`**: Escrow value-weighted reputation record.
- **`RatingWeight(u64)`**: Snapshot of the weight attributed to an escrow rating.
- **`ClientReputation(Address)`**: Counter-party rating for client addresses.
- **`ClientRated(u64)`**: Boolean guard preventing duplicate ratings submitted by a freelancer for the client on an escrow.

### 5.8 Paginated Participant Indexes

- **`ClientEscrows(Address)`**: Total count `u32` of escrows created by a client.
- **`FreelancerEscrows(Address)`**: Total count `u32` of escrows created for a freelancer.
- **`ClientPage(Address, u32)`**: Paginated array of up to 50 escrow IDs (`Vec<u64>`) created by a client.
- **`FreelancerPage(Address, u32)`**: Paginated array of up to 50 escrow IDs (`Vec<u64>`) created for a freelancer.

---

## 6. Storage Verification & Test Coverage

All storage behaviors, serialization guarantees, and TTL maintenance are verified in the test suite ([`contracts/escrow/src/test.rs`](../contracts/escrow/src/test.rs)):

- **`extend_escrow_ttl_keeps_quiet_escrow_and_side_keys_alive`**: Asserts that calling `extend_escrow_ttl` on a quiet escrow bumps `ContractV`, `FundBy`, `Funders`, and instance storage to `>= PERSISTENT_TTL_THRESHOLD`, keeping the escrow reachable past its initial TTL window.
- **`extend_reputation_ttl_keeps_reputation_alive`**: Verifies that calling `extend_reputation_ttl` extends `Reputation` and `WeightedRep` beyond `PERSISTENT_TTL_THRESHOLD`.
- **`party_indices_paginate_in_creation_order`**: Verifies that creating 120 escrows allocates `ClientPage` and `FreelancerPage` across 3 distinct pages (pages of 50) in strictly deterministic creation order.
- **`metadata_writes_bump_ttl`**: Asserts that updating metadata (`Terms`, `MilestoneSpecs`, `Delivery`) bumps persistent TTL.
- **`reads_on_legacy_entry_do_not_mutate_storage`**: Verifies that read operations on legacy `Contract` entries do not trigger writes, while mutations cleanly migrate them to `ContractV`.
