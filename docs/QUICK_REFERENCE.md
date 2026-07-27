# Time Management Quick Reference

## The Rule

**Always use `now_seconds(env)` for time operations. Never call `env.ledger().timestamp()` directly.**

## Import

```rust
use crate::utils::now_seconds;
```

## Common Patterns

### Check if expired
```rust
pub fn is_expired(env: &Env, deadline: u64) -> bool {
    now_seconds(env) > deadline
}
```

### Schedule future event
```rust
pub fn schedule(env: &Env, duration: u64) -> u64 {
    now_seconds(env) + duration
}
```

### Check time window
```rust
pub fn within_window(env: &Env, deadline: u64) -> bool {
    now_seconds(env) <= deadline
}
```

## Testing

### Set time
```rust
use soroban_sdk::testutils::{Ledger, LedgerInfo};

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
```

### Advance time
```rust
// Set new time
env.ledger().set(LedgerInfo {
    timestamp: 2_000_000,  // Advanced time
    // ... other fields
});
```

## Time Constants

```rust
const MINUTE: u64 = 60;
const HOUR: u64 = 3_600;
const DAY: u64 = 86_400;
const WEEK: u64 = 604_800;
```

## Example Test

```rust
#[test]
fn test_expiration() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    // Set time
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

    // Test logic
    let deadline = 1_500_000;
    assert!(!client.is_milestone_expired(&deadline));
    
    // Advance time
    env.ledger().set(LedgerInfo {
        timestamp: 1_600_000,
        protocol_version: 20,
        sequence_number: 11,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 10,
        min_persistent_entry_ttl: 10,
        max_entry_ttl: 3110400,
    });
    
    assert!(client.is_milestone_expired(&deadline));
}
```

## Verification Checklist

- [ ] Imported `now_seconds` from `crate::utils`
- [ ] No direct `env.ledger().timestamp()` calls in contract code
- [ ] Tests set explicit ledger time
- [ ] Tests don't rely on real-world time
- [ ] Time-based logic is documented
