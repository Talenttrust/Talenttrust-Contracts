Title: feat(escrow): audit events for fee and admin changes

Summary

Add audit events for governance-sensitive parameter changes in the TalentTrust escrow contract:
- Emit `protocol_fee_bps` events when protocol fee bps change (old_bps, new_bps, admin, timestamp).
- Emit admin transfer events for proposal and acceptance under `admin` topic: `(admin, "proposed")` and `(admin, "accepted")` with payloads (old_admin, new_admin, timestamp).
- Persist a `PendingAdmin` key for two-step admin transfers.

Files changed (new/updated)

- Added: `contracts/escrow/src/governance.rs` — governance functions + event emission
- Added: `contracts/escrow/src/test/governance_events.rs` — tests asserting events emitted
- Updated: `contracts/escrow/src/types.rs` — added `PendingAdmin` DataKey
- Updated: `contracts/escrow/src/lib.rs` — module registration
- Updated: `docs/escrow/governance-security.md` — governance audit notes

Implementation notes

- Event topics use `symbol_short!` to remain consistent with existing topics (`init`, `paused`, `emergency`).
- `set_protocol_fee_bps` requires initialization and admin authorization, stores the new bps, then emits the event.
- Admin transfer is two-step: `propose_governance_admin(proposed)` (admin auth) stores `PendingAdmin` + emits `(admin, "proposed")`; `accept_governance_admin()` requires `PendingAdmin` auth, performs swap, clears pending, and emits `(admin, "accepted")`.
- Events include timestamps and actor addresses for reliable off-chain indexing.

Security & Invariants

- All governance mutations require initialization and appropriate authorization.
- The two-step admin transfer prevents accidental or unauthorized transfers; the pending admin must call `accept_governance_admin()` and `require_auth()`.
- Protocol fee bps update is atomic and logged; consider multisig for the governance admin key in production.

Testing

- Run formatting and linting first:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```

- Run escrow tests:

```bash
cargo test -p escrow
```

- Run only the new governance tests:

```bash
cargo test -p escrow -- test::governance_events
```

PR checklist

- [ ] Tests pass (`cargo test -p escrow`)
- [ ] `cargo fmt` and `clippy` pass
- [ ] Documentation updated (`docs/escrow/governance-security.md`)
- [ ] CI green on PR

Suggested commit

`feat(escrow): audit events for fee and admin changes`

Suggested branch name

`enhancement/governance-audit-events`

Notes for reviewers

- Review event payload shapes and topic naming for compatibility with existing indexers.
- Confirm that the `PendingAdmin` key naming and storage semantics match repository conventions.
- Confirm NatSpec/rustdoc comments are sufficient for public functions; I can add more inline docs if required.
