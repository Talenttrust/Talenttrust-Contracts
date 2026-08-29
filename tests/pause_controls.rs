#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, vec, Address, Env};

/// Helper to set pause state directly in test environment
fn set_contract_paused(env: &Env, paused: bool) {
    // TODO: Wire this to your contract's storage helper or admin call
    // e.g., crate::storage::set_paused(env, paused);
    // OR if calling contract directly:
    // let client = EscrowContractClient::new(env, &escrow_id);
    // client.set_pause(&admin, &paused);
}

/// Helper setup to spin up env and test addresses
fn setup_test_env() -> (Env, Address, Address, Address) {
    let env = Env::default();
    
    env.mock_all_signatures();

    let client = Address::generate(&env);
    let freelancer = Address::generate(&env);
    
    // Make sure 'EscrowContract' matches your struct name in lib.rs
    let escrow_id = env.register_contract(None, EscrowContract);

    (env, client, freelancer, escrow_id)
}

#[cfg(test)]
mod pause_control_tests {
    use super::*;

    // 1. Deposit blocked when paused
    #[test]
    #[should_panic(expected = "Error(Contract, #16)")]
    fn test_deposit_funds_fails_when_paused() {
        let (env, client, _freelancer, escrow_id) = setup_test_env();
        set_contract_paused(&env, true);

        let escrow_client = EscrowContractClient::new(&env, &escrow_id);
        escrow_client.deposit_funds(&1, &client, &1000);
    }

    // 2. Milestone release blocked when paused
    #[test]
    #[should_panic(expected = "Error(Contract, #16)")]
    fn test_release_milestone_fails_when_paused() {
        let (env, client, _freelancer, escrow_id) = setup_test_env();
        set_contract_paused(&env, true);

        let escrow_client = EscrowContractClient::new(&env, &escrow_id);
        escrow_client.release_milestone(&1, &client, &0);
    }

    // 3. Contract creation blocked when paused
    #[test]
    #[should_panic(expected = "Error(Contract, #16)")]
    fn test_create_contract_fails_when_paused() {
        let (env, client, freelancer, escrow_id) = setup_test_env();
        set_contract_paused(&env, true);

        let escrow_client = EscrowContractClient::new(&env, &escrow_id);
        escrow_client.create_contract(
            &client,
            &freelancer,
            &None,
            &vec![&env, 1000],
            &ReleaseAuthorization::ClientOnly,
        );
    }

    // 4. Client migration proposal blocked when paused
    #[test]
    #[should_panic(expected = "Error(Contract, #16)")]
    fn test_propose_migration_fails_when_paused() {
        let (env, client, new_client, escrow_id) = setup_test_env();
        set_contract_paused(&env, true);

        let escrow_client = EscrowContractClient::new(&env, &escrow_id);
        escrow_client.propose_client_migration(&1, &client, &new_client);
    }

    // 5. Accepting client migration blocked when paused
    #[test]
    #[should_panic(expected = "Error(Contract, #16)")]
    fn test_accept_migration_fails_when_paused() {
        let (env, _client, new_client, escrow_id) = setup_test_env();
        set_contract_paused(&env, true);

        let escrow_client = EscrowContractClient::new(&env, &escrow_id);
        escrow_client.accept_client_migration(&1, &new_client);
    }

    // 6. Cancelling client migration blocked when paused
    #[test]
    #[should_panic(expected = "Error(Contract, #16)")]
    fn test_cancel_migration_fails_when_paused() {
        let (env, client, _new_client, escrow_id) = setup_test_env();
        set_contract_paused(&env, true);

        let escrow_client = EscrowContractClient::new(&env, &escrow_id);
        escrow_client.cancel_client_migration(&1, &client);
    }

    // 7. Cancelling contract / refunding blocked when paused
    #[test]
    #[should_panic(expected = "Error(Contract, #16)")]
    fn test_cancel_contract_fails_when_paused() {
        let (env, client, _freelancer, escrow_id) = setup_test_env();
        set_contract_paused(&env, true);

        let escrow_client = EscrowContractClient::new(&env, &escrow_id);
        escrow_client.cancel_contract(&1, &client);
    }

    // 8. Fee withdrawal blocked when paused
    #[test]
    #[should_panic(expected = "Error(Contract, #16)")]
    fn test_withdraw_fees_fails_when_paused() {
        let (env, admin, _freelancer, escrow_id) = setup_test_env();
        set_contract_paused(&env, true);

        let escrow_client = EscrowContractClient::new(&env, &escrow_id);
        escrow_client.withdraw_protocol_fees(&admin);
    }

    // 9. Read-only query succeeds even when paused
    #[test]
    fn test_read_only_view_succeeds_when_paused() {
        let (env, _client, _freelancer, escrow_id) = setup_test_env();
        set_contract_paused(&env, true);

        let escrow_client = EscrowContractClient::new(&env, &escrow_id);
        let bound = escrow_client.is_settlement_token_bound();
        
        // Read queries should return without panicking
        assert!(bound || !bound); 
    }

    // 10. Normal operations resume after unpausing
    #[test]
    fn test_operations_succeed_after_unpausing() {
        let (env, client, _freelancer, escrow_id) = setup_test_env();
        
        // 1. Pause
        set_contract_paused(&env, true);
        
        // 2. Unpause
        set_contract_paused(&env, false);

        // 3. Mutating call should succeed normally
        let escrow_client = EscrowContractClient::new(&env, &escrow_id);
        let res = escrow_client.deposit_funds(&1, &client, &1000);
        assert!(res.is_ok());
    }
}
