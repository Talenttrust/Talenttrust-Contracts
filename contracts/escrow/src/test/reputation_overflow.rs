use super::{complete_contract, register_client};
use crate::{DataKey, EscrowError, ReleaseAuthorization};
use soroban_sdk::{Address, Env, String};

fn valid_comment(env: &Env) -> String {
    String::from_str(env, "Great job!")
}

#[test]
fn reputation_arithmetic_handles_normal_values() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, contract_id) = complete_contract(&env, &client);
    
    // Normal operation should work
    assert!(client.issue_reputation(&contract_id, &client_addr, &5, &valid_comment(&env)));
    
    let rep = client.get_reputation(&freelancer_addr).unwrap();
    assert_eq!(rep.completed_contracts, 1);
    assert_eq!(rep.total_rating, 5);
}

#[test]
fn reputation_arithmetic_handles_many_contracts() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let freelancer_addr = Address::generate(&env);
    
    // Simulate 100 contracts (realistic high volume)
    for i in 0..100 {
        let client_addr = Address::generate(&env);
        let contract_id = client.create_contract(
            &client_addr,
            &freelancer_addr,
            &None,
            &super::default_milestones(&env),
            &ReleaseAuthorization::ClientOnly,
        );
        let total = super::total_milestone_amount();
        client.deposit_funds(&contract_id, &client_addr, &total);
        for milestone_index in 0..3u32 {
            client.approve_milestone_release(&contract_id, &client_addr, &milestone_index);
            client.release_milestone(&contract_id, &client_addr, &milestone_index);
        }
        client.issue_reputation(&contract_id, &client_addr, &5, &valid_comment(&env));
    }
    
    let rep = client.get_reputation(&freelancer_addr).unwrap();
    assert_eq!(rep.completed_contracts, 100);
    assert_eq!(rep.total_rating, 500);
}

#[test]
fn get_average_rating_uses_checked_arithmetic() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    
    // Test that get_average_rating uses checked arithmetic
    // by simulating a reputation with extreme values
    let freelancer_addr = Address::generate(&env);
    let rep_key = DataKey::Reputation(freelancer_addr.clone());
    
    // Create a reputation with values that could cause overflow in unchecked arithmetic
    let mut rep = crate::types::Reputation::default();
    rep.completed_contracts = 1;
    rep.total_rating = i128::MAX / 10_000 - 1; // Just below overflow threshold
    rep.last_rating = 5;
    
    env.storage().persistent().set(&rep_key, &rep);
    
    // This should not overflow due to checked arithmetic
    let avg = client.get_average_rating(&freelancer_addr);
    assert!(avg.is_some());
}

#[test]
fn get_average_rating_handles_zero_completed_contracts() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let freelancer_addr = Address::generate(&env);
    
    // Create a reputation with zero completed contracts
    let rep_key = DataKey::Reputation(freelancer_addr.clone());
    let mut rep = crate::types::Reputation::default();
    rep.completed_contracts = 0;
    rep.total_rating = 100;
    rep.last_rating = 5;
    
    env.storage().persistent().set(&rep_key, &rep);
    
    // Should return None to avoid division by zero
    let avg = client.get_average_rating(&freelancer_addr);
    assert!(avg.is_none());
}

#[test]
fn reputation_increment_does_not_overflow_at_realistic_values() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let freelancer_addr = Address::generate(&env);
    
    // Simulate a freelancer with very high completed_contracts
    // but still within realistic bounds (not i128::MAX)
    let rep_key = DataKey::Reputation(freelancer_addr.clone());
    let mut rep = crate::types::Reputation::default();
    rep.completed_contracts = 1_000_000; // 1 million contracts
    rep.total_rating = 5_000_000; // Average rating of 5
    rep.last_rating = 5;
    
    env.storage().persistent().set(&rep_key, &rep);
    
    // Add one more contract
    let client_addr = Address::generate(&env);
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &super::default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );
    let total = super::total_milestone_amount();
    client.deposit_funds(&contract_id, &client_addr, &total);
    for milestone_index in 0..3u32 {
        client.approve_milestone_release(&contract_id, &client_addr, &milestone_index);
        client.release_milestone(&contract_id, &client_addr, &milestone_index);
    }
    
    // This should succeed without overflow
    assert!(client.issue_reputation(&contract_id, &client_addr, &5, &valid_comment(&env)));
    
    let updated_rep = client.get_reputation(&freelancer_addr).unwrap();
    assert_eq!(updated_rep.completed_contracts, 1_000_001);
    assert_eq!(updated_rep.total_rating, 5_000_005);
}

