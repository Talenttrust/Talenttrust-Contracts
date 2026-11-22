//! Creation, completion, cancellation, expiry and other lifecycle behaviour.

use super::*;

#[test]
fn initialize_only_once() {
    let s = setup();
    let other = Address::generate(&s.env);
    assert_eq!(
        s.escrow.try_initialize(&other),
        Err(Ok(EscrowError::AlreadyInitialized))
    );
}

#[test]
fn full_happy_path() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U, 200 * U],
    );
    assert_eq!(id, 1);
    s.escrow.deposit_funds(&id, &(300 * U));
    assert_eq!(s.token.balance(&s.client), 700 * U);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Funded);
    s.escrow.release_milestone(&id, &0);
    s.escrow.release_milestone(&id, &1);
    assert_eq!(s.token.balance(&s.freelancer), 300 * U);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);
    s.escrow.issue_reputation(&id, &5);
    let rep = s.escrow.get_reputation(&s.freelancer);
    assert_eq!(rep.completed_contracts, 1);
    assert_eq!(rep.total_rating, 5);
}

#[test]
fn cancel_unfunded_escrow_does_not_count_cancellation() {
    let s = setup();

    // 1. Unfunded cancellation -> cancellations remains 0
    let id1 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.cancel_contract(&id1);
    assert_eq!(s.escrow.get_outcomes(&s.client).cancellations, 0);

    // 2. Funded cancellation -> cancellations increments to 1
    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id2, &(100 * U));
    s.escrow.cancel_contract(&id2);
    assert_eq!(s.escrow.get_outcomes(&s.client).cancellations, 1);
}

#[test]
fn test_all_36_status_transition_pairs() {
    let statuses = [
        ContractStatus::Created,
        ContractStatus::Funded,
        ContractStatus::Completed,
        ContractStatus::Disputed,
        ContractStatus::Resolved,
        ContractStatus::Cancelled,
    ];

    let allowed_edges = [
        (ContractStatus::Created, ContractStatus::Funded),
        (ContractStatus::Created, ContractStatus::Completed),
        (ContractStatus::Funded, ContractStatus::Completed),
        (ContractStatus::Created, ContractStatus::Disputed),
        (ContractStatus::Funded, ContractStatus::Disputed),
        (ContractStatus::Disputed, ContractStatus::Resolved),
        (ContractStatus::Created, ContractStatus::Cancelled),
        (ContractStatus::Funded, ContractStatus::Cancelled),
    ];

    let env = Env::default();
    let client = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let token = Address::generate(&env);

    let mut count = 0;
    for &from in &statuses {
        for &to in &statuses {
            count += 1;
            let mut c = EscrowContract {
                id: 1,
                client: client.clone(),
                freelancer: freelancer.clone(),
                token: token.clone(),
                milestones: Vec::new(&env),
                total_amount: 100,
                funded_amount: 0,
                released_amount: 0,
                refunded_amount: 0,
                status: from,
                reputation_issued: false,
            };

            let is_allowed = allowed_edges.contains(&(from, to));
            let res = transition(&mut c, to);

            if is_allowed {
                assert!(res.is_ok(), "Expected ({:?} -> {:?}) to succeed", from, to);
                assert_eq!(c.status, to);
            } else {
                assert_eq!(
                    res,
                    Err(EscrowError::InvalidTransition),
                    "Expected ({:?} -> {:?}) to fail with InvalidTransition",
                    from,
                    to
                );
                assert_eq!(
                    c.status, from,
                    "State must remain untouched on invalid transition ({:?} -> {:?})",
                    from, to
                );
            }
        }
    }
    assert_eq!(count, 36, "Must test exactly all 36 (from, to) pairs");
}

#[test]
fn randomized_lifecycle_property_test() {
    const SEED: u64 = 0x5EED_2026_1019;
    let mut rng = Lcg::new(SEED);

    let s = setup();
    // Mint ample tokens to client for all iterations
    StellarAssetClient::new(&s.env, &s.token.address).mint(&s.client, &(10_000_000 * U));

    let mut id1 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50 * U, 50 * U],
    );
    let mut id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 40 * U, 60 * U],
    );

    let mut c1 = s.escrow.get_contract(&id1);
    let mut c2 = s.escrow.get_contract(&id2);
    let mut bal_contract = s.token.balance(&s.escrow.address);

    for iteration in 0..500 {
        if matches!(
            c1.status,
            ContractStatus::Completed | ContractStatus::Resolved | ContractStatus::Cancelled
        ) {
            let m0 = (rng.next_bounded(50) + 10) as i128 * U;
            let m1 = (rng.next_bounded(50) + 10) as i128 * U;
            id1 = s.escrow.create_contract(
                &s.client,
                &s.freelancer,
                &s.token.address,
                &vec![&s.env, m0, m1],
            );
            c1 = s.escrow.get_contract(&id1);
        }

        if matches!(
            c2.status,
            ContractStatus::Completed | ContractStatus::Resolved | ContractStatus::Cancelled
        ) {
            let m0 = (rng.next_bounded(40) + 15) as i128 * U;
            let m1 = (rng.next_bounded(40) + 15) as i128 * U;
            id2 = s.escrow.create_contract(
                &s.client,
                &s.freelancer,
                &s.token.address,
                &vec![&s.env, m0, m1],
            );
            c2 = s.escrow.get_contract(&id2);
        }

        let escrow_ids = [id1, id2];
        let steps = rng.next_bounded(2) + 1; // 1 to 2 operations per sequence (up to 12)

        for _step in 0..steps {
            let target_idx = rng.next_bounded(2) as usize;
            let target_id = escrow_ids[target_idx];
            let op = rng.next_bounded(6);

            let snap_target_held = if target_idx == 0 {
                c1.funded_amount - c1.released_amount
            } else {
                c2.funded_amount - c2.released_amount
            };
            // Dispute bonds (#1683) are held by the contract outside `held`.
            let snap_target_bond = s
                .escrow
                .get_dispute_bond(&target_id)
                .map(|(_, b)| b)
                .unwrap_or(0);

            let op_res = match op {
                0 => {
                    let amt = (rng.next_bounded(120) + 1) as i128 * U;
                    s.escrow.try_deposit_funds(&target_id, &amt)
                }
                1 => {
                    let idx = rng.next_bounded(3);
                    s.escrow.try_release_milestone(&target_id, &idx)
                }
                2 => {
                    let caller = match rng.next_bounded(3) {
                        0 => s.client.clone(),
                        1 => s.freelancer.clone(),
                        _ => Address::generate(&s.env),
                    };
                    s.escrow.try_open_dispute(&target_id, &caller)
                }
                3 => {
                    let bps = rng.next_bounded(12_000);
                    s.escrow
                        .try_resolve_dispute(&s.admin, &target_id, &bps, &(bps % 2 == 0))
                }
                4 => s.escrow.try_cancel_contract(&target_id),
                5 => {
                    let rating = rng.next_bounded(7);
                    s.escrow.try_issue_reputation(&target_id, &rating)
                }
                _ => unreachable!(),
            };

            if op_res.is_ok() {
                let new_c = s.escrow.get_contract(&target_id);
                check_invariants(&new_c);
                let new_bal = s.token.balance(&s.escrow.address);
                let new_held = new_c.funded_amount - new_c.released_amount;
                let delta_held = new_held - snap_target_held;
                let new_bond = s
                    .escrow
                    .get_dispute_bond(&target_id)
                    .map(|(_, b)| b)
                    .unwrap_or(0);
                let delta_bond = new_bond - snap_target_bond;
                assert_eq!(
                    new_bal,
                    bal_contract + delta_held + delta_bond,
                    "Contract token balance mismatch in iteration {} (seed: {:#x})",
                    iteration,
                    SEED
                );
                if target_idx == 0 {
                    c1 = new_c;
                } else {
                    c2 = new_c;
                }
                bal_contract = new_bal;
            } else {
                let current_c = s.escrow.get_contract(&target_id);
                let snap_c = if target_idx == 0 { &c1 } else { &c2 };
                assert_eq!(
                    &current_c,
                    snap_c,
                    "State mutated on failure in iteration {} (seed: {:#x})",
                    iteration,
                    SEED
                );
                assert_eq!(
                    s.token.balance(&s.escrow.address),
                    bal_contract,
                    "Balance mutated on failure in iteration {} (seed: {:#x})",
                    iteration,
                    SEED
                );
            }
        }
    }
}

#[test]
fn bonus_on_completed_escrow() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.release_milestone(&id, &0);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);

    let contract_before = s.token.balance(&s.escrow.address);
    s.escrow.pay_bonus(&id, &(25 * U));
    assert_eq!(s.escrow.get_bonus(&id), 25 * U);
    assert_eq!(s.escrow.get_bonus_total(&s.freelancer), 25 * U);
    assert_eq!(s.token.balance(&s.freelancer), 125 * U);
    assert_eq!(s.token.balance(&s.client), 875 * U);
    assert_eq!(s.token.balance(&s.escrow.address), contract_before);
    let c = s.escrow.get_contract(&id);
    assert_eq!(c.funded_amount, 100 * U);
    assert_eq!(c.released_amount, 100 * U);
}

#[test]
fn bonus_on_funded_escrow() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Funded);

    let contract_before = s.token.balance(&s.escrow.address);
    s.escrow.pay_bonus(&id, &(10 * U));
    assert_eq!(s.escrow.get_bonus(&id), 10 * U);
    assert_eq!(s.token.balance(&s.freelancer), 10 * U);
    assert_eq!(s.token.balance(&s.escrow.address), contract_before);
    let c = s.escrow.get_contract(&id);
    assert_eq!(c.funded_amount, 100 * U);
    assert_eq!(c.released_amount, 0);
}

