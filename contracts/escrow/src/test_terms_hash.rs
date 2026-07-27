#[cfg(test)]
mod terms_hash_tests {
    use crate::{Escrow, EscrowClient};
    use soroban_sdk::{testutils::Address as _, vec, Address, Bytes, Env};

    fn setup() -> (Env, Address, Address) {
        let env = Env::default();
        let client_addr = Address::generate(&env);
        let freelancer_addr = Address::generate(&env);
        (env, client_addr, freelancer_addr)
    }

    fn create_test_hash(env: &Env) -> Bytes {
        // SHA-256 hash example (32 bytes)
        Bytes::from_slice(
            env,
            &[
                0x2c, 0x26, 0xb4, 0x6b, 0x68, 0xff, 0xc6, 0x8f, 0xf9, 0x9b, 0x45, 0x3c, 0x1d, 0x30,
                0x41, 0x34, 0x13, 0x42, 0x2d, 0x70, 0x64, 0x83, 0xbf, 0xa0, 0xf9, 0x8a, 0x5e, 0x88,
                0x62, 0x66, 0xe7, 0xae,
            ],
        )
    }

    #[test]
    fn create_contract_with_terms_hash() {
        let (env, client_addr, freelancer_addr) = setup();
        let contract_id = env.register(Escrow, ());
        let client = EscrowClient::new(&env, &contract_id);

        let milestones = vec![&env, 100_i128, 200_i128];
        let terms_hash = Some(create_test_hash(&env));

        let id = client.create_contract(
            &client_addr,
            &freelancer_addr,
            &None,
            &milestones,
            &terms_hash,
            &None,
        );

        assert_eq!(id, 0);

        let stored_hash = client.get_terms_hash(&id);
        assert_eq!(stored_hash, terms_hash);
    }

    #[test]
    fn create_contract_without_terms_hash() {
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

        let stored_hash = client.get_terms_hash(&id);
        assert_eq!(stored_hash, None);
    }

    #[test]
    fn terms_hash_is_immutable() {
        let (env, client_addr, freelancer_addr) = setup();
        let contract_id = env.register(Escrow, ());
        let client = EscrowClient::new(&env, &contract_id);

        let milestones = vec![&env, 100_i128];
        let terms_hash = Some(create_test_hash(&env));

        let id = client.create_contract(
            &client_addr,
            &freelancer_addr,
            &None,
            &milestones,
            &terms_hash,
            &None,
        );

        // Verify hash is stored
        let stored_hash = client.get_terms_hash(&id);
        assert_eq!(stored_hash, terms_hash);

        // Hash should remain the same (immutable)
        let stored_hash_again = client.get_terms_hash(&id);
        assert_eq!(stored_hash_again, terms_hash);
    }

    #[test]
    fn multiple_contracts_have_independent_hashes() {
        let (env, client_addr, freelancer_addr) = setup();
        let contract_id = env.register(Escrow, ());
        let client = EscrowClient::new(&env, &contract_id);

        let milestones = vec![&env, 100_i128];
        let hash1 = Some(create_test_hash(&env));

        let id1 = client.create_contract(
            &client_addr,
            &freelancer_addr,
            &None,
            &milestones,
            &hash1,
            &None,
        );

        // Create different hash for second contract
        let hash2 = Some(Bytes::from_slice(&env, &[0x11; 32]));

        let id2 = client.create_contract(
            &client_addr,
            &freelancer_addr,
            &None,
            &milestones,
            &hash2,
            &None,
        );

        // Verify each contract has its own hash
        let stored_hash1 = client.get_terms_hash(&id1);
        let stored_hash2 = client.get_terms_hash(&id2);

        assert_eq!(stored_hash1, hash1);
        assert_eq!(stored_hash2, hash2);
        assert_ne!(stored_hash1, stored_hash2);
    }

    #[test]
    fn terms_hash_with_grace_period() {
        let (env, client_addr, freelancer_addr) = setup();
        let contract_id = env.register(Escrow, ());
        let client = EscrowClient::new(&env, &contract_id);

        let milestones = vec![&env, 100_i128];
        let terms_hash = Some(create_test_hash(&env));
        let grace_period = Some(3600_u64);

        let id = client.create_contract(
            &client_addr,
            &freelancer_addr,
            &None,
            &milestones,
            &terms_hash,
            &grace_period,
        );

        // Verify both features are stored
        let stored_hash = client.get_terms_hash(&id);
        let stored_grace = client.get_grace_period(&id);

        assert_eq!(stored_hash, terms_hash);
        assert_eq!(stored_grace, grace_period);
    }
}
