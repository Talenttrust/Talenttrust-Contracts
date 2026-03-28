use proptest::prelude::*;
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};
// Depending on how Soroban is compiled, we need to import Escrow and EscrowClient
use crate::{Escrow, EscrowClient};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn fuzz_deposit_funds_valid(amount in 1..i128::MAX) {
        let env = Env::default();
        let contract_id = env.register(Escrow, ());
        let client = EscrowClient::new(&env, &contract_id);

        let result = client.deposit_funds(&1, &amount);
        assert!(result);
    }

    #[test]
    fn fuzz_issue_reputation_valid(rating in 1..=5i128) {
        let env = Env::default();
        let contract_id = env.register(Escrow, ());
        let client = EscrowClient::new(&env, &contract_id);

        let freelancer_addr = Address::generate(&env);
        let result = client.issue_reputation(&freelancer_addr, &rating);
        assert!(result);
    }

    #[test]
    fn fuzz_create_contract_single_milestone_valid(amount in 1..i128::MAX) {
        let env = Env::default();
        let contract_id = env.register(Escrow, ());
        let client = EscrowClient::new(&env, &contract_id);

        let client_addr = Address::generate(&env);
        let freelancer_addr = Address::generate(&env);

        let mut sdk_amounts = Vec::new(&env);
        sdk_amounts.push_back(amount);

        let id = client.create_contract(&client_addr, &freelancer_addr, &sdk_amounts);
        assert_eq!(id, 1); // Returns our placeholder id 1
    }
}
