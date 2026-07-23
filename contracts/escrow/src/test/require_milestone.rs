//! Unit tests for `require_milestone` helper function.

use crate::test::EscrowFixture;
use crate::types::Error;
use crate::Escrow;

#[test]
fn test_require_milestone_in_range_ok() {
    let fixture = EscrowFixture::builder().funded().build();
    let env = &fixture.env;

    env.as_contract(&fixture.escrow_address, || {
        let milestone_0 = Escrow::require_milestone(env, fixture.escrow_id, 0);
        assert!(milestone_0.is_ok());
        let milestone_0 = milestone_0.unwrap();
        assert_eq!(milestone_0.amount, crate::test::MILESTONE_ONE);

        let milestone_1 = Escrow::require_milestone(env, fixture.escrow_id, 1);
        assert!(milestone_1.is_ok());
        let milestone_1 = milestone_1.unwrap();
        assert_eq!(milestone_1.amount, crate::test::MILESTONE_TWO);

        let milestone_2 = Escrow::require_milestone(env, fixture.escrow_id, 2);
        assert!(milestone_2.is_ok());
        let milestone_2 = milestone_2.unwrap();
        assert_eq!(milestone_2.amount, crate::test::MILESTONE_THREE);
    });
}

#[test]
fn test_require_milestone_out_of_range_rejected() {
    let fixture = EscrowFixture::builder().funded().build();
    let env = &fixture.env;

    env.as_contract(&fixture.escrow_address, || {
        // Fixture has 3 milestones (indices 0, 1, 2), index 3 is out of range
        let result = Escrow::require_milestone(env, fixture.escrow_id, 3);
        assert_eq!(result, Err(Error::IndexOutOfBounds));

        let result_large = Escrow::require_milestone(env, fixture.escrow_id, 999);
        assert_eq!(result_large, Err(Error::IndexOutOfBounds));
    });
}

#[test]
fn test_require_milestone_unknown_contract_rejected() {
    let fixture = EscrowFixture::builder().funded().build();
    let env = &fixture.env;

    let unknown_contract_id = 9999;
    env.as_contract(&fixture.escrow_address, || {
        let result = Escrow::require_milestone(env, unknown_contract_id, 0);
        assert_eq!(result, Err(Error::ContractNotFound));
    });
}
