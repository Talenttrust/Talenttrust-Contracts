//! Stream escrows, vesting, amendments and change orders.

use super::*;

#[test]
fn change_order_on_funded_requires_top_up() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U, 200 * U],
    );
    s.escrow.deposit_funds(&id, &(300 * U));
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Funded);

    s.escrow.propose_change_order(&id, &vec![&s.env, 50 * U]);
    let (count, total) = s.escrow.accept_change_order(&id);
    assert_eq!(count, 1);
    assert_eq!(total, 50 * U);

    let c = s.escrow.get_contract(&id);
    assert_eq!(c.status, ContractStatus::Created);
    assert_eq!(c.total_amount, 350 * U);
    assert_eq!(c.funded_amount, 300 * U);
    assert_eq!(c.milestones.len(), 3);

    s.escrow.release_milestone(&id, &0);
    s.escrow.release_milestone(&id, &1);
    assert_eq!(s.token.balance(&s.freelancer), 300 * U);

    // The new milestone is unfunded: existing balance is exhausted.
    assert!(s.escrow.try_release_milestone(&id, &2).is_err());

    // Client tops up the added amount.
    s.escrow.deposit_funds(&id, &(50 * U));
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Funded);
    s.escrow.release_milestone(&id, &2);
    assert_eq!(s.token.balance(&s.freelancer), 350 * U);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);
}

#[test]
fn change_order_on_completed_reopens_and_completes_again() {
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

    s.escrow.propose_change_order(&id, &vec![&s.env, 40 * U]);
    let (count, total) = s.escrow.accept_change_order(&id);
    assert_eq!(count, 1);
    assert_eq!(total, 40 * U);

    let c = s.escrow.get_contract(&id);
    assert_eq!(c.status, ContractStatus::Created);
    assert_eq!(c.total_amount, 140 * U);
    assert_eq!(c.funded_amount, 100 * U);

    s.escrow.deposit_funds(&id, &(40 * U));
    s.escrow.release_milestone(&id, &1);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);
    assert_eq!(s.token.balance(&s.freelancer), 140 * U);
}

#[test]
fn change_order_exceeding_max_milestones_rejected() {
    let s = setup();
    let mut amounts = Vec::new(&s.env);
    for _ in 0..MAX_MILESTONES {
        amounts.push_back(U);
    }
    let id = s
        .escrow
        .create_contract(&s.client, &s.freelancer, &s.token.address, &amounts);
    s.escrow.deposit_funds(&id, &(20 * U));

    let mut extra = Vec::new(&s.env);
    extra.push_back(U);
    assert_eq!(
        s.escrow.try_propose_change_order(&id, &extra),
        Err(Ok(EscrowError::InvalidMilestones))
    );
}

#[test]
fn freelancer_can_reject_change_order() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));

    s.escrow.propose_change_order(&id, &vec![&s.env, 50 * U]);
    s.escrow.reject_change_order(&id);

    assert_eq!(
        s.escrow.try_accept_change_order(&id),
        Err(Ok(EscrowError::ChangeOrderNotFound))
    );

    s.escrow.propose_change_order(&id, &vec![&s.env, 25 * U]);
    let (count, total) = s.escrow.accept_change_order(&id);
    assert_eq!(count, 1);
    assert_eq!(total, 25 * U);
}

#[test]
fn client_cannot_accept_own_change_order() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.propose_change_order(&id, &vec![&s.env, 50 * U]);

    // Switch from mocked auth to real auth: no address is authorized now.
    s.env.set_auths(&[]);

    // The client cannot accept their own change order (only the freelancer can).
    assert!(s.escrow.try_accept_change_order(&id).is_err());

    // With mocking back on, the freelancer accepts normally.
    s.env.mock_all_auths();
    let (count, total) = s.escrow.accept_change_order(&id);
    assert_eq!((count, total), (1, 50 * U));
}

#[test]
fn change_order_resets_funding_deadline_and_keeps_rating_blocked() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.release_milestone(&id, &0);
    s.escrow.issue_reputation(&id, &5);

    // Long after the original funding window, a change order reopens the escrow
    let later = s.escrow.get_fund_by(&id) + 30 * 86_400;
    s.env.ledger().with_mut(|li| li.timestamp = later);
    s.escrow.propose_change_order(&id, &vec![&s.env, 40 * U]);
    s.escrow.accept_change_order(&id);
    assert_eq!(s.escrow.get_fund_by(&id), later + FUNDING_WINDOW_SECS);
    check_invariants(&s.escrow.get_contract(&id));

    s.escrow.deposit_funds(&id, &(40 * U));
    s.escrow.release_milestone(&id, &1);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);
    // Reputation was already issued for this escrow and stays blocked
    assert_eq!(
        s.escrow.try_issue_reputation(&id, &5),
        Err(Ok(EscrowError::ReputationAlreadyIssued))
    );
}