#[test]
fn bonus_rejected_on_disputed_and_cancelled() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    open_dispute_bonded(&s, &id, &s.freelancer);
    assert!(s.escrow.try_pay_bonus(&id, &(10 * U)).is_err());

    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50 * U],
    );
    s.escrow.deposit_funds(&id2, &(50 * U));
    s.escrow.cancel_contract(&id2);
    assert!(s.escrow.try_pay_bonus(&id2, &(10 * U)).is_err());
}

#[test]
fn bonus_rejects_zero_and_negative() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    assert!(s.escrow.try_pay_bonus(&id, &0).is_err());
    assert!(s.escrow.try_pay_bonus(&id, &-1).is_err());
    assert_eq!(s.escrow.get_bonus(&id), 0);
}

#[test]
fn bonus_requires_client_auth() {
    use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
    use soroban_sdk::IntoVal;
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    // Only the freelancer authorizes: the client's auth is missing, so it must fail.
    s.env.mock_auths(&[MockAuth {
        address: &s.freelancer,
        invoke: &MockAuthInvoke {
            contract: &s.escrow.address,
            fn_name: "pay_bonus",
            args: (id, 10 * U).into_val(&s.env),
            sub_invokes: &[],
        },
    }]);
    assert!(s.escrow.try_pay_bonus(&id, &(10 * U)).is_err());
    assert_eq!(s.escrow.get_bonus(&id), 0);
    assert_eq!(s.token.balance(&s.freelancer), 0);
}

#[test]
fn bonus_cumulative_across_two_escrows() {
    let s = setup();
    let id1 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id1, &(100 * U));
    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50 * U],
    );
    s.escrow.deposit_funds(&id2, &(50 * U));

    s.escrow.pay_bonus(&id1, &(15 * U));
    s.escrow.pay_bonus(&id2, &(25 * U));
    assert_eq!(s.escrow.get_bonus(&id1), 15 * U);
    assert_eq!(s.escrow.get_bonus(&id2), 25 * U);
    assert_eq!(s.escrow.get_bonus_total(&s.freelancer), 40 * U);
    assert_eq!(s.token.balance(&s.freelancer), 40 * U);
}

#[test]
fn get_payout_address_defaults_to_freelancer() {
    let s = setup();
    let id = funded_escrow(&s);
    assert_eq!(s.escrow.get_payout_address(&id), s.freelancer);
}

#[test]
fn release_credits_payout_address_not_freelancer() {
    let s = setup();
    let id = funded_escrow(&s);
    let payout_wallet = Address::generate(&s.env);

    s.escrow.set_payout_address(&id, &payout_wallet);
    assert_eq!(s.escrow.get_payout_address(&id), payout_wallet);

    s.escrow.release_milestone(&id, &0);

    assert_eq!(s.token.balance(&payout_wallet), 100 * U);
    assert_eq!(s.token.balance(&s.freelancer), 0);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);
}

#[test]
fn resolve_dispute_credits_payout_address_not_freelancer() {
    let s = setup();
    let id = funded_escrow(&s);
    let payout_wallet = Address::generate(&s.env);

    s.escrow.set_payout_address(&id, &payout_wallet);
    open_dispute_bonded(&s, &id, &s.freelancer);
    // set_payout_address must be rejected while disputed
    let other = Address::generate(&s.env);
    assert_eq!(
        s.escrow.try_set_payout_address(&id, &other),
        Err(Ok(EscrowError::InvalidStatus))
    );
    resolve_refunding_bond(&s, &id, &10_000); // full share to freelancer side

    assert_eq!(s.token.balance(&payout_wallet), 100 * U);
    assert_eq!(s.token.balance(&s.freelancer), 0);
}

#[test]
fn reputation_still_keyed_by_freelancer() {
    let s = setup();
    let id = funded_escrow(&s);
    let payout_wallet = Address::generate(&s.env);

    s.escrow.set_payout_address(&id, &payout_wallet);
    s.escrow.release_milestone(&id, &0);
    s.escrow.issue_reputation(&id, &5);

    let rep = s.escrow.get_reputation(&s.freelancer);
    assert_eq!(rep.completed_contracts, 1);
    assert_eq!(rep.total_rating, 5);
    // payout wallet gets no reputation
    let rep_payout = s.escrow.get_reputation(&payout_wallet);
    assert_eq!(rep_payout.completed_contracts, 0);
}

#[test]
fn client_cannot_set_payout_address() {
    let s = setup();
    let id = funded_escrow(&s);
    let payout_wallet = Address::generate(&s.env);

    // Authorize only the client (not the freelancer). Since the contract
    // calls `c.freelancer.require_auth()`, this must fail.
    s.env.mock_auths(&[MockAuth {
        address: &s.client,
        invoke: &MockAuthInvoke {
            contract: &s.escrow.address,
            fn_name: "set_payout_address",
            args: (id, payout_wallet.clone()).try_into_val(&s.env).unwrap(),
            sub_invokes: &[],
        },
    }]);
    assert!(s.escrow.try_set_payout_address(&id, &payout_wallet).is_err());
    s.env.mock_all_auths();
}

#[test]
fn setting_payout_to_client_rejected() {
    let s = setup();
    let id = funded_escrow(&s);

    assert_eq!(
        s.escrow.try_set_payout_address(&id, &s.client),
        Err(Ok(EscrowError::InvalidParties))
    );
}

#[test]
fn setting_payout_to_contract_address_rejected() {
    let s = setup();
    let id = funded_escrow(&s);
    let contract_addr = s.escrow.address.clone();

    assert_eq!(
        s.escrow.try_set_payout_address(&id, &contract_addr),
        Err(Ok(EscrowError::InvalidParties))
    );
}

#[test]
fn setting_payout_during_dispute_rejected() {
    let s = setup();
    let id = funded_escrow(&s);
    let payout_wallet = Address::generate(&s.env);

    open_dispute_bonded(&s, &id, &s.freelancer);
    assert_eq!(
        s.escrow.try_set_payout_address(&id, &payout_wallet),
        Err(Ok(EscrowError::InvalidStatus))
    );
}

#[test]
fn set_payout_address_emits_event() {
    use soroban_sdk::Symbol;

    let s = setup();
    let id = funded_escrow(&s);
    let payout_wallet = Address::generate(&s.env);

    s.escrow.set_payout_address(&id, &payout_wallet);

    // Events are (contract_address, Vec<Val> topics, Val data).
    // Our event publishes topics = (symbol_short!("payout"), id) and
    // data = (old_addr, new_addr).
    let events = s.env.events().all();
    let payout_sym: Symbol = symbol_short!("payout");
    let matching: std::vec::Vec<_> = events
        .iter()
        .filter(|(_contract, topics, _data)| {
            topics.len() >= 2
                && Symbol::try_from_val(&s.env, &topics.get(0).unwrap()).ok().as_ref()
                    == Some(&payout_sym)
        })
        .collect();
    assert!(!matching.is_empty(), "no 'payout' event found in {:?}", events);
    let (_contract, _topics, data) = &matching[0];
    let (old, new): (Address, Address) =
        <(Address, Address)>::try_from_val(&s.env, data).expect("data decode failed");
    assert_eq!(old, s.freelancer);
    assert_eq!(new, payout_wallet);
}

#[test]
fn rejects_client_equal_to_contract_address() {
    let s = setup();
    let contract_addr = s.escrow.address.clone();
    assert_eq!(
        s.escrow.try_create_contract(
            &contract_addr,
            &s.freelancer,
            &s.token.address,
            &milestones(&s, 1_000_000),
        ),
        Err(Ok(EscrowError::InvalidParties))
    );
}

#[test]
fn rejects_freelancer_equal_to_contract_address() {
    let s = setup();
    let contract_addr = s.escrow.address.clone();
    assert_eq!(
        s.escrow.try_create_contract(
            &s.client,
            &contract_addr,
            &s.token.address,
            &milestones(&s, 1_000_000),
        ),
        Err(Ok(EscrowError::InvalidParties))
    );
}

#[test]
fn rejects_token_equal_to_contract_address() {
    let s = setup();
    let contract_addr = s.escrow.address.clone();
    assert_eq!(
        s.escrow.try_create_contract(
            &s.client,
            &s.freelancer,
            &contract_addr,
            &milestones(&s, 1_000_000),
        ),
        Err(Ok(EscrowError::InvalidParties))
    );
}

#[test]
fn rejects_client_equal_to_admin() {
    let s = setup();
    assert_eq!(
        s.escrow.try_create_contract(
            &s.admin,
            &s.freelancer,
            &s.token.address,
            &milestones(&s, 1_000_000),
        ),
        Err(Ok(EscrowError::AdminCannotBeParty))
    );
}

#[test]
fn rejects_freelancer_equal_to_admin() {
    let s = setup();
    assert_eq!(
        s.escrow.try_create_contract(
            &s.client,
            &s.admin,
            &s.token.address,
            &milestones(&s, 1_000_000),
        ),
        Err(Ok(EscrowError::AdminCannotBeParty))
    );
}

#[test]
fn rejects_milestone_below_minimum() {
    let s = setup();
    assert_eq!(
        s.escrow.try_create_contract(
            &s.client,
            &s.freelancer,
            &s.token.address,
            &milestones(&s, MIN_MILESTONE_AMOUNT - 1),
        ),
        Err(Ok(EscrowError::MilestoneTooSmall))
    );
}

#[test]
fn accepts_milestone_at_minimum() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &milestones(&s, MIN_MILESTONE_AMOUNT),
    );
    assert_eq!(id, 1);
}

#[test]
fn rejects_total_above_maximum() {
    let s = setup();
    // Two milestones each at MAX_ESCROW_TOTAL / 2 + 1 — total exceeds the cap
    // while each individual milestone is valid.
    let half = MAX_ESCROW_TOTAL / 2;
    assert_eq!(
        s.escrow.try_create_contract(
            &s.client,
            &s.freelancer,
            &s.token.address,
            &vec![&s.env, half + 1, half + 1],
        ),
        Err(Ok(EscrowError::InvalidAmount))
    );
}

