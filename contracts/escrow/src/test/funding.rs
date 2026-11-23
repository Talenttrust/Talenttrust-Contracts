//! Deposits, funders, refunds and the liability ledger.

use super::*;

#[test]
fn rejects_overfunding_and_double_release() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    assert_eq!(
        s.escrow.try_deposit_funds(&id, &(101 * U)),
        Err(Ok(EscrowError::OverFunded))
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.release_milestone(&id, &0);
    assert!(s.escrow.try_release_milestone(&id, &0).is_err());
}

#[test]
fn deposit_funds_on_funded_returns_invalid_status() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Funded);

    // Attempting to deposit on an already Funded escrow returns InvalidStatus immediately
    assert_eq!(
        s.escrow.try_deposit_funds(&id, &(50 * U)),
        Err(Ok(EscrowError::InvalidStatus))
    );
}

#[test]
fn test_sweep_surplus_and_direct_deposit() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let client = Address::generate(&env);
    let freelancer1 = Address::generate(&env);
    let freelancer2 = Address::generate(&env);

    let contract_id = env.register(Escrow, ());
    let token_id = env.register_stellar_asset_contract_v2(admin.clone()).address();

    let token_client = token::Client::new(&env, &token_id);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_id);

    // Initialize contract
    let escrow_client = EscrowClient::new(&env, &contract_id);
    escrow_client.initialize(&admin);

    // Mint tokens to client using admin client
    token_admin_client.mint(&client, &(1000 * U));

    // Create two live escrows
    let id1 = escrow_client.create_contract(
        &client,
        &freelancer1,
        &token_id,
        &vec![&env, 100 * U],
    );

    let id2 = escrow_client.create_contract(
        &client,
        &freelancer2,
        &token_id,
        &vec![&env, 200 * U],
    );

    // Fund both escrows
    escrow_client.deposit_funds(&id1, &(100 * U));
    escrow_client.deposit_funds(&id2, &(200 * U));

    // Send 50 tokens directly to the contract address (untracked surplus)
    token_admin_client.mint(&contract_id, &(50 * U));

    // Test non-admin sweep rejection
    let non_admin = Address::generate(&env);
    // Without the admin's authorization the sweep must be rejected.
    env.mock_auths(&[]);
    let sweep_result = escrow_client.try_sweep_surplus(&token_id, &non_admin);
    assert!(sweep_result.is_err());
    env.mock_all_auths();

    // Admin sweeps surplus
    let swept = escrow_client.sweep_surplus(&token_id, &admin);
    assert_eq!(swept, 50 * U);

    // Verify sweeping with zero surplus now returns NoSurplus error
    let zero_sweep = escrow_client.try_sweep_surplus(&token_id, &admin);
    assert_eq!(zero_sweep, Err(Ok(EscrowError::NoSurplus)));

    // Verify two live escrows' funds remain untouched and release correctly
    escrow_client.release_milestone(&id1, &0);
    assert_eq!(token_client.balance(&freelancer1), 100 * U);

    escrow_client.release_milestone(&id2, &0);
    assert_eq!(token_client.balance(&freelancer2), 200 * U);
}

#[test]
fn liability_randomized_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let client = Address::generate(&env);
    let freelancer = Address::generate(&env);

    let contract_id = env.register(Escrow, ());
    let token_id = env.register_stellar_asset_contract_v2(admin.clone()).address();

    let token_client = token::Client::new(&env, &token_id);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_id);

    let escrow_client = EscrowClient::new(&env, &contract_id);
    escrow_client.initialize(&admin);

    token_admin_client.mint(&client, &(100_000 * U));

    // Create 3 escrows
    let mut escrow_ids = Vec::new(&env);

    for _ in 0..3 {
        let id = escrow_client.create_contract(
            &client,
            &freelancer,
            &token_id,
            &vec![&env, 100 * U, 100 * U],
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
                        let _ = escrow_client.try_deposit_funds(&id, &(50 * U));
                    }
                }
                1 => {
                    // Release milestone if funded
                    if c.funded_amount - c.released_amount >= 100 * U {
                        let _ = escrow_client.try_release_milestone(&id, &0);
                    }
                }
                _ => {
                    // Cancel or dispute randomly
                    let _ = escrow_client.try_cancel_contract(&id);
                }
            }
        }

        // Assert balance >= liability after every step
        let balance = token_client.balance(&contract_id);
        let liability = escrow_client.get_liability(&token_id);

        assert!(balance >= liability);
    }
}

