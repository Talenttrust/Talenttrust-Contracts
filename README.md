# TalentTrust Contracts

Soroban smart contracts for the TalentTrust decentralized freelancer escrow protocol on the Stellar network.

## What's in this repo

- **Escrow contract** (`contracts/escrow`): Holds funds in escrow, supports milestone-based payments, reputation credential issuance, and emergency pause controls.
- **Escrow docs** (`docs/escrow`): Escrow operations, security notes, and pause/emergency threat model.
- **Escrow contract** (`contracts/escrow`): Holds funds in escrow, supports milestone-based payments and reputation credential issuance.
- **Escrow docs** (`docs/escrow`): Upgradeable storage layout strategy, migration safety notes, and security assumptions.

### Release Readiness Checklist

The escrow contract includes an on-chain **release readiness checklist** that automatically tracks and enforces deployment, verification, and post-deploy monitoring gates:

| Phase | Items |
|---|---|
| Deployment | Contract created, funds deposited |
| Verification | Parties authenticated, milestones defined |
| Post-Deploy Monitoring | All milestones released, reputation issued |

`release_milestone` is **hard-blocked** until all Deployment and Verification items are satisfied.  
Query checklist state with `get_release_checklist`, `is_release_ready`, and `is_post_deploy_complete`.

See [docs/escrow/release-readiness-checklist.md](docs/escrow/release-readiness-checklist.md) for full details, function reference, error codes, and security model.

### Input Sanitization Hardening

The escrow contract rejects malformed contract-creation inputs before any state is written:

- `client` and `freelancer` must be different addresses.
- Every milestone amount must be strictly positive (`> 0`).
- Milestone count must be between `1` and `MAX_MILESTONES` (`20`).

## Escrow timeout behavior

- Each milestone has a deterministic `deadline_at` timestamp set at contract creation.
- Deadline boundary is inclusive:
  - valid while `ledger_timestamp <= deadline_at`
  - expired when `ledger_timestamp > deadline_at`
- Any approval or release attempt after expiry transitions the contract from `Funded` to `Disputed` and rejects the action.
- See `docs/escrow/timeout-behavior.md` for threat model and testing notes.

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

# Run access-control focused tests
cargo test access_control

# Run upgradeable storage planning tests only
cargo test test::storage


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

## Upgradeable Storage Planning

- Versioned storage metadata and key namespaces are implemented in `contracts/escrow/src/lib.rs`.
- Dedicated storage planning tests are in:
  - `contracts/escrow/src/test/storage.rs`
  - `contracts/escrow/src/test/flows.rs`
  - `contracts/escrow/src/test/security.rs`
- Contract-specific documentation:
  - `docs/escrow/upgradeable-storage.md`
  - `docs/escrow/security.md`

## License

MIT
