# Centralized Ledger Time Management

> **See also:** [`docs/escrow/ledger-time-source.md`](escrow/ledger-time-source.md)
> for the authoritative reference on precision, trust assumptions, and call sites.

## Overview

This project uses a centralized time management system to ensure deterministic behavior and reliable testing. All time-related operations must use the `now_seconds()` helper function.

## Architecture

### Core Helper Function

Located in `contracts/escrow/src/utils.rs`:

```rust
pub fn now_seconds(env: &Env) -> u64
```

This function is the **single source of truth** for all time operations in the contract.

### Why Centralized Time Management?

1. **Consistency**: All modules use the same time source
2. **Determinism**: Behavior is predictable and reproducible
3. **Testability**: Tests can control time precisely
4. **Security**: Time logic remains on-chain/ledger-side
5. **Maintainability**: Single point of change for time-related updates

## Usage

### In Contract Code

Always use `now_seconds()` instead of directly calling `env.ledger().timestamp()`:

```rust
use crate::utils::now_seconds;

// ✅ CORRECT
pub fn check_timeout(env: &Env, deadline: u64) -> bool {
    now_seconds(env) > deadline
}

// ❌ WRONG - Don't do this!
pub fn check_timeout(env: &Env, deadline: u64) -> bool {
    env.ledger().timestamp() > deadline
}
```

### Common Patterns

#### 1. Checking Expiration

```rust
pub fn is_milestone_expired(env: Env, deadline: u64) -> bool {
    now_seconds(&env) > deadline
}
```

#### 2. Scheduling Future Events

```rust
pub fn schedule_milestone(env: Env, duration_seconds: u64) -> u64 {
    now_seconds(&env) + duration_seconds
}
```

#### 3. Time Window Checks

```rust
pub fn can_dispute(env: Env, dispute_deadline: u64) -> bool {
    now_seconds(&env) <= dispute_deadline
}
```

## Testing

### Setting Ledger Time

In tests, use `env.ledger().set()` to control time:

```rust
use soroban_sdk::testutils::{Ledger, LedgerInfo};

#[test]
fn test_time_based_logic() {
    let env = Env::default();
    
    // Set initial time
    env.ledger().set(LedgerInfo {
        timestamp: 1_000_000,
        protocol_version: 20,
        sequence_number: 10,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 10,
        min_persistent_entry_ttl: 10,
        max_entry_ttl: 3110400,
    });
    
    // Your test logic here
}
```

### Advancing Time

To test time-dependent behavior, advance the ledger time:

```rust
#[test]
fn test_expiration() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    
    // Set initial time
    env.ledger().set(LedgerInfo {
        timestamp: 1_000_000,
        // ... other fields
    });
    
    let deadline = 1_500_000;
    assert!(!client.is_milestone_expired(&deadline));
    
    // Advance time past deadline
    env.ledger().set(LedgerInfo {
        timestamp: 1_600_000,
        // ... other fields
    });
    
    assert!(client.is_milestone_expired(&deadline));
}
```

### Test Examples

See `contracts/escrow/src/test.rs` for comprehensive examples:

- `test_schedule_milestone`: Scheduling future events
- `test_milestone_not_expired`: Checking unexpired deadlines
- `test_milestone_expired`: Checking expired deadlines
- `test_can_dispute_within_window`: Time window validation
- `test_time_advancement`: Simulating time progression
- `test_exact_deadline_boundary`: Edge case testing

## Security Considerations

### Timestamp Manipulation Resistance

The ledger timestamp is controlled by the Stellar network validators, not by contract callers. This means:

- ✅ Time cannot be manipulated by users
- ✅ Time is consensus-driven and trustworthy
- ✅ All nodes see the same time
- ✅ Time progresses monotonically

### Best Practices

1. **Never use wall-clock time**: Always use ledger time
2. **No magic numbers**: Define time constants clearly
3. **Use seconds**: Ledger timestamps are in seconds since Unix epoch
4. **Test edge cases**: Test exact boundary conditions (e.g., `deadline == current_time`)
5. **Document assumptions**: Clearly state time-related assumptions in comments

## Migration Guide

If you have existing code that directly accesses ledger time:

### Before
```rust
pub fn check_expired(env: &Env, deadline: u64) -> bool {
    env.ledger().timestamp() > deadline
}
```

### After
```rust
use crate::utils::now_seconds;

pub fn check_expired(env: &Env, deadline: u64) -> bool {
    now_seconds(env) > deadline
}
```

## Acceptance Criteria

- ✅ `now_seconds(env)` is the only method used to fetch ledger time
- ✅ No direct `env.ledger().timestamp()` calls outside `utils.rs`
- ✅ All tests use mocked ledger time via `env.ledger().set()`
- ✅ No reliance on real-world wall-clock time in tests
- ✅ Comprehensive documentation with examples
- ✅ Time logic remains server-side/ledger-side (secure)

## Time Constants Reference

Common time durations in seconds:

```rust
const MINUTE: u64 = 60;
const HOUR: u64 = 3_600;
const DAY: u64 = 86_400;
const WEEK: u64 = 604_800;
const MONTH_30: u64 = 2_592_000;
const YEAR: u64 = 31_536_000;
```

Example usage:
```rust
// Schedule milestone 7 days from now
let deadline = now_seconds(&env) + (7 * DAY);
```

## Troubleshooting

### Tests Failing Due to Time

If tests are flaky or time-dependent:

1. Ensure you're setting ledger time explicitly
2. Don't rely on default time values
3. Advance time explicitly between checks
4. Test boundary conditions (before, at, and after deadlines)

### Compilation Errors

If you see errors about `now_seconds`:

1. Ensure `mod utils;` is declared in `lib.rs`
2. Import with `use crate::utils::now_seconds;`
3. Pass `&env` reference to the function

## See also

- [`docs/escrow/ledger-time-source.md`](escrow/ledger-time-source.md) — comprehensive reference covering
  `now_seconds` precision/trust assumptions, every call site in the contract,
  ledger-vs-sequence time mechanisms, and deterministic test patterns with
  `env.ledger().with_mut()`.
- [`contracts/escrow/src/utils.rs`](../contracts/escrow/src/utils.rs) — the `now_seconds` definition.
- [`contracts/escrow/src/test/timeout_tests.rs`](../contracts/escrow/src/test/timeout_tests.rs) — worked examples.