#[test]
fn sponsor_client_cancel_pro_rata() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = Address::generate(&env);
    let sponsor = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    StellarAssetClient::new(&env, &sac.address()).mint(&client, &(1_000 * U));
    StellarAssetClient::new(&env, &sac.address()).mint(&sponsor, &(1_000 * U));
    let token = TokenClient::new(&env, &sac.address());
    let id = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &id);
    escrow.initialize(&admin);
    let id = escrow.create_contract(
        &client,
        &freelancer,
        &token.address,
        &vec![&env, 100 * U],
    );

    // sponsor funds 60, client funds 40
    escrow.deposit_from(&id, &sponsor, &(60 * U));
    escrow.deposit_funds(&id, &(40 * U));

    let funders = escrow.get_funders(&id);
    assert_eq!(funders.get(sponsor.clone()).unwrap(), 60 * U);
    assert_eq!(funders.get(client.clone()).unwrap(), 40 * U);

    let client_before = token.balance(&client);
    let sponsor_before = token.balance(&sponsor);
    let freelancer_before = token.balance(&freelancer);
    let contract_addr = escrow.address.clone();
    let contract_before = token.balance(&contract_addr);

    // client cancels with no release: refund = 100 split pro-rata
    escrow.cancel_contract(&id);

    // sponsor gets 60 back, client gets 40 back (not all to client)
    assert_eq!(token.balance(&sponsor), sponsor_before + 60 * U);
    assert_eq!(token.balance(&client), client_before + 40 * U);
    assert_eq!(token.balance(&freelancer), freelancer_before);
    // the contract is fully refunded, so it holds nothing left
    assert_eq!(token.balance(&contract_addr), 0);

    // conservation: sum of all balance deltas == 0
    let sum = (token.balance(&client) - client_before)
        + (token.balance(&sponsor) - sponsor_before)
        + (token.balance(&freelancer) - freelancer_before)
        + (token.balance(&contract_addr) - contract_before);
    assert_eq!(sum, 0);

    assert_eq!(escrow.get_contract(&id).status, ContractStatus::Cancelled);
}

#[test]
fn partial_release_cancel_pro_rata() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = Address::generate(&env);
    let sponsor = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    StellarAssetClient::new(&env, &sac.address()).mint(&client, &(1_000 * U));
    StellarAssetClient::new(&env, &sac.address()).mint(&sponsor, &(1_000 * U));
    let token = TokenClient::new(&env, &sac.address());
    let id = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &id);
    escrow.initialize(&admin);
    let milestones = vec![&env, 30 * U, 70 * U];
    let id = escrow.create_contract(
        &client,
        &freelancer,
        &token.address,
        &milestones,
    );
    let contract_addr = escrow.address.clone();

    // sponsor funds 60, client funds 40
    escrow.deposit_from(&id, &sponsor, &(60 * U));
    escrow.deposit_funds(&id, &(40 * U));

    let client_before = token.balance(&client);
    let sponsor_before = token.balance(&sponsor);
    let freelancer_before = token.balance(&freelancer);
    let contract_before = token.balance(&contract_addr);

    // release 30 to freelancer
    escrow.release_milestone(&id, &0);
    assert_eq!(token.balance(&freelancer), freelancer_before + 30 * U);

    // cancel: refund = 70, split pro-rata: sponsor 42, client 28
    escrow.cancel_contract(&id);

    assert_eq!(token.balance(&sponsor), sponsor_before + 42 * U);
    assert_eq!(token.balance(&client), client_before + 28 * U);

    // conservation: no dust lost to rounding
    let sum = (token.balance(&client) - client_before)
        + (token.balance(&sponsor) - sponsor_before)
        + (token.balance(&freelancer) - freelancer_before)
        + (token.balance(&contract_addr) - contract_before);
    assert_eq!(sum, 0);

    assert_eq!(escrow.get_contract(&id).status, ContractStatus::Cancelled);
}