#[test]
fn accepts_total_at_maximum() {
    let s = setup();
    // Two milestones summing exactly to MAX_ESCROW_TOTAL
    let half = MAX_ESCROW_TOTAL / 2;
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, half, MAX_ESCROW_TOTAL - half],
    );
    assert_eq!(id, 1);
}

#[test]
fn validation_order_parties_before_amounts() {
    // An input that violates BOTH the party rule (client == admin) and the
    // amount rule (below minimum) must return the party error first.
    let s = setup();
    assert_eq!(
        s.escrow.try_create_contract(
            &s.admin,
            &s.freelancer,
            &s.token.address,
            &milestones(&s, 1), // below MIN_MILESTONE_AMOUNT
        ),
        Err(Ok(EscrowError::AdminCannotBeParty))
    );
}

#[test]
fn team_split_pays_three_way_with_lead_remainder() {
    let s = setup();
    let teammate_a = Address::generate(&s.env);
    let teammate_b = Address::generate(&s.env);
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 1_000_001i128],
    );
    let payees = vec![
        &s.env,
        (s.freelancer.clone(), 5_000u32),
        (teammate_a.clone(), 3_000u32),
        (teammate_b.clone(), 2_000u32),
    ];
    s.escrow.set_team_split(&id, &payees);
    assert_eq!(s.escrow.get_team_split(&id), payees);

    s.escrow.deposit_funds(&id, &1_000_001);
    s.escrow.release_milestone(&id, &0);

    // 1_000_001 floors to 500_000 / 300_000 / 200_000; the lead gets the 1-unit remainder.
    assert_eq!(s.token.balance(&s.freelancer), 500_001);
    assert_eq!(s.token.balance(&teammate_a), 300_000);
    assert_eq!(s.token.balance(&teammate_b), 200_000);
    assert_eq!(
        s.token.balance(&s.freelancer)
            + s.token.balance(&teammate_a)
            + s.token.balance(&teammate_b),
        1_000_001
    );
    assert_eq!(s.token.balance(&s.client), 1_000 * U - 1_000_001);
}

#[test]
fn rejects_invalid_team_splits() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    let other = Address::generate(&s.env);

    let bad_sum = vec![
        &s.env,
        (s.freelancer.clone(), 5_000u32),
        (other.clone(), 4_000u32),
    ];
    assert_eq!(
        s.escrow.try_set_team_split(&id, &bad_sum),
        Err(Ok(EscrowError::InvalidSplitConfig))
    );

    let zero_share = vec![&s.env, (s.freelancer.clone(), 0u32)];
    assert_eq!(
        s.escrow.try_set_team_split(&id, &zero_share),
        Err(Ok(EscrowError::InvalidSplitConfig))
    );

    let duplicate = vec![
        &s.env,
        (s.freelancer.clone(), 5_000u32),
        (s.freelancer.clone(), 5_000u32),
    ];
    assert_eq!(
        s.escrow.try_set_team_split(&id, &duplicate),
        Err(Ok(EscrowError::InvalidSplitConfig))
    );

    let six_payees = vec![
        &s.env,
        (Address::generate(&s.env), 1_666u32),
        (Address::generate(&s.env), 1_666u32),
        (Address::generate(&s.env), 1_666u32),
        (Address::generate(&s.env), 1_666u32),
        (Address::generate(&s.env), 1_666u32),
        (Address::generate(&s.env), 1_670u32),
    ];
    assert_eq!(
        s.escrow.try_set_team_split(&id, &six_payees),
        Err(Ok(EscrowError::InvalidSplitConfig))
    );

    let includes_client = vec![
        &s.env,
        (s.freelancer.clone(), 5_000u32),
        (s.client.clone(), 5_000u32),
    ];
    assert_eq!(
        s.escrow.try_set_team_split(&id, &includes_client),
        Err(Ok(EscrowError::InvalidSplitConfig))
    );

    let valid = vec![&s.env, (s.freelancer.clone(), 10_000u32)];
    s.escrow.set_team_split(&id, &valid);
    // A partial deposit keeps the status at Created, but splits are frozen once
    // any funds are in escrow.
    s.escrow.deposit_funds(&id, &(40 * U));
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Created);
    assert_eq!(
        s.escrow.try_set_team_split(&id, &valid),
        Err(Ok(EscrowError::InvalidStatus))
    );
}

#[test]
fn dispute_freelancer_share_respects_team_split() {
    let s = setup();
    let teammate_a = Address::generate(&s.env);
    let teammate_b = Address::generate(&s.env);
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 4_000_004i128],
    );
    let payees = vec![
        &s.env,
        (s.freelancer.clone(), 5_000u32),
        (teammate_a.clone(), 3_000u32),
        (teammate_b.clone(), 2_000u32),
    ];
    s.escrow.set_team_split(&id, &payees);
    s.escrow.deposit_funds(&id, &4_000_004);
    open_dispute_bonded(&s, &id, &s.freelancer);
    resolve_refunding_bond(&s, &id, &2_500);

    // 25% of 4_000_004 is 1_000_001; the split floors to 500_000/300_000/200_000
    // and the lead gets the 1-unit remainder.
    assert_eq!(s.token.balance(&s.freelancer), 500_001);
    assert_eq!(s.token.balance(&teammate_a), 300_000);
    assert_eq!(s.token.balance(&teammate_b), 200_000);
    assert_eq!(s.token.balance(&s.client), 1_000 * U - 1_000_001);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Resolved);
}

#[test]
fn summary_tracks_lifecycle_and_milestones() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U, 200 * U, 300 * U],
    );

    s.escrow.deposit_funds(&id, &(100 * U));
    assert_eq!(
        s.escrow.get_escrow_summary(&id),
        EscrowSummary {
            id,
            status: ContractStatus::Created,
            total_amount: 600 * U,
            funded_amount: 100 * U,
            released_amount: 0,
            held: 100 * U,
            remaining_to_fund: 500 * U,
            milestones_total: 3,
            milestones_released: 0,
            next_unreleased: Some(0),
            next_release_affordable: true,
        }
    );

    s.escrow.deposit_funds(&id, &(500 * U));
    assert_eq!(
        s.escrow.get_escrow_summary(&id),
        EscrowSummary {
            id,
            status: ContractStatus::Funded,
            total_amount: 600 * U,
            funded_amount: 600 * U,
            released_amount: 0,
            held: 600 * U,
            remaining_to_fund: 0,
            milestones_total: 3,
            milestones_released: 0,
            next_unreleased: Some(0),
            next_release_affordable: true,
        }
    );

    s.escrow.release_milestone(&id, &0);
    assert_eq!(
        s.escrow.get_escrow_summary(&id),
        EscrowSummary {
            id,
            status: ContractStatus::Funded,
            total_amount: 600 * U,
            funded_amount: 600 * U,
            released_amount: 100 * U,
            held: 500 * U,
            remaining_to_fund: 0,
            milestones_total: 3,
            milestones_released: 1,
            next_unreleased: Some(1),
            next_release_affordable: true,
        }
    );
    assert_eq!(
        s.escrow.get_milestone(&id, &0),
        Milestone {
            amount: 100 * U,
            released: true,
        }
    );
    assert_eq!(
        s.escrow.get_milestone(&id, &1),
        Milestone {
            amount: 200 * U,
            released: false,
        }
    );
    assert_eq!(
        s.escrow.try_get_milestone(&id, &3),
        Err(Ok(EscrowError::MilestoneNotFound))
    );

    s.escrow.release_milestone(&id, &1);
    s.escrow.release_milestone(&id, &2);
    assert_eq!(
        s.escrow.get_escrow_summary(&id),
        EscrowSummary {
            id,
            status: ContractStatus::Completed,
            total_amount: 600 * U,
            funded_amount: 600 * U,
            released_amount: 600 * U,
            held: 0,
            remaining_to_fund: 0,
            milestones_total: 3,
            milestones_released: 3,
            next_unreleased: None,
            next_release_affordable: false,
        }
    );
}

#[test]
fn next_release_affordable_requires_full_milestone_coverage() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U, 200 * U],
    );

    s.escrow.deposit_funds(&id, &(99 * U));
    let summary = s.escrow.get_escrow_summary(&id);
    assert_eq!(summary.status, ContractStatus::Created);
    assert_eq!(summary.next_unreleased, Some(0));
    assert!(!summary.next_release_affordable);

    s.escrow.deposit_funds(&id, &(1 * U));
    let summary = s.escrow.get_escrow_summary(&id);
    assert_eq!(summary.next_unreleased, Some(0));
    assert!(summary.next_release_affordable);

    s.escrow.release_milestone(&id, &0);
    s.escrow.deposit_funds(&id, &(100 * U));
    let summary = s.escrow.get_escrow_summary(&id);
    assert_eq!(summary.next_unreleased, Some(1));
    assert!(!summary.next_release_affordable);
}