#[test]
fn change_order_rejected_while_disputed_or_too_small() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    assert_eq!(
        s.escrow.try_propose_change_order(&id, &vec![&s.env, 1i128]),
        Err(Ok(EscrowError::MilestoneTooSmall))
    );
    assert_eq!(
        s.escrow.try_propose_change_order(&id, &Vec::new(&s.env)),
        Err(Ok(EscrowError::InvalidMilestones))
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    open_dispute_bonded(&s, &id, &s.client);
    assert_eq!(
        s.escrow.try_propose_change_order(&id, &vec![&s.env, 10 * U]),
        Err(Ok(EscrowError::InvalidStatus))
    );
}

#[test]
fn stream_vesting_boundaries_are_exact() {
    let s = setup();
    let id = s.escrow.create_stream(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &1_000_001,
        &10_000,
        &13_600,
    );
    let cases: [(u64, i128); 6] = [
        (9_999, 0),
        (10_000, 0),
        (11_800, 500_000),
        (13_599, 999_723),
        (13_600, 1_000_001),
        (13_601, 1_000_001),
    ];
    for (ts, want) in cases.iter() {
        at_time(&s.env, *ts);
        assert_eq!(s.escrow.vested(&id), *want, "at {}", ts);
    }
    assert_eq!(
        s.escrow.get_stream(&id),
        Stream {
            start: 10_000,
            end: 13_600,
            withdrawn: 0,
            stopped_at: None,
        }
    );
}

#[test]
fn stream_withdrawals_sum_to_amount() {
    let s = setup();
    let id = s.escrow.create_stream(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &(100 * U),
        &0,
        &10_000,
    );
    // Withdraw before funding is rejected.
    at_time(&s.env, 2_500);
    assert_eq!(
        s.escrow.try_withdraw_vested(&id),
        Err(Ok(EscrowError::InvalidStatus))
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    assert_eq!(s.escrow.withdraw_vested(&id), 25 * U);
    at_time(&s.env, 5_000);
    assert_eq!(s.escrow.withdraw_vested(&id), 25 * U);
    at_time(&s.env, 10_000);
    assert_eq!(s.escrow.withdraw_vested(&id), 50 * U);
    assert_eq!(s.token.balance(&s.freelancer), 100 * U);
    let c = s.escrow.get_contract(&id);
    assert_eq!(c.status, ContractStatus::Completed);
    check_invariants(&c);
    assert_eq!(s.token.balance(&s.escrow.address), 0);
}

#[test]
fn stop_stream_at_40_percent_pays_40_60() {
    let s = setup();
    let id = s.escrow.create_stream(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &(100 * U),
        &0,
        &10_000,
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    at_time(&s.env, 4_000);
    s.escrow.stop_stream(&id);
    assert_eq!(s.token.balance(&s.freelancer), 40 * U);
    assert_eq!(s.token.balance(&s.client), 1_000 * U - 40 * U);
    assert_eq!(s.token.balance(&s.escrow.address), 0);
    let c = s.escrow.get_contract(&id);
    assert_eq!(c.status, ContractStatus::Cancelled);
    check_invariants(&c);
    // Vesting is frozen at the stop time.
    at_time(&s.env, 9_000);
    assert_eq!(s.escrow.vested(&id), 40 * U);
    // Double stop rejected
    assert_eq!(
        s.escrow.try_stop_stream(&id),
        Err(Ok(EscrowError::InvalidStatus))
    );
}

#[test]
fn stream_auth_schedule_and_milestone_api_guards() {
    let s = setup();
    assert_eq!(
        s.escrow.try_create_stream(
            &s.client,
            &s.freelancer,
            &s.token.address,
            &(100 * U),
            &100,
            &(100 + MIN_STREAM_SECS - 1),
        ),
        Err(Ok(EscrowError::InvalidSchedule))
    );
    assert_eq!(
        s.escrow.try_create_stream(
            &s.client,
            &s.freelancer,
            &s.token.address,
            &(100 * U),
            &100,
            &100,
        ),
        Err(Ok(EscrowError::InvalidSchedule))
    );
    let id = s.escrow.create_stream(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &(100 * U),
        &0,
        &10_000,
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    at_time(&s.env, 5_000);
    // Streams do not use milestone releases or cancel.
    assert_eq!(
        s.escrow.try_release_milestone(&id, &0),
        Err(Ok(EscrowError::InvalidStatus))
    );
    assert_eq!(
        s.escrow.try_cancel_contract(&id),
        Err(Ok(EscrowError::InvalidStatus))
    );
    // The client cannot withdraw the freelancer's vested pay.
    s.env.mock_auths(&[MockAuth {
        address: &s.client,
        invoke: &MockAuthInvoke {
            contract: &s.escrow.address,
            fn_name: "withdraw_vested",
            args: (id,).into_val(&s.env),
            sub_invokes: &[],
        },
    }]);
    assert!(s.escrow.try_withdraw_vested(&id).is_err());
    s.env.mock_all_auths();
    assert_eq!(s.token.balance(&s.freelancer), 0);
}

#[test]
fn vesting_math_is_overflow_safe() {
    let env = Env::default();
    let amount = i128::MAX / 2;
    assert_eq!(
        vested_at(&env, amount, 0, u64::MAX, u64::MAX / 2),
        42_535_295_865_117_307_930_615_982_919_757_332_479
    );
    assert_eq!(vested_at(&env, amount, 0, u64::MAX, u64::MAX), amount);
    assert_eq!(vested_at(&env, amount, 5, 10, 0), 0);
}

#[test]
fn amendment_decrease_refunds_excess() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U, 50 * U],
    );
    s.escrow.deposit_funds(&id, &(150 * U));
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Funded);
    let nonce = s
        .escrow
        .propose_amendment(&id, &s.client, &vec![&s.env, 50 * U, 50 * U]);
    s.escrow.accept_amendment(&id, &s.freelancer, &nonce);
    let ev = s
        .env
        .events()
        .all()
        .iter()
        .filter(|e| e.0 == s.escrow.address)
        .last()
        .unwrap();
    assert_eq!(
        <(i128, i128, i128)>::try_from_val(&s.env, &ev.2).unwrap(),
        (150 * U, 100 * U, 50 * U)
    );
    let c = s.escrow.get_contract(&id);
    assert_eq!(c.total_amount, 100 * U);
    assert_eq!(c.funded_amount, 100 * U);
    assert_eq!(c.status, ContractStatus::Funded);
    assert_eq!(s.token.balance(&s.client), 900 * U);
    assert_amendment_invariants(&s, id);
    assert_eq!(s.escrow.get_amendment(&id), None);
}

