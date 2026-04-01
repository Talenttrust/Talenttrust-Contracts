# TalentTrust Contracts

Soroban smart contracts for the TalentTrust freelancer escrow protocol on Stellar.

## Repository Scope

- **Escrow contract** (`contracts/escrow`): Holds funds in escrow, supports milestone-based payments and reputation credential issuance.
- **Escrow fee model**: Configurable protocol fee per release with accounting/withdrawal paths (`protocol_fee_bps`, `protocol_fee_account`).

Reviewer-oriented notes live in [docs/escrow/README.md](docs/escrow/README.md), with storage-key details in [docs/escrow/state-persistence.md](docs/escrow/state-persistence.md) and threat analysis in [docs/escrow/security.md](docs/escrow/security.md).

## Security Model

The escrow implementation follows a fail-closed state machine:

- contract creation requires client authorization and rejects invalid participant or milestone metadata before persisting state
- deposits cannot exceed the required escrow total
- releases require the recorded client, a valid unreleased milestone, and enough funded balance to cover the payment
- reputation is gated behind contract completion and is issued once per contract
- governance changes use a one-time initialization plus a two-step admin transfer
- pause and emergency controls block all state-changing escrow operations while active

# Run tests (includes 95%+ coverage negative path testing for escrow)
cargo test

# Run escrow performance/gas baseline tests only
cargo test test::performance

# Check formatting
cargo fmt --all -- --check
cargo test -p escrow
cargo test test::performance -p escrow
```

## Escrow Emergency Controls

The escrow contract now supports critical-incident response with admin-managed controls:

- `initialize(admin)` (one-time setup)
- `pause()` and `unpause()`
- `activate_emergency_pause()` and `resolve_emergency()`
- `is_paused()` and `is_emergency()`

When paused, mutating escrow operations are blocked.

## Contributing

1. Fork the repo and create a branch from `main`.
2. Make changes; keep tests and formatting passing:
   - `cargo fmt --all`
   - `cargo test`
   - `cargo build`
3. Open a pull request. CI runs `cargo fmt --all -- --check`, `cargo build`, and `cargo test` on push/PR to `main`.

## Contract status transition guardrails

Prerequisites:

- Rust 1.75+
- `rustfmt`
- optional Stellar CLI for deployment workflows

Common commands:

## Escrow closure finalization

- `finalize_contract` records immutable close metadata (timestamp, finalizer, summary)
- Finalization allowed only from `Completed` or `Disputed` status
- Finalization can only be executed by contract parties (client/freelancer/arbiter)
- Once finalized, the contract summary and record are immutable

## CI/CD

On every push and pull request to `main`, GitHub Actions:

- Checks formatting (`cargo fmt --all -- --check`)
- Builds the workspace (`cargo build`)
- Runs tests (`cargo test`)

Ensure these pass locally before pushing.

## Escrow Performance and Security

- Performance/gas baseline tests for key flows are in `contracts/escrow/src/test/performance.rs`.
- Functional and failure-path coverage is split by module:
  - `contracts/escrow/src/test/flows.rs`
  - `contracts/escrow/src/test/security.rs`
- Contract-specific reviewer docs:
  - `docs/escrow/performance-baselines.md`
  - `docs/escrow/security.md`

## License

MIT
