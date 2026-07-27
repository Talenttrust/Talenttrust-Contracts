# Fuzz Testing the Escrow Contract

## Overview

Fuzz testing (property‑based testing) is used to automatically generate a wide range of inputs for the **Escrow** contract and verify that the implementation maintains its invariants under all possible edge‑cases.  

The escrow module includes a dedicated fuzz suite in `src/fuzz_test.rs`. The suite exercises:

| Property | Description |
|----------|-------------|
| **Milestone total** | The sum of all milestone amounts must never exceed `MAX_TOTAL_ESCROW_STROOPS`. |
| **Milestone count** | The number of milestones must stay ≤ `MAX_MILESTONES`. |
| **Deposit limits** | Deposits cannot over‑fund the contract and must respect the total cap. |
| **Unauthorized actions** | Only authorized roles (client, freelancer, arbiter) can call privileged functions. |
| **State transitions** | Contract moves through the correct states (`Created → Funded → Completed/Refunded`). |
| **Overflow safety** | All arithmetic uses `checked_*` helpers to prevent i128 overflow. |
| **Boundary handling** | Values at, just below and just above limits are explicitly tested. |

## How the Fuzz Suite Works

1. **Input Generation** – The `proptest` crate generates random vectors of `i128` milestone amounts, random deposit amounts, and random role combinations.
2. **Invariant Checks** – After each operation the suite calls the contract’s public methods and asserts that:
   * No panic occurs unless an expected error is triggered.
   * The contract’s internal balances (`funded_amount`, `released_amount`, `refunded_amount`) stay within the legal range.
   * The contract’s status matches the expected lifecycle phase.
3. **Boundary Cases** – Dedicated tests (`fuzz_milestone_total_exact`, `fuzz_milestone_total_over`) explicitly cover:
   * **Exact cap** (`MAX_TOTAL_ESCROW_STROOPS`) – should succeed.
   * **One‑above cap** (`MAX_TOTAL_ESCROW_STROOPS + 1`) – must be rejected with `EscrowError::InvalidMilestoneAmount`.
4. **State‑ful Fuzzing** – The suite runs a series of actions in random order (deposit, approve, release, refund) to ensure that the contract never reaches an illegal state.

## Running the Fuzz Tests

```bash
# From the escrow contract directory
cargo test --manifest-path Cargo.toml fuzz_test -- --nocapture
```

*The `--nocapture` flag prints the generated inputs when a test fails, making debugging easier.*

## Adding New Fuzz Scenarios

When adding new functionality (e.g., dispute resolution or fee logic) follow these steps:

1. **Define the property** you want to guarantee (e.g., “dispute can only be opened by the arbiter”).
2. **Add a new `proptest!` block** in `src/fuzz_test.rs` that:
   * Generates random role‑addresses and contract IDs.
   * Calls the new method.
   * Asserts the expected error or state transition.
3. **Update `docs/fuzzing.md`** with a short description of the new scenario and the property it checks.

### Example: New Fee Validation

```rust
proptest! {
    #[test]
    fn fuzz_fee_validation(
        fee_bps in 0u32..=10_000, // 0% – 100%
        total in 1i128..=MAX_TOTAL_ESCROW_STROOPS
    ) {
        // Setup contract with max total and fee
        let mut contract = create_contract_with_fee(fee_bps, total);
        // Deposit the full amount
        contract.deposit_funds(total).unwrap();
        // Verify that the fee stored equals total * fee_bps / 10_000
        assert_eq!(contract.protocol_fee(), total * fee_bps as i128 / 10_000);
    }
}
```

After adding the test, run the fuzz suite as described above to confirm that the fee logic never over‑charges or under‑charges.

## Documentation Generation

The fuzz suite doubles as an executable specification. Each test’s name and comments are extracted automatically for the docs page using the `cargo doc` system. Ensure that every test has a concise comment explaining the invariant it validates.

---

**Next Steps**

* Review `src/fuzz_test.rs` for any missing edge‑cases.
* Add the new fee‑validation test (or any other feature you plan).
* Regenerate the documentation with `cargo doc` and verify that the updated `fuzzing.md` reflects the new tests.

Feel free to ask if you need help writing a specific fuzz scenario or updating the documentation.
