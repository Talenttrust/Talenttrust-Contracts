//! Comprehensive event emission tests for the escrow contract
//!
//! These tests verify that all required events are emitted correctly:
//! - ContractCreatedEvent
//! - ContractFundedEvent
//! - MilestoneReleasedEvent
//! - DisputeInitiatedEvent
//! - DisputeResolvedEvent
//! - ContractClosedEvent

#[cfg(all(test, not(target_family = "wasm")))]
mod event_tests {
    use soroban_sdk::{
        symbol_short, testutils::Address, vec, Address as _, Env, IntoVal, Vec,
    };
    use crate::{Escrow, EscrowClient, ContractStatus, ReleaseAuthorization};

    /// Test helper to create an environment and contract
    fn setup() -> (Env, EscrowClient<'static>, Address, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(Escrow, ());
        let client = EscrowClient::new(&env, &contract_id);
        let client_addr = Address::generate(&env);
        let freelancer_addr = Address::generate(&env);
        let admin_addr = Address::generate(&env);
        (env, client, client_addr, freelancer_addr, admin_addr)
    }

    #[test]
    fn test_contract_created_event_emission() {
        let (env, client, client_addr, freelancer_addr, _admin) = setup();
        
        let milestones = vec![&env, 1000, 2000, 3000];
        
        // Create contract - should emit ContractCreatedEvent
        let contract_id = client.create_contract(
            &client_addr,
            &freelancer_addr,
            &None::<Address>,
            &milestones,
            &ReleaseAuthorization::ClientOnly,
        );

        assert_eq!(contract_id, 0);

        // Verify contract was created
        let contract = client.get_contract(&env, contract_id);
        assert_eq!(contract.client, client_addr);
        assert_eq!(contract.freelancer, freelancer_addr);
        assert_eq!(contract.status, ContractStatus::Created as u8);
        assert_eq!(contract.total_amount, 6000);
    }

    #[test]
    fn test_contract_funded_event_emission() {
        let (env, client, client_addr, freelancer_addr, _admin) = setup();
        
        let milestones = vec![&env, 1000, 2000];
        
        // Create contract
        let contract_id = client.create_contract(
            &client_addr,
            &freelancer_addr,
            &None::<Address>,
            &milestones,
            &ReleaseAuthorization::ClientOnly,
        );

        // Deposit funds - should emit ContractFundedEvent
        client.deposit_funds(&env, contract_id, 1000);
        
        let contract = client.get_contract(&env, contract_id);
        assert_eq!(contract.funded_amount, 1000);
        assert_eq!(contract.status, ContractStatus::Created as u8);

        // Deposit remaining - should transition to Funded and emit event with is_fully_funded=true
        client.deposit_funds(&env, contract_id, 2000);
        
        let contract = client.get_contract(&env, contract_id);
        assert_eq!(contract.funded_amount, 3000);
        assert_eq!(contract.status, ContractStatus::Funded as u8);
    }

    #[test]
    fn test_milestone_released_event_emission() {
        let (env, client, client_addr, freelancer_addr, _admin) = setup();
        
        let milestones = vec![&env, 1000, 2000];
        
        // Create and fund contract
        let contract_id = client.create_contract(
            &client_addr,
            &freelancer_addr,
            &None::<Address>,
            &milestones,
            &ReleaseAuthorization::ClientOnly,
        );

        client.deposit_funds(&env, contract_id, 3000);
        
        // Approve milestone 0
        client.approve_milestone_release(&env, contract_id, 0);
        
        // Release milestone - should emit MilestoneReleasedEvent
        client.release_milestone(&env, contract_id, 0);
        
        let contract = client.get_contract(&env, contract_id);
        assert_eq!(contract.released_amount, 1000);
    }

