# TalentTrust Contracts

Soroban smart contracts for the TalentTrust decentralized freelancer escrow protocol on the Stellar network.

## What's in this repo

- **Escrow contract** (`contracts/escrow`): Holds funds in escrow, supports milestone-based payments, reputation credential issuance, and emergency pause controls.
- **Escrow docs** (`docs/escrow`): Escrow operations, security notes, and pause/emergency threat model.

## Security model

The escrow contract enforces a minimal on-chain state machine with comprehensive security controls:

- Contract creation requires client authorization and validates immutable milestone inputs.
- Funding is accepted exactly once and must match the total milestone amount.
- Milestones can be released once each and only by the recorded client.
- Reputation entries are gated behind completed-contract credits and are treated as informational data.
- Protocol-wide validation parameters can be guarded by a governance admin and updated through audited state transitions.

Comprehensive security documentation:
- [Threat Model](/docs/escrow/threat-model.md) - Complete threat analysis, attack vectors, and mitigations
- [Security Notes](/docs/escrow/security.md) - Pause/emergency controls and operational security
- [Contract Documentation](/docs/escrow/README.md) - Reviewer-focused contract notes

## Threat Model

The escrow contract has been analyzed for security threats across all functionality. Key security features include:

- Multiple authorization layers preventing unauthorized fund access
- State machine enforcement preventing invalid transitions
- Input validation on all user-supplied data
- Emergency pause controls for incident response
- Two-step governance admin transfer
- Protocol parameter validation

See [docs/escrow/threat-model.md](/docs/escrow/threat-model.md) for the complete threat model including:
- 15 identified threat scenarios with mitigations
- Attack surface analysis
- Security assumptions and residual risks
- Recommended hardening steps
- Incident response procedures
- Security audit checklist

## Protocol governance

The escrow contract supports guarded protocol parameter updates for live validation logic:

- A one-time governance initialization assigns the first protocol admin via `initialize_governance`.
- The admin can update protocol parameters such as minimum milestone amount, maximum milestones per contract, and permitted reputation rating bounds via `update_protocol_parameters`.
- Admin transfer is two-step: current admin proposes via `propose_governance_admin`, pending admin accepts via `accept_governance_admin`.
- Before governance is initialized, the contract uses safe built-in defaults so existing flows remain available.
- Governance operations are independent of pause controls and can be executed even when the contract is paused.

Current defaults:

- `min_milestone_amount = 1`
- `max_milestones = 16`
- `min_reputation_rating = 1`
- `max_reputation_rating = 5`

Governance functions:
- `initialize_governance(admin)` - One-time initialization of governance admin
- `update_protocol_parameters(min_milestone, max_milestones, min_rating, max_rating)` - Update validation parameters
- `propose_governance_admin(new_admin)` - Propose admin transfer
- `accept_governance_admin()` - Accept admin transfer (called by pending admin)
- `get_governance_admin()` - Query current governance admin
- `get_pending_governance_admin()` - Query pending admin transfer
- `get_protocol_parameters()` - Query current parameters

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
