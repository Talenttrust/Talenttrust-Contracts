//! Overflow-safe `NextContractId` allocation tests.

use super::{default_milestones, generated_participants, register_client};
use crate::{DataKey, Error, ReleaseAuthorization};
use soroban_sdk::{testutils::Address as _, Address, Env};

fn assert_error<T: core::fmt::Debug>(
    result: Result<
        Result<T, soroban_sdk::ConversionError>,
        Result<soroban_sdk::Error, soroban_sdk::InvokeError>,
    >,
    expected: Error,
) {
    match result {
        Err(Ok(e)) => {
            let expected_err: soroban_sdk::Error = expected.into();
            assert_eq!(e, expected_err);
        }
        other => panic!("expected {:?}, got {:?}", expected, other),
    }
}

#[test]
fn next_contract_id_overflow_at_u32_max() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register_client(&env);
    let (client_addr, freelancer_addr, _) = generated_participants(&env);
    let milestones = default_milestones(&env);

    env.as_contract(&escrow.address, || {
        env.storage()
            .persistent()
            .set(&DataKey::NextContractId, &u32::MAX);
    });

    let result = escrow.try_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_error(result, Error::ContractIdOverflow);

    env.as_contract(&escrow.address, || {
        let next: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::NextContractId)
            .unwrap();
        assert_eq!(next, u32::MAX);
    });
}

#[test]
fn next_contract_id_rejects_occupied_slot() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register_client(&env);
    let (client_addr, freelancer_addr, _) = generated_participants(&env);
    let milestones = default_milestones(&env);

    let existing_id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    env.as_contract(&escrow.address, || {
        env.storage()
            .persistent()
            .set(&DataKey::NextContractId, &existing_id);
    });

    let intruder = Address::generate(&env);
    let result = escrow.try_create_contract(
        &intruder,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_error(result, Error::ContractIdCollision);
}
