# Design Document: Fee Accounting Tests

## Overview

This design implements fee accounting functionality for the TalentTrust escrow smart contract on the Stellar network. The system calculates fees from milestone releases, handles fractional stroops through rounding, accumulates fees in a treasury, and supports fee splitting between multiple recipients.

The implementation integrates fee calculation directly into the existing `release_milestone` function, ensuring fees are deducted atomically during payment processing. The design prioritizes correctness through a rounding strategy that guarantees no stroops are lost or created, maintaining the invariant that `fee + net_amount = milestone_amount` for all transactions.

Key design principles:
- **Atomic fee processing**: Fees are calculated and deducted in the same transaction as milestone release
- **Zero-loss rounding**: All fractional stroops from fee calculations are allocated to the freelancer
- **Transparent accounting**: Treasury totals and fee splits are queryable and auditable
- **Security-first**: Fee configuration and treasury access are restricted to authorized administrators

## Architecture

### Component Integration

The fee accounting system integrates into the existing escrow contract architecture:

```
┌─────────────────────────────────────────────────────────┐
│                    Escrow Contract                       │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  ┌──────────────────┐         ┌──────────────────┐     │
│  │  Milestone       │         │  Fee Accounting  │     │
│  │  Management      │────────▶│  Module          │     │
│  │                  │         │                  │     │
│  │  - approve()     │         │  - calculate()   │     │
│  │  - release()     │         │  - split()       │     │
│  └──────────────────┘         │  - accumulate()  │     │
│                                └──────────────────┘     │
│                                         │               │
│                                         ▼               │
│                                ┌──────────────────┐     │
│                                │  Treasury        │     │
│                                │  Storage         │     │
│                                │                  │     │
│                                │  - total         │     │
│                                │  - recipients    │     │
│                                └──────────────────┘     │
│                                                          │
└─────────────────────────────────────────────────────────┘
```

### Data Flow

1. **Milestone Release Trigger**: Client or arbiter calls `release_milestone()`
2. **Fee Calculation**: System calculates `fee = milestone_amount * fee_rate` (rounded down)
3. **Remainder Handling**: Remainder `r = milestone_amount - fee` is added to net amount
4. **Fee Split** (if enabled): Fee is distributed to multiple recipients based on percentages
5. **Treasury Update**: Each recipient's accumulated total is incremented
6. **Payment Transfer**: Net amount is transferred to freelancer
7. **State Persistence**: Updated treasury totals are saved to storage

## Components and Interfaces

### FeeConfig Structure

Stores the global fee configuration for the contract:

```rust
#[contracttype]
#[derive(Clone, Debug)]
pub struct FeeConfig {
    /// Fee rate as basis points (e.g., 250 = 2.5%)
    pub rate_bps: u32,
    
    /// Whether fee splitting is enabled
    pub split_enabled: bool,
    
    /// Fee split recipients and their percentages
    pub recipients: Vec<FeeRecipient>,
}
```

**Constraints**:
- `rate_bps` must be between 0 and 1000 (0% to 10%)
- If `split_enabled` is true, `recipients` must have 1-3 entries
- Sum of all recipient percentages must equal 10000 (100%)

### FeeRecipient Structure

Defines a single fee recipient and their share:

```rust
#[contracttype]
#[derive(Clone, Debug)]
pub struct FeeRecipient {
    /// Address to receive fees
    pub address: Address,
    
    /// Percentage as basis points (e.g., 7000 = 70%)
    pub percentage_bps: u32,
    
    /// Whether this is the primary recipient (receives rounding remainders)
    pub is_primary: bool,
}
```

### Treasury Structure

Tracks accumulated fees for each recipient:

```rust
#[contracttype]
#[derive(Clone, Debug)]
pub struct Treasury {
    /// Total fees accumulated across all recipients
    pub total: i128,
    
    /// Per-recipient accumulated fees
    pub balances: Map<Address, i128>,
}
```

### Fee Calculation Result

Internal structure returned by fee calculation:

