# Dispute Resolution API Reference

## Dispute Resolution Types

### FullRefund
- **Code**: 0
- **Description**: Client receives 100% of escrowed funds
- **Use Case**: Work not delivered, contract breach by freelancer
- **Payout**: Client = 100%, Freelancer = 0%

### PartialRefund  
- **Code**: 1
- **Description**: Fixed 70/30 split favoring client
- **Use Case**: Partial delivery, quality issues
- **Payout**: Client = 70%, Freelancer = 30%

### FullPayout
- **Code**: 2  
- **Description**: Freelancer receives 100% of escrowed funds
- **Use Case**: Work completed as agreed, client dispute without merit
- **Payout**: Client = 0%, Freelancer = 100%

### Split
- **Code**: 3
- **Description**: Custom split determined by arbitrator
- **Use Case**: Complex situations requiring nuanced resolution
- **Payout**: Custom amounts (must total 100%)

## Function Signatures

### create_dispute
```rust
pub fn create_dispute(
    env: Env,
    contract_id: u32,
    reason: Symbol,
    evidence: Vec<Symbol>,
) -> u32
```

**Parameters:**
- `contract_id`: ID of the escrow contract
- `reason`: Symbol representing dispute reason (max 10 chars)
- `evidence`: Vector of evidence symbols

**Returns:**
- `u32`: Unique dispute ID

**Authorization:** Client or Freelancer

**Preconditions:**
- Contract must be in `Funded` state
- Caller must be client or freelancer
- No existing dispute for contract

**Postconditions:**
- Contract status changes to `Disputed`
- Dispute created with `Open` status

### resolve_dispute
```rust
pub fn resolve_dispute(
    env: Env,
    dispute_id: u32,
    resolution: DisputeResolution,
    client_payout: i128,
    freelancer_payout: i128,
) -> bool
```

**Parameters:**
- `dispute_id`: ID of the dispute to resolve
- `resolution`: Type of resolution (FullRefund, PartialRefund, FullPayout, Split)
- `client_payout`: Amount for client (only for Split resolution)
- `freelancer_payout`: Amount for freelancer (only for Split resolution)

**Returns:**
- `bool`: True if resolution successful

**Authorization:** Arbitrator only

**Preconditions:**
- Dispute must be in `Open` or `InReview` state
- Caller must be arbitrator
- For Split resolution: payouts must equal contract total

**Postconditions:**
- Dispute status changes to `Resolved`
- Contract status changes to `Resolved`
- Payout amounts calculated and stored

## Error Conditions

### create_dispute Errors
- `"contract not found"`: Invalid contract ID
- `"only client or freelancer can create dispute"`: Unauthorized caller
- `"invalid contract status"`: Contract not in Funded state
- Authorization failure: Caller not authenticated

### resolve_dispute Errors
- `"dispute not found"`: Invalid dispute ID
- `"arbitrator not set"`: Contract not properly initialized
- `"dispute already resolved"`: Dispute already resolved
- `"split amounts must equal total contract amount"`: Invalid split amounts
- Authorization failure: Caller not arbitrator

## State Transitions

### Contract State Flow
```
Created → Funded → Completed
           ↓
         Disputed → Resolved
```

### Dispute State Flow
```
Open → InReview → Resolved
```

## Security Considerations

### Access Control
- **Admin**: Can update arbitrator address
- **Arbitrator**: Can resolve disputes only
- **Client/Freelancer**: Can create disputes only
- **Public**: Can view contract and dispute data

### Financial Safety
- All payouts are mathematically validated
- No funds can be lost in the resolution process
- Deterministic outcomes prevent manipulation

### Audit Trail
- All actions timestamped
- Resolutions track which arbitrator decided
- Evidence stored permanently

## Integration Examples

### JavaScript/TypeScript
```typescript
// Create dispute
const disputeId = await contract.create_dispute({
  contractId: 1,
  reason: "quality_issues",
  evidence: ["photo_evidence", "chat_logs"]
});

// Resolve dispute (arbitrator)
await contract.resolve_dispute({
  disputeId: 1,
  resolution: "PartialRefund",
  clientPayout: 0,  // Ignored for PartialRefund
  freelancerPayout: 0  // Ignored for PartialRefund
});
```

### Rust
```rust
// Create dispute
let dispute_id = escrow.create_dispute(
    contract_id,
    symbol_short!("delay"),
    vec![symbol_short!("evidence1")]
);

// Resolve with custom split
escrow.resolve_dispute(
    dispute_id,
    DisputeResolution::Split,
    600_0000000,  // 60% to client
    400_0000000   // 40% to freelancer
);
```

## Best Practices

### For Clients
- Document all issues with evidence
- Create disputes promptly when issues arise
- Provide clear, concise reason codes

### For Freelancers  
- Maintain documentation of work completed
- Respond to disputes with counter-evidence if possible
- Monitor contract status regularly

### For Arbitrators
- Review all evidence carefully
- Use appropriate resolution type for situation
- Document rationale for custom splits
- Maintain impartiality and consistency
