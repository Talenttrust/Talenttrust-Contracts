# Storage Layout Reference — TalentTrust Escrow Contract

This document provides a canonical specification of the `DataKey` enum variants used for managing contract state inside `contracts/escrow`. It dictates how variables are stored within Soroban's state isolation layout, including data types, storage permanence, and lifecycle metrics.

## Storage Map Specification

| DataKey Variant | Stored Value Type | Storage Class | TTL Policy | Access Hierarchy | Status |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `Initialized` | `bool` | Persistent | Instance-bound | Read / Write | **Active** |
| `Admin` | `Address` | Persistent | Instance-bound | Read / Write | **Active** |
| `Paused` | `bool` | Persistent | Instance-bound | Read / Write | **Active** |
| `Emergency` | `bool` | Persistent | Instance-bound | Read / Write | **Active** |
| `NextContractId` | `u32` | Persistent | Instance-bound | Read / Write | **Active** |
| `ReadinessChecklist`| `ReadinessChecklist` | Persistent | Instance-bound | Read / Write | **Active** |
| `Contract(u32)` | `EscrowContract` | Persistent | Extended on access| Entrypoint API | **Active** |
| `MilestoneReleased(u32, u32)` | `bool` | Persistent | Extended on access| Entrypoint API | **Active** |
| `ReleasedAmount(u32)`| `i128` | Persistent | Extended on access| Entrypoint API | **Active** |
| `PendingReputation(Address)` | `u32` | Persistent | Extended on access| Entrypoint API | **Active** |
| `ReputationIssued(u32)`| `bool` | Persistent | Extended on access| Entrypoint API | **Active** |
| `MilestoneApprovals` | `Map<u32, bool>` | Temporary | Ephemeral | Unused | ⚠️ *Declared-But-Unused* |
| `PendingClientMigration` | `Address` | Temporary | Ephemeral | Unused | ⚠️ *Declared-But-Unused* |
| `ProtocolFeeBps` | `u32` | Persistent | Instance-bound | Unused | ⚠️ *Declared-But-Unused* |
| `AccumulatedProtocolFees` | `i128` | Persistent | Instance-bound | Unused | ⚠️ *Declared-But-Unused* |

---

## State & Lifecycle Constraints

### 1. Active Infrastructure Keys
* **`Initialized` / `Admin` / `Paused` / `Emergency`**
    * **Description:** Operational management variables that orchestrate contract lock states and multi-tier access pathways.
    * **Storage Lifespan:** Handled under default `Persistent` storage rules. They share instance lifecycles to guarantee contract configuration properties remain intact as long as the instance exists.

### 2. Operational Escrow Data
* **`Contract(u32)` / `MilestoneReleased(u32, u32)` / `ReleasedAmount(u32)`**
    * **Description:** Tracks active engagement funds, distribution records, and execution checkpoints for specific active escrows.
    * **Storage Lifespan:** `Persistent`. Every mutation or validation via state entrypoints invokes automated extensions specified in `ttl.rs` to secure data preservation during extended payment cycles.

### 3. Reputation Auditing States
* **`PendingReputation(Address)` / `ReputationIssued(u32)`**
    * **Description:** Bookkeeping indices capturing un-issued tokens and completion certificates for network participants.
    * **Storage Lifespan:** `Persistent`. Preserved explicitly to guarantee deterministic chronological processing when users harvest pending system values.

---

## Declared-But-Unused Storage Keys

The following keys exist within the `DataKey` definition block but do not possess operational access pathways in the current execution loop. 

### `MilestoneApprovals`
* **Intended Type:** `Map<u32, bool>`
* **Target Lifecycle:** `Temporary`
* **Tracking Issue Reference:** *[Issue #104: Implementation of Ephemeral Multi-Sig Milestone Sign-Offs]*

### `PendingClientMigration`
* **Intended Type:** `Address`
* **Target Lifecycle:** `Temporary`
* **Tracking Issue Reference:** *[Issue #112: Upgradable Client Context and Contract Migration Protocol]*

### `ProtocolFeeBps` / `AccumulatedProtocolFees`
* **Intended Type:** `u32` / `i128`
* **Target Lifecycle:** `Persistent`
* **Tracking Issue Reference:** *[Issue #125: Protocol-Wide Revenue Extraction and Fee Distributor Framework]*