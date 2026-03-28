use crate::{ContractStatus, Escrow, EscrowClient};
use proptest::prelude::*;
use soroban_sdk::{testutils::Address as _, vec, Address, Env};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn test_balance_and_status_invariants(
        amounts in proptest::collection::vec(1..1000_0000000i128, 1..5),
        deposit_extra in 0..1000_0000000i128,
        release_indices in proptest::collection::vec(0..5u32, 0..10)
    ) {
        let env = Env::default();
        let env_ref = &env;
        let contract_id = env.register(Escrow, ());
        let client = EscrowClient::new(env_ref, &contract_id);

        let client_addr = Address::generate(env_ref);
        let freelancer_addr = Address::generate(env_ref);

        let mut total_milestone_amount = 0;
        let mut milestone_vec = vec![env_ref];
        for a in &amounts {
            total_milestone_amount += a;
            milestone_vec.push_back(*a);
        }

        let esc_id = client.create_contract(&client_addr, &freelancer_addr, &milestone_vec);
        prop_assert_eq!(esc_id, 1);

        // Not funded yet
        let state_before = client.get_state(&esc_id);
        prop_assert_eq!(state_before.status, ContractStatus::Created);
        prop_assert_eq!(state_before.balance, 0);

        let deposit_amount = total_milestone_amount + deposit_extra;
        let deposit_result = client.deposit_funds(&esc_id, &deposit_amount);
        prop_assert!(deposit_result);

        let mut expected_balance = deposit_amount;
        let mut expected_status = ContractStatus::Funded;
        let mut released_total = 0;
        let mut released_tracker = [false; 5];

        for idx in release_indices {
            let res = client.release_milestone(&esc_id, &idx);
            let idx_usize = idx as usize;
            if expected_status == ContractStatus::Funded && idx_usize < amounts.len() && !released_tracker[idx_usize] {
                prop_assert!(res);
                released_tracker[idx_usize] = true;
                expected_balance -= amounts[idx_usize];
                released_total += amounts[idx_usize];

                let mut all_released = true;
                for i in 0..amounts.len() {
                    if !released_tracker[i] {
                        all_released = false;
                        break;
                    }
                }

                if all_released {
                    expected_status = ContractStatus::Completed;
                }
            } else {
                prop_assert!(!res); // Should fail if already released, out of bounds, or not Funded
            }

            let current_state = client.get_state(&esc_id);
            prop_assert_eq!(current_state.balance, expected_balance);
            prop_assert_eq!(current_state.status, expected_status);

            // Invariant: balance + released == deposit_amount
            prop_assert_eq!(current_state.balance + released_total, deposit_amount);
        }
    }
}