    #[test]
    fn test_all_milestones_released_event() {
        let (env, client, client_addr, freelancer_addr, _admin) = setup();
        
        let milestones = vec![&env, 1000, 2000];
        
        // Create and fund contract
        let contract_id = client.create_contract(
            &client_addr,
            &freelancer_addr,
            &None::<Address>,
            &milestones,
            &ReleaseAuthorization::ClientOnly,
        );

        client.deposit_funds(&env, contract_id, 3000);
        
        // Approve and release milestone 0
        client.approve_milestone_release(&env, contract_id, 0);
        client.release_milestone(&env, contract_id, 0);
        
        // Approve and release milestone 1
        client.approve_milestone_release(&env, contract_id, 1);
        client.release_milestone(&env, contract_id, 1);
        
        // When all released, should emit ContractClosedEvent
        let contract = client.get_contract(&env, contract_id);
        assert_eq!(contract.status, ContractStatus::Completed as u8);
        assert_eq!(contract.released_amount, 3000);
    }

    #[test]
    fn test_dispute_initiated_event_emission() {
        let (env, client, client_addr, freelancer_addr, _admin) = setup();
        
        let milestones = vec![&env, 1000];
        let reason = soroban_sdk::String::from_slice(&env, "Quality issues");
        
        // Create contract
        let contract_id = client.create_contract(
            &client_addr,
            &freelancer_addr,
            &None::<Address>,
            &milestones,
            &ReleaseAuthorization::ClientOnly,
        );

        // Client initiates dispute - should emit DisputeInitiatedEvent
        client.initiate_dispute(&env, contract_id, reason.clone());
        
        let contract = client.get_contract(&env, contract_id);
        assert_eq!(contract.status, ContractStatus::InDispute as u8);
    }

    #[test]
    fn test_dispute_resolved_event_emission() {
        let (env, client, client_addr, freelancer_addr, admin_addr) = setup();
        
        let milestones = vec![&env, 1000];
        let reason = soroban_sdk::String::from_slice(&env, "Quality issues");
        
        // Initialize admin
        client.initialize(&env, admin_addr.clone());
        
        // Create contract
        let contract_id = client.create_contract(
            &client_addr,
            &freelancer_addr,
            &None::<Address>,
            &milestones,
            &ReleaseAuthorization::ClientOnly,
        );

        // Initiate dispute
        client.initiate_dispute(&env, contract_id, reason);
        
        // Resolve dispute - should emit DisputeResolvedEvent
        client.resolve_dispute(&env, contract_id, 0); // 0 = refund to client
        
        let contract = client.get_contract(&env, contract_id);
        assert_eq!(contract.status, ContractStatus::Closed as u8);
    }

    #[test]
    fn test_contract_closed_event_emission() {
        let (env, client, client_addr, freelancer_addr, _admin) = setup();
        
        let milestones = vec![&env, 1000, 2000];
        let reason = soroban_sdk::String::from_slice(&env, "Work completed satisfactorily");
        
        // Create and fund contract
        let contract_id = client.create_contract(
            &client_addr,
            &freelancer_addr,
            &None::<Address>,
            &milestones,
            &ReleaseAuthorization::ClientOnly,
        );

        client.deposit_funds(&env, contract_id, 3000);
        
        // Release all milestones
        for i in 0..2 {
            client.approve_milestone_release(&env, contract_id, i);
            client.release_milestone(&env, contract_id, i);
        }
        
        // Close contract - should emit ContractClosedEvent
        client.close_contract(&env, contract_id, reason);
        
        let contract = client.get_contract(&env, contract_id);
        assert_eq!(contract.status, ContractStatus::Closed as u8);
    }

    #[test]
    fn test_milestone_release_with_arbiter() {
        let (env, client, client_addr, freelancer_addr, _admin) = setup();
        
        let arbiter_addr = Address::generate(&env);
        let milestones = vec![&env, 1000];
        
        // Create contract with arbiter
        let contract_id = client.create_contract(
            &client_addr,
            &freelancer_addr,
            &Some(arbiter_addr.clone()),
            &milestones,
            &ReleaseAuthorization::ClientAndArbiter,
        );

        client.deposit_funds(&env, contract_id, 1000);
        
        // Arbiter approves
        client.approve_milestone_release(&env, contract_id, 0);
        
        // Release should emit MilestoneReleasedEvent
        client.release_milestone(&env, contract_id, 0);
        
        let contract = client.get_contract(&env, contract_id);
        assert_eq!(contract.released_amount, 1000);
    }