#[test]
fn summary_after_cancel_and_resolve() {
    let s = setup();
    let cancelled = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U, 100 * U],
    );
    s.escrow.deposit_funds(&cancelled, &(200 * U));
    s.escrow.release_milestone(&cancelled, &0);
    s.escrow.cancel_contract(&cancelled);
    assert_eq!(
        s.escrow.get_escrow_summary(&cancelled),
        EscrowSummary {
            id: cancelled,
            status: ContractStatus::Cancelled,
            total_amount: 200 * U,
            funded_amount: 100 * U,
            released_amount: 100 * U,
            held: 0,
            remaining_to_fund: 100 * U,
            milestones_total: 2,
            milestones_released: 1,
            next_unreleased: Some(1),
            next_release_affordable: false,
        }
    );

    let resolved = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 400 * U],
    );
    s.escrow.deposit_funds(&resolved, &(400 * U));
    open_dispute_bonded(&s, &resolved, &s.freelancer);
    resolve_refunding_bond(&s, &resolved, &2_500);
    assert_eq!(
        s.escrow.get_escrow_summary(&resolved),
        EscrowSummary {
            id: resolved,
            status: ContractStatus::Resolved,
            total_amount: 400 * U,
            funded_amount: 100 * U,
            released_amount: 100 * U,
            held: 0,
            remaining_to_fund: 300 * U,
            milestones_total: 1,
            milestones_released: 0,
            next_unreleased: Some(0),
            next_release_affordable: false,
        }
    );
}

#[test]
fn batch_summaries_skip_unknown_and_enforce_cap() {
    let s = setup();
    let first = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 10 * U],
    );
    let second = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 20 * U],
    );
    let third = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 30 * U],
    );

    let summaries = s
        .escrow
        .get_escrow_summaries(&vec![&s.env, first, 99, second, third]);
    assert_eq!(summaries.len(), 3);
    assert_eq!(summaries.get(0).unwrap().id, first);
    assert_eq!(summaries.get(1).unwrap().id, second);
    assert_eq!(summaries.get(2).unwrap().id, third);

    let mut max_ids = Vec::new(&s.env);
    for i in 0..MAX_SUMMARY_BATCH {
        max_ids.push_back(1_000 + i as u64);
    }
    assert_eq!(s.escrow.get_escrow_summaries(&max_ids).len(), 0);

    max_ids.push_back(1_000 + MAX_SUMMARY_BATCH as u64);
    assert_eq!(
        s.escrow.try_get_escrow_summaries(&max_ids),
        Err(Ok(EscrowError::InvalidAmount))
    );
}

#[test]
fn sequential_milestones_enforced() {
    let s = setup();
    let id = s.escrow.create_contract_sequential(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U, 200 * U, 300 * U],
    );
    s.escrow.deposit_funds(&id, &(600 * U));

    // Cannot release milestone 2 before milestone 1
    assert_eq!(
        s.escrow.try_release_milestone(&id, &2),
        Err(Ok(EscrowError::OutOfOrder))
    );

    // Cannot release milestone 1 before milestone 0
    assert_eq!(
        s.escrow.try_release_milestone(&id, &1),
        Err(Ok(EscrowError::OutOfOrder))
    );

    // 0 then 1 succeeds
    s.escrow.release_milestone(&id, &0);
    s.escrow.release_milestone(&id, &1);

    // Cannot release milestone 0 again
    assert_eq!(
        s.escrow.try_release_milestone(&id, &0),
        Err(Ok(EscrowError::MilestoneAlreadyReleased))
    );

    s.escrow.release_milestone(&id, &2);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);
    assert_eq!(s.token.balance(&s.freelancer), 600 * U);
}

#[test]
fn non_sequential_milestones_allows_any_order() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U, 200 * U, 300 * U],
    );
    s.escrow.deposit_funds(&id, &(600 * U));

    s.escrow.release_milestone(&id, &2);
    s.escrow.release_milestone(&id, &0);
    s.escrow.release_milestone(&id, &1);

    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);
}

#[test]
fn next_releasable_tracks_lowest_unreleased() {
    let s = setup();
    let id = s.escrow.create_contract_sequential(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U, 200 * U],
    );
    s.escrow.deposit_funds(&id, &(300 * U));
    assert_eq!(s.escrow.next_releasable(&id), Some(0));
    s.escrow.release_milestone(&id, &0);
    assert_eq!(s.escrow.next_releasable(&id), Some(1));
    s.escrow.release_milestone(&id, &1);
    assert_eq!(s.escrow.next_releasable(&id), None);

    // Works for non-sequential escrows too
    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U, 200 * U],
    );
    s.escrow.deposit_funds(&id2, &(300 * U));
    s.escrow.release_milestone(&id2, &1);
    assert_eq!(s.escrow.next_releasable(&id2), Some(0));
}

#[test]
fn sequential_create_emits_seq_event_and_validates_like_create() {
    let s = setup();
    let id = s.escrow.create_contract_sequential(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    let events = s.env.events().all();
    let last = events.last().unwrap();
    assert_eq!(last.0, s.escrow.address);
    assert_eq!(
        Symbol::try_from_val(&s.env, &last.1.get(0).unwrap()).unwrap(),
        symbol_short!("seq")
    );
    assert_eq!(u64::try_from_val(&s.env, &last.1.get(1).unwrap()).unwrap(), id);

    assert_eq!(
        s.escrow.try_create_contract_sequential(
            &s.client,
            &s.client,
            &s.token.address,
            &vec![&s.env, 100 * U],
        ),
        Err(Ok(EscrowError::InvalidParties))
    );
    assert_eq!(
        s.escrow.try_create_contract_sequential(
            &s.client,
            &s.freelancer,
            &s.token.address,
            &vec![&s.env, 1i128],
        ),
        Err(Ok(EscrowError::MilestoneTooSmall))
    );
}

#[test]
fn release_milestones_basic() {
    let s = setup();
    let id = three_milestone_escrow(&s, false);
    let paid = s
        .escrow
        .release_milestones(&id, &vec![&s.env, 0u32, 1u32, 2u32]);
    assert_eq!(paid, 600 * U);
    assert_eq!(s.token.balance(&s.freelancer), 600 * U);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);
}

#[test]
fn release_milestones_partial_then_single_completes() {
    let s = setup();
    let id = three_milestone_escrow(&s, false);
    s.escrow.release_milestones(&id, &vec![&s.env, 0u32, 1u32]);
    assert_eq!(s.token.balance(&s.freelancer), 300 * U);
    assert_eq!(s.escrow.get_contract(&id).released_amount, 300 * U);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Funded);
    s.escrow.release_milestone(&id, &2);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);
    assert_eq!(s.token.balance(&s.freelancer), 600 * U);
}

#[test]
fn release_milestones_rejects_empty() {
    let s = setup();
    let id = three_milestone_escrow(&s, false);
    assert_eq!(
        s.escrow.try_release_milestones(&id, &Vec::new(&s.env)),
        Err(Ok(EscrowError::InvalidMilestones))
    );
}

#[test]
fn release_milestones_rejects_duplicates() {
    let s = setup();
    let id = three_milestone_escrow(&s, false);
    assert_eq!(
        s.escrow
            .try_release_milestones(&id, &vec![&s.env, 0u32, 1u32, 0u32]),
        Err(Ok(EscrowError::DuplicateIndex))
    );
}

#[test]
fn release_milestones_already_released_fails_without_state_change() {
    let s = setup();
    let id = three_milestone_escrow(&s, false);
    s.escrow.release_milestone(&id, &0);
    let before = s.escrow.get_contract(&id);
    let bal_before = s.token.balance(&s.freelancer);
    assert_eq!(
        s.escrow.try_release_milestones(&id, &vec![&s.env, 1u32, 0u32]),
        Err(Ok(EscrowError::MilestoneAlreadyReleased))
    );
    assert_eq!(s.escrow.get_contract(&id), before);
    assert_eq!(s.token.balance(&s.freelancer), bal_before);
}

#[test]
fn release_milestones_rejects_insufficient_funds() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U, 200 * U, 300 * U],
    );
    s.escrow.deposit_funds(&id, &(400 * U)); // Only fund 400 out of 600
    assert_eq!(
        s.escrow
            .try_release_milestones(&id, &vec![&s.env, 0u32, 1u32, 2u32]),
        Err(Ok(EscrowError::InsufficientFunds))
    );
    // A batch that fits in the held balance succeeds
    assert_eq!(
        s.escrow.release_milestones(&id, &vec![&s.env, 0u32, 2u32]),
        400 * U
    );
}

#[test]
fn release_milestones_matches_single_releases() {
    let a = setup();
    let id_a = three_milestone_escrow(&a, false);
    a.escrow.release_milestone(&id_a, &0);
    a.escrow.release_milestone(&id_a, &1);
    a.escrow.release_milestone(&id_a, &2);

    let b = setup();
    let id_b = three_milestone_escrow(&b, false);
    b.escrow
        .release_milestones(&id_b, &vec![&b.env, 0u32, 1u32, 2u32]);

    let ca = a.escrow.get_contract(&id_a);
    let cb = b.escrow.get_contract(&id_b);
    assert_eq!(ca.milestones, cb.milestones);
    assert_eq!(ca.status, cb.status);
    assert_eq!(ca.funded_amount, cb.funded_amount);
    assert_eq!(ca.released_amount, cb.released_amount);
    assert_eq!(
        a.token.balance(&a.freelancer),
        b.token.balance(&b.freelancer)
    );
    assert_eq!(
        a.escrow.get_liability(&a.token.address),
        b.escrow.get_liability(&b.token.address)
    );
}

#[test]
fn release_milestones_sequential_enforced() {
    let s = setup();
    let id = three_milestone_escrow(&s, true);
    assert_eq!(
        s.escrow.try_release_milestones(&id, &vec![&s.env, 0u32, 2u32]),
        Err(Ok(EscrowError::OutOfOrder))
    );
    // Order within the batch does not matter as long as the prefix is complete
    s.escrow
        .release_milestones(&id, &vec![&s.env, 1u32, 0u32]);
    s.escrow.release_milestones(&id, &vec![&s.env, 2u32]);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);
}

#[test]
fn release_milestones_non_sequential_any_order() {
    let s = setup();
    let id = three_milestone_escrow(&s, false);
    s.escrow
        .release_milestones(&id, &vec![&s.env, 2u32, 0u32, 1u32]);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);
}

