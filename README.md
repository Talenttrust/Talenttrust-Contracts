# TalentTrust Contracts

Soroban smart contracts for the TalentTrust freelancer escrow protocol on Stellar.

## Repository Scope

- **Escrow contract** (`contracts/escrow`): Holds funds in escrow, supports milestone-based payments and reputation credential issuance. **Token custody is on-chain** via a Stellar Asset Contract (SAC) bound at admin setup; `deposit_funds` and `release_milestone` perform real `token::Client::transfer` calls.
- **Planned escrow fee model**: Configurable protocol fee is now wired into `release_milestone` (`set_protocol_fee_bps`); fee retention into `AccumulatedProtocolFees` is implemented. A separate `withdraw_protocol_fees` entrypoint remains tracked in [#314](https://github.com/Talenttrust/Talenttrust-Contracts/issues/314).

Reviewer-oriented notes live in [docs/escrow/README.md](docs/escrow/README.md), with the crate-level rustdoc module map in [contracts/escrow/src/lib.rs](contracts/escrow/src/lib.rs), storage-key details in [docs/escrow/state-persistence.md](docs/escrow/state-persistence.md), threat analysis in [docs/escrow/SECURITY.md](docs/escrow/SECURITY.md), and release authorization modes in [docs/escrow/authorization.md](docs/escrow/authorization.md).

To generate the escrow module map locally, run:

```bash
cargo doc -p escrow --no-deps
```

Then open `target/doc/escrow/index.html`.

The supported deployed interface is documented in the [Escrow ABI reference](docs/escrow/abi-reference.md). It intentionally excludes template scaffolding endpoints; consumers should integrate only with the listed contract methods.

---

## Feature Status Matrix

| Feature / Entrypoint       | Status          | Context / Tracking Reference                                                                      |
| -------------------------- | --------------- | ------------------------------------------------------------------------------------------------- |
| `create_contract`          | **Implemented** | Core initialization and state persistence                                                         |
| `create_and_fund`          | **Implemented** | Atomic create + full fund in one transaction ([#1705](https://github.com/Talenttrust/Talenttrust-Contracts/issues/1705)) |
| `set_terms`                | **Implemented** | Anchor job terms and per-milestone deliverable hashes before funding ([#1707](https://github.com/Talenttrust/Talenttrust-Contracts/issues/1707)) |
| `deliver`                  | **Implemented** | Freelancer records per-milestone delivery hashes, overwritable until release ([#1707](https://github.com/Talenttrust/Talenttrust-Contracts/issues/1707)) |
| `get_terms` / `get_delivery` | **Implemented** | Read-only retrieval of stored terms and per-milestone delivery hashes ([#1707](https://github.com/Talenttrust/Talenttrust-Contracts/issues/1707)) |
| `deposit_funds`            | **Implemented** | Escrow fund collection tracking                                                                   |
| `release_milestone`        | **Implemented** | Authenticated milestone payouts via `caller.require_auth()`                                       |
| `finalize_contract`        | **Implemented** | Freezes contract mutable updates                                                                  |
| `cancel_contract`          | **Implemented** | Early termination contract transitions                                                            |
| `issue_reputation`         | **Implemented** | Milestone rating metrics for completed contracts                                                  |
| `get_bounds`               | **Implemented** | Read protocol compile-time limits (max milestones, amounts, fee bps) via `ContractBounds` type   |
| Emergency Circuit Breakers | **Implemented** | Public administrative pause and emergency flags                                                   |
| Protocol Fee Accumulation  | **Implemented** | Logic built directly into milestone releases                                                      |
| Protocol Fee Withdrawal    | **Planned**     | Entrypoints tracked in [#314](https://github.com/Talenttrust/Talenttrust-Contracts/issues/314)    |
| Two-step Admin Transfer    | **Planned**     | Infrastructure tracked in [#318](https://github.com/Talenttrust/Talenttrust-Contracts/issues/318) |

---

## Security Model

The escrow implementation follows a fail-closed state machine:

- contract creation requires client authorization and rejects invalid participant or milestone metadata before persisting state
- a Stellar Asset Contract (SAC) settlement token is admin-bound exactly once via `bind_settlement_token`; each subsequent `deposit_funds` and `release_milestone` calls `token::Client::transfer` and updates accounting atomically
- deposits pull SAC tokens from the client BEFORE `funded_amount` is updated, so a failed transfer leaves accounting untouched
- deposits cannot exceed the required escrow total
- cancellation returns the full refundable SAC balance to the client while preserving funds held for other active contracts
- releases pay the freelancer (less protocol fee) via SAC transfer BEFORE milestone state is updated, so a failed payout leaves state untouched
- releases require a valid unreleased milestone, caller authorization via `caller.require_auth()`, and valid non-expired approvals matching the contract's `ReleaseAuthorization` mode
- reputation is gated behind contract completion and is issued once per contract; each completed escrow accrues one pending credit for its freelancer, while fully refunded escrows accrue none. Issuing reputation consumes one credit and updates the freelancer's completed-contract count
- finalization records immutable close metadata for completed or disputed contracts and blocks later contract-specific mutations
- one-time admin initialization protects pause and emergency controls; two-step admin transfer is planned in [#318](https://github.com/Talenttrust/Talenttrust-Contracts/issues/318)
- pause and emergency controls block all state-changing escrow operations while active

Planned governance-transfer and migration features are explicitly labeled in the escrow docs until their entrypoints land.

```bash
# Run tests (includes 95%+ coverage negative path testing for escrow)
cargo test

# Run escrow performance/gas baseline tests only
cargo test test::performance

# Check formatting
cargo fmt --all -- --check

# Run escrow-specific tests
cargo test -p escrow

# Run escrow performance tests only
cargo test test::performance -p escrow
```

---

## Escrow Terms and Delivery Hashes

`set_terms(env, contract_id, caller, terms_hash, milestone_hashes)` commits a 32-byte terms hash and optional per-milestone deliverable hashes before the escrow is funded.

- Client-only authorization; only callable while the contract is in `Created` status with `funded_amount == 0`.
- `milestone_hashes.len()` must be 0 or equal the milestone count, otherwise `InvalidMetadata`.
- Once any deposit is made, terms are frozen — `set_terms` returns `InvalidState`.
- Emits `("terms", contract_id) → terms_hash`.

`deliver(env, contract_id, caller, index, delivery_hash)` records a 32-byte delivery hash for a single milestone.

- Freelancer-only authorization; overwriting is allowed until the milestone is released.
- Emits `("deliver", contract_id) → (index, delivery_hash)`.

Read-only entrypoints `get_terms` and `get_delivery` retrieve the stored hashes.

---

## Escrow Event Catalogue (#1712)

All escrow events carry a schema-version sentinel as the first topic so indexers can evolve without re-deploying.

**Topic shapes**

| Class | Topics | Data struct |
|---|---|---|
| Escrow-scoped | `(1u32, symbol, contract_id: u32)` | event struct (see below) |
| Global | `(1u32, symbol)` | event struct (see below) |

**`EVENT_SCHEMA_V = 1`** — bump this constant when any payload struct layout changes.

| Symbol | When emitted | Payload struct | Key fields |
|---|---|---|---|
| `init` | `initialize` succeeds | `InitEvent` | `admin` |
| `token_bound` | `bind_settlement_token` succeeds | `SettlementTokenBoundEvent` | `admin`, `token` |
| `created` | `create_contract` / `create_and_fund` | `CreatedEvent` | `client`, `freelancer`, `token`, `total`, `milestone_count` |
| `deposit` | `deposit_funds` / `create_and_fund` | `DepositEvent` | `from`, `amount`, `funded_amount`, `status` |
| `mlstn_rls` | `release_milestone` (each release) | `ReleaseEvent` | `index`, `amount`, `fee`, `to`, `released_amount`, `status` |
| `complete` | `release_milestone` when last milestone completes | `CompletedEvent` | `released_amount` |
| `cancelled` | `cancel_contract` | `CancelEvent` | `refund`, `to`, `status` |
| `refunded` | `refund_unreleased_milestones` | `RefundedEvent` | `amount`, `status` |
| `dispute_opened` | `raise_dispute` | `DisputeEvent` | `by`, `status` |
| `dispute_resolved` | `resolve_dispute` | `ResolvedEvent` | `to_freelancer`, `to_client`, `status` |
| `reputation` | `issue_reputation` | `ReputationEvent` | `freelancer`, `rating`, `completed_contracts`, `total_rating` |
| `evidence` | `submit_work_evidence` | `EvidenceEvent` | `milestone_index`, `freelancer` |
| `paused` | `pause` | `PausedEvent` | `admin` |
| `unpaused` | `unpause` | `UnpausedEvent` | `admin` |
| `emergency_on` | `activate_emergency_pause` | `EmergencyActivatedEvent` | `admin` |
| `emergency_off` | `resolve_emergency` | `EmergencyResolvedEvent` | `admin` |

---

## Escrow Emergency Controls

The escrow contract supports critical-incident response with admin-managed controls:

- `initialize(admin)` _(one-time setup)_
- `pause()` and `unpause()`
- `activate_emergency_pause()` and `resolve_emergency()`
- `is_paused()` and `is_emergency()`

When paused, all mutating escrow operations (`create_contract`, `deposit_funds`, `release_milestone`, `issue_reputation`, `cancel_contract`) are blocked with `ContractPaused`.

Read-only queries are never blocked.

See `docs/escrow/emergency-controls.md` for the full flag semantics, event model, and security properties.

---

## Escrow Closure Finalization

`finalize_contract(contract_id, finalizer)` records immutable close metadata for contracts in `Completed` or `Disputed` status.

The finalizer must be the stored client, freelancer, or assigned arbiter and must authorize the call.

After finalization, subsequent contract-specific mutating calls fail with `AlreadyFinalized`.

---

## Prerequisites

- Rust 1.75+
- rustfmt
- Stellar CLI _(optional for deployment workflows)_

---

## Contributing

1. Fork the repository and create a branch from `main`.
2. Make changes while keeping tests, linting, and formatting checks passing:

```bash
cargo fmt --all

cargo clippy --workspace --all-targets -- -D warnings

cargo test

cargo build
```

3. Open a pull request.

CI runs verification checks automatically.

---

## CI/CD

On every push and pull request to `main`, GitHub Actions:

- Checks formatting:

```bash
cargo fmt --all -- --check
```

- Lints with warnings denied:

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

- Builds the workspace:

```bash
cargo build
```

- Runs tests:

```bash
cargo test
```

Ensure these pass locally before pushing.

---

## Escrow Performance and Security

Performance/gas baseline tests for key flows are located at:

```text
contracts/escrow/src/test/performance.rs
```

Functional and failure-path coverage is split by module:

```text
contracts/escrow/src/test/flows.rs
contracts/escrow/src/test/security.rs
```

Contract-specific reviewer documentation:

```text
docs/escrow/performance-baselines.md
docs/escrow/SECURITY.md
```

---

## License

MIT
