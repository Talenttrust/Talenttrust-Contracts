# Escrow Performance Baselines

The current active escrow test module covers pause/emergency controls,
amount-validation helpers, and documentation/API drift guards.

Performance baselines for dispute, refund, approval, finalization, and protocol
fee flows are not applicable because those entrypoints are not implemented in
`contracts/escrow/src/lib.rs`.

Before adding a new performance table, verify the referenced entrypoint exists in
`lib.rs` and is included in the active test module tree.