#[test]
fn test_funding_deadline_boundaries_expire() {
    let s = setup();
    set_now(&s.env, 1_000_000);

    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    let fund_by = s.escrow.get_fund_by(&id);

    // At now == fund_by, expire_unfunded must fail with NotExpired
    set_now(&s.env, fund_by);
    assert_eq!(
        s.escrow.try_expire_unfunded(&id),
        Err(Ok(EscrowError::NotExpired))
    );
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Created);

    // At now == fund_by + 1, expire_unfunded must succeed
    set_now(&s.env, fund_by + 1);
    assert_eq!(s.escrow.try_expire_unfunded(&id), Ok(Ok(())));
    assert_eq!(
        s.escrow.get_contract(&id).status,
        ContractStatus::Cancelled
    );
}

#[test]
fn test_released_milestone_protection() {
    let s = setup();
    set_now(&s.env, 1_000_000);

    // Create contract with 2 milestones (50, 50)
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50 * U, 50 * U],
    );
    // Partial deposit 50
    s.escrow.deposit_funds(&id, &(50 * U));
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Created);

    // Release first milestone
    s.escrow.release_milestone(&id, &0);
    assert_eq!(s.token.balance(&s.freelancer), 50 * U);

    let c = s.escrow.get_contract(&id);
    assert_eq!(c.status, ContractStatus::Created);
    assert_eq!(c.released_amount, 50 * U);

    let fund_by = s.escrow.get_fund_by(&id);
    set_now(&s.env, fund_by + 1);

    // Escrow with released_amount > 0 cannot be expired
    assert_eq!(
        s.escrow.try_expire_unfunded(&id),
        Err(Ok(EscrowError::InvalidStatus))
    );
    // Verify no unintended refund or state change occurs
    let c_after = s.escrow.get_contract(&id);
    assert_eq!(c_after.status, ContractStatus::Created);
    assert_eq!(c_after.released_amount, 50 * U);
    assert_eq!(c_after.funded_amount, 50 * U);
    assert_eq!(s.token.balance(&s.client), 950 * U);
    assert_eq!(s.token.balance(&s.freelancer), 50 * U);
}

#[test]
fn test_expire_unfunded_event_verification() {
    let s = setup();
    set_now(&s.env, 1_000_000);

    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(40 * U));

    let fund_by = s.escrow.get_fund_by(&id);
    set_now(&s.env, fund_by + 1);

    s.escrow.expire_unfunded(&id);

    // Assert successful expiration emits ("expired", id) with refund data (40)
    let events = s.env.events().all();
    let last = events.last().unwrap();
    assert_eq!(last.0, s.escrow.address);
    assert_eq!(last.1.len(), 2);
    let topic_sym = Symbol::try_from_val(&s.env, &last.1.get(0).unwrap()).unwrap();
    let topic_id = u64::try_from_val(&s.env, &last.1.get(1).unwrap()).unwrap();
    assert_eq!(topic_sym, symbol_short!("expired"));
    assert_eq!(topic_id, id);

    let refunded = i128::try_from_val(&s.env, &last.2).unwrap();
    assert_eq!(refunded, 40 * U);

    // Second escrow which is not expired
    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    // Expiration fails with NotExpired
    assert_eq!(
        s.escrow.try_expire_unfunded(&id2),
        Err(Ok(EscrowError::NotExpired))
    );
    // Verify that unsuccessful expiration does not produce an expired event for id2
    let has_expired_event_id2 = s.env.events().all().iter().any(|e| {
        if e.0 == s.escrow.address && e.1.len() >= 2 {
            let sym = Symbol::try_from_val(&s.env, &e.1.get(0).unwrap());
            let topic_id = u64::try_from_val(&s.env, &e.1.get(1).unwrap());
            sym == Ok(symbol_short!("expired")) && topic_id == Ok(id2)
        } else {
            false
        }
    });
    assert!(!has_expired_event_id2);
}

#[test]
fn partial_release_completes_milestone_and_escrow() {
    let s = setup();
    let id = s
        .escrow
        .create_contract(&s.client, &s.freelancer, &s.token.address, &vec![&s.env, 100 * U]);
    s.escrow.deposit_funds(&id, &(100 * U));
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Funded);

    s.escrow.release_partial(&id, &0, &(30 * U));
    assert_invariants(&s, &s.escrow.get_contract(&id));
    assert_eq!(s.token.balance(&s.freelancer), 30 * U);
    assert_eq!(s.escrow.get_milestone_paid(&id, &0), 30 * U);

    s.escrow.release_partial(&id, &0, &(70 * U));
    assert_invariants(&s, &s.escrow.get_contract(&id));
    assert_eq!(s.token.balance(&s.freelancer), 100 * U);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);
    assert_eq!(s.escrow.get_milestone_paid(&id, &0), 100 * U);
}

#[test]
fn partial_then_release_milestone_pays_remaining() {
    let s = setup();
    let id = s
        .escrow
        .create_contract(&s.client, &s.freelancer, &s.token.address, &vec![&s.env, 100 * U]);
    s.escrow.deposit_funds(&id, &(100 * U));

    s.escrow.release_partial(&id, &0, &(30 * U));
    assert_invariants(&s, &s.escrow.get_contract(&id));

    s.escrow.release_milestone(&id, &0);
    assert_invariants(&s, &s.escrow.get_contract(&id));
    assert_eq!(s.token.balance(&s.freelancer), 100 * U);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);
    assert_eq!(s.escrow.get_milestone_paid(&id, &0), 100 * U);
}

#[test]
fn over_release_is_rejected() {
    let s = setup();
    let id = s
        .escrow
        .create_contract(&s.client, &s.freelancer, &s.token.address, &vec![&s.env, 100 * U]);
    s.escrow.deposit_funds(&id, &(100 * U));

    s.escrow.release_partial(&id, &0, &(30 * U));
    assert_eq!(
        s.escrow.try_release_partial(&id, &0, &(80 * U)),
        Err(Ok(EscrowError::InvalidAmount))
    );
    assert_invariants(&s, &s.escrow.get_contract(&id));
    assert_eq!(s.token.balance(&s.freelancer), 30 * U);
    assert_eq!(s.escrow.get_milestone_paid(&id, &0), 30 * U);

    // fully paying via release_milestone then releasing again is rejected
    s.escrow.release_milestone(&id, &0);
    assert!(s.escrow.try_release_milestone(&id, &0).is_err());
    assert!(s.escrow.try_release_partial(&id, &0, &U).is_err());
    assert_invariants(&s, &s.escrow.get_contract(&id));
}

#[test]
fn zero_or_negative_partial_release_is_rejected() {
    let s = setup();
    let id = s
        .escrow
        .create_contract(&s.client, &s.freelancer, &s.token.address, &vec![&s.env, 100 * U]);
    s.escrow.deposit_funds(&id, &(100 * U));

    assert_eq!(
        s.escrow.try_release_partial(&id, &0, &0),
        Err(Ok(EscrowError::InvalidAmount))
    );
    assert_eq!(
        s.escrow.try_release_partial(&id, &0, &-5i128),
        Err(Ok(EscrowError::InvalidAmount))
    );
    assert_invariants(&s, &s.escrow.get_contract(&id));
    assert_eq!(s.token.balance(&s.freelancer), 0);
}

#[test]
fn cancel_after_partial_refunds_funded_minus_released() {
    let s = setup();
    let id = s
        .escrow
        .create_contract(&s.client, &s.freelancer, &s.token.address, &vec![&s.env, 100 * U]);
    s.escrow.deposit_funds(&id, &(100 * U));
    let client_before = s.token.balance(&s.client);

    s.escrow.release_partial(&id, &0, &(30 * U));
    assert_invariants(&s, &s.escrow.get_contract(&id));

    s.escrow.cancel_contract(&id);
    assert_eq!(s.token.balance(&s.client), client_before + 70 * U);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Cancelled);
    assert_invariants(&s, &s.escrow.get_contract(&id));
}

#[test]
fn resolve_dispute_after_partial_splits_held_remainder() {
    let s = setup();
    let id = s
        .escrow
        .create_contract(&s.client, &s.freelancer, &s.token.address, &vec![&s.env, 100 * U]);
    s.escrow.deposit_funds(&id, &(100 * U));

    s.escrow.release_partial(&id, &0, &(30 * U));
    assert_invariants(&s, &s.escrow.get_contract(&id));

    open_dispute_bonded(&s, &id, &s.freelancer);
    resolve_refunding_bond(&s, &id, &2_500);
    assert_invariants(&s, &s.escrow.get_contract(&id));
    // held = 100 - 30 = 70 units; freelancer gets 70 * 2500 / 10000 = 17.5, client 52.5
    assert_eq!(s.token.balance(&s.freelancer), 30 * U + 70 * U / 4);
    assert_eq!(s.token.balance(&s.client), 1_000 * U - 100 * U + (70 * U - 70 * U / 4));
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Resolved);
}

#[test]
fn partial_release_respects_sequential_order_and_batch_pays_remaining() {
    let s = setup();
    let id = s.escrow.create_contract_sequential(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(200 * U));
    assert_eq!(
        s.escrow.try_release_partial(&id, &1, &(10 * U)),
        Err(Ok(EscrowError::OutOfOrder))
    );
    s.escrow.release_partial(&id, &0, &(40 * U));
    let paid = s
        .escrow
        .release_milestones(&id, &vec![&s.env, 0u32, 1u32]);
    assert_eq!(paid, 160 * U);
    assert_eq!(s.token.balance(&s.freelancer), 200 * U);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);
    assert_eq!(s.escrow.get_liability(&s.token.address), 0);
    assert_invariants(&s, &s.escrow.get_contract(&id));
}

