# Storage Layout Reference — TalentTrust Escrow Contract

This document maps the currently implemented `DataKey` storage used by
`contracts/escrow/src/lib.rs`. A fuller key-by-key reference, including
declared-but-unused keys, is tracked in
[#342](https://github.com/Talenttrust/Talenttrust-Contracts/issues/342).

## Live Storage Keys

These participant indexes are **append-only**: every `create_contract` appends the new id to the appropriate index vectors.
The contract list readers (`list_contracts_by_participant`) are therefore consistent with contract creation order.



| Key | Value | Written by |
| --- | --- | --- |
| `Initialized` | `bool` | `initialize` |
| `Admin` | `Address` | `initialize` |
| `Paused` | `bool` | `pause`, `unpause`, emergency controls |
| `Emergency` | `bool` | emergency controls |
| `Contract(id)` | `EscrowContractData` | create/deposit/release/reputation/cancel |
| `NextContractId` | `u32` | `create_contract` |
| `ReputationIssued(id)` | `bool` | `issue_reputation` |
| `PendingReputationCredits(address)` | `u32` | final release, `issue_reputation` |
| `Reputation(address)` | `ReputationRecord` | `issue_reputation` |
| `Finalization(id)` | `FinalizationRecord` | `finalize_contract` |
| `ReadinessChecklist` | `ReadinessChecklist` | initialize and emergency controls |
| `ClientContracts(address)` | `Vec<u32>` | create_contract |
| `FreelancerContracts(address)` | `Vec<u32>` | create_contract |

## Declared But Not Live

These keys are declared in `types.rs` but no public entrypoint currently uses
them as a complete feature:

- `MilestoneApprovals`
- `PendingClientMigration`
- `ProtocolFeeBps`
- `AccumulatedProtocolFees`

Protocol fee implementation is tracked in
[#313](https://github.com/Talenttrust/Talenttrust-Contracts/issues/313) and
[#314](https://github.com/Talenttrust/Talenttrust-Contracts/issues/314).

## Milestone Released State — Single Source of Truth

`release_milestone` sets `milestone.released = true` inside the persisted
`Vec<Milestone>` stored under `(DataKey::Contract(id), "milestones")`.

`summarize_contract` (called by `finalize_contract`) derives
`released_milestone_count` by iterating that same vector and counting
`ms.released == true`. There is **no** separate `DataKey::MilestoneReleased`
key — that variant was removed in fix [#416] because it was never written,
causing `released_milestone_count` to always report zero in finalization
summaries.

Read and write path are now identical: the milestone vector is the sole
authority for released state.

### 3. Reputation Auditing States
* **`PendingReputation(Address)` / `ReputationIssued(u32)`**
    * **Description:** Bookkeeping indices capturing un-issued tokens and completion certificates for network participants.
    * **Storage Lifespan:** `Persistent`. Preserved explicitly to guarantee deterministic chronological processing when users harvest pending system values.

## Contract-Id Allocation Invariants

`NextContractId` is the monotonic counter used by `create_contract` to assign
unique, gap-free ids.  The allocation path in
`contracts/escrow/src/create_contract.rs` upholds the following invariants:

1. **Single allocation per create.** `next_contract_id` is called exactly once
   per `create_contract` invocation.  The first call reads the counter and
   checks the target slot; there is no second call that could shadow the first
   or trigger a double collision check.

2. **Atomic counter advance.** `bump_next_contract_id` writes `id + 1` to
   persistent storage only after the contract and milestone entries have been
   persisted.  If the write fails, the counter is not advanced.

3. **Overflow protection.**  `bump_next_contract_id` uses `checked_add(1)`.
   If the counter is already `u32::MAX`, the function panics with
   `Error::ContractIdOverflow` before writing anything.  The counter is left
   unchanged.

4. **Collision protection.**  `next_contract_id` reads the candidate id and
   immediately checks whether a `Contract` entry already exists at that key.
   If one does, it panics with `Error::ContractIdCollision`.  The counter is
   left unchanged.

5. **Sequential, gap-free ids.**  Because the counter starts at `1` and is
   advanced by exactly 1 on every successful create, allocated ids form a
   contiguous sequence `1, 2, 3, …`.  Id `0` is never issued and is reserved
   as a sentinel "not found" value for off-chain indexers.

6. **No id reuse.**  Once a `Contract(id)` entry is written to persistent
   storage it is never deleted by any existing entrypoint, so the collision
   check in `next_contract_id` permanently blocks reuse of that id.

These invariants are verified by the test suite in
`contracts/escrow/src/test/contract_id_allocation.rs`.

---

- Contract ids are monotonically assigned from `NextContractId`.
- Milestone amounts and participant addresses are immutable after creation.
- `total_deposited`, `released_amount`, and `refunded_amount` are checked after
  balance-changing operations.
- A milestone release flag can move from absent/false to true only once.
- Reputation issuance is guarded by `ReputationIssued(contract_id)`.