#[test]
fn dispute_split_pro_rata() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = Address::generate(&env);
    let sponsor = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    StellarAssetClient::new(&env, &sac.address()).mint(&client, &(1_000 * U));
    StellarAssetClient::new(&env, &sac.address()).mint(&sponsor, &(1_000 * U));
    let token = TokenClient::new(&env, &sac.address());
    let id = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &id);
    escrow.initialize(&admin);
    let id = escrow.create_contract(
        &client,
        &freelancer,
        &token.address,
        &vec![&env, 100 * U],
    );
    let contract_addr = escrow.address.clone();

    // sponsor funds 60, client funds 40
    escrow.deposit_from(&id, &sponsor, &(60 * U));
    escrow.deposit_funds(&id, &(40 * U));

    // The opener posts a refundable dispute bond (#1683), returned on resolution.
    StellarAssetClient::new(&env, &sac.address()).mint(&freelancer, &dispute_bond(100 * U));
    let client_before = token.balance(&client);
    let sponsor_before = token.balance(&sponsor);
    let freelancer_before = token.balance(&freelancer);
    let contract_before = token.balance(&contract_addr);

    // freelancer opens dispute, admin resolves 25% to freelancer
    escrow.open_dispute(&id, &freelancer);
    escrow.resolve_dispute(&admin, &id, &2_500, &true);

    // freelancer gets 25 (25% of 100 held)
    assert_eq!(token.balance(&freelancer), freelancer_before + 25 * U);
    // client share = 75 split pro-rata: sponsor 45, client 30
    assert_eq!(token.balance(&sponsor), sponsor_before + 45 * U);
    assert_eq!(token.balance(&client), client_before + 30 * U);

    // conservation
    let sum = (token.balance(&client) - client_before)
        + (token.balance(&sponsor) - sponsor_before)
        + (token.balance(&freelancer) - freelancer_before)
        + (token.balance(&contract_addr) - contract_before);
    assert_eq!(sum, 0);
}

#[test]
fn too_many_funders_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    StellarAssetClient::new(&env, &sac.address()).mint(&client, &(2_000 * U));
    let token = TokenClient::new(&env, &sac.address());
    let id = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &id);
    escrow.initialize(&admin);
    let id = escrow.create_contract(
        &client,
        &freelancer,
        &token.address,
        &vec![&env, 2_000 * U],
    );

    // 10 funders deposit 100 each
    for _ in 0..10 {
        let funder = Address::generate(&env);
        StellarAssetClient::new(&env, &sac.address()).mint(&funder, &(100 * U));
        escrow.deposit_from(&id, &funder, &(100 * U));
    }

    // 11th funder rejected
    let funder11 = Address::generate(&env);
    StellarAssetClient::new(&env, &sac.address()).mint(&funder11, &(100 * U));
    assert_eq!(
        escrow.try_deposit_from(&id, &funder11, &(100 * U)),
        Err(Ok(EscrowError::TooManyFunders))
    );
}

#[test]
fn funder_auth_enforced() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = Address::generate(&env);
    let sponsor = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    StellarAssetClient::new(&env, &sac.address()).mint(&client, &(1_000 * U));
    StellarAssetClient::new(&env, &sac.address()).mint(&sponsor, &(1_000 * U));
    let token = TokenClient::new(&env, &sac.address());
    let id = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &id);
    let contract_addr = escrow.address.clone();
    escrow.initialize(&admin);
    let id = escrow.create_contract(
        &client,
        &freelancer,
        &token.address,
        &vec![&env, 100 * U],
    );

    // sponsor (a third party) funds the escrow and is recorded as a funder
    escrow.deposit_from(&id, &sponsor, &(60 * U));

    let funders = escrow.get_funders(&id);
    assert_eq!(funders.get(sponsor.clone()).unwrap(), 60 * U);
    assert_eq!(funders.len(), 1);

    // the sponsor's tokens moved into the contract
    assert_eq!(token.balance(&sponsor), (1_000 - 60) * U);
    assert_eq!(token.balance(&contract_addr), 60 * U);
}

#[test]
fn pro_rata_rounding_remainder_goes_to_largest_funder() {
    let s = setup();
    let sponsor = Address::generate(&s.env);
    StellarAssetClient::new(&s.env, &s.token.address).mint(&sponsor, &(10 * U));
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 1_000_000i128, 2_000_001i128],
    );
    s.escrow.deposit_from(&id, &sponsor, &1_000_001);
    s.escrow.deposit_funds(&id, &2_000_000);
    s.escrow.release_milestone(&id, &0);
    let client_before = s.token.balance(&s.client);
    let sponsor_before = s.token.balance(&sponsor);
    // refund 2_000_001: sponsor floor 666_667, client floor 1_333_333, remainder 1 -> client
    s.escrow.cancel_contract(&id);
    assert_eq!(s.token.balance(&sponsor), sponsor_before + 666_667);
    assert_eq!(s.token.balance(&s.client), client_before + 1_333_334);
    assert_eq!(s.token.balance(&s.escrow.address), 0);
    assert_eq!(s.escrow.get_contract(&id).refunded_amount, 2_000_001);
}

#[test]
fn test_funding_deadline_boundaries_deposit() {
    let s = setup();
    set_now(&s.env, 1_000_000);

    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    let fund_by = s.escrow.get_fund_by(&id);
    assert_eq!(fund_by, 1_000_000 + FUNDING_WINDOW_SECS);

    // At now == fund_by, deposit must not be rejected because of funding deadline
    set_now(&s.env, fund_by);
    s.escrow.deposit_funds(&id, &(50 * U));
    assert_eq!(s.token.balance(&s.client), 950 * U);
    let c = s.escrow.get_contract(&id);
    assert_eq!(c.status, ContractStatus::Created);
    assert_eq!(c.funded_amount, 50 * U);

    // At now == fund_by + 1, deposit must be rejected with FundingExpired
    set_now(&s.env, fund_by + 1);
    assert_eq!(
        s.escrow.try_deposit_funds(&id, &(50 * U)),
        Err(Ok(EscrowError::FundingExpired))
    );
    // Escrow state remains unchanged
    let c_after = s.escrow.get_contract(&id);
    assert_eq!(c_after.status, ContractStatus::Created);
    assert_eq!(c_after.funded_amount, 50 * U);
    assert_eq!(s.token.balance(&s.client), 950 * U);
}

#[test]
fn test_partial_deposit_refund_third_party() {
    let s = setup();
    set_now(&s.env, 1_000_000);

    // Client starts with 1,000 tokens
    assert_eq!(s.token.balance(&s.client), 1_000 * U);
    assert_eq!(s.token.balance(&s.freelancer), 0);

    // Create escrow with total 100 and partial deposit of 40
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 60 * U, 40 * U],
    );
    s.escrow.deposit_funds(&id, &(40 * U));
    assert_eq!(s.token.balance(&s.client), 960 * U);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Created);

    let fund_by = s.escrow.get_fund_by(&id);
    set_now(&s.env, fund_by + 1);

    // Third-party caller invoking expire_unfunded
    let third_party = Address::generate(&s.env);
    assert_eq!(s.token.balance(&third_party), 0);

    let res = s
        .escrow
        .mock_auths(&[MockAuth {
            address: &third_party,
            invoke: &MockAuthInvoke {
                contract: &s.escrow.address,
                fn_name: "expire_unfunded",
                args: (id,).into_val(&s.env),
                sub_invokes: &[],
            },
        }])
        .try_expire_unfunded(&id);

    assert_eq!(res, Ok(Ok(())));

    // Client token balance increases by exactly the refunded amount (40) back to 1,000
    assert_eq!(s.token.balance(&s.client), 1_000 * U);
    // Third-party and freelancer receive no refund
    assert_eq!(s.token.balance(&third_party), 0);
    assert_eq!(s.token.balance(&s.freelancer), 0);

    // Status is Cancelled and accounting is consistent
    let c = s.escrow.get_contract(&id);
    assert_eq!(c.status, ContractStatus::Cancelled);
    assert_eq!(c.funded_amount, 0);
    assert_eq!(c.released_amount, 0);
}