#[test]
fn set_terms_then_fund_succeeds() {
    let s = setup();
    let id = two_milestone_escrow(&s);
    let specs = vec![&s.env, hash32(&s.env, 2), hash32(&s.env, 3)];
    s.escrow.set_terms(&id, &hash32(&s.env, 1), &specs);
    assert_eq!(s.escrow.get_terms(&id), Some((hash32(&s.env, 1), specs.clone())));
    s.escrow.deposit_funds(&id, &(300 * U));
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Funded);
    // Terms without milestone specs are accepted too
    let id2 = two_milestone_escrow(&s);
    s.escrow.set_terms(&id2, &hash32(&s.env, 9), &Vec::new(&s.env));
    assert_eq!(
        s.escrow.get_terms(&id2),
        Some((hash32(&s.env, 9), Vec::new(&s.env)))
    );
    assert_eq!(s.escrow.get_terms(&99), None);
}

#[test]
fn set_terms_after_deposit_rejected() {
    let s = setup();
    let id = two_milestone_escrow(&s);
    s.escrow.deposit_funds(&id, &(10 * U));
    assert_eq!(
        s.escrow.try_set_terms(&id, &hash32(&s.env, 1), &Vec::new(&s.env)),
        Err(Ok(EscrowError::InvalidStatus))
    );
}

#[test]
fn set_terms_length_mismatch_rejected() {
    let s = setup();
    let id = two_milestone_escrow(&s);
    assert_eq!(
        s.escrow
            .try_set_terms(&id, &hash32(&s.env, 1), &vec![&s.env, hash32(&s.env, 2)]),
        Err(Ok(EscrowError::InvalidMetadata))
    );
}

#[test]
fn deliver_recorded_and_overwritable_until_release() {
    let s = setup();
    let id = two_milestone_escrow(&s);
    s.escrow.deposit_funds(&id, &(300 * U));
    assert_eq!(s.escrow.get_delivery(&id, &0), None);
    s.escrow.deliver(&id, &0, &hash32(&s.env, 5));
    assert_eq!(s.escrow.get_delivery(&id, &0), Some(hash32(&s.env, 5)));
    s.escrow.deliver(&id, &0, &hash32(&s.env, 6));
    assert_eq!(s.escrow.get_delivery(&id, &0), Some(hash32(&s.env, 6)));
    s.escrow.release_milestone(&id, &0);
    assert_eq!(
        s.escrow.try_deliver(&id, &0, &hash32(&s.env, 7)),
        Err(Ok(EscrowError::MilestoneAlreadyReleased))
    );
    assert_eq!(
        s.escrow.try_deliver(&id, &5, &hash32(&s.env, 7)),
        Err(Ok(EscrowError::MilestoneNotFound))
    );
}

#[test]
fn client_cannot_deliver_and_freelancer_cannot_set_terms() {
    let s = setup();
    let id = two_milestone_escrow(&s);
    let h = hash32(&s.env, 1);
    s.env.mock_auths(&[MockAuth {
        address: &s.client,
        invoke: &MockAuthInvoke {
            contract: &s.escrow.address,
            fn_name: "deliver",
            args: (id, 0u32, h.clone()).into_val(&s.env),
            sub_invokes: &[],
        },
    }]);
    assert!(s.escrow.try_deliver(&id, &0, &h).is_err());
    s.env.mock_auths(&[MockAuth {
        address: &s.freelancer,
        invoke: &MockAuthInvoke {
            contract: &s.escrow.address,
            fn_name: "set_terms",
            args: (id, h.clone(), Vec::<BytesN<32>>::new(&s.env)).into_val(&s.env),
            sub_invokes: &[],
        },
    }]);
    assert!(s
        .escrow
        .try_set_terms(&id, &h, &Vec::new(&s.env))
        .is_err());
    s.env.mock_all_auths();
    assert_eq!(s.escrow.get_delivery(&id, &0), None);
    assert_eq!(s.escrow.get_terms(&id), None);
}

#[test]
fn metadata_writes_bump_ttl() {
    let s = setup();
    let id = two_milestone_escrow(&s);
    s.escrow.set_terms(
        &id,
        &hash32(&s.env, 1),
        &vec![&s.env, hash32(&s.env, 2), hash32(&s.env, 3)],
    );
    assert!(persistent_ttl(&s, &DataKey::Terms(id)) >= PERSISTENT_TTL_THRESHOLD);
    assert!(persistent_ttl(&s, &DataKey::MilestoneSpecs(id)) >= PERSISTENT_TTL_THRESHOLD);
    s.escrow.deliver(&id, &1, &hash32(&s.env, 4));
    assert!(persistent_ttl(&s, &DataKey::Delivery(id, 1)) >= PERSISTENT_TTL_THRESHOLD);
}

#[test]
fn cancel_after_submission_cannot_claw_back_submitted_work() {
    // Attack: freelancer delivers milestone 0, client cancels in the same ledger.
    // Before this change the client was refunded everything.
    let s = setup();
    let id = funded_two_milestones(&s);
    s.escrow.submit_milestone(&id, &0);
    assert_eq!(s.escrow.get_milestone_status(&id, &0), MilestoneStatus::Submitted);
    s.escrow.cancel_contract(&id);
    // Only the pending milestone (200) is refunded; 100 stays for the freelancer.
    assert_eq!(s.token.balance(&s.client), 1_000 * U - 100 * U);
    assert_eq!(s.token.balance(&s.escrow.address), 100 * U);
    let c = s.escrow.get_contract(&id);
    assert_eq!(c.status, ContractStatus::Cancelled);
    assert_eq!(c.refunded_amount, 200 * U);
    // Not compactable while submitted funds are outstanding.
    assert_eq!(
        s.escrow.try_compact_escrow(&id),
        Err(Ok(EscrowError::NotCompactable))
    );
    // The freelancer claims once the review window has passed.
    assert_eq!(
        s.escrow.try_claim_milestone(&id, &0),
        Err(Ok(EscrowError::ReviewWindowActive))
    );
    s.env
        .ledger()
        .with_mut(|li| li.timestamp += DEFAULT_REVIEW_WINDOW_SECS);
    s.escrow.claim_milestone(&id, &0);
    assert_eq!(s.token.balance(&s.freelancer), 100 * U);
    assert_eq!(s.token.balance(&s.escrow.address), 0);
    assert_eq!(s.escrow.get_liability(&s.token.address), 0);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Cancelled);
    check_invariants(&s.escrow.get_contract(&id));
}

#[test]
fn claim_after_review_window_completes_escrow() {
    let s = setup();
    let id = s.escrow.create_contract_with_review(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
        &MIN_REVIEW_WINDOW_SECS,
    );
    assert_eq!(s.escrow.get_review_window(&id), MIN_REVIEW_WINDOW_SECS);
    s.escrow.deposit_funds(&id, &(100 * U));
    s.env.ledger().with_mut(|li| li.timestamp = 5_000);
    s.escrow.submit_milestone(&id, &0);
    assert_eq!(s.escrow.get_submitted_at(&id, &0), Some(5_000));
    s.env
        .ledger()
        .with_mut(|li| li.timestamp = 5_000 + MIN_REVIEW_WINDOW_SECS - 1);
    assert_eq!(
        s.escrow.try_claim_milestone(&id, &0),
        Err(Ok(EscrowError::ReviewWindowActive))
    );
    s.env
        .ledger()
        .with_mut(|li| li.timestamp = 5_000 + MIN_REVIEW_WINDOW_SECS);
    s.escrow.claim_milestone(&id, &0);
    assert_eq!(s.token.balance(&s.freelancer), 100 * U);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);
    assert_eq!(s.escrow.get_milestone_status(&id, &0), MilestoneStatus::Released);
    assert_eq!(s.escrow.get_submitted_at(&id, &0), None);
}

#[test]
fn review_window_bounds_validated() {
    let s = setup();
    for bad in [MIN_REVIEW_WINDOW_SECS - 1, MAX_REVIEW_WINDOW_SECS + 1] {
        assert_eq!(
            s.escrow.try_create_contract_with_review(
                &s.client,
                &s.freelancer,
                &s.token.address,
                &vec![&s.env, 100 * U],
                &bad,
            ),
            Err(Ok(EscrowError::InvalidReviewWindow))
        );
    }
    let id = s.escrow.create_contract_with_review(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
        &MAX_REVIEW_WINDOW_SECS,
    );
    assert_eq!(s.escrow.get_review_window(&id), MAX_REVIEW_WINDOW_SECS);
    let id2 = funded_two_milestones(&s);
    assert_eq!(s.escrow.get_review_window(&id2), DEFAULT_REVIEW_WINDOW_SECS);
}

#[test]
fn submit_milestone_failure_paths() {
    let s = setup();
    // Not funded yet
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U, 200 * U],
    );
    assert_eq!(
        s.escrow.try_submit_milestone(&id, &0),
        Err(Ok(EscrowError::InvalidStatus))
    );
    s.escrow.deposit_funds(&id, &(300 * U));
    // Out of range
    assert_eq!(
        s.escrow.try_submit_milestone(&id, &5),
        Err(Ok(EscrowError::MilestoneNotFound))
    );
    // Wrong caller: only the client authorises
    s.env.mock_auths(&[MockAuth {
        address: &s.client,
        invoke: &MockAuthInvoke {
            contract: &s.escrow.address,
            fn_name: "submit_milestone",
            args: (id, 0u32).into_val(&s.env),
            sub_invokes: &[],
        },
    }]);
    assert!(s.escrow.try_submit_milestone(&id, &0).is_err());
    s.env.mock_all_auths();
    // Paused
    s.escrow.set_paused(&s.admin, &true);
    assert_eq!(
        s.escrow.try_submit_milestone(&id, &0),
        Err(Ok(EscrowError::Paused))
    );
    s.escrow.set_paused(&s.admin, &false);
    // Double submit and submit after release
    s.escrow.submit_milestone(&id, &0);
    assert_eq!(
        s.escrow.try_submit_milestone(&id, &0),
        Err(Ok(EscrowError::MilestoneAlreadyReleased))
    );
    s.escrow.release_milestone(&id, &1);
    assert_eq!(
        s.escrow.try_submit_milestone(&id, &1),
        Err(Ok(EscrowError::MilestoneAlreadyReleased))
    );
}