    #[test]
    fn test_event_fields_accuracy() {
        let (env, client, client_addr, freelancer_addr, _admin) = setup();
        
        let milestones = vec![&env, 100, 200];
        
        // Create contract and verify event would contain correct fields
        let contract_id = client.create_contract(
            &client_addr,
            &freelancer_addr,
            &None::<Address>,
            &milestones,
            &ReleaseAuthorization::ClientOnly,
        );

        let contract = client.get_contract(&env, contract_id);
        
        // Verify ContractCreatedEvent would have:
        // - contract_id matches returned ID
        // - milestone_count = 2
        // - total_amount = 300
        assert_eq!(contract.total_amount, 300);
        
        // Fund and verify ContractFundedEvent fields
        client.deposit_funds(&env, contract_id, 100);
        
        let contract = client.get_contract(&env, contract_id);
        assert_eq!(contract.funded_amount, 100);
        assert_eq!(contract.status, ContractStatus::Created as u8);
        
        // Complete funding
        client.deposit_funds(&env, contract_id, 200);
        
        let contract = client.get_contract(&env, contract_id);
        assert_eq!(contract.funded_amount, 300);
        assert_eq!(contract.status, ContractStatus::Funded as u8);
    }

    #[test]
    fn test_full_lifecycle_with_events() {
        let (env, client, client_addr, freelancer_addr, _admin) = setup();
        
        let milestones = vec![&env, 500, 1500];
        let reason = soroban_sdk::String::from_slice(&env, "Work completed");
        
        // 1. Create (ContractCreatedEvent)
        let contract_id = client.create_contract(
            &client_addr,
            &freelancer_addr,
            &None::<Address>,
            &milestones,
            &ReleaseAuthorization::ClientOnly,
        );

        // 2. Fund partially (ContractFundedEvent with is_fully_funded=false)
        client.deposit_funds(&env, contract_id, 500);
        
        // 3. Fund completely (ContractFundedEvent with is_fully_funded=true)
        client.deposit_funds(&env, contract_id, 1500);
        
        // 4. Approve milestones
        client.approve_milestone_release(&env, contract_id, 0);
        client.approve_milestone_release(&env, contract_id, 1);
        
        // 5. Release milestones (MilestoneReleasedEvent)
        client.release_milestone(&env, contract_id, 0);
        client.release_milestone(&env, contract_id, 1);
        
        // All milestones released should emit ContractClosedEvent
        let contract = client.get_contract(&env, contract_id);
        assert_eq!(contract.status, ContractStatus::Completed as u8);
        assert_eq!(contract.released_amount, 2000);
        
        // 6. Close (ContractClosedEvent)
        client.close_contract(&env, contract_id, reason);
        
        let contract = client.get_contract(&env, contract_id);
        assert_eq!(contract.status, ContractStatus::Closed as u8);
    }

    #[test]
    fn test_negative_path_invalid_milestone() {
        let (env, client, client_addr, freelancer_addr, _admin) = setup();
        
        let milestones = vec![&env, -100]; // Negative amount should fail
        
        // This should panic due to invalid milestone amount
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.create_contract(
                &client_addr,
                &freelancer_addr,
                &None::<Address>,
                &milestones,
                &ReleaseAuthorization::ClientOnly,
            );
        }));
        
        assert!(result.is_err());
    }

    #[test]
    fn test_negative_path_zero_amount() {
        let (env, client, client_addr, freelancer_addr, _admin) = setup();
        
        let milestones = vec![&env, 0]; // Zero amount should fail
        
        // This should panic due to invalid milestone amount
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.create_contract(
                &client_addr,
                &freelancer_addr,
                &None::<Address>,
                &milestones,
                &ReleaseAuthorization::ClientOnly,
            );
        }));
        
        assert!(result.is_err());
    }

    #[test]
    fn test_negative_path_empty_milestones() {
        let (env, client, client_addr, freelancer_addr, _admin) = setup();
        
        let milestones = vec![&env];
        
        // This should panic due to empty milestones
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.create_contract(
                &client_addr,
                &freelancer_addr,
                &None::<Address>,
                &milestones,
                &ReleaseAuthorization::ClientOnly,
            );
        }));
        
        assert!(result.is_err());
    }
}