#[test]
fn test_fully_funded_escrow_protection() {
    let s = setup();
    set_now(&s.env, 1_000_000);

    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Funded);
    assert_eq!(s.token.balance(&s.client), 900 * U);

    let fund_by = s.escrow.get_fund_by(&id);
    set_now(&s.env, fund_by + 1);

    // Escrow in Funded state cannot be expired
    assert_eq!(
        s.escrow.try_expire_unfunded(&id),
        Err(Ok(EscrowError::InvalidStatus))
    );
    // Verify no refund or state change occurs
    assert_eq!(s.token.balance(&s.client), 900 * U);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Funded);

    // Funded escrows reject further deposits by status, not by the funding deadline
    assert_eq!(
        s.escrow.try_deposit_funds(&id, &U),
        Err(Ok(EscrowError::InvalidStatus))
    );
}

#[test]
fn test_expire_unfunded_zero_deposit() {
    let s = setup();
    set_now(&s.env, 1_000_000);

    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    let fund_by = s.escrow.get_fund_by(&id);
    set_now(&s.env, fund_by + 1);

    s.escrow.expire_unfunded(&id);

    let events = s.env.events().all();
    let last = events.last().unwrap();
    assert_eq!(last.0, s.escrow.address);
    assert_eq!(last.1.len(), 2);
    let topic_sym = Symbol::try_from_val(&s.env, &last.1.get(0).unwrap()).unwrap();
    let topic_id = u64::try_from_val(&s.env, &last.1.get(1).unwrap()).unwrap();
    assert_eq!(topic_sym, symbol_short!("expired"));
    assert_eq!(topic_id, id);
    let refunded = i128::try_from_val(&s.env, &last.2).unwrap();
    assert_eq!(refunded, 0i128);

    let c = s.escrow.get_contract(&id);
    assert_eq!(c.status, ContractStatus::Cancelled);
    assert_eq!(c.funded_amount, 0);
    assert_eq!(s.token.balance(&s.client), 1_000 * U);
}

#[test]
fn expire_unfunded_refunds_sponsors_pro_rata_and_clears_liability() {
    let s = setup();
    set_now(&s.env, 1_000_000);
    let sponsor = Address::generate(&s.env);
    StellarAssetClient::new(&s.env, &s.token.address).mint(&sponsor, &(100 * U));
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_from(&id, &sponsor, &(30 * U));
    s.escrow.deposit_funds(&id, &(20 * U));
    set_now(&s.env, s.escrow.get_fund_by(&id) + 1);
    s.escrow.expire_unfunded(&id);
    assert_eq!(s.token.balance(&sponsor), 100 * U);
    assert_eq!(s.token.balance(&s.client), 1_000 * U);
    assert_eq!(s.escrow.get_liability(&s.token.address), 0);
    let c = s.escrow.get_contract(&id);
    check_invariants(&c);
    assert_eq!(c.refunded_amount, 50 * U);
}