#[test]
fn claim_milestone_failure_paths() {
    let s = setup();
    let id = funded_two_milestones(&s);
    // Not submitted
    assert_eq!(
        s.escrow.try_claim_milestone(&id, &0),
        Err(Ok(EscrowError::MilestoneNotSubmitted))
    );
    assert_eq!(
        s.escrow.try_claim_milestone(&id, &9),
        Err(Ok(EscrowError::MilestoneNotFound))
    );
    s.escrow.submit_milestone(&id, &0);
    s.env
        .ledger()
        .with_mut(|li| li.timestamp += DEFAULT_REVIEW_WINDOW_SECS);
    // Wrong caller
    s.env.mock_auths(&[MockAuth {
        address: &s.client,
        invoke: &MockAuthInvoke {
            contract: &s.escrow.address,
            fn_name: "claim_milestone",
            args: (id, 0u32).into_val(&s.env),
            sub_invokes: &[],
        },
    }]);
    assert!(s.escrow.try_claim_milestone(&id, &0).is_err());
    s.env.mock_all_auths();
    // Paused
    s.escrow.set_paused(&s.admin, &true);
    assert_eq!(
        s.escrow.try_claim_milestone(&id, &0),
        Err(Ok(EscrowError::Paused))
    );
    s.escrow.set_paused(&s.admin, &false);
    // Disputed escrows freeze claims
    open_dispute_bonded(&s, &id, &s.client);
    assert_eq!(
        s.escrow.try_claim_milestone(&id, &0),
        Err(Ok(EscrowError::InvalidStatus))
    );
}

#[test]
fn client_release_accepts_submitted_and_claim_after_release_fails() {
    let s = setup();
    let id = funded_two_milestones(&s);
    s.escrow.submit_milestone(&id, &1);
    s.escrow.release_milestone(&id, &1);
    assert_eq!(s.escrow.get_milestone_status(&id, &1), MilestoneStatus::Released);
    s.env
        .ledger()
        .with_mut(|li| li.timestamp += DEFAULT_REVIEW_WINDOW_SECS);
    assert_eq!(
        s.escrow.try_claim_milestone(&id, &1),
        Err(Ok(EscrowError::MilestoneAlreadyReleased))
    );
    // Pending milestones can still be paid early
    assert_eq!(s.escrow.get_milestone_status(&id, &0), MilestoneStatus::Pending);
    s.escrow.release_milestone(&id, &0);
    assert_eq!(s.token.balance(&s.freelancer), 300 * U);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);
}

#[test]
fn cancel_without_submissions_is_unchanged() {
    let s = setup();
    let id = funded_two_milestones(&s);
    s.escrow.cancel_contract(&id);
    assert_eq!(s.token.balance(&s.client), 1_000 * U);
    let c = s.escrow.get_contract(&id);
    assert_eq!(c.funded_amount, c.released_amount);
    check_invariants(&c);
}

#[test]
fn emergency_withdraw_boundary_and_funded_refund() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    set_ts(&s.env, 10_000);
    s.escrow.set_paused(&s.admin, &true);
    assert_eq!(s.escrow.get_paused_at(), Some(10_000));

    set_ts(&s.env, 10_000 + EMERGENCY_EXIT_DELAY_SECS - 1);
    assert_eq!(
        s.escrow.try_emergency_withdraw(&id, &s.client),
        Err(Ok(EscrowError::EmergencyExitNotAvailable))
    );

    set_ts(&s.env, 10_000 + EMERGENCY_EXIT_DELAY_SECS);
    s.escrow.emergency_withdraw(&id, &s.client);
    assert_eq!(s.token.balance(&s.client), 1_000 * U);
    assert_eq!(s.token.balance(&s.freelancer), 0);
    let c = s.escrow.get_contract(&id);
    assert_eq!(c.status, ContractStatus::Cancelled);
    assert_eq!(c.funded_amount, c.released_amount);
    assert_eq!(s.token.balance(&s.escrow.address), 0);
    assert_eq!(s.escrow.get_liability(&s.token.address), 0);
    assert_eq!(
        s.escrow.try_emergency_withdraw(&id, &s.client),
        Err(Ok(EscrowError::InvalidStatus))
    );
}

#[test]
fn emergency_withdraw_disputed_split_rounding() {
    let s = setup();
    // Odd amount: 1_000_001 splits 500_000 to the freelancer, 500_001 to the client.
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 1_000_001i128],
    );
    s.escrow.deposit_funds(&id, &1_000_001);
    open_dispute_bonded(&s, &id, &s.client);
    let bond = dispute_bond(1_000_001);
    let client_before = s.token.balance(&s.client);
    set_ts(&s.env, 1);
    s.escrow.set_paused(&s.admin, &true);
    set_ts(&s.env, 1 + EMERGENCY_EXIT_DELAY_SECS);

    s.escrow.emergency_withdraw(&id, &s.freelancer);
    assert_eq!(s.token.balance(&s.freelancer), 500_000);
    // Client gets the larger half plus its dispute bond back.
    assert_eq!(s.token.balance(&s.client), client_before + 500_001 + bond);
    assert_eq!(s.token.balance(&s.escrow.address), 0);
    assert_eq!(s.escrow.get_dispute_bond(&id), None);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Cancelled);
}

#[test]
fn emergency_withdraw_pays_submitted_work_and_keeps_balances_consistent() {
    let s = setup();
    let a = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U, 200 * U],
    );
    s.escrow.deposit_funds(&a, &(300 * U));
    s.escrow.submit_milestone(&a, &0);
    let b = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50 * U],
    );
    s.escrow.deposit_funds(&b, &(50 * U));
    s.escrow.set_paused(&s.admin, &true);
    set_ts(&s.env, EMERGENCY_EXIT_DELAY_SECS);

    s.escrow.emergency_withdraw(&a, &s.freelancer);
    assert_eq!(s.token.balance(&s.freelancer), 100 * U);
    // Remaining escrow b is still fully backed.
    let cb = s.escrow.get_contract(&b);
    assert_eq!(
        s.token.balance(&s.escrow.address),
        cb.funded_amount - cb.released_amount
    );
    assert_eq!(s.escrow.get_liability(&s.token.address), 50 * U);
    s.escrow.emergency_withdraw(&b, &s.client);
    assert_eq!(s.token.balance(&s.escrow.address), 0);
    assert_eq!(s.token.balance(&s.client), 1_000 * U - 100 * U);
}

#[test]
fn resign_in_created_refunds_partial_deposit() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U, 200 * U],
    );
    // Partial deposit of 150 out of 300 required.
    s.escrow.deposit_funds(&id, &(150 * U));
    assert_eq!(s.token.balance(&s.client), 850 * U);
    assert_eq!(s.token.balance(&s.freelancer), 0);
    assert_eq!(s.token.balance(&s.escrow.address), 150 * U);

    s.escrow.resign(&id);

    assert_eq!(s.token.balance(&s.client), 1000 * U);
    assert_eq!(s.token.balance(&s.freelancer), 0);
    assert_eq!(s.token.balance(&s.escrow.address), 0);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Cancelled);
    assert!(s.escrow.was_resigned(&id));
}

#[test]
fn resign_in_funded_after_one_release_refunds_remaining() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U, 200 * U],
    );
    s.escrow.deposit_funds(&id, &(300 * U));
    s.escrow.release_milestone(&id, &0);
    assert_eq!(s.token.balance(&s.freelancer), 100 * U);
    assert_eq!(s.token.balance(&s.client), 700 * U);
    assert_eq!(s.token.balance(&s.escrow.address), 200 * U);

    s.escrow.resign(&id);

    // Released milestone stays paid; remaining 200 refunded to client.
    assert_eq!(s.token.balance(&s.client), 900 * U);
    assert_eq!(s.token.balance(&s.freelancer), 100 * U);
    assert_eq!(s.token.balance(&s.escrow.address), 0);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Cancelled);
    assert!(s.escrow.was_resigned(&id));
}

#[test]
fn resign_while_disputed_rejected() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    open_dispute_bonded(&s, &id, &s.freelancer);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Disputed);
    assert_eq!(
        s.escrow.try_resign(&id),
        Err(Ok(EscrowError::InvalidStatus))
    );
    // Held funds plus the opener's dispute bond stay in escrow.
    assert_eq!(
        s.token.balance(&s.escrow.address),
        100 * U + dispute_bond(100 * U)
    );
    assert!(!s.escrow.was_resigned(&id));
}

#[test]
fn client_calling_resign_rejected() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    // Only the client authorises: the freelancer's auth is missing.
    s.env.mock_auths(&[MockAuth {
        address: &s.client,
        invoke: &MockAuthInvoke {
            contract: &s.escrow.address,
            fn_name: "resign",
            args: (id,).into_val(&s.env),
            sub_invokes: &[],
        },
    }]);
    assert!(s.escrow.try_resign(&id).is_err());
    s.env.mock_all_auths();
    assert_eq!(s.token.balance(&s.escrow.address), 100 * U);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Funded);
    assert!(!s.escrow.was_resigned(&id));
}

