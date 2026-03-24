# TalentTrust Contracts

Soroban smart contracts for the TalentTrust decentralized freelancer escrow protocol on the Stellar network.

## What's in this repo

- **Escrow contract** (`contracts/escrow`): Holds funds in escrow, supports milestone-based payments and reputation credential issuance.

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

## Escrow contract — acceptance handshake

Before a client can fund an escrow contract, the assigned freelancer must explicitly accept the terms. This two-party handshake ensures no funds are committed without mutual agreement.

### State machine

```
Created ──► Accepted ──► Funded ──► Completed
                                └──► Disputed
```

| Status      | Meaning                                                       |
| ----------- | ------------------------------------------------------------- |
| `Created`   | Contract created by the client; awaiting freelancer response. |
| `Accepted`  | Freelancer has signed off; client may now deposit funds.      |
| `Funded`    | Funds are held in escrow; milestones may be released.         |
| `Completed` | All milestones released; engagement concluded.                |
| `Disputed`  | Under dispute resolution.                                     |

### Key functions

| Function            | Caller     | Requires status | Resulting status |
| ------------------- | ---------- | --------------- | ---------------- |
| `create_contract`   | client     | —               | `Created`        |
| `accept_contract`   | freelancer | `Created`       | `Accepted`       |
| `deposit_funds`     | client     | `Accepted`      | `Funded`         |
| `release_milestone` | client     | `Funded`        | `Funded`         |
| `get_status`        | anyone     | —               | —                |

See [`docs/escrow/README.md`](docs/escrow/README.md) for the full contract reference.

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

## License

MIT