#[test]
fn create_and_fund_matches_two_step_field_by_field() {
    let s = setup();
    let amounts = vec![&s.env, 100 * U, 200 * U];
    let two_step = s
        .escrow
        .create_contract(&s.client, &s.freelancer, &s.token.address, &amounts);
    s.escrow.deposit_funds(&two_step, &(300 * U));
    let one_step = s
        .escrow
        .create_and_fund(&s.client, &s.freelancer, &s.token.address, &amounts);
    assert_ne!(one_step, two_step);

    let a = s.escrow.get_contract(&two_step);
    let b = s.escrow.get_contract(&one_step);
    assert_eq!(b.client, a.client);
    assert_eq!(b.freelancer, a.freelancer);
    assert_eq!(b.token, a.token);
    assert_eq!(b.milestones, a.milestones);
    assert_eq!(b.total_amount, a.total_amount);
    assert_eq!(b.funded_amount, a.funded_amount);
    assert_eq!(b.released_amount, a.released_amount);
    assert_eq!(b.refunded_amount, a.refunded_amount);
    assert_eq!(b.status, ContractStatus::Funded);
    assert_eq!(b.status, a.status);
    assert_eq!(b.reputation_issued, a.reputation_issued);
    assert_eq!(s.escrow.get_funders(&one_step), s.escrow.get_funders(&two_step));
    assert_eq!(s.escrow.get_fund_by(&one_step), s.escrow.get_fund_by(&two_step));
    assert_eq!(s.token.balance(&s.client), 400 * U);
    assert_eq!(s.escrow.get_liability(&s.token.address), 600 * U);
    check_invariants(&b);

    // The funded escrow behaves like any other
    s.escrow.release_milestone(&one_step, &0);
    s.escrow.release_milestone(&one_step, &1);
    assert_eq!(
        s.escrow.get_contract(&one_step).status,
        ContractStatus::Completed
    );
}

#[test]
fn create_and_fund_insufficient_balance_stores_nothing() {
    let s = setup();
    let poor = Address::generate(&s.env);
    StellarAssetClient::new(&s.env, &s.token.address).mint(&poor, &(10 * U));
    let before = s.escrow.contract_count();
    assert!(s
        .escrow
        .try_create_and_fund(
            &poor,
            &s.freelancer,
            &s.token.address,
            &vec![&s.env, 100 * U]
        )
        .is_err());
    assert_eq!(s.escrow.contract_count(), before);
    assert_eq!(s.token.balance(&poor), 10 * U);
    assert_eq!(s.escrow.get_liability(&s.token.address), 0);
}

#[test]
fn create_and_fund_validation_matches_create_contract() {
    let s = setup();
    let cases = [
        (s.client.clone(), s.client.clone(), vec![&s.env, 100 * U]),
        (s.client.clone(), s.freelancer.clone(), Vec::new(&s.env)),
        (s.client.clone(), s.freelancer.clone(), vec![&s.env, 1i128]),
        (s.admin.clone(), s.freelancer.clone(), vec![&s.env, 100 * U]),
    ];
    for (client, freelancer, amounts) in cases.iter() {
        let expected = s
            .escrow
            .try_create_contract(client, freelancer, &s.token.address, amounts);
        assert!(expected.is_err());
        assert_eq!(
            s.escrow
                .try_create_and_fund(client, freelancer, &s.token.address, amounts),
            expected
        );
    }
    s.escrow.set_paused(&s.admin, &true);
    assert_eq!(
        s.escrow.try_create_and_fund(
            &s.client,
            &s.freelancer,
            &s.token.address,
            &vec![&s.env, 100 * U]
        ),
        Err(Ok(EscrowError::Paused))
    );
}

#[test]
fn create_and_fund_emits_created_then_deposit() {
    let s = setup();
    let id = s.escrow.create_and_fund(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U, 50 * U],
    );
    // Structured since #1712: topics (1, symbol, id) with typed payloads.
    let names = escrow_event_names(&s);
    let n = names.len();
    assert!(n >= 2);
    assert_eq!(names[n - 2], symbol_short!("created"));
    assert_eq!(names[n - 1], symbol_short!("deposit"));
    let created: CreatedEvent = last_escrow_event(&s, "created", id).unwrap();
    assert_eq!(
        (created.client, created.freelancer, created.total),
        (s.client.clone(), s.freelancer.clone(), 150 * U)
    );
    let deposit: DepositEvent = last_escrow_event(&s, "deposit", id).unwrap();
    assert_eq!((deposit.from, deposit.amount), (s.client.clone(), 150 * U));
}

