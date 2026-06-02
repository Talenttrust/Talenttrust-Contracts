# Escrow Tests

The active escrow unit-test tree is declared in `contracts/escrow/src/test/mod.rs`.
At the time of this documentation pass it includes:

- `pause_controls`
- `emergency_controls`
- `lifecycle`
- `summary`

`summary` is a documentation/API drift guard that compares `lib.rs` public
entrypoints against canonical docs and ensures planned entrypoints are not listed
as live API.

Some stale test files remain in `contracts/escrow/src/test/` but are not included
by `mod.rs`. They should not be used as evidence of implemented entrypoints
until they are reconciled and compiled.