#[test]
fn total_rating_addition_does_not_overflow_at_realistic_values() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let freelancer_addr = Address::generate(&env);
    
    // Simulate a freelancer with very high total_rating
    // but still within realistic bounds
    let rep_key = DataKey::Reputation(freelancer_addr.clone());
    let mut rep = crate::types::Reputation::default();
    rep.completed_contracts = 1_000_000;
    rep.total_rating = i128::MAX / 2; // Very high but not near overflow
    rep.last_rating = 5;
    
    env.storage().persistent().set(&rep_key, &rep);
    
    // Add one more contract with max rating
    let client_addr = Address::generate(&env);
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &super::default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );
    let total = super::total_milestone_amount();
    client.deposit_funds(&contract_id, &client_addr, &total);
    for milestone_index in 0..3u32 {
        client.approve_milestone_release(&contract_id, &client_addr, &milestone_index);
        client.release_milestone(&contract_id, &client_addr, &milestone_index);
    }
    
    // This should succeed without overflow
    assert!(client.issue_reputation(&contract_id, &client_addr, &5, &valid_comment(&env)));
    
    let updated_rep = client.get_reputation(&freelancer_addr).unwrap();
    assert_eq!(updated_rep.completed_contracts, 1_000_001);
    assert_eq!(updated_rep.total_rating, (i128::MAX / 2) + 5);
}

#[test]
fn pending_credits_subtraction_is_protected_by_check() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    
    // Try to issue reputation without pending credits
    let freelancer_addr = Address::generate(&env);
    let rep_key = DataKey::Reputation(freelancer_addr.clone());
    let rep = crate::types::Reputation::default();
    env.storage().persistent().set(&rep_key, &rep);
    
    // Set pending credits to 0
    let pending_key = DataKey::PendingReputationCredits(freelancer_addr.clone());
    env.storage().persistent().set(&pending_key, &0_i128);
    
    let client_addr = Address::generate(&env);
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &super::default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );
    let total = super::total_milestone_amount();
    client.deposit_funds(&contract_id, &client_addr, &total);
    for milestone_index in 0..3u32 {
        client.approve_milestone_release(&contract_id, &client_addr, &milestone_index);
        client.release_milestone(&contract_id, &client_addr, &milestone_index);
    }
    
    // This should fail with InvalidState, not underflow
    let result = client.try_issue_reputation(&contract_id, &client_addr, &5, &valid_comment(&env));
    super::assert_contract_error(result, EscrowError::InvalidState);
}

#[test]
fn completed_contracts_overflow_is_detected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let freelancer_addr = Address::generate(&env);
    
    // Set completed_contracts to i128::MAX to test overflow detection
    let rep_key = DataKey::Reputation(freelancer_addr.clone());
    let mut rep = crate::types::Reputation::default();
    rep.completed_contracts = i128::MAX;
    rep.total_rating = i128::MAX;
    rep.last_rating = 5;
    
    env.storage().persistent().set(&rep_key, &rep);
    
    // Set pending credits to 1 to allow reputation issuance
    let pending_key = DataKey::PendingReputationCredits(freelancer_addr.clone());
    env.storage().persistent().set(&pending_key, &1_i128);
    
    let client_addr = Address::generate(&env);
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &super::default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );
    let total = super::total_milestone_amount();
    client.deposit_funds(&contract_id, &client_addr, &total);
    for milestone_index in 0..3u32 {
        client.approve_milestone_release(&contract_id, &client_addr, &milestone_index);
        client.release_milestone(&contract_id, &client_addr, &milestone_index);
    }
    
    // This should fail with ArithmeticOverflow
    let result = client.try_issue_reputation(&contract_id, &client_addr, &5, &valid_comment(&env));
    super::assert_contract_error(result, EscrowError::ArithmeticOverflow);
}

#[test]
fn total_rating_overflow_is_detected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let freelancer_addr = Address::generate(&env);
    
    // Set total_rating to i128::MAX to test overflow detection
    let rep_key = DataKey::Reputation(freelancer_addr.clone());
    let mut rep = crate::types::Reputation::default();
    rep.completed_contracts = 1_000_000;
    rep.total_rating = i128::MAX;
    rep.last_rating = 5;
    
    env.storage().persistent().set(&rep_key, &rep);
    
    // Set pending credits to 1 to allow reputation issuance
    let pending_key = DataKey::PendingReputationCredits(freelancer_addr.clone());
    env.storage().persistent().set(&pending_key, &1_i128);
    
    let client_addr = Address::generate(&env);
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &super::default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );
    let total = super::total_milestone_amount();
    client.deposit_funds(&contract_id, &client_addr, &total);
    for milestone_index in 0..3u32 {
        client.approve_milestone_release(&contract_id, &client_addr, &milestone_index);
        client.release_milestone(&contract_id, &client_addr, &milestone_index);
    }
    
    // This should fail with ArithmeticOverflow
    let result = client.try_issue_reputation(&contract_id, &client_addr, &5, &valid_comment(&env));
    super::assert_contract_error(result, EscrowError::ArithmeticOverflow);
}