```rust
pub struct FeeCalculation {
    /// Total fee amount (rounded down)
    pub fee_amount: i128,
    
    /// Net amount to freelancer (includes rounding remainder)
    pub net_amount: i128,
    
    /// Fee splits per recipient (if splitting enabled)
    pub splits: Vec<FeeSplit>,
}

pub struct FeeSplit {
    pub recipient: Address,
    pub amount: i128,
}
```

### Public Functions

#### configure_fees

```rust
pub fn configure_fees(
    env: Env,
    admin: Address,
    rate_bps: u32,
    recipients: Option<Vec<FeeRecipient>>,
) -> bool
```

**Purpose**: Configure or update fee rate and split recipients

**Authorization**: Only contract admin

**Parameters**:
- `admin`: Address of the administrator (must be authorized)
- `rate_bps`: Fee rate in basis points (0-1000)
- `recipients`: Optional fee split configuration

**Returns**: `true` on success

**Errors**:
- Panics if caller is not admin
- Panics if `rate_bps` > 1000
- Panics if recipients percentages don't sum to 10000
- Panics if more than 3 recipients specified

#### get_treasury_total

```rust
pub fn get_treasury_total(env: Env) -> i128
```

**Purpose**: Query the total accumulated fees

**Returns**: Total fees in stroops

#### get_recipient_balance

```rust
pub fn get_recipient_balance(env: Env, recipient: Address) -> i128
```

**Purpose**: Query accumulated fees for a specific recipient

**Returns**: Recipient's fee balance in stroops

#### withdraw_fees

```rust
pub fn withdraw_fees(
    env: Env,
    admin: Address,
    recipient: Address,
    amount: i128,
) -> bool
```

**Purpose**: Withdraw accumulated fees to a recipient

**Authorization**: Only contract admin

**Errors**:
- Panics if caller is not admin
- Panics if amount exceeds recipient's balance
- Panics if amount is negative or zero

### Internal Functions

#### calculate_fee

```rust
fn calculate_fee(
    env: &Env,
    milestone_amount: i128,
    config: &FeeConfig,
) -> FeeCalculation
```

**Purpose**: Calculate fee, net amount, and splits for a milestone

**Algorithm**:
1. Calculate raw fee: `raw_fee = (milestone_amount * rate_bps) / 10000`
2. Round down to whole stroops: `fee = raw_fee.floor()`
3. Calculate net: `net = milestone_amount - fee`
4. If splitting enabled, distribute fee among recipients
5. Allocate any rounding remainder to primary recipient

**Returns**: `FeeCalculation` with all amounts

#### split_fee

```rust
fn split_fee(
    env: &Env,
    total_fee: i128,
    recipients: &Vec<FeeRecipient>,
) -> Vec<FeeSplit>
```

**Purpose**: Distribute a fee amount among multiple recipients

**Algorithm**:
1. For each recipient, calculate: `share = (total_fee * percentage_bps) / 10000`
2. Round down each share
3. Calculate remainder: `remainder = total_fee - sum(shares)`
4. Add remainder to primary recipient's share

**Returns**: Vector of fee splits

#### update_treasury

```rust
fn update_treasury(
    env: &Env,
    splits: Vec<FeeSplit>,
)
```

**Purpose**: Update treasury storage with new fee amounts

**Algorithm**:
1. Load current treasury from storage
2. For each split, increment recipient's balance
3. Increment total treasury amount
4. Save updated treasury to storage

## Data Models

### Storage Keys

The contract uses the following storage keys:

```rust
// Fee configuration
const FEE_CONFIG: Symbol = symbol_short!("fee_cfg");

// Treasury data
const TREASURY: Symbol = symbol_short!("treasury");

// Admin address
const ADMIN: Symbol = symbol_short!("admin");
```

### Storage Layout

```
Persistent Storage:
├── "fee_cfg" → FeeConfig
├── "treasury" → Treasury
├── "admin" → Address
└── "contract" → EscrowContract (existing)
```

### Initialization

On contract deployment:
1. Admin address is set to deployer
2. FeeConfig is initialized with `rate_bps = 0` (no fees)
3. Treasury is initialized with `total = 0` and empty balances map

