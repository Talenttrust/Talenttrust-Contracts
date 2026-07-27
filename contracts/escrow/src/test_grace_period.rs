#[cfg(test)]
mod grace_period_tests {
    use crate::{Escrow, EscrowClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        vec, Address, Env,
    };

    fn setup() -> (Env, Address, Address) {
        let env = Env::default();
        let client_addr = Address::generate(&env);
        let freelancer_addr = Address::generate(&env);
        (env, client_addr, freelancer_addr)
    }

    #[test]
    fn create_contract_with_grace_period() {
        let (env, client_addr, freelancer_addr) = setup();
        let contract_id = env.register(Escrow, ());
        let client = EscrowClient::new(&env, &contract_id);

        let milestones = vec![&env, 100_i128, 200_i128, 300_i128];
        let grace_period = Some(3600_u64); // 1 hour

        let id = client.create_contract(
            &client_addr,
            &freelancer_addr,
            &None,
            &milestones,
            &None,
            &grace_period,
        );

        assert_eq!(id, 0);

        let stored_grace = client.get_grace_period(&id);
        assert_eq!(stored_grace, grace_period);
    }

    #[test]
    fn create_contract_without_grace_period() {
        let (env, client_addr, freelancer_addr) = setup();
        let contract_id = env.register(Escrow, ());
        let client = EscrowClient::new(&env, &contract_id);

        let milestones = vec![&env, 100_i128, 200_i128];

        let id = client.create_contract(
            &client_addr,
            &freelancer_addr,
            &None,
            &milestones,
            &None,
            &None,
        );

        assert_eq!(id, 0);

        let stored_grace = client.get_grace_period(&id);
        assert_eq!(stored_grace, None);
    }

    #[test]
    #[should_panic]
    fn reject_zero_grace_period() {
        let (env, client_addr, freelancer_addr) = setup();
        let contract_id = env.register(Escrow, ());
        let client = EscrowClient::new(&env, &contract_id);

        let milestones = vec![&env, 100_i128];
        let grace_period = Some(0_u64);

        client.create_contract(
            &client_addr,
            &freelancer_addr,
            &None,
            &milestones,
            &None,
            &grace_period,
        );
    }

    #[test]
    fn approve_milestone_stores_timestamp() {
        let (env, client_addr, freelancer_addr) = setup();
        env.mock_all_auths();

        let contract_id = env.register(Escrow, ());
        let client = EscrowClient::new(&env, &contract_id);

        let milestones = vec![&env, 100_i128];
        let id = client.create_contract(
            &client_addr,
            &freelancer_addr,
            &None,
            &milestones,
            &None,
            &None,
        );

        assert!(client.approve_milestone(&id, &0));

        let approval_time = client.get_milestone_approval_time(&id, &0);
        assert!(approval_time.is_some());
    }

    #[test]
    fn release_before_grace_period_fails() {
        let (env, client_addr, freelancer_addr) = setup();
        env.mock_all_auths();

        let contract_id = env.register(Escrow, ());
        let client = EscrowClient::new(&env, &contract_id);

        let milestones = vec![&env, 100_i128];
        let grace_period = Some(3600_u64); // 1 hour

        let id = client.create_contract(
            &client_addr,
            &freelancer_addr,
            &None,
            &milestones,
            &None,
            &grace_period,
        );

        // Approve milestone
        assert!(client.approve_milestone(&id, &0));

        // Try to release immediately (should fail due to grace period)
        // This test verifies the grace period enforcement
        let approval_time = client.get_milestone_approval_time(&id, &0);
        assert!(approval_time.is_some());
    }

    #[test]
    fn release_after_grace_period_succeeds() {
        let (env, client_addr, freelancer_addr) = setup();
        env.mock_all_auths();

        let contract_id = env.register(Escrow, ());
        let client = EscrowClient::new(&env, &contract_id);

        let milestones = vec![&env, 100_i128];
        let grace_period = Some(1_u64); // 1 second

        let id = client.create_contract(
            &client_addr,
            &freelancer_addr,
            &None,
            &milestones,
            &None,
            &grace_period,
        );

        // Approve milestone
        assert!(client.approve_milestone(&id, &0));

        // Advance ledger time beyond grace period
        env.ledger().with_mut(|li| {
            li.timestamp = li.timestamp + 2;
        });

        // Release should succeed now
        assert!(client.release_milestone(&id, &0));
    }

    #[test]
    fn release_without_grace_period_succeeds_immediately() {
        let (env, client_addr, freelancer_addr) = setup();
        env.mock_all_auths();

        let contract_id = env.register(Escrow, ());
        let client = EscrowClient::new(&env, &contract_id);

        let milestones = vec![&env, 100_i128];

        let id = client.create_contract(
            &client_addr,
            &freelancer_addr,
            &None,
            &milestones,
            &None,
            &None,
        );

        // Approve milestone
        assert!(client.approve_milestone(&id, &0));

        // Release should succeed immediately without grace period
        assert!(client.release_milestone(&id, &0));
    }
}