#[test]
fn resign_emits_event_and_is_not_a_client_cancellation() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(40 * U));
    s.escrow.resign(&id);
    let ev = s
        .env
        .events()
        .all()
        .iter()
        .filter(|e| e.0 == s.escrow.address)
        .last()
        .unwrap();
    assert_eq!(
        Symbol::try_from_val(&s.env, &ev.1.get(0).unwrap()).unwrap(),
        symbol_short!("resign")
    );
    assert_eq!(u64::try_from_val(&s.env, &ev.1.get(1).unwrap()).unwrap(), id);
    assert_eq!(i128::try_from_val(&s.env, &ev.2).unwrap(), 40 * U);
    assert_eq!(s.escrow.get_outcomes(&s.client).cancellations, 0);
    assert_eq!(s.escrow.get_liability(&s.token.address), 0);
    // Paused contracts block resignation
    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.set_paused(&s.admin, &true);
    assert_eq!(s.escrow.try_resign(&id2), Err(Ok(EscrowError::Paused)));
}

// ---------------------------------------------------------------------------
// Per-client open-escrow cap (#1785)
// ---------------------------------------------------------------------------

/// Creates a minimal unfunded escrow for the setup client.
fn create_open_escrow(s: &Setup) -> u64 {
    s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    )
}

#[test]
fn open_escrow_cap_defaults_to_unlimited() {
    let s = setup();
    assert_eq!(s.escrow.get_open_escrows(&s.client), 0);
    for expected in 1..=5u32 {
        let id = create_open_escrow(&s);
        assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Created);
        assert_eq!(s.escrow.get_open_escrows(&s.client), expected);
    }
}

#[test]
fn open_escrow_cap_blocks_creation_at_limit_and_frees_on_completion() {
    let s = setup();
    s.escrow.set_max_open_escrows(&2);

    let first = create_open_escrow(&s);
    s.escrow.deposit_funds(&first, &(100 * U));
    let second = create_open_escrow(&s);
    s.escrow.deposit_funds(&second, &(100 * U));
    assert_eq!(s.escrow.get_open_escrows(&s.client), 2);

    // Every creation entry point shares the same `build_escrow` guard.
    assert_eq!(
        s.escrow.try_create_contract(
            &s.client,
            &s.freelancer,
            &s.token.address,
            &vec![&s.env, 100 * U],
        ),
        Err(Ok(EscrowError::InvalidStatus))
    );
    assert_eq!(
        s.escrow.try_create_and_fund(
            &s.client,
            &s.freelancer,
            &s.token.address,
            &vec![&s.env, 100 * U],
        ),
        Err(Ok(EscrowError::InvalidStatus))
    );
    assert_eq!(
        s.escrow.try_create_stream(
            &s.client,
            &s.freelancer,
            &s.token.address,
            &(100 * U),
            &0u64,
            &7_200u64,
        ),
        Err(Ok(EscrowError::InvalidStatus))
    );
    // Rejected creations neither increment the counter nor move tokens.
    assert_eq!(s.escrow.get_open_escrows(&s.client), 2);
    assert_eq!(s.token.balance(&s.client), 800 * U);

    s.escrow.release_milestone(&first, &0);
    assert_eq!(s.escrow.get_open_escrows(&s.client), 1);
    let third = create_open_escrow(&s);
    assert_eq!(s.escrow.get_open_escrows(&s.client), 2);
    assert_eq!(
        s.escrow.get_contract(&third).status,
        ContractStatus::Created
    );
}

#[test]
fn open_escrow_slot_freed_on_cancellation_and_counter_never_underflows() {
    let s = setup();
    s.escrow.set_max_open_escrows(&1);

    let first = create_open_escrow(&s);
    s.escrow.deposit_funds(&first, &(100 * U));
    assert_eq!(s.escrow.get_open_escrows(&s.client), 1);
    s.escrow.cancel_contract(&first);
    assert_eq!(s.escrow.get_open_escrows(&s.client), 0);

    // A repeated terminal call is rejected and cannot decrement again.
    assert_eq!(
        s.escrow.try_cancel_contract(&first),
        Err(Ok(EscrowError::InvalidStatus))
    );
    assert_eq!(s.escrow.get_open_escrows(&s.client), 0);

    let second = create_open_escrow(&s);
    assert_eq!(s.escrow.get_open_escrows(&s.client), 1);
    s.escrow.cancel_contract(&second);
    assert_eq!(s.escrow.get_open_escrows(&s.client), 0);
}

#[test]
fn open_escrow_slot_freed_on_expiry() {
    let s = setup();
    s.escrow.set_max_open_escrows(&1);
    let id = create_open_escrow(&s);
    assert_eq!(s.escrow.get_open_escrows(&s.client), 1);

    set_now(&s.env, FUNDING_WINDOW_SECS + 1);
    s.escrow.expire_unfunded(&id);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Cancelled);
    assert_eq!(s.escrow.get_open_escrows(&s.client), 0);
    create_open_escrow(&s);
}

#[test]
fn open_escrow_slot_freed_on_resignation() {
    let s = setup();
    s.escrow.set_max_open_escrows(&1);
    let id = create_open_escrow(&s);
    s.escrow.deposit_funds(&id, &(100 * U));
    assert_eq!(s.escrow.get_open_escrows(&s.client), 1);

    s.escrow.resign(&id);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Cancelled);
    assert_eq!(s.escrow.get_open_escrows(&s.client), 0);
    create_open_escrow(&s);
}

#[test]
fn open_escrow_slot_freed_on_stream_termination() {
    let s = setup();
    s.escrow.set_max_open_escrows(&1);
    let id = s.escrow.create_stream(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &(100 * U),
        &0u64,
        &7_200u64,
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    at_time(&s.env, 2_880);
    assert_eq!(s.escrow.get_open_escrows(&s.client), 1);

    s.escrow.stop_stream(&id);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Cancelled);
    assert_eq!(s.escrow.get_open_escrows(&s.client), 0);
}

#[test]
fn open_escrow_slot_freed_on_stream_completion() {
    let s = setup();
    s.escrow.set_max_open_escrows(&1);
    let id = s.escrow.create_stream(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &(100 * U),
        &0u64,
        &7_200u64,
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    at_time(&s.env, 7_200);

    s.escrow.withdraw_vested(&id);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);
    assert_eq!(s.escrow.get_open_escrows(&s.client), 0);
}

#[test]
fn open_escrow_slot_freed_on_emergency_withdrawal() {
    let s = setup();
    s.escrow.set_max_open_escrows(&1);
    let id = create_open_escrow(&s);
    s.escrow.deposit_funds(&id, &(100 * U));

    s.escrow.set_paused(&s.admin, &true);
    set_now(&s.env, EMERGENCY_EXIT_DELAY_SECS + 1);
    s.escrow.emergency_withdraw(&id, &s.client);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Cancelled);
    assert_eq!(s.escrow.get_open_escrows(&s.client), 0);
}

#[test]
fn open_escrow_slot_freed_on_dispute_resolution() {
    let s = setup();
    s.escrow.set_max_open_escrows(&1);
    let id = create_open_escrow(&s);
    s.escrow.deposit_funds(&id, &(100 * U));

    open_dispute_bonded(&s, &id, &s.client);
    // A disputed escrow is still open and still occupies its slot.
    assert_eq!(s.escrow.get_open_escrows(&s.client), 1);
    s.escrow.resolve_dispute(&s.admin, &id, &5_000, &true);
    assert_eq!(s.escrow.get_open_escrows(&s.client), 0);
}

#[test]
fn change_order_reopen_uses_and_respects_open_escrow_cap() {
    let s = setup();
    s.escrow.set_max_open_escrows(&1);
    let id = create_open_escrow(&s);
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.release_milestone(&id, &0);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);
    assert_eq!(s.escrow.get_open_escrows(&s.client), 0);

    let blocker = create_open_escrow(&s);
    assert_eq!(s.escrow.get_open_escrows(&s.client), 1);

    s.escrow.propose_change_order(&id, &vec![&s.env, 50 * U]);
    // Reopening would exceed the cap: rejected and the escrow stays completed.
    assert_eq!(
        s.escrow.try_accept_change_order(&id),
        Err(Ok(EscrowError::InvalidStatus))
    );
    let c = s.escrow.get_contract(&id);
    assert_eq!(c.status, ContractStatus::Completed);
    assert_eq!(c.total_amount, 100 * U);
    assert_eq!(c.milestones.len(), 1);
    assert!(s.escrow.get_change_order(&id).is_some());
    assert_eq!(s.escrow.get_open_escrows(&s.client), 1);

    s.escrow.cancel_contract(&blocker);
    assert_eq!(s.escrow.get_open_escrows(&s.client), 0);

    let (added, total) = s.escrow.accept_change_order(&id);
    assert_eq!((added, total), (1, 50 * U));
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Created);
    assert_eq!(s.escrow.get_open_escrows(&s.client), 1);
}

#[test]
fn open_escrow_counts_are_per_client() {
    let s = setup();
    s.escrow.set_max_open_escrows(&1);
    let other = Address::generate(&s.env);
    StellarAssetClient::new(&s.env, &s.token.address).mint(&other, &(100 * U));

    let first = create_open_escrow(&s);
    let second = s.escrow.create_contract(
        &other,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    assert_eq!(s.escrow.get_open_escrows(&s.client), 1);
    assert_eq!(s.escrow.get_open_escrows(&other), 1);

    // Each client is independently at its own cap.
    assert!(s
        .escrow
        .try_create_contract(
            &s.client,
            &s.freelancer,
            &s.token.address,
            &vec![&s.env, 100 * U],
        )
        .is_err());
    assert!(s
        .escrow
        .try_create_contract(
            &other,
            &s.freelancer,
            &s.token.address,
            &vec![&s.env, 100 * U],
        )
        .is_err());

    s.escrow.cancel_contract(&first);
    assert_eq!(s.escrow.get_open_escrows(&s.client), 0);
    assert_eq!(s.escrow.get_open_escrows(&other), 1);
    assert_eq!(
        s.escrow.get_contract(&second).client,
        other,
        "the other client's escrow is tracked independently"
    );
}