### Fee Rate Representation

Fee rates are stored as basis points (bps) where:
- 1 bps = 0.01%
- 100 bps = 1%
- 10000 bps = 100%

Examples:
- 2.5% fee = 250 bps
- 5% fee = 500 bps
- 10% fee = 1000 bps

This representation avoids floating-point arithmetic and provides precision to 0.01%.

## Rounding Strategy

### Problem

Stellar uses stroops as the smallest unit (1 XLM = 10,000,000 stroops). Fee calculations often produce fractional stroops that cannot be represented:

```
milestone_amount = 1000 stroops
fee_rate = 2.5% (250 bps)
raw_fee = 1000 * 250 / 10000 = 25 stroops (exact)

milestone_amount = 1001 stroops
fee_rate = 2.5%
raw_fee = 1001 * 250 / 10000 = 25.025 stroops (fractional!)
```

### Solution

**Round down fees, allocate remainder to freelancer**:

1. Calculate fee: `fee = floor((milestone_amount * rate_bps) / 10000)`
2. Calculate net: `net = milestone_amount - fee`
3. Verify: `fee + net == milestone_amount` (always true)

This strategy:
- Ensures no stroops are lost or created
- Favors the freelancer (they receive rounding benefit)
- Simplifies implementation (no complex remainder tracking)
- Maintains conservation property

### Examples

**Example 1: Exact division**
```
milestone = 10000 stroops
rate = 5% (500 bps)
fee = (10000 * 500) / 10000 = 500 stroops
net = 10000 - 500 = 9500 stroops
check: 500 + 9500 = 10000 ✓
```

**Example 2: Fractional result**
```
milestone = 1001 stroops
rate = 2.5% (250 bps)
fee = floor((1001 * 250) / 10000) = floor(25.025) = 25 stroops
net = 1001 - 25 = 976 stroops
check: 25 + 976 = 1001 ✓
```

**Example 3: Fee rounds to zero**
```
milestone = 10 stroops
rate = 2.5% (250 bps)
fee = floor((10 * 250) / 10000) = floor(0.25) = 0 stroops
net = 10 - 0 = 10 stroops
check: 0 + 10 = 10 ✓
```

### Fee Split Rounding

When fees are split among multiple recipients, the same strategy applies:

1. Calculate each recipient's share (rounded down)
2. Sum all shares
3. Calculate remainder: `remainder = total_fee - sum(shares)`
4. Add remainder to primary recipient

**Example**:
```
total_fee = 100 stroops
recipients:
  - Platform: 70% → 70 stroops
  - Referrer: 30% → 30 stroops
sum = 100 stroops
remainder = 0 stroops (no adjustment needed)

total_fee = 101 stroops
recipients:
  - Platform: 70% → floor(70.7) = 70 stroops
  - Referrer: 30% → floor(30.3) = 30 stroops
sum = 100 stroops
remainder = 1 stroop → added to Platform (primary)
final: Platform = 71, Referrer = 30
check: 71 + 30 = 101 ✓
```

## Integration with release_milestone

The existing `release_milestone` function is modified to integrate fee accounting:

### Modified Function Flow

```rust
pub fn release_milestone(
    env: Env,
    contract_id: u32,
    caller: Address,
    milestone_id: u32,
) -> bool {
    // [Existing validation logic...]
    
    // NEW: Load fee configuration
    let fee_config = load_fee_config(&env);
    
    // NEW: Calculate fees
    let fee_calc = calculate_fee(&env, milestone.amount, &fee_config);
    
    // NEW: Update treasury
    if fee_calc.fee_amount > 0 {
        update_treasury(&env, fee_calc.splits);
    }
    
    // MODIFIED: Transfer net amount instead of full amount
    // transfer_to_freelancer(&env, contract.freelancer, fee_calc.net_amount);
    
    // [Existing milestone update logic...]
    
    true
}
```

### Backward Compatibility

- If `fee_rate = 0`, the function behaves identically to the original
- No changes to function signature or external interface
- Existing tests continue to pass with zero fee configuration

