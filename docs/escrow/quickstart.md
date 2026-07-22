# Escrow Integrator Quickstart

A copy-pasteable, end-to-end walkthrough that takes the **TalentTrust escrow
contract** from a freshly cloned repository to a **funded, approved, and
released** milestone on Stellar testnet.

Every command in this guide uses the same identifier paths, signer identities,
and amount conventions that the escrow contract enforces through its
`ReleaseAuthorization` enum, milestone vectors, and authentication guards
defined in [`contracts/escrow/src/lib.rs`](../../contracts/escrow/src/lib.rs).

**Audience:** integrators wiring up the escrow contract into a backend, a CLI
tool, or a frontend SDK.

**Source of truth:** every example below is cross-checked against the
live public surface listed in [`abi-reference.md`](abi-reference.md) and the
entrypoint-level NatSpec in [`contracts/escrow/src/lib.rs`](../../contracts/escrow/src/lib.rs).

---

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Quickstart Map](#quickstart-map)
3. [Step 1 — Build the WASM](#step-1--build-the-wasm)
4. [Step 2 — Deploy the escrow contract](#step-2--deploy-the-escrow-contract)
5. [Step 3 — Initialize the admin](#step-3--initialize-the-admin)
6. [Step 4 — Bind a settlement token (SAC)](#step-4--bind-a-settlement-token-sac)
7. [Step 5 — Create an escrow contract](#step-5--create-an-escrow-contract)
8. [Step 6 — Deposit funds](#step-6--deposit-funds)
9. [Step 7 — Approve milestone release](#step-7--approve-milestone-release)
10. [Step 8 — Release a milestone](#step-8--release-a-milestone)
11. [Domain-specific authorization modes](#domain-specific-authorization-modes)
12. [Argument encoding reference](#argument-encoding-reference)
13. [Troubleshooting: EscrowError codes](#troubleshooting-escrowerror-codes)
14. [Security notes for integrators](#security-notes-for-integrators)
15. [NatSpec cross-check](#natspec-cross-check)
16. [Where to go next](#where-to-go-next)

---

## Prerequisites

| Tool | Minimum | Used for | Install |
| --- | --- | --- | --- |
| **Rust** | `1.75+` | Building the `[no_std]` Soroban contract to WASM | [`rustup`](https://rustup.rs) |
| **`wasm32-unknown-unknown` target** | latest | Compiling to Soroban-compatible WASM | `rustup target add wasm32-unknown-unknown` |
| **`stellar` CLI** | `22.x+` | Deploying WASM, invoking entrypoints, signing auth | [`stellar-cli` releases](https://github.com/StellarCN/stellar-cli) |
| **A funded testnet identity** | ≥ 100 XLM | Pays rent and signing fees | `stellar network fund <addr> --network testnet` |
| **Three test identities** | `alice` (client), `bob` (freelancer), `carol` (arbiter/admin as needed) | Each party signs its own calls | `stellar keys generate <name>` |

Verify the toolchain:

```bash
rustc --version            # ≥ rustc 1.75.0
cargo --version
rustup target list --installed | grep wasm32
stellar --version          # ≥ 22.0.0
stellar network ls         # "testnet" should appear
```

> **Production note:** every example below assumes `--network testnet`. Swap
> to `--network mainnet` after the integration has been fully tested.

---

## Quickstart Map

| Step | Entrypoint | Purpose |
| --- | --- | --- |
| 1 | *(build only)* | Produce `escrow.wasm` |
| 2 | *(deploy only)* | Publish the WASM, capture the contract id |
| 3 | `initialize(admin)` | One-time admin bootstrap |
| 4 | `bind_settlement_token(admin, token)` | Bind the Stellar Asset Contract used for custody (`set_settlement_token` is a deprecated delegate) |
| 5 | `create_contract(client, freelancer, arbiter?, milestones, release_authorization)` | Allocate a fresh escrow id |
| 6 | `deposit_funds(contract_id, caller, amount)` | Move SAC balance from client to escrow |
| 7 | `approve_milestone_release(contract_id, caller, milestone_index)` | Record an approval (per `ReleaseAuthorization`) |
| 8 | `release_milestone(contract_id, caller, milestone_index)` | Move SAC balance from escrow to freelancer |

---

## Step 1 — Build the WASM

Build the escrow contract's release artifact.

```bash
cargo build --target wasm32-unknown-unknown --release -p escrow
```

The compiled WASM is written to:

```text
target/wasm32-unknown-unknown/release/escrow.wasm
```

> The `escrow` crate uses `#![no_std]` and allocates its WASM target through
> the standard Soroban toolchain. No additional rebuild flags are required.

---

## Step 2 — Deploy the escrow contract

Upload the WASM to the target network and capture the resulting contract id.

```bash
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/escrow.wasm \
  --source alice \
  --network testnet \
  > escrow_id.txt

ESCROW_ID=$(cat escrow_id.txt)
echo "Escrow contract deployed at: $ESCROW_ID"
```

> **Why `alice`?** The deploying identity pays the upload fee, but does
> **not** become the admin — admin binding happens in **Step 3**.

---

## Step 3 — Initialize the admin

Bind the operational admin that controls pause, emergency, fee-withdrawal,
and settlement-token binding. This must be done **once** for the lifetime of
the contract. A second call panics with `Error::AlreadyInitialized`.

```bash
stellar contract invoke \
  --id "$ESCROW_ID" \
  --source carol \
  --network testnet \
  -- initialize \
  --admin carol
```

**Returns:** `true` on success.

**Authorization:** `carol.require_auth()` runs at the start of `initialize`.
The identity you use **here** is the admin that will later drive
`pause()`, `unpause()`, `set_protocol_fee_bps`, and
`withdraw_protocol_fees`.

**Event:** `("init", "admin_set")` with `(admin, timestamp)`.

Re-running `initialize` against the same contract id returns
`Error::AlreadyInitialized`. The error is surfaced by the Soroban host as a
contract panic — see [Troubleshooting](#troubleshooting-escrowerror-codes).

---

## Step 4 — Bind a settlement token (SAC)

The escrow contract holds a **Stellar Asset Contract (SAC)** balance for
every contract it funds. Until the admin binds a SAC address, `deposit_funds` and `release_milestone`
abort with a host-level panic (`"Settlement token not set"`) because the
contract calls `Self::read_settlement_token(&env).expect(...)` before issuing
the SAC `transfer`. There is no SCVal error variant for this case; the
state is left unchanged.

### 4a. Deploy a SAC for the integration

If you do not already have a SAC, deploy one with the Stellar asset issuing
account that your integration controls:

```bash
stellar contract deploy \
  --asset native \
  --source alice \
  --network testnet \
  > sac_id.txt

SAC_ID=$(cat sac_id.txt)
```

### 4b. Bind the SAC to the escrow contract

```bash
stellar contract invoke \
  --id "$ESCROW_ID" \
  --source carol \
  --network testnet \
  -- bind_settlement_token \
  --admin carol \
  --token "$SAC_ID"
```

**Returns:** `true` on success.

**Security:**

- `admin.require_auth()` enforces that the operational admin signs this call.
- The function performs `require_initialized` first, so calling it before
  `initialize` panics with `NotInitialized`.
- Single-use write-once binding: a second binding attempt via `bind_settlement_token`
  or deprecated delegate `set_settlement_token` panics with `SettlementTokenAlreadyBound`.
- The escrow contract will only ever move funds through the bound SAC
  address held in `DataKey::SettlementToken`.

> **Trustline requirement:** the client identity (`alice` in this guide)
> must hold a trustline to the SAC before `deposit_funds` succeeds.

---

## Step 5 — Create an escrow contract

Allocate a new escrow contract. This is the only entrypoint the **client**
must call to set the parties, milestones, and authorization policy.

```bash
stellar contract invoke \
  --id "$ESCROW_ID" \
  --source alice \
  --network testnet \
  -- create_contract \
  --client alice \
  --freelancer bob \
  --arbiter "$ARBITER_ADDRESS" \
  --milestones "[1000000000, 2000000000]" \
  --release_authorization '"ClientOnly"'
```

**Returns:** the new `contract_id` (a `u32` allocated atomically). Capture
this value — every subsequent per-contract call refers to it.

**Authorization:** `alice.require_auth()`. The client is the only identity
authorized to create the contract.

**Argument encoding:**

- `--milestones` is a JSON array of `i128` stroop amounts. `1_000_000_000`
  stroops = `0.1 XLM`; the contract treats amounts as dollars-and-cents-free
  integer values.
- `--release_authorization` accepts the variant name as a JSON string,
  e.g. `"ClientOnly"`, `"ClientAndArbiter"`, `"ArbiterOnly"`, `"MultiSig"`.
  Internally these map to `0`, `1`, `2`, `3` respectively — see
  [Argument encoding reference](#argument-encoding-reference).

**Validation rules:**

| Constraint | Source | Error on violation |
| --- | --- | --- |
| `client != freelancer` | `create_contract` inv guard | `InvalidParticipants` |
| `arbiter` provided iff `ArbiterOnly` or `ClientAndArbiter` | `create_contract` | `MissingArbiter` |
| `arbiter != client && arbiter != freelancer` | `create_contract` | `InvalidArbiter` |
| `milestones` not empty | `create_contract` | `EmptyMilestones` |
| All `milestones[i] > 0` | `create_contract` | `InvalidMilestoneAmount` |

**Event:** `("created", contract_id)`.

Capture the output for use in the next steps:

```bash
CONTRACT_ID=$(stellar contract invoke \
  --id "$ESCROW_ID" \
  --source alice \
  --network testnet \
  -- create_contract \
  --client alice \
  --freelancer bob \
  --arbiter "$ARBITER_ADDRESS" \
  --milestones "[1000000000, 2000000000]" \
  --release_authorization '"ClientOnly"' \
  --output json | jq -r .)
```

(Above shell pattern is illustrative; capture the SCVal-decoded output of the
final contract id using whatever JSON parsing your shell prefers.)

---

## Step 6 — Deposit funds

Pull SAC tokens from the client to the escrow contract. The escrow contract
performs a real `token::Client::transfer(client, escrow, amount)` **before**
it mutates `funded_amount`, so a failed SAC transfer leaves contract
accounting unchanged.

```bash
AMOUNT=3000000000   # 0.3 XLM in stroops — covers both milestones

stellar contract invoke \
  --id "$ESCROW_ID" \
  --source alice \
  --network testnet \
  -- deposit_funds \
  --contract_id "$CONTRACT_ID" \
  --caller alice \
  --amount "$AMOUNT"
```

**Returns:** `true` on success.

**Authorization:** `alice.require_auth()`. The escrow's `Caller.require_auth`
call defends against replayed unsigned deposits.

**State transitions:** the contract transitions from `Created` →
`PartiallyFunded` (after this single deposit) or `Created` → `Funded` (when
the deposit exactly matches the milestone sum). Partial `Incremental`
deposits are supported by calling `deposit_funds` repeatedly; an
`ExactTotal` deposit must equal the milestone sum exactly.

**Error paths:**

| Trigger | Code |
| --- | --- |
| `amount <= 0` | `AmountMustBePositive` |
| `caller` is not the stored `client` | `UnauthorizedRole` |
| Unknown `contract_id` | `ContractNotFound` |
| Status is not `Created` / `PartiallyFunded` | `InvalidState` |
| Total deposit would exceed milestone sum | `InvalidDepositAmount` |
| SAC transfer fails | Host error: `TransactionFailed` (contract state unchanged) |

**Event:** `("deposited", contract_id)` with payload
`(caller, amount, funded_amount, total, settlement_token)`.

---

## Step 7 — Approve milestone release

Record an approval so the `release_milestone` gate is satisfied. The
required approver(s) depend on the contracts' `ReleaseAuthorization` mode
configured in **Step 5**.

For a **`ClientOnly`** contract:

```bash
stellar contract invoke \
  --id "$ESCROW_ID" \
  --source alice \
  --network testnet \
  -- approve_milestone_release \
  --contract_id "$CONTRACT_ID" \
  --caller alice \
  --milestone_index 0
```

**Returns:** `true` on success.

**Authorization:** `caller.require_auth()`. The contract also enforces that
the caller is allowed to approve under the active `ReleaseAuthorization`:

| Mode | Allowed approvers |
| --- | --- |
| `ClientOnly` | client |
| `ArbiterOnly` | arbiter |
| `ClientAndArbiter` | client **or** arbiter |
| `MultiSig` | client **and** freelancer |

**Storage:** approvals are written to **temporary** storage with a TTL of
approximately 7 days (`PENDING_APPROVAL_TTL_LEDGERS = LEDGERS_PER_DAY * 7`).
Calling `approve_milestone_release` again on the same milestone from the
**same** party panics with `AlreadyApproved`. Calling again from a different
party **extends the TTL** but does not duplicate the approval.

Read back the current approval state:

```bash
stellar contract invoke \
  --id "$ESCROW_ID" \
  --source alice \
  --network testnet \
  -- get_milestone_approvals \
  --contract_id "$CONTRACT_ID" \
  --milestone_index 0
```

**Returns:** `Option<MilestoneApprovals>` —
`Some({ client_approved: true, freelancer_approved: false, arbiter_approved: false })`
or `None` if no live approval exists.

---

## Step 8 — Release a milestone

Execute the payout. The escrow contract performs the SAC transfer to the
freelancer **before** marking the milestone as released, so a failed payout
leaves state untouched.

```bash
stellar contract invoke \
  --id "$ESCROW_ID" \
  --source alice \
  --network testnet \
  -- release_milestone \
  --contract_id "$CONTRACT_ID" \
  --caller alice \
  --milestone_index 0
```

**Returns:** `true` on success.

**Authorization:** `caller.require_auth()`. The caller must be authorized
under the contract's `ReleaseAuthorization` mode.

**Pre-conditions (checked in this exact order):**

1. `require_not_paused(contract)` — fails with `ContractPaused` /
   `EmergencyActive` if either flag is set.
2. Contract status is `Accepted` — else `InvalidState`.
3. `caller` matches the active mode's allowed release caller — else
   `UnauthorizedRole`.
4. `milestone_index < milestones.len()` — else `IndexOutOfBounds`.
5. Milestone is unreleased and unrefunded — else
   `MilestoneAlreadyReleased` / `AlreadyRefunded`.
6. `approvals::check_approvals(...)` succeeds — else `InsufficientApprovals`
   (or `ApprovalExpired` when a TTL has elapsed).
7. `milestone.funded_amount >= milestone.amount` — else `InsufficientFunds`.
8. Aggregate `available_balance >= milestone.amount` — else
   `InsufficientFunds`.
9. SAC `transfer(escrow, freelancer, milestone.amount - fee)` succeeds —
   else host error and milestone state unchanged.

**Side effects on success:**

| Field | Change |
| --- | --- |
| `milestone.released` | `false → true` |
| `contract.released_amount` | `+= milestone.amount` |
| `DataKey::AccumulatedProtocolFees` | `+= fee` (when `fee_bps > 0`) |
| `contract.status` | `Completed` if all milestones now terminal |
| `DataKey::MilestoneApprovals(contract_id, milestone_index)` | cleared |

**Events:**

| Topic | Payload |
| --- | --- |
| `("mlstn_rls", contract_id)` | `(milestone_index, amount, fee, new_released_amount, caller, timestamp)` |
| `("rep_issd", contract_id)` *(pending credit)* | _(none — set internally)_ |
| `("ctrct_cmp", contract_id)` *(only when the release completes the contract)* | `(caller, timestamp)` |

---

## Domain-specific authorization modes

The four `ReleaseAuthorization` modes drive **who** can approve **and who**
can release a milestone. Pick the mode that matches the operational reality
of the engagement.

### `ClientOnly` (u32 discriminant `0`)

- **Approvers:** client only
- **Releaser:** client only
- **Arbiter:** not required at contract creation
- **Use case:** straightforward client-controlled engagement where the
  client accepts work and pays directly.

Example (Steps 5–8 above use this mode).

### `ClientAndArbiter` (u32 discriminant `1`)

- **Approvers:** client **or** arbiter (one is enough)
- **Releaser:** client **or** arbiter
- **Arbiter:** required at contract creation
- **Use case:** engagements with an optional escalation path to a third
  party if the client is unresponsive.

```bash
# Step 5: arbiter is required at creation
stellar contract invoke --id "$ESCROW_ID" --source alice --network testnet \
  -- create_contract --client alice --freelancer bob --arbiter "$ARBITER_ADDRESS" \
  --milestones "[1000000000, 2000000000]" \
  --release_authorization '"ClientAndArbiter"'

# Step 7: arbiter can approve
stellar contract invoke --id "$ESCROW_ID" --source "$ARBITER_ADDRESS" --network testnet \
  -- approve_milestone_release \
  --contract_id "$CONTRACT_ID" --caller "$ARBITER_ADDRESS" --milestone_index 0

# Step 8: client (or arbiter) can release
stellar contract invoke --id "$ESCROW_ID" --source alice --network testnet \
  -- release_milestone \
  --contract_id "$CONTRACT_ID" --caller alice --milestone_index 0
```

### `ArbiterOnly` (u32 discriminant `2`)

- **Approvers:** arbiter only
- **Releaser:** arbiter only
- **Arbiter:** required at contract creation
- **Use case:** escrow-as-custody where an external service signs payouts
  based on off-chain attestation.

```bash
# Step 5: arbiter is required
stellar contract invoke --id "$ESCROW_ID" --source alice --network testnet \
  -- create_contract --client alice --freelancer bob --arbiter "$ARBITER_ADDRESS" \
  --milestones "[1000000000, 2000000000]" \
  --release_authorization '"ArbiterOnly"'

# Step 7: only the arbiter can approve
stellar contract invoke --id "$ESCROW_ID" --source "$ARBITER_ADDRESS" --network testnet \
  -- approve_milestone_release \
  --contract_id "$CONTRACT_ID" --caller "$ARBITER_ADDRESS" --milestone_index 0

# Step 8: only the arbiter can release
stellar contract invoke --id "$ESCROW_ID" --source "$ARBITER_ADDRESS" --network testnet \
  -- release_milestone \
  --contract_id "$CONTRACT_ID" --caller "$ARBITER_ADDRESS" --milestone_index 0
```

### `MultiSig` (u32 discriminant `3`)

- **Approvers:** client **and** freelancer (both required)
- **Releaser:** client **or** freelancer (after both have approved)
- **Arbiter:** optional
- **Use case:** mutual-signature releases; protects against unilateral
  payout by either party.

```bash
# Step 5: arbiter optional — pass `--arbiter` with explicit JSON null.
# Soroban `Option<Address>` is bound to `None` when you pass the literal
# JSON scalar `null`. Empty strings are a parser pitfall; prefer `null`.
stellar contract invoke --id "$ESCROW_ID" --source alice --network testnet \
  -- create_contract \
  --client alice \
  --freelancer bob \
  --arbiter null \
  --milestones "[1000000000, 2000000000]" \
  --release_authorization '"MultiSig"'

# Step 7a: client approves
stellar contract invoke --id "$ESCROW_ID" --source alice --network testnet \
  -- approve_milestone_release \
  --contract_id "$CONTRACT_ID" --caller alice --milestone_index 0

# Step 7b: freelancer approves (now both flags are set)
stellar contract invoke --id "$ESCROW_ID" --source bob --network testnet \
  -- approve_milestone_release \
  --contract_id "$CONTRACT_ID" --caller bob --milestone_index 0

# Step 8: either party can release
stellar contract invoke --id "$ESCROW_ID" --source bob --network testnet \
  -- release_milestone \
  --contract_id "$CONTRACT_ID" --caller bob --milestone_index 0
```

---

## Argument encoding reference

### `ReleaseAuthorization` (enum `u32` discriminant)

`stellar contract invoke` accepts the variant name as a JSON-encoded
**string** to keep commands readable; the CLI internally maps it to the
SCVal enum discriminant that the Soroban host expects.

| Variant | JSON string | SCVal discriminant (u32) |
| --- | --- | --- |
| `ClientOnly` | `"ClientOnly"` | `0` |
| `ClientAndArbiter` | `"ClientAndArbiter"` | `1` |
| `ArbiterOnly` | `"ArbiterOnly"` | `2` |
| `MultiSig` | `"MultiSig"` | `3` |

The discriminant values come from the canonical enum definition in
`contracts/escrow/src/types.rs`:

```rust
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseAuthorization {
    ClientOnly = 0,
    ClientAndArbiter = 1,
    ArbiterOnly = 2,
    MultiSig = 3,
}
```

> **Tip:** if a downstream tool requires the raw integer, pass the
> corresponding `u32` directly. Both forms are accepted by the Stellar CLI.

### `Vec<i128>` for milestones

Milestone amounts are passed as a JSON array of signed 128-bit integers
measured in **stroops** (1 XLM = 10 000 000 stroops).

```bash
--milestones "[1000000000, 2000000000]"
```

Restrictions enforced by `create_contract`:

- The vector must be non-empty — empty => `EmptyMilestones`.
- Every element must be strictly positive — non-positive => `InvalidMilestoneAmount`.

### Address formatting

Use the standard Stellar `G...` 56-character ed25519 public-key form
throughout. Contract addresses returned by `stellar contract deploy` are
`C...` 56-character values — these are interchangeable as long as the
identity holds the appropriate trustline or authorization.

### Amounts (`i128`)

- All amounts are **stroops**, signed `i128`. Do **not** convert to floating
  point — the contract performs checked `i128` arithmetic and an overflow
  triggers `Error::InsufficientFunds` (via `safe_add_amounts` /
  `safe_subtract_amounts`).
- Use string literals in shell scripts to avoid 53-bit float truncation:

  ```bash
  AMOUNT="3000000000"   # 0.3 XLM in stroops
  ```

### Optional arbiter

To create a contract **without** an arbiter, omit `--arbiter` (the CLI
binds `None`). For `ArbiterOnly` or `ClientAndArbiter` modes the arbiter is
**required** — see [Troubleshooting](#troubleshooting-escrowerror-codes).

---

## Troubleshooting: EscrowError codes

Every entrypoint panic surfaces as a host-level transaction failure with the
error **name** (and a corresponding SCVal discriminant). The reference table
below maps the most common user-facing failures to actionable remediation.

| Repo error (`Error::Variant`) | Discriminant (`u32`) | Most likely entrypoint | Trigger | Fix |
| --- | --- | --- | --- | --- |
| `AlreadyInitialized` | `34` | `initialize` | A second `initialize` against the same contract id. | The admin is already bound. Use `get_admin` to confirm; do **not** re-deploy unless the existing instance is corrupted. |
| `NotInitialized` | `36` | Mutating entrypoints | `initialize` was never called. | Bind an admin once via **Step 3**. |
| `ContractNotFound` | `10` | `deposit_funds`, `release_milestone`, all getters | The `contract_id` was never allocated or has been finalized and evicted. | Confirm via `get_contract(<id>)`. Recreate via **`create_contract`** if `None`. |
| `UnauthorizedRole` | `11` | `deposit_funds`, `release_milestone`, `approve_milestone_release`, etc. | The signed `--source` identity does not match the role required (e.g. releasing as `--freelancer` on a `ClientOnly` contract). | Switch `--source` to the role the contract's `ReleaseAuthorization` accepts. |
| `MissingArbiter` | `12` | `create_contract` | `ArbiterOnly` or `ClientAndArbiter` was selected without providing `--arbiter`. | Re-run with `--arbiter <G...>` chosen up-front. |
| `InvalidArbiter` | `13` | `create_contract` | `--arbiter` equals `--client` or `--freelancer`. | Use a distinct third-party address. |
| `InvalidParticipants` | `14` | `create_contract` | `--client == --freelancer`. | Use a distinct freelancer address. |
| `EmptyMilestones` | `25` | `create_contract` | `--milestones` is `[]`. | Pass at least one positive amount. |
| `InvalidMilestoneAmount` | `26` | `create_contract` | A milestone amount is `<= 0`. | Use strictly positive stroops. |
| `AmountMustBePositive` | `15` | `deposit_funds`, `withdraw_protocol_fees` | `amount <= 0`. | Pass a strictly positive stroop amount. |
| `InvalidDepositAmount` | `32` | `deposit_funds` | Deposit sum exceeds milestone total. | Match the milestone sum exactly (or split into partial `Incremental` deposits). |
| `InvalidState` | `16` | Any lifecycle entrypoint | Status is incompatible with the call (e.g. `release_milestone` while `Created`). | Drive the contract through the expected sequence: `Created → Funded → Completed`. |
| `ContractPaused` | `37` | Any mutating entrypoint | Admin has called `pause()`. | Wait for `unpause()` or escalate to admin. |
| `EmergencyActive` | `38` | Any mutating entrypoint (except `resolve_emergency`) | Admin has called `activate_emergency_pause()`. | Only `resolve_emergency()` can clear this — escalate to admin. |
| `IndexOutOfBounds` | `3` | `release_milestone`, `refund_unreleased_milestones`, `get_work_evidence` | `--milestone_index >= milestones.len()`. | Enumerate milestones via `get_milestones(contract_id)` and pick a valid index. |
| `MilestoneAlreadyReleased` | `17` | `release_milestone` | The milestone's `released` flag is already `true`. | Use `get_milestones(...)` to confirm state; release a different milestone. |
| `AlreadyApproved` | `18` | `approve_milestone_release` | The same party re-approves a milestone recorded in temporary storage. | Different party can still approve (TTL is also extended). |
| `InsufficientApprovals` | `20` | `release_milestone` | `check_approvals(...)` did not have the requisite flags set for the active `ReleaseAuthorization`. | Call `approve_milestone_release` from the missing approver and retry within the 7-day TTL. |
| `AlreadyRefunded` | `8` | `release_milestone`, `refund_unreleased_milestones` | The milestone has been refunded. | Confirm via `get_milestones(<id>)`; refunds and releases are mutually exclusive. |
| `InsufficientFunds` | `9` | `release_milestone`, `refund_unreleased_milestones` | Either the milestone or the aggregate balance is under-funded. | Deposit the missing amount via `deposit_funds` — see **Step 6**. |
| `AlreadyFinalized` | `46` | `release_milestone`, `deposit_funds`, `refund_unreleased_milestones`, `approve_milestone_release` | A `finalize_contract(contract_id, finalizer)` record exists. | Finalization is one-way; recreate the contract if needed. |
| `ContractIdCollision` / `ContractIdOverflow` | `27` / `28` | `create_contract` | Internal allocator exhaustion. | Re-deploy the contract from a fresh `stellar contract deploy`. |
| `EvidenceTooLong` | `47` | `submit_work_evidence` | Evidence string exceeds 256 bytes. | Trim the deliverable reference; IPFS CIDs are typically well under the cap. |
| `SelfRating` | `39` | `issue_reputation` | `client == freelancer` on the contract. | Reputation cannot be issued on self-funded contracts. |

> A full mapping (with intent vs. live reachability and helper-module
> cross-references) lives in [`ERROR_CATALOG.md`](ERROR_CATALOG.md).

### Common CLI / Stellar-host errors

| Symptom | Probable cause | Fix |
| --- | --- | --- |
| `TransactionFailed` with no SCVal error | Underlying SAC `transfer` failed (insufficient trustline balance, missing auth, contract auth missing). | Confirm `--source` holds a trustline to the bound SAC and has enough balance. Retry. |
| `MissingValue` for `--contract_id` | Argument name mismatch with the entrypoint. | Match the names printed by `stellar contract bindings json --wasm ...` — they mirror each `pub fn`'s parameter list. |
| `HostError: Authorization` | A `require_auth()` call was signed by the wrong identity. | Re-run with the role expected by the entrypoint. |
| `Bad union switch` / SCVal decode error | A `--release_authorization` literal was passed as a raw number outside the 0–3 range. | Use the JSON string form (`"ClientOnly"`, `"ClientAndArbiter"`, `"ArbiterOnly"`, `"MultiSig"`). |

---

## Security notes for integrators

1. **Never sign on behalf of an admin address you do not control.** Every
   admin-side call (`initialize`, `bind_settlement_token` / `set_settlement_token`, `set_protocol_fee_bps`,
   `pause`, `unpause`, `activate_emergency_pause`, `resolve_emergency`,
   `withdraw_protocol_fees`) requires `admin.require_auth()`. Integrators
   must integrate with the admin's signing environment — never embed a
   secret key in CI scripts, web clients, or shared infrastructure.

2. **Trustlines matter.** Before `deposit_funds` succeeds, the client must
   hold a trustline to the bound SAC. On testnet, the `native` asset works
   out of the box; for issued assets, add a trustline first.

3. **Reuse of milestones is impossible.** A milestone can be released **or**
   refunded exactly once — never both. The contract enforces this via
   `MilestoneAlreadyReleased` / `AlreadyRefunded` checks
   ([`contracts/escrow/src/lib.rs`](../../contracts/escrow/src/lib.rs)).

4. **Approvals are time-bounded.** `approve_milestone_release` writes to
   Soroban **temporary** storage with a TTL of ~7 days. If releases are
   not called within that window, approvals must be re-recorded.

5. **Pause and emergency halt all mutations.** Read-only queries
   (`get_contract`, `get_milestone_approvals`, ...) continue to work, but
   every mutating call returns `ContractPaused` / `EmergencyActive`.

6. **Finalize is one-way.** `finalize_contract` writes an immutable record
   that blocks every contract-specific mutation thereafter. Coordinate
   finalization with off-chain systems before invoking it.

7. **Authorization modes are immutable.** A contract's `release_authorization`
   is bound at `create_contract` time and cannot be changed
   retroactively. Pick carefully during **Step 5**.

8. **Use the read-only getters for status checks.** Avoid reading state
   from events alone — events are best-effort signal data, not the
   source of truth. Call `get_contract_summary(contract_id)` for a
   structured summary that includes `status`, `funded_amount`,
   `released_amount`, `refundable_balance`, and a per-milestone table.

9. **Don't redeploy for state recovery.** When the entrypoint guard
   rejects a call, fix the input — not the deployment. Re-deployment
   creates a fresh `NextContractId` counter and orphans existing
   contract ids.

10. **Don't pass raw private keys to the CLI in CI.** Use
    `stellar keys show <name>`-style flows in a managed signer
    environment. On local machines, prefer `secret` keys held by the
    OS keychain rather than `.env` files.

11. **Admin rotation is possible; re-initialization is not.** `initialize`
    is single-use (a second call returns `AlreadyInitialized`), but the
    operational admin address itself can be rotated through the two-step
    `propose_governance_admin(proposed)` + `accept_governance_admin` flow
    guarded by `ADMIN_ROTATION_MIN_DELAY_LEDGERS` timelock. Plan signing
    infrastructure to survive an admin rotation rather than baking a
    single admin key into a long-lived CI service.

---

## NatSpec cross-check

Every example in this guide maps back to a NatSpec-documented public
function. The cross-check below is the authoritative entrypoint → step
index.

| Step | Entrypoint | NatSpec source |
| --- | --- | --- |
| 3 | `initialize` | `/// Initializes the escrow contract with the operational admin. ...` |
| 4 | `bind_settlement_token` | `/// Bind the single Stellar Asset Contract (SAC) token this escrow instance will custody. ...` |
| 5 | `create_contract` | `/// Creates a new escrow contract with the specified client, freelancer, and milestone amounts.` |
| 6 | `deposit_funds` | `/// Deposits funds into the contract. Transitions to Funded status when fully funded.` |
| 7 | `approve_milestone_release` | `/// Approves a milestone for release.` |
| 8 | `release_milestone` | `/// Releases a specific milestone, transferring funds to the freelancer.` |
| (lookup) | `get_milestone_approvals` | `/// Retrieves approval status for a milestone.` |
| (lookup) | `get_contract` | `/// Retrieves contract information.` |
| (lookup) | `get_milestones` | `/// Retrieves all milestones for a contract.` |
| (lookup) | `get_admin` | `/// Returns the stored governance admin address.` |

For the **complete** NatSpec surface (every entrypoint, every error, every
event topic) see [`abi-reference.md`](abi-reference.md).

---

## Where to go next

- **Production deployment checklist:** [`release-readiness-checklist.md`](release-readiness-checklist.md)
- **Two-step governance admin transfer (propose / accept + timelock):**
  see the `propose_governance_admin` and `accept_governance_admin` rows in
  [`abi-reference.md`](abi-reference.md) and
  [`docs/escrow/governance-security.md`](governance-security.md).
- **Authorization deep-dive:** [`authorization.md`](authorization.md)
- **Error catalog:** [`ERROR_CATALOG.md`](ERROR_CATALOG.md)
- **Custody model and SAC balance reconciliation:** [`README.md`](README.md#custody-lifecycle-sac-token-integration)
- **Disputes and arbitration:** [`dispute-resolution.md`](dispute-resolution.md)
- **Reputation flow after completion:** [`REPUTATION.md`](REPUTATION.md)
- **Threat model assumptions to validate against your integration:**
  [`SECURITY.md`](SECURITY.md)
