use soroban_sdk::vec;

use crate::{ContractStatus, ReleaseAuthorization};

use super::{assert_contract_state, create_client, setup};

/// Tests that contract creation persists milestones correctly.
/// 
/// # Security
/// - Validates contract initialization
/// - Ensures milestone data integrity
/// - Verifies initial state is Created
#[test]
fn creates_contract_and_persists_milestones() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128, 600_0000000_i128];

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    assert_eq!(contract_id, 1);

    let contract = client.get_contract(&contract_id);
    assert_contract_state(contract, ContractStatus::Created, 0, 0, 0);

    let stored_milestones = client.get_milestones(&contract_id);
    assert_eq!(stored_milestones.len(), 3);
    assert_eq!(stored_milestones.get(0).unwrap().amount, 200_0000000_i128);
    assert_eq!(stored_milestones.get(1).unwrap().amount, 400_0000000_i128);
    assert_eq!(stored_milestones.get(2).unwrap().amount, 600_0000000_i128);
}

/// Tests that contract creation with empty milestones is rejected.
/// 
/// # Security
/// - Prevents invalid contract initialization
/// - Validates input sanitization
#[test]
#[should_panic]
fn rejects_empty_milestones() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);

    let milestones = vec![&env];
    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
}

/// Tests that contract creation with zero-amount milestone is rejected.
/// 
/// # Security
/// - Prevents dust attacks
/// - Validates milestone amount constraints
#[test]
#[should_panic]
fn rejects_zero_amount_milestone() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);

    let milestones = vec![&env, 0_i128];
    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
}

/// Tests that contract creation with same client and freelancer is rejected.
/// 
/// # Security
/// - Prevents self-dealing
/// - Validates participant uniqueness
#[test]
#[should_panic]
fn rejects_same_participants() {
    let (env, client_addr, _) = setup();
    let client = create_client(&env);

    let milestones = vec![&env, 100_0000000_i128];
    client.create_contract(
        &client_addr,
        &client_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
}
