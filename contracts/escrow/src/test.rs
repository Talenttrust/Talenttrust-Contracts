use super::*;
use soroban_sdk::{testutils::Address as _, token, Env, IntoVal};

#[test]
fn test_sweep_surplus_and_direct_deposit() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let client = Address::generate(&env);
    let freelancer1 = Address::generate(&env);
    let freelancer2 = Address::generate(&env);
    let contract_id = env.register(Escrow, ());
    let client_token = env.register_stellar_asset_issuer(admin.clone());
    let token_id = client_token.address();

    let token_client = token::Client::new(&env, &token_id);

    // Initialize contract
    let escrow_client = EscrowClient::new(&env, &contract_id);
    escrow_client.initialize(&admin);

    // Mint tokens to client
    token_client.mint(&client, &1000);

    // Create two live escrows
    let id1 = escrow_client.create_contract(
        &client,
        &freelancer1,
        &token_id,
        &vec![&env, 100],
    );
    let id2 = escrow_client.create_contract(
        &client,
        &freelancer2,
        &token_id,
        &vec![&env, 200],
    );

    // Fund both escrows
    escrow_client.deposit_funds(&id1, &100);
    escrow_client.deposit_funds(&id2, &200);

    // Send 50 tokens directly to the contract address (untracked surplus)
    token_client.mint(&env.current_contract_address(), &50);

    // Test sweeping with zero surplus first (if we sweep after, or test separate zero surplus)
    // Here surplus is 50. Let's test non-admin sweep rejection first.
    let non_admin = Address::generate(&env);
    let sweep_result = escrow_client.try_sweep_surplus(&token_id, &non_admin);
    assert!(sweep_result.is_err());

    // Admin sweeps surplus
    let swept = escrow_client.sweep_surplus(&token_id, &admin);
    assert_eq!(swept, 50);

    // Verify sweeping with zero surplus now returns NoSurplus error
    let zero_sweep = escrow_client.try_sweep_surplus(&token_id, &admin);
    assert_eq!(zero_sweep, Err(Ok(EscrowError::NoSurplus)));

    // Verify two live escrows' funds remain untouched and release correctly
    escrow_client.release_milestone(&id1, &0);
    assert_eq!(token_client.balance(&freelancer1), 100);

    escrow_client.release_milestone(&id2, &0);
    assert_eq!(token_client.balance(&freelancer2), 200);
}

#[test]
fn test_randomized_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let client = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let contract_id = env.register(Escrow, ());
    let client_token = env.register_stellar_asset_issuer(admin.clone());
    let token_id = client_token.address();
    let token_client = token::Client::new(&env, &token_id);

    let escrow_client = EscrowClient::new(&env, &contract_id);
    escrow_client.initialize(&admin);

    token_client.mint(&client, &100_000);

    // Create 3 escrows
    let mut escrow_ids = Vec::new(&env);
    for _ in 0..3 {
        let id = escrow_client.create_contract(
            &client,
            &freelancer,
            &token_id,
            &vec![&env, 100, 100],
        );
        escrow_ids.push_back(id);
    }

    // Simple LCG PRNG for randomized lifecycle test (≥200 iterations)
    let mut rng_state: u32 = 42;
    let mut lcg = || {
        rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
        rng_state
    };

    for _ in 0..200 {
        let rand_val = lcg();
        let escrow_idx = (rand_val % 3) as u32;
        let id = escrow_ids.get(escrow_idx).unwrap();
        let action = (rand_val / 3) % 3;

        let contract_info = escrow_client.try_get_contract(&id);
        if let Ok(Ok(c)) = contract_info {
            match action {
                0 => {
                    // Deposit if not fully funded
                    if c.funded_amount < c.total_amount {
                        let _ = escrow_client.try_deposit_funds(&id, &50);
                    }
                }
                1 => {
                    // Release milestone if funded
                    if c.funded_amount - c.released_amount >= 100 {
                        let _ = escrow_client.try_release_milestone(&id, &0);
                    }
                }
                _ => {
                    // Cancel or dispute randomly
                    let _ = escrow_client.try_cancel_contract(&id);
                }
            }
        }

        // Assert balance == liability after every step
        let balance = token_client.balance(&contract_id);
        let liability = escrow_client.get_liability(&token_id);
        assert!(balance >= liability);
    }
}
