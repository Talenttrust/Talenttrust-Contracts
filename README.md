# TalentTrust Contracts

Soroban smart contracts for the TalentTrust decentralized freelancer escrow protocol on the Stellar network.

## What's in this repo

- **Escrow contract** (`contracts/escrow`): Holds funds in escrow, supports milestone-based payments, reputation credential issuance, and **partial refunds** for unreleased balances.

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

# Run tests
cargo test

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

## Features

### Partial Refund Logic

The escrow contract now supports a partial refund mechanism for unreleased milestone balances. This allows the client to recover funds if a contract is cancelled before all milestones are completed.

- **Secure**: Only the client can initiate a refund.
- **Accurate**: Computes the refund amount based on on-chain milestone data.
- **Protected**: Guards against double-refunds and refunds on completed contracts.

For more details, see the [Partial Refund Documentation](file:///c:/Users/ADMIN/Desktop/midea-drips/Talenttrust-Contracts/docs/escrow/PARTIAL_REFUND.md).

## CI/CD

On every push and pull request to `main`, GitHub Actions:

- Checks formatting (`cargo fmt --all -- --check`)
- Builds the workspace (`cargo build`)
- Runs tests (`cargo test`)

Ensure these pass locally before pushing.

## License

MIT
