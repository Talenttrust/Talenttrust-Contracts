# TalentTrust Contracts

Soroban smart contracts for the TalentTrust decentralized freelancer escrow protocol on the Stellar network.

## What's in this repo

- **Escrow contract** (`contracts/escrow`): Holds funds in escrow, supports milestone-based payments, reputation credential issuance, dispute resolution mechanism, and emergency pause controls.
- **Escrow docs** (`docs/escrow`): Escrow operations, security notes, and pause/emergency threat model.

## Features

### Core Escrow Functionality
- Create escrow contracts with milestone-based payments
- Deposit and release funds securely
- Issue reputation credentials

### Dispute Resolution Mechanism
- **Admin/Arbitrator roles**: Secure access control for dispute resolution
- **Deterministic payout outcomes**: Four resolution types with predictable results
  - `FullRefund`: Client gets 100% refund
  - `PartialRefund`: Client gets 70%, freelancer gets 30%
  - `FullPayout`: Freelancer gets 100%
  - `Split`: Custom split determined by arbitrator
- **Evidence tracking**: Store dispute reasons and evidence
- **Secure workflow**: Only authorized parties can create and resolve disputes

### Emergency Controls
- `initialize(admin)` (one-time setup)
- `pause()` and `unpause()`
- `activate_emergency_pause()` and `resolve_emergency()`
- `is_paused()` and `is_emergency()`

When paused, mutating escrow operations are blocked.

## Security model

The escrow contract now enforces a minimal on-chain state machine instead of placeholder return values:

- Contract creation requires client authorization and validates immutable milestone inputs.
- Funding is accepted exactly once and must match the total milestone amount.
- Milestones can be released once each and only by the recorded client.
- Reputation entries are gated behind completed-contract credits and are treated as informational data.
- Protocol-wide validation parameters can be guarded by a governance admin and updated through audited state transitions.

## Protocol governance

The escrow contract supports guarded protocol parameter updates for live validation logic:

- A one-time governance initialization assigns the first protocol admin.
- The admin can update protocol parameters such as minimum milestone amount, maximum milestones per contract, and permitted reputation rating bounds.
- Admin transfer is two-step: current admin proposes, pending admin accepts.
- Before governance is initialized, the contract uses safe built-in defaults so existing flows remain available.

Current defaults:

- `min_milestone_amount = 1`
- `max_milestones = 16`
- `min_reputation_rating = 1`
- `max_reputation_rating = 5`

## Prerequisites

- [Rust](https://rustup.rs/) (stable, 1.75+)
- `rustfmt`: `rustup component add rustfmt`
- Optional: [Stellar CLI](https://developers.stellar.org/docs/tools/stellar-cli) for deployment

## Setup

```bash
# Clone (or you're already in the repo)
git clone <your-repo-url>
cd talenttrust-contracts

# Build
cargo build

# Run tests (includes 95%+ coverage negative path testing for escrow)
cargo test

# Run escrow performance/gas baseline tests only
cargo test test::performance

# Check formatting
cargo fmt --all -- --check

# Format code
cargo fmt --all
```

## Contributing

1. Fork the repo and create a branch from `main`.
2. Make changes; keep tests and formatting passing:
   - `cargo fmt --all`
   - `cargo test`
   - `cargo build`
3. Open a pull request. CI runs `cargo fmt --all -- --check`, `cargo build`, and `cargo test` on push/PR to `main`.

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