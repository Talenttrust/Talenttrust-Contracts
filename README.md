# TalentTrust Contracts

Soroban smart contracts for the TalentTrust freelancer escrow protocol on Stellar.

## Repository Scope

- `contracts/escrow`: milestone escrow contract with persisted lifecycle state, participant metadata, governed validation parameters, reputation issuance, and pause controls
- `docs/escrow`: reviewer-focused escrow design, storage notes, and threat assumptions

## Escrow State Persistence

The escrow contract now persists the full payment lifecycle instead of relying on placeholder behavior:

- each escrow record stores the client, freelancer, milestone definitions, funded and released balances, milestone counters, lifecycle status, and timestamps
- milestone releases are one-way state transitions and cannot be replayed
- completed contracts mint a pending reputation credit for the recorded freelancer, and that credit is consumed exactly once when a rating is issued
- protocol governance parameters and pause or emergency flags are persisted separately from escrow records so operational controls survive across calls

Default protocol parameters:

- `min_milestone_amount = 1`
- `max_milestones = 16`
- `min_reputation_rating = 1`
- `max_reputation_rating = 5`

Reviewer-oriented notes live in [docs/escrow/README.md](docs/escrow/README.md), with storage-key details in [docs/escrow/state-persistence.md](docs/escrow/state-persistence.md) and threat analysis in [docs/escrow/security.md](docs/escrow/security.md).

## Security Model

The escrow implementation follows a fail-closed state machine:

- contract creation requires client authorization and rejects invalid participant or milestone metadata before persisting state
- deposits cannot exceed the required escrow total
- releases require the recorded client, a valid unreleased milestone, and enough funded balance to cover the payment
- reputation is gated behind contract completion and is issued once per contract
- governance changes use a one-time initialization plus a two-step admin transfer
- pause and emergency controls block all state-changing escrow operations while active

## Local Verification

```bash
cargo fmt --all -- --check
cargo test -p escrow
```

Latest local escrow test result:

```text
running 42 tests
test result: ok. 42 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## Development

Prerequisites:

- Rust 1.75+
- `rustfmt`
- optional Stellar CLI for deployment workflows

Common commands:

```bash
cargo build
cargo test -p escrow
cargo test test::performance -p escrow
cargo fmt --all
```

## License

MIT