#[test]
fn amendment_increase_flips_to_created_and_requires_deposit() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50 * U],
    );
    s.escrow.deposit_funds(&id, &(50 * U));
    let nonce = s
        .escrow
        .propose_amendment(&id, &s.client, &vec![&s.env, 100 * U]);
    s.escrow.accept_amendment(&id, &s.freelancer, &nonce);
    let c = s.escrow.get_contract(&id);
    assert_eq!(c.status, ContractStatus::Created);
    assert_eq!(c.total_amount, 100 * U);
    assert_eq!(c.funded_amount, 50 * U);
    assert_amendment_invariants(&s, id);
    s.escrow.deposit_funds(&id, &(50 * U));
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Funded);
    s.escrow.release_milestone(&id, &0);
    assert_eq!(s.token.balance(&s.freelancer), 100 * U);
    assert_eq!(
        s.escrow.get_contract(&id).status,
        ContractStatus::Completed
    );
}

#[test]
fn amendment_released_milestone_amount_change_rejected() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50 * U, 50 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.release_milestone(&id, &0);
    assert_eq!(
        s.escrow
            .try_propose_amendment(&id, &s.client, &vec![&s.env, 40 * U, 50 * U]),
        Err(Ok(EscrowError::MilestoneAlreadyReleased))
    );
    // Changing only the unreleased milestone is fine.
    let nonce = s
        .escrow
        .propose_amendment(&id, &s.freelancer, &vec![&s.env, 50 * U, 30 * U]);
    s.escrow.accept_amendment(&id, &s.client, &nonce);
    assert_eq!(s.escrow.get_contract(&id).total_amount, 80 * U);
    assert_amendment_invariants(&s, id);
    assert_eq!(
        s.escrow
            .try_propose_amendment(&id, &s.client, &vec![&s.env, 50 * U]),
        Err(Ok(EscrowError::InvalidMilestones))
    );
}

#[test]
fn amendment_self_accept_rejected() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50 * U, 50 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    let nonce = s
        .escrow
        .propose_amendment(&id, &s.client, &vec![&s.env, 40 * U, 60 * U]);
    assert_eq!(
        s.escrow.try_accept_amendment(&id, &s.client, &nonce),
        Err(Ok(EscrowError::Unauthorized))
    );
}

#[test]
fn amendment_stale_nonce_rejected() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50 * U, 50 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    let first = s
        .escrow
        .propose_amendment(&id, &s.client, &vec![&s.env, 40 * U, 60 * U]);
    assert_eq!(
        s.escrow.try_accept_amendment(&id, &s.freelancer, &(first + 1)),
        Err(Ok(EscrowError::SettlementMismatch))
    );
    // A newer proposal makes the first nonce stale.
    let second = s
        .escrow
        .propose_amendment(&id, &s.client, &vec![&s.env, 30 * U, 70 * U]);
    assert_eq!(second, first + 1);
    assert_eq!(
        s.escrow.try_accept_amendment(&id, &s.freelancer, &first),
        Err(Ok(EscrowError::SettlementMismatch))
    );
    s.escrow.accept_amendment(&id, &s.freelancer, &second);
    assert_eq!(
        s.escrow.get_contract(&id).milestones.get(1).unwrap().amount,
        70 * U
    );
    assert_amendment_invariants(&s, id);
}
