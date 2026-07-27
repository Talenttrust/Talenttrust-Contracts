# EscrowClient Function Signatures

## Overview

`EscrowClient` is an auto-generated client binding provided by the Soroban SDK (version 22.0). It wraps the contract functions defined in `src/lib.rs` with a convenient interface for testing and client calls.

### How to Create an EscrowClient

```rust
// Register the contract and create a client
let contract_id = env.register(Escrow, ());
let client = EscrowClient::new(&env, &contract_id);
```

---

## Function Signatures

### 1. `create_contract`

**Contract Signature:**
```rust
pub fn create_contract(
    env: Env,
    client: Address,
    freelancer: Address,
    arbiter: Option<Address>,
    milestones: Vec<i128>,
    release_authorization: ReleaseAuthorization,
) -> u32
```

**Client Call:**
```rust
let contract_id = client.create_contract(
    &client_addr,
    &freelancer_addr,
    &Some(arbiter_addr),  // or &None
    &milestones,
    &ReleaseAuthorization::ClientOnly,
);
```

**Parameters:**
- `client: Address` - The client funding the contract (requires auth)
- `freelancer: Address` - The freelancer performing the work
- `arbiter: Option<Address>` - Optional arbiter for dispute resolution
- `milestones: Vec<i128>` - Vector of milestone amounts in stroops
- `release_authorization: ReleaseAuthorization` - Who can approve/release milestones

**Returns:**
- `u32` - The unique contract ID

**Errors:**
- `InvalidParticipants` - If client and freelancer are the same address
- `EmptyMilestones` - If no milestones are provided
- `InvalidMilestoneAmount` - If any milestone amount is ≤ 0
- `MissingArbiter` - If arbiter is required but not provided
- `InvalidArbiter` - If arbiter is same as client or freelancer

---

### 2. `deposit_funds`

**Contract Signature:**
```rust
pub fn deposit_funds(
    env: Env,
    contract_id: u32,
    caller: Address,
    amount: i128,
) -> bool
```

**Client Call:**
```rust
let success = client.deposit_funds(
    &contract_id,
    &caller_address,
    &1_200_0000000_i128,
);
```

**Parameters:**
- `contract_id: u32` - The contract ID to deposit into
- `caller: Address` - The address depositing funds (must be the contract client)
- `amount: i128` - The amount to deposit in stroops (must be > 0)

**Returns:**
- `bool` - `true` if deposit was successful

**State Transitions:**
- Contract transitions from `Created` → `Funded` when fully funded

**Errors:**
- `AmountMustBePositive` - If amount ≤ 0
- `ContractNotFound` - If contract doesn't exist
- `InvalidState` - If contract is not in `Created` state
- `UnauthorizedRole` - If caller is not the contract client

---

### 3. `approve_milestone_release`

**Contract Signature:**
```rust
pub fn approve_milestone_release(
    env: Env,
    contract_id: u32,
    caller: Address,
    milestone_index: u32,
) -> bool
```

**Client Call:**
```rust
let approved = client.approve_milestone_release(
    &contract_id,
    &approver_address,
    &milestone_index,
);
```

**Parameters:**
- `contract_id: u32` - The contract ID
- `caller: Address` - The approver's address (must be authorized)
- `milestone_index: u32` - The 0-based index of the milestone to approve

**Returns:**
- `bool` - `true` if approval was recorded successfully

**Storage:**
- Approvals are stored in temporary storage with automatic TTL expiry
- They automatically expire after `PENDING_APPROVAL_TTL_LEDGERS`
- Approvals are auto-evicted upon expiry

**Errors:**
- `ContractNotFound` - If contract doesn't exist
- `InvalidState` - If contract is not in `Funded` state
- `IndexOutOfBounds` - If milestone index is invalid
- `MilestoneAlreadyReleased` - If milestone was already released
- `AlreadyApproved` - If caller has already approved this milestone
- `UnauthorizedRole` - If caller is not authorized to approve (based on `ReleaseAuthorization` mode)

