# TalentTrust Contracts

Soroban smart contracts for the TalentTrust decentralized freelancer escrow protocol on the Stellar network.

## What's in this repo

- **Escrow contract** (`contracts/escrow`): Holds funds in escrow, supports milestone-based payments, and issues reputation credentials only after successful completion and final settlement.

## Contract overview

### Escrow

| Function | Description |
|---|---|
| `create_contract(client, freelancer, milestone_amounts)` | Create a new escrow engagement; returns a unique numeric contract ID. |
| `deposit_funds(contract_id, amount)` | Client deposits funds; transitions status to `Funded`. |
| `release_milestone(contract_id, milestone_id)` | Client releases a single milestone payment to the freelancer. |
| `complete_contract(contract_id)` | Client finalises the contract as `Completed`. Requires all milestones released. |
| `issue_reputation(contract_id, rating)` | Issue a reputation credential (rating 1–5) for the freelancer. |

### Reputation issuance constraints

`issue_reputation` enforces the following ordered constraints — any violation panics with a descriptive error:

1. **Contract existence** – the `contract_id` must exist.
2. **Completion gate** – contract `status` must be `Completed`.
3. **Final settlement** – every milestone must have `released == true`.
4. **Single issuance** – a credential can only be issued once per contract (prevents replay / double-issuance).
5. **Valid rating** – `rating` must be in `[1, 5]`.

The full lifecycle to reach a state where reputation can be issued:

```
create_contract → deposit_funds → release_milestone (×N) → complete_contract → issue_reputation
```

See [`docs/escrow/README.md`](docs/escrow/README.md) for the full contract specification.

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

## CI/CD

On every push and pull request to `main`, GitHub Actions:

- Checks formatting (`cargo fmt --all -- --check`)
- Builds the workspace (`cargo build`)
- Runs tests (`cargo test`)

Ensure these pass locally before pushing.

## License

MIT
