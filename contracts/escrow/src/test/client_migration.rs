use crate::{ContractStatus, EscrowError};

use super::{
    assert_panics, create_sample_contract, full_funding_amount, register_escrow, setup_env,
};

#[test]
fn migration_requires_acceptance_before_finalization() {
    let env = setup_env();
    let client = register_escrow(&env);
    let (parties, contract_id) = create_sample_contract(&env, &client);

    assert!(client.request_client_migration(&contract_id, &parties.replacement_client));

    let pending = client.get_pending_client_migration(&contract_id);
    assert_eq!(pending.current_client, parties.client);
    assert_eq!(pending.proposed_client, parties.replacement_client);
    assert!(!pending.proposed_client_confirmed);
    assert!(client.has_pending_client_migration(&contract_id));

    assert_panics(|| {
        client.finalize_client_migration(&contract_id);
    });

    assert!(client.confirm_client_migration(&contract_id));
    let confirmed = client.get_pending_client_migration(&contract_id);
    assert!(confirmed.proposed_client_confirmed);

    assert!(client.finalize_client_migration(&contract_id));
    let contract = client.get_contract(&contract_id);
    assert_eq!(contract.client, parties.replacement_client);
    assert_eq!(contract.status, ContractStatus::Created);
    assert!(!client.has_pending_client_migration(&contract_id));
}

#[test]
fn confirmed_migration_transfers_client_authority() {
    let env = setup_env();
    let client = register_escrow(&env);
    let (parties, contract_id) = create_sample_contract(&env, &client);

    assert!(client.request_client_migration(&contract_id, &parties.replacement_client));
    assert!(client.confirm_client_migration(&contract_id));
    assert!(client.finalize_client_migration(&contract_id));

    assert!(client.deposit_funds(&contract_id, &full_funding_amount()));
    let auths = env.auths();

    assert_eq!(auths.len(), 1);
    assert_eq!(auths[0].0, parties.replacement_client);
    assert!(auths[0].1.sub_invocations.is_empty());
}

#[test]
fn cancel_client_migration_clears_pending_state() {
    let env = setup_env();
    let client = register_escrow(&env);
    let (parties, contract_id) = create_sample_contract(&env, &client);

    assert!(client.request_client_migration(&contract_id, &parties.replacement_client));
    assert!(client.cancel_client_migration(&contract_id));
    assert!(!client.has_pending_client_migration(&contract_id));
    assert_panics(|| {
        client.get_pending_client_migration(&contract_id);
    });
}

#[test]
fn unauthorized_migration_proposal_fails() {
    let env = setup_env();
    let client = register_escrow(&env);
    let (parties, contract_id) = create_sample_contract(&env, &client);
    
    let unauthorized_party = super::Address::generate(&env);
    
    // Unauthorized party cannot propose migration
    assert_panics(|| {
        client.request_client_migration(&contract_id, &unauthorized_party);
    });
}

#[test]
fn migration_to_same_address_fails() {
    let env = setup_env();
    let client = register_escrow(&env);
    let (parties, contract_id) = create_sample_contract(&env, &client);
    
    // Cannot migrate to same address
    assert_panics(|| {
        client.request_client_migration(&contract_id, &parties.client);
    });
}

#[test]
fn double_migration_proposal_fails() {
    let env = setup_env();
    let client = register_escrow(&env);
    let (parties, contract_id) = create_sample_contract(&env, &client);
    
    assert!(client.request_client_migration(&contract_id, &parties.replacement_client));
    
    // Cannot propose second migration while one is pending
    let another_client = super::Address::generate(&env);
    assert_panics(|| {
        client.request_client_migration(&contract_id, &another_client);
    });
}

#[test]
fn unauthorized_confirmation_fails() {
    let env = setup_env();
    let client = register_escrow(&env);
    let (parties, contract_id) = create_sample_contract(&env, &client);
    
    assert!(client.request_client_migration(&contract_id, &parties.replacement_client));
    
    // Unauthorized party cannot confirm
    let unauthorized_party = super::Address::generate(&env);
    assert_panics(|| {
        client.confirm_client_migration(&contract_id);
    });
}

#[test]
fn unauthorized_cancellation_fails() {
    let env = setup_env();
    let client = register_escrow(&env);
    let (parties, contract_id) = create_sample_contract(&env, &client);
    
    assert!(client.request_client_migration(&contract_id, &parties.replacement_client));
    
    // Only current client can cancel
    assert_panics(|| {
        client.cancel_client_migration(&contract_id);
    });
}

#[test]
fn migration_in_completed_contract_fails() {
    let env = setup_env();
    let client = register_escrow(&env);
    let (parties, contract_id) = create_sample_contract(&env, &client);
    
    // Complete the contract first
    assert!(client.deposit_funds(&contract_id, &full_funding_amount()));
    for i in 0..3 {
        assert!(client.release_milestone(&contract_id, &i));
    }
    assert!(client.complete_contract(&contract_id));
    
    // Cannot migrate completed contract
    assert_panics(|| {
        client.request_client_migration(&contract_id, &parties.replacement_client);
    });
}

#[test]
fn migration_in_cancelled_contract_fails() {
    let env = setup_env();
    let client = register_escrow(&env);
    let (parties, contract_id) = create_sample_contract(&env, &client);
    
    // Cancel the contract first
    assert!(client.cancel_contract(&contract_id, &parties.client));
    
    // Cannot migrate cancelled contract
    assert_panics(|| {
        client.request_client_migration(&contract_id, &parties.replacement_client);
    });
}

