# Mainnet readiness (contract-side) — minimal checklist

**Issue:** #178  
**Scope:** `contracts/escrow` only.

This document is a reviewer-oriented summary of what the contract exposes for mainnet operations: **limits**, **monitoring hooks**, and **known risks**.

## On-chain limits

| Mechanism | What it does |
|-----------|----------------|
| `MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS` | Hard cap on the sum of milestone amounts per escrow. **Not** changeable via governance; requires a WASM upgrade to adjust. |
| `ProtocolParameters` | Governed limits: minimum milestone size, maximum milestone count, reputation rating bounds. Manipulated only through `initialize_protocol_governance` and `update_protocol_parameters` after the admin is set. |
| `get_mainnet_readiness_info` | Read-only snapshot: `protocol_version`, `max_escrow_total_stroops`, and the current governed fields (flattened for Soroban storage limits). Use for dashboards and preflight scripts. |

## Monitoring hooks (events)

Contract events use a stable two-symbol topic prefix so indexers can filter without parsing custom data shapes:

| Phase | Topics (`symbol_short!`) | Data payload |
|-------|--------------------------|--------------|
| Creation | `tt_esc`, `create` | `(contract_id: u32, total_amount: i128)` |
| Funding | `tt_esc`, `deposit` | `(contract_id: u32, amount: i128)` |
| Release | `tt_esc`, `release` | `(contract_id: u32, milestone_id: u32, amount: i128)` |

**Note:** Validate event visibility with your RPC / integration tests; Soroban host test helpers may not mirror production event delivery exactly.

## Known risks (residual)

- **Asset movement:** This crate models escrow balances in contract state. Actual token custody and transfers must be integrated with Stellar asset contracts and audited separately.
- **Reputation:** Ratings are self-submitted by the freelancer after completion credits; treat reputation as informational, not as a gate for fund safety.
- **Governance:** A compromised or malicious governance admin can tighten or loosen governed parameters within validation rules. Operational multisig and timelocks are out of contract scope but required for mainnet.
- **Per-contract cap:** The mainnet total cap is fixed in code; if product needs a different ceiling, ship a new WASM revision before go-live.

## Tests

Focused unit tests live in `contracts/escrow/src/test/mainnet_readiness.rs` (defaults, governance reflection, hard-cap panic).
