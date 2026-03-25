# Escrow Performance and Gas Baselines

This document defines regression guardrails for the escrow contract's key flows.

## Scope

Baselines are enforced by tests in `contracts/escrow/src/test/performance.rs` for:

- `create_contract`
- `deposit_funds`
- `release_milestone`
- end-to-end sequence (`create_contract` -> `deposit_funds` -> `release_milestone`)

## Methodology

1. Use Soroban test environment invocation metering via `env.cost_estimate()`.
2. Capture invocation resources and fee estimate after each top-level call.
3. Assert each metric is below stable upper limits to detect regressions.

Note: Soroban resource estimates in unit tests are approximations and can differ from on-chain simulation. Baselines are conservative ceilings intended for CI regression detection.

## Baseline Ceilings

### create_contract

- max instructions: `8,000,000`
- max memory bytes: `800,000`
- max read entries: `2`
- max write entries: `3`
- max read bytes: `2,048`
- max write bytes: `8,192`
- max fee total (stroops): `1,650,000`

### deposit_funds

- max instructions: `6,500,000`
- max memory bytes: `700,000`
- max read entries: `2`
- max write entries: `2`
- max read bytes: `2,048`
- max write bytes: `8,192`
- max fee total (stroops): `1,550,000`

### release_milestone

- max instructions: `7,000,000`
- max memory bytes: `750,000`
- max read entries: `2`
- max write entries: `2`
- max read bytes: `2,048`
- max write bytes: `10,240`
- max fee total (stroops): `1,550,000`

### end-to-end sequence

- max total instructions: `18,000,000`
- max total memory bytes: `2,000,000`

## How to Run

```bash
cargo test test::performance
```

## Latest Local Test Output

Date: `2026-03-23`

```text
running 23 tests
.......................
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.35s
```
