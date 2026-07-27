# Pull Request Request

## Summary
This PR adds an admin-gated governance parameter setter to the escrow contract in `contracts/escrow`. It introduces `set_governed_params(admin, protocol_fee_bps, max_escrow_total_stroops)`, persists governed parameters, and flips the readiness checklist flag `ReadinessChecklist.governed_params_set` to `true`.

## Files changed
- `contracts/escrow/src/governance.rs`
- `contracts/escrow/src/lib.rs`
- `contracts/escrow/src/types.rs`
- `contracts/escrow/src/test/mainnet_readiness.rs`
- `contracts/escrow/src/test/mod.rs`
- `docs/escrow/mainnet-readiness.md`

## What this change does
- Adds `GovernedParameters` storage and `DataKey::GovernedParameters`
- Adds a secure admin-controlled setter for governed parameters
- Enforces validation of `protocol_fee_bps` and `max_escrow_total_stroops`
- Ensures `governed_params_set` becomes `true` only after a successful admin-set operation
- Exposes `get_governed_parameters()` for read-back
- Updates readiness documentation to match the new setter flow

## Security and behavior notes
- `set_governed_params` requires `initialize()` to have been called and authenticated as the registered admin.
- Invalid parameter updates and unauthorized callers do not mutate readiness state.
- The readiness checklist update is atomic and persisted together with the governed parameter values.

## Testing
Run the following commands from the repo root:

```bash
cargo fmt --all -- --check
cargo test -p escrow
```

## Branch and commit guidance
- Branch name: `feature/governed-params-setter`
- Example commit message: `feat(escrow): add governed params setter wiring readiness`

## Additional notes
- This PR is scoped to the TalentTrust escrow Soroban contract.
- Documentation in `docs/escrow/mainnet-readiness.md` has been aligned with the new contract behavior.