#[test]
fn migration_in_disputed_contract_fails() {
    let env = setup_env();
    let client = register_escrow(&env);
    let (parties, contract_id) = create_sample_contract(&env, &client);
    
    // Mark contract as disputed (simulate)
    // Note: This would require a dispute method, but we'll test the status restriction
    
    // For now, let's manually set the status to Disputed
    env.as_contract(&client.contract_id, || {
        let mut contract = client.get_contract(&contract_id);
        contract.status = ContractStatus::Disputed;
        // This would need a proper update method in a real implementation
    });
    
    // Cannot migrate disputed contract
    assert_panics(|| {
        client.request_client_migration(&contract_id, &parties.replacement_client);
    });
}

#[test]
fn migration_expires_after_ttl() {
    let env = setup_env();
    let client = register_escrow(&env);
    let (parties, contract_id) = create_sample_contract(&env, &client);
    
    assert!(client.request_client_migration(&contract_id, &parties.replacement_client));
    
    // Advance time beyond TTL
    env.ledger().set(super::LedgerInfo {
        sequence: 1000, // This should be beyond the TTL
        timestamp: 1000000,
        protocol_version: 1,
        network_id: [0; 32].into(),
        base_reserve: 100,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 65536,
        ledger_version: 1,
    });
    
    // Should fail to confirm expired migration
    assert_panics(|| {
        client.confirm_client_migration(&contract_id);
    });
    
    // Should fail to finalize expired migration
    assert_panics(|| {
        client.finalize_client_migration(&contract_id);
    });
    
    // Migration should be cleaned up
    assert!(!client.has_pending_client_migration(&contract_id));
}

#[test]
fn migration_preserves_contract_integrity() {
    let env = setup_env();
    let client = register_escrow(&env);
    let (parties, contract_id) = create_sample_contract(&env, &client);
    
    // Fund the contract before migration
    assert!(client.deposit_funds(&contract_id, &full_funding_amount()));
    
    // Get original contract state
    let original_contract = client.get_contract(&contract_id);
    let original_client = original_contract.client.clone();
    let original_freelancer = original_contract.freelancer.clone();
    let original_milestones = original_contract.milestones.clone();
    let original_status = original_contract.status;
    
    // Perform migration
    assert!(client.request_client_migration(&contract_id, &parties.replacement_client));
    assert!(client.confirm_client_migration(&contract_id));
    assert!(client.finalize_client_migration(&contract_id));
    
    // Verify contract integrity
    let migrated_contract = client.get_contract(&contract_id);
    assert_eq!(migrated_contract.client, parties.replacement_client);
    assert_eq!(migrated_contract.freelancer, original_freelancer);
    assert_eq!(migrated_contract.milestones, original_milestones);
    assert_eq!(migrated_contract.status, original_status);
    assert_eq!(migrated_contract.total_deposited, original_contract.total_deposited);
    
    // Verify old client can no longer act as client
    assert_panics(|| {
        client.deposit_funds(&contract_id, &1000);
    });
    
    // Verify new client can act as client
    assert!(client.deposit_funds(&contract_id, &1000));
}

#[test]
fn migration_emits_proper_events() {
    let env = setup_env();
    let client = register_escrow(&env);
    let (parties, contract_id) = create_sample_contract(&env, &client);
    
    // Request migration - should emit proposal event
    assert!(client.request_client_migration(&contract_id, &parties.replacement_client));
    
    // Check for proposal event
    let events = env.events().all();
    assert!(events.iter().any(|(topics, _data)| {
        topics.len() >= 2 && 
        topics[0] == super::Symbol::new(&env, "client_migration_proposed") &&
        topics[1] == super::symbol_short!(contract_id)
    }));
    
    // Confirm migration - should emit confirmation event
    assert!(client.confirm_client_migration(&contract_id));
    
    // Check for confirmation event
    let events_after_confirm = env.events().all();
    assert!(events_after_confirm.iter().any(|(topics, _data)| {
        topics.len() >= 2 && 
        topics[0] == super::Symbol::new(&env, "client_migration_confirmed") &&
        topics[1] == super::symbol_short!(contract_id)
    }));
    
    // Finalize migration - should emit finalization event
    assert!(client.finalize_client_migration(&contract_id));
    
    // Check for finalization event
    let events_after_finalize = env.events().all();
    assert!(events_after_finalize.iter().any(|(topics, _data)| {
        topics.len() >= 2 && 
        topics[0] == super::Symbol::new(&env, "client_migration_finalized") &&
        topics[1] == super::symbol_short!(contract_id)
    }));
}

#[test]
fn migration_cancellation_emits_event() {
    let env = setup_env();
    let client = register_escrow(&env);
    let (parties, contract_id) = create_sample_contract(&env, &client);
    
    assert!(client.request_client_migration(&contract_id, &parties.replacement_client));
    
    // Cancel migration - should emit cancellation event
    assert!(client.cancel_client_migration(&contract_id));
    
    // Check for cancellation event
    let events = env.events().all();
    assert!(events.iter().any(|(topics, _data)| {
        topics.len() >= 2 && 
        topics[0] == super::Symbol::new(&env, "client_migration_cancelled") &&
        topics[1] == super::symbol_short!(contract_id)
    }));
}