**Security:**
- Caller must be authenticated
- Only authorized parties can approve based on `ReleaseAuthorization` mode
- Approvals expire via TTL and are auto-evicted
- Duplicate approvals are rejected

---

### 4. `release_milestone`

**Contract Signature:**
```rust
pub fn release_milestone(
    env: Env,
    contract_id: u32,
    caller: Address,
    milestone_index: u32,
) -> bool
```

**Client Call:**
```rust
let released = client.release_milestone(
    &contract_id,
    &releaser_address,
    &milestone_index,
);
```

**Parameters:**
- `contract_id: u32` - The contract ID
- `caller: Address` - The releaser's address (must be authorized)
- `milestone_index: u32` - The 0-based index of the milestone to release

**Returns:**
- `bool` - `true` if release was successful

**Preconditions:**
- Contract must be in `Funded` state
- Valid, non-expired approvals must exist based on `ReleaseAuthorization` mode
- Milestone must not already be released or refunded
- Sufficient funds must be available

**State Transitions:**
- Milestone marked as `released = true`
- Contract transitions to `Completed` when all milestones are released or refunded
- Approvals are cleared after successful release

**Errors:**
- `ContractNotFound` - If contract doesn't exist
- `InvalidState` - If contract is not in `Funded` state
- `IndexOutOfBounds` - If milestone index is out of bounds
- `MilestoneAlreadyReleased` - If milestone was already released
- `AlreadyRefunded` - If milestone was already refunded
- `InsufficientFunds` - If contract doesn't have enough funded balance
- `InsufficientApprovals` - If required approvals are missing
- `ApprovalExpired` - If approvals have expired
- `UnauthorizedRole` - If caller is not authorized to release

**Security:**
- Requires valid approvals that haven't expired
- Fail-closed: missing or expired approvals prevent release
- Caller must be authenticated

---

## ReleaseAuthorization Enum

```rust
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseAuthorization {
    /// Only client can approve and release
    ClientOnly = 0,
    /// Either client or arbiter can approve and release
    ClientAndArbiter = 1,
    /// Only arbiter can approve and release
    ArbiterOnly = 2,
    /// Both client and arbiter must approve (multisig)
    MultiSig = 3,
}
```

---

## Example Test Usage

```rust
#[test]
fn example_workflow() {
    // Setup
    let env = Env::default();
    env.mock_all_auths();
    
    // Create client
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    
    // Generate addresses
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    
    // Create contract
    let milestones = vec![
        &env,
        500_0000000_i128,  // 500 XLM in stroops
        500_0000000_i128,  // 500 XLM in stroops
    ];
    
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(contract_id, 1);
    
    // Deposit funds (full amount)
    let total = 1_000_0000000_i128;
    assert!(client.deposit_funds(&contract_id, &client_addr, &total));
    
    // Approve first milestone
    assert!(client.approve_milestone_release(
        &contract_id,
        &client_addr,
        &0
    ));
    
    // Release first milestone
    assert!(client.release_milestone(
        &contract_id,
        &client_addr,
        &0
    ));
}
```

---

## Important Notes

1. **Address Parameter**: The `caller` parameter in `deposit_funds`, `approve_milestone_release`, and `release_milestone` represents the account performing the action. The contract uses this to verify authorization and apply role-based access control.

2. **Stroops**: All amounts are in stroops (1 stroops = 1e-7 XLM). The example uses `500_0000000` = 500 XLM.

3. **TTL for Approvals**: Approvals stored in temporary storage with automatic expiry. If approval expires before release, the release will fail.

4. **Mock Auth**: In tests, `env.mock_all_auths()` bypasses the actual signature verification, allowing any address to pass authorization checks.

5. **Contract ID**: Each registered contract gets a unique u32 contract ID. This ID is used in all subsequent operations.

6. **Soroban SDK Version**: This project uses `soroban-sdk = "22.0"`, and the `EscrowClient` is automatically generated by this SDK version.