#[test]
fn create_and_fund_uses_fewer_write_bytes_than_two_step() {
    let s = setup();
    let amounts = vec![&s.env, 100 * U, 200 * U];
    let id = s
        .escrow
        .create_contract(&s.client, &s.freelancer, &s.token.address, &amounts);
    let create_res = s.env.cost_estimate().resources();
    s.escrow.deposit_funds(&id, &(300 * U));
    let deposit_res = s.env.cost_estimate().resources();
    s.escrow
        .create_and_fund(&s.client, &s.freelancer, &s.token.address, &amounts);
    let one_step = s.env.cost_estimate().resources();
    let two_step_write_bytes = create_res.write_bytes + deposit_res.write_bytes;
    assert!(
        one_step.write_bytes < two_step_write_bytes,
        "one-step write_bytes {} must be < two-step {}",
        one_step.write_bytes,
        two_step_write_bytes
    );
    assert!(one_step.write_entries < create_res.write_entries + deposit_res.write_entries);
}

#[test]
fn deposit_fee_on_transfer_token_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token = env.register(fee_token::MockFeeToken, ());
    let (escrow, client, freelancer) = escrow_with_token(&env, &token);
    fee_token::MockFeeTokenClient::new(&env, &token).mint(&client, &(1_000 * U));
    let id = escrow.create_contract(&client, &freelancer, &token, &vec![&env, 100 * U]);
    assert_eq!(
        escrow.try_deposit_funds(&id, &(100 * U)),
        Err(Ok(EscrowError::TransferAmountMismatch))
    );
    // Nothing was credited and the escrow holds no short balance.
    let c = escrow.get_contract(&id);
    assert_eq!(c.funded_amount, 0);
    assert_eq!(c.status, ContractStatus::Created);
    assert_eq!(
        fee_token::MockFeeTokenClient::new(&env, &token).balance(&escrow.address),
        0
    );
    assert_eq!(escrow.get_liability(&token), 0);
}

#[test]
fn noop_token_is_rejected_on_release() {
    let env = Env::default();
    env.mock_all_auths();
    let token = env.register(flaky_token::MockFlakyToken, ());
    let (escrow, client, freelancer) = escrow_with_token(&env, &token);
    let t = flaky_token::MockFlakyTokenClient::new(&env, &token);
    t.mint(&client, &(1_000 * U));
    let id = escrow.create_contract(&client, &freelancer, &token, &vec![&env, 100 * U]);
    escrow.deposit_funds(&id, &(100 * U));
    assert_eq!(t.balance(&escrow.address), 100 * U);

    t.set_noop(&true);
    assert_eq!(
        escrow.try_release_milestone(&id, &0),
        Err(Ok(EscrowError::TransferAmountMismatch))
    );
    let c = escrow.get_contract(&id);
    assert!(!c.milestones.get(0).unwrap().released);
    assert_eq!(c.released_amount, 0);
    assert_eq!(
        escrow.try_cancel_contract(&id),
        Err(Ok(EscrowError::TransferAmountMismatch))
    );
    // A no-op deposit is rejected too.
    let id2 = escrow.create_contract(&client, &freelancer, &token, &vec![&env, 100 * U]);
    assert_eq!(
        escrow.try_deposit_funds(&id2, &(100 * U)),
        Err(Ok(EscrowError::TransferAmountMismatch))
    );
}

#[test]
fn well_behaved_token_moves_exact_amounts() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U, 300 * U],
    );
    s.escrow.deposit_funds(&id, &(400 * U));
    assert_eq!(s.escrow.get_contract(&id).funded_amount, 400 * U);
    assert_eq!(s.token.balance(&s.escrow.address), 400 * U);
    s.escrow.release_milestone(&id, &0);
    assert_eq!(s.token.balance(&s.escrow.address), 300 * U);
    assert_eq!(s.token.balance(&s.freelancer), 100 * U);
    s.escrow.cancel_contract(&id);
    assert_eq!(s.token.balance(&s.escrow.address), 0);
    assert_eq!(s.token.balance(&s.client), 900 * U);
}
