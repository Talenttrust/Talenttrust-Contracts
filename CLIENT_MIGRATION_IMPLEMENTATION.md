# Client Migration Implementation

## Overview

This implementation adds secure client account migration functionality to the Talenttrust escrow contract. The migration follows a two-step proposal + confirmation flow to ensure no unauthorized takeover of contract authority.

## Features

### Core Functionality
- **Proposal Phase**: Current client can propose migration to a new address
- **Confirmation Phase**: Proposed client must confirm the migration
- **Finalization**: Atomic update of contract client address
- **Cancellation**: Current client can cancel pending migration
- **Expiration**: Migrations expire after TTL to prevent stale proposals

### Security Features
- **Authorization**: Only current client can propose/cancel migration
- **Confirmation**: Only proposed client can confirm migration
- **Status Restrictions**: Migration only allowed in `Created` and `Funded` states
- **Duplicate Prevention**: No concurrent migrations allowed
- **Same Address Protection**: Cannot migrate to same address
- **Atomic Operations**: Migration finalization is atomic
- **Audit Trail**: Full event emissions for all migration steps

## Implementation Details

### Data Structures

```rust
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingClientMigration {
    pub current_client: Address,
    pub proposed_client: Address,
    pub proposed_client_confirmed: bool,
    pub requested_at_ledger: u32,
    pub expires_at_ledger: u32,
}
```

### Storage Keys

```rust
enum DataKey {
    // ... existing keys
    PendingClientMigration(u32),
}
```

### Public Methods

1. **propose_client_migration(env, contract_id, current_client, new_client) -> bool**
   - Propose migration to new address
   - Requires current client authorization
   - Emits `client_migration_proposed` event

2. **accept_client_migration(env, contract_id, new_client) -> bool**
   - Accept migration by proposed client
   - Requires proposed client authorization
   - Updates contract client address atomically
   - Emits `client_migration_accepted` event

3. **cancel_client_migration(env, contract_id, current_client) -> bool**
   - Cancel pending migration
   - Requires current client authorization
   - Removes pending migration entry
   - Emits `client_migration_cancelled` event

4. **get_pending_client_migration(env, contract_id) -> PendingClientMigration**
   - Get pending migration information
   - Panics with `InvalidState` if no pending migration exists

5. **has_pending_client_migration(env, contract_id) -> bool**
   - Check if migration is pending

### Status Restrictions

Migration is only allowed in these contract statuses:
- `Created` - Contract not yet funded
- `Funded` - Contract funded but not completed

Migration is NOT allowed in:
- `Completed` - Contract finished
- `Cancelled` - Contract cancelled
- `Disputed` - Contract under dispute
- `Refunded` - Contract refunded

### TTL Configuration

Migration proposals expire after `PENDING_MIGRATION_TTL_LEDGERS` (defined in ttl module).

### Event Emissions

All migration operations emit events with the following structure:
- `client_migration_proposed`: (contract_id, current_client, proposed_client, timestamp)
- `client_migration_accepted`: (contract_id, old_client, new_client, timestamp)
- `client_migration_cancelled`: (contract_id, current_client, timestamp)

### Security Considerations

### Authorization Model
- **Proposal**: Only current client can initiate migration
- **Acceptance**: Only proposed client can accept migration
- **Cancellation**: Only current client can cancel migration

### Attack Vectors Mitigated
1. **Unauthorized Takeover**: Requires both current and proposed client authorization
2. **Stale Proposals**: TTL-based expiration prevents indefinite pending migrations; alternatively, current client can cancel immediately
3. **Race Conditions**: Atomic acceptance prevents partial state updates
4. **Status Abuse**: Migration restricted to appropriate contract states
5. **Duplicate Migrations**: Only one pending migration allowed per contract (cancel previous to propose new one)
6. **Wrong Address Proposals**: Current client can cancel and re-propose if wrong address was proposed

### Audit Trail
All migration operations emit events providing:
- Complete migration timeline
- Participant addresses
- Operation timestamps
- Contract state changes

## Testing

The implementation includes comprehensive tests covering:

### Basic Functionality
- Migration proposal, confirmation, and finalization flow
- Authorization transfer verification
- Pending state management

### Security Tests
- Unauthorized proposal attempts
- Unauthorized confirmation attempts
- Same address migration prevention
- Double proposal prevention
- Status restriction enforcement

### Edge Cases
- Migration expiration (TTL)
- Contract integrity preservation
- Event emission verification
- Cancellation scenarios

### Integration Tests
- Migration with funded contracts
- Migration with milestone releases
- Authority transfer validation

## Usage Example

```rust
// 1. Current client proposes migration
client.propose_client_migration(contract_id, current_client_address, new_client_address);

// 2. Check pending migration
let pending = client.get_pending_client_migration(contract_id);
assert_eq!(pending.proposed_client, new_client_address);

// 3. Option A: Proposed client accepts migration (atomic update)
client.accept_client_migration(contract_id, new_client_address);

// Verify migration completed
let contract = client.get_contract(contract_id);
assert_eq!(contract.client, new_client_address);

// --- OR ---

// 3. Option B: Current client realizes wrong address and cancels
client.cancel_client_migration(contract_id, current_client_address);

// 4. Propose the correct address
let correct_client = Address::generate(&env);
client.propose_client_migration(contract_id, current_client_address, correct_client);

// 5. Correct client accepts
client.accept_client_migration(contract_id, correct_client);
```

## Error Handling

The implementation uses existing error codes where appropriate:
- `InvalidStatusTransition` - Migration not allowed in current state
- `UnauthorizedRole` - Authorization failures
- `InvalidParticipant` - Same address migration
- `AlreadyCancelled` - Duplicate migration proposal
- `ContractNotFound` - Missing contract or pending migration

## Future Enhancements

Potential future improvements:
1. **Migration Delays**: Add configurable delay between confirmation and finalization
2. **Multi-signature**: Support for multi-signature client accounts
3. **Migration Limits**: Rate limiting on migration frequency
4. **Emergency Controls**: Admin override capabilities for disputed cases

## Compliance

This implementation addresses all requirements from issue #250:
✅ Secure two-step proposal + confirmation flow
✅ No unauthorized takeover protection
✅ Atomic confirmation with audit trail
✅ Status-based migration restrictions
✅ Comprehensive test coverage
✅ Event emissions for all operations
✅ Documentation and security considerations
