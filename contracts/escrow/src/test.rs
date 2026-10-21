extern crate std;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger, MockAuth, MockAuthInvoke},
    token::StellarAssetClient,
    token::TokenClient,
    vec, IntoVal, Symbol, TryFromVal,
};

struct Setup<'a> {
    env: Env,
    admin: Address,
    escrow: EscrowClient<'a>,
    token: TokenClient<'a>,
    client: Address,
    freelancer: Address,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    StellarAssetClient::new(&env, &sac.address()).mint(&client, &1_000);
    let id = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &id);
    escrow.initialize(&admin);
    let token = TokenClient::new(&env, &sac.address());
    Setup {
        env,
        admin,
        escrow,
        token,
        client,
        freelancer,
    }
}

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
        &vec![&s.env, 100i128, 200i128],
    );
    assert_eq!(id, 1);
    s.escrow.deposit_funds(&id, &300);
    assert_eq!(s.token.balance(&s.client), 700);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Funded);
    s.escrow.release_milestone(&id, &0);
    s.escrow.release_milestone(&id, &1);
    assert_eq!(s.token.balance(&s.freelancer), 300);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);
    s.escrow.issue_reputation(&id, &5);
    let rep = s.escrow.get_reputation(&s.freelancer);
    assert_eq!(rep.completed_contracts, 1);
    assert_eq!(rep.total_rating, 5);
}

#[test]
fn rejects_overfunding_and_double_release() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    assert_eq!(
        s.escrow.try_deposit_funds(&id, &101),
        Err(Ok(EscrowError::OverFunded))
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.release_milestone(&id, &0);
    assert!(s.escrow.try_release_milestone(&id, &0).is_err());
}

#[test]
fn dispute_split_and_cancel() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 400i128],
    );
    s.escrow.deposit_funds(&id, &400);
    s.escrow.open_dispute(&id, &s.freelancer);
    s.escrow.resolve_dispute(&id, &2_500);
    assert_eq!(s.token.balance(&s.freelancer), 100);
    assert_eq!(s.token.balance(&s.client), 900);

    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50i128],
    );
    s.escrow.deposit_funds(&id2, &50);
    s.escrow.cancel_contract(&id2);
    assert_eq!(s.token.balance(&s.client), 900);
    assert_eq!(
        s.escrow.get_contract(&id2).status,
        ContractStatus::Cancelled
    );
}

#[test]
fn pause_blocks_mutations() {
    let s = setup();
    s.escrow.set_paused(&true);
    assert_eq!(
        s.escrow.try_create_contract(
            &s.client,
            &s.freelancer,
            &s.token.address,
            &vec![&s.env, 1i128]
        ),
        Err(Ok(EscrowError::Paused))
    );
}

#[test]
fn test_rate_client_completed_success_and_event() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 150i128],
    );
    s.escrow.deposit_funds(&id, &150);
    s.escrow.release_milestone(&id, &0);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);

    // Initial reputation of client is default (0 completed, 0 rating)
    assert_eq!(
        s.escrow.get_client_reputation(&s.client),
        Reputation {
            completed_contracts: 0,
            total_rating: 0,
        }
    );

    // Freelancer rates client with rating 5
    s.escrow.rate_client(&id, &5);

    // Verify event topics: ("rep_cli", id) and data: (client, rating)
    let events = s.env.events().all();
    let last = events.last().unwrap();
    assert_eq!(last.0, s.escrow.address);
    assert_eq!(last.1.len(), 2);
    let topic_sym = Symbol::try_from_val(&s.env, &last.1.get(0).unwrap()).unwrap();
    let topic_id = u64::try_from_val(&s.env, &last.1.get(1).unwrap()).unwrap();
    assert_eq!(topic_sym, symbol_short!("rep_cli"));
    assert_eq!(topic_id, id);

    let (event_client, event_rating) = <(Address, u32)>::try_from_val(&s.env, &last.2).unwrap();
    assert_eq!(event_client, s.client);
    assert_eq!(event_rating, 5u32);

    // Reputation updated under client address
    let rep = s.escrow.get_client_reputation(&s.client);
    assert_eq!(
        rep,
        Reputation {
            completed_contracts: 1,
            total_rating: 5,
        }
    );

    // Unrelated addresses have default empty reputation
    let other = Address::generate(&s.env);
    assert_eq!(
        s.escrow.get_client_reputation(&other),
        Reputation::default()
    );

    // Freelancer's reputation is not touched by client rating
    assert_eq!(
        s.escrow.get_reputation(&s.freelancer),
        Reputation::default()
    );
}

#[test]
fn test_rate_client_resolved_success() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 200i128],
    );
    s.escrow.deposit_funds(&id, &200);
    s.escrow.open_dispute(&id, &s.freelancer);
    s.escrow.resolve_dispute(&id, &5_000);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Resolved);

    // Freelancer rates client after dispute resolution
    s.escrow.rate_client(&id, &4);

    let rep = s.escrow.get_client_reputation(&s.client);
    assert_eq!(
        rep,
        Reputation {
            completed_contracts: 1,
            total_rating: 4,
        }
    );
}

#[test]
fn test_rate_client_authorization() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.release_milestone(&id, &0);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);

    // Client attempting to call rate_client is rejected
    let res = s
        .escrow
        .mock_auths(&[MockAuth {
            address: &s.client,
            invoke: &MockAuthInvoke {
                contract: &s.escrow.address,
                fn_name: "rate_client",
                args: (id, 5u32).into_val(&s.env),
                sub_invokes: &[],
            },
        }])
        .try_rate_client(&id, &5);
    assert!(res.is_err());

    // Arbitrary third party calling rate_client is rejected
    let attacker = Address::generate(&s.env);
    let res_attacker = s
        .escrow
        .mock_auths(&[MockAuth {
            address: &attacker,
            invoke: &MockAuthInvoke {
                contract: &s.escrow.address,
                fn_name: "rate_client",
                args: (id, 5u32).into_val(&s.env),
                sub_invokes: &[],
            },
        }])
        .try_rate_client(&id, &5);
    assert!(res_attacker.is_err());

    // Freelancer calling rate_client succeeds
    let res_ok = s
        .escrow
        .mock_auths(&[MockAuth {
            address: &s.freelancer,
            invoke: &MockAuthInvoke {
                contract: &s.escrow.address,
                fn_name: "rate_client",
                args: (id, 5u32).into_val(&s.env),
                sub_invokes: &[],
            },
        }])
        .try_rate_client(&id, &5);
    assert_eq!(res_ok, Ok(Ok(())));
}

#[test]
fn test_rate_client_duplicate_rejection() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.release_milestone(&id, &0);

    // First rating succeeds
    s.escrow.rate_client(&id, &5);

    // Second rating for the same escrow receives ReputationAlreadyIssued
    assert_eq!(
        s.escrow.try_rate_client(&id, &4),
        Err(Ok(EscrowError::ReputationAlreadyIssued))
    );
}

#[test]
fn test_rate_client_validation() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.release_milestone(&id, &0);

    // Rating 0 is rejected
    assert_eq!(
        s.escrow.try_rate_client(&id, &0),
        Err(Ok(EscrowError::InvalidRating))
    );

    // Rating 6 is rejected
    assert_eq!(
        s.escrow.try_rate_client(&id, &6),
        Err(Ok(EscrowError::InvalidRating))
    );

    // Valid rating 1..=5 accepted
    assert_eq!(s.escrow.try_rate_client(&id, &3), Ok(Ok(())));
    let rep = s.escrow.get_client_reputation(&s.client);
    assert_eq!(rep.completed_contracts, 1);
    assert_eq!(rep.total_rating, 3);
}

#[test]
fn test_rate_client_status_rejections() {
    let s = setup();

    // 1. Escrow in Funded status rejected
    let id_funded = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id_funded, &100);
    assert_eq!(
        s.escrow.get_contract(&id_funded).status,
        ContractStatus::Funded
    );
    assert_eq!(
        s.escrow.try_rate_client(&id_funded, &5),
        Err(Ok(EscrowError::InvalidStatus))
    );

    // 2. Escrow in Cancelled status rejected
    let id_cancelled = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50i128],
    );
    s.escrow.deposit_funds(&id_cancelled, &50);
    s.escrow.cancel_contract(&id_cancelled);
    assert_eq!(
        s.escrow.get_contract(&id_cancelled).status,
        ContractStatus::Cancelled
    );
    assert_eq!(
        s.escrow.try_rate_client(&id_cancelled, &5),
        Err(Ok(EscrowError::InvalidStatus))
    );
}

#[test]
fn test_reputation_independence() {
    let s = setup();

    // Contract 1: Client issues reputation first, then freelancer rates client
    let id1 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id1, &100);
    s.escrow.release_milestone(&id1, &0);

    // Client rates freelancer
    s.escrow.issue_reputation(&id1, &5);
    let fl_rep1 = s.escrow.get_reputation(&s.freelancer);
    assert_eq!(fl_rep1.completed_contracts, 1);
    assert_eq!(fl_rep1.total_rating, 5);

    // Freelancer rates client afterwards; client-rating is not blocked
    s.escrow.rate_client(&id1, &4);
    let cl_rep1 = s.escrow.get_client_reputation(&s.client);
    assert_eq!(cl_rep1.completed_contracts, 1);
    assert_eq!(cl_rep1.total_rating, 4);

    // Contract 2: Freelancer rates client first, then client rates freelancer
    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id2, &100);
    s.escrow.release_milestone(&id2, &0);

    // Freelancer rates client first
    s.escrow.rate_client(&id2, &5);
    let cl_rep2 = s.escrow.get_client_reputation(&s.client);
    assert_eq!(cl_rep2.completed_contracts, 2);
    assert_eq!(cl_rep2.total_rating, 9);

    // Client rates freelancer afterwards; freelancer-rating is not blocked
    s.escrow.issue_reputation(&id2, &4);
    let fl_rep2 = s.escrow.get_reputation(&s.freelancer);
    assert_eq!(fl_rep2.completed_contracts, 2);
    assert_eq!(fl_rep2.total_rating, 9);

    // Duplicate client rating on id2 rejected
    assert_eq!(
        s.escrow.try_rate_client(&id2, &5),
        Err(Ok(EscrowError::ReputationAlreadyIssued))
    );
    // Duplicate freelancer rating on id2 rejected
    assert_eq!(
        s.escrow.try_issue_reputation(&id2, &4),
        Err(Ok(EscrowError::ReputationAlreadyIssued))
    );
}

#[test]
fn test_rate_client_pause_gate() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.release_milestone(&id, &0);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);

    // Pause contract
    s.escrow.set_paused(&true);
    assert_eq!(
        s.escrow.try_rate_client(&id, &5),
        Err(Ok(EscrowError::Paused))
    );

    // Unpause contract and verify rating succeeds
    s.escrow.set_paused(&false);
    assert_eq!(s.escrow.try_rate_client(&id, &5), Ok(Ok(())));
    let rep = s.escrow.get_client_reputation(&s.client);
    assert_eq!(rep.completed_contracts, 1);
    assert_eq!(rep.total_rating, 5);
}

fn set_now(env: &Env, secs: u64) {
    env.ledger().with_mut(|li| {
        li.timestamp = secs;
    });
}

#[test]
fn test_funding_deadline_boundaries_deposit() {
    let s = setup();
    set_now(&s.env, 1_000_000);

    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    let fund_by = s.escrow.get_fund_by(&id);
    assert_eq!(fund_by, 1_000_000 + FUNDING_WINDOW_SECS);

    // At now == fund_by, deposit must not be rejected because of funding deadline
    set_now(&s.env, fund_by);
    s.escrow.deposit_funds(&id, &50);
    assert_eq!(s.token.balance(&s.client), 950);
    let c = s.escrow.get_contract(&id);
    assert_eq!(c.status, ContractStatus::Created);
    assert_eq!(c.funded_amount, 50);

    // At now == fund_by + 1, deposit must be rejected with FundingExpired
    set_now(&s.env, fund_by + 1);
    assert_eq!(
        s.escrow.try_deposit_funds(&id, &50),
        Err(Ok(EscrowError::FundingExpired))
    );
    // Escrow state remains unchanged
    let c_after = s.escrow.get_contract(&id);
    assert_eq!(c_after.status, ContractStatus::Created);
    assert_eq!(c_after.funded_amount, 50);
    assert_eq!(s.token.balance(&s.client), 950);
}

#[test]
fn test_funding_deadline_boundaries_expire() {
    let s = setup();
    set_now(&s.env, 1_000_000);

    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
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
fn test_partial_deposit_refund_third_party() {
    let s = setup();
    set_now(&s.env, 1_000_000);

    // Client starts with 1,000 tokens
    assert_eq!(s.token.balance(&s.client), 1_000);
    assert_eq!(s.token.balance(&s.freelancer), 0);

    // Create escrow with total 100 and partial deposit of 40
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 60i128, 40i128],
    );
    s.escrow.deposit_funds(&id, &40);
    assert_eq!(s.token.balance(&s.client), 960);
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
    assert_eq!(s.token.balance(&s.client), 1_000);
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
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Funded);
    assert_eq!(s.token.balance(&s.client), 900);

    let fund_by = s.escrow.get_fund_by(&id);
    set_now(&s.env, fund_by + 1);

    // Escrow in Funded state cannot be expired
    assert_eq!(
        s.escrow.try_expire_unfunded(&id),
        Err(Ok(EscrowError::InvalidStatus))
    );
    // Verify no refund or state change occurs
    assert_eq!(s.token.balance(&s.client), 900);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Funded);

    // Escrows already reached Funded remain unaffected by funding-deadline in deposit_funds
    // (deposit exceeds total amount, failing with OverFunded rather than FundingExpired)
    assert_eq!(
        s.escrow.try_deposit_funds(&id, &1),
        Err(Ok(EscrowError::OverFunded))
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
        &vec![&s.env, 50i128, 50i128],
    );
    // Partial deposit 50
    s.escrow.deposit_funds(&id, &50);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Created);

    // Release first milestone
    s.escrow.release_milestone(&id, &0);
    assert_eq!(s.token.balance(&s.freelancer), 50);

    let c = s.escrow.get_contract(&id);
    assert_eq!(c.status, ContractStatus::Created);
    assert_eq!(c.released_amount, 50);

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
    assert_eq!(c_after.released_amount, 50);
    assert_eq!(c_after.funded_amount, 50);
    assert_eq!(s.token.balance(&s.client), 950);
    assert_eq!(s.token.balance(&s.freelancer), 50);
}

#[test]
fn test_expire_unfunded_event_verification() {
    let s = setup();
    set_now(&s.env, 1_000_000);

    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &40);

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
    assert_eq!(refunded, 40i128);

    // Second escrow which is not expired
    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
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
fn test_expire_unfunded_zero_deposit() {
    let s = setup();
    set_now(&s.env, 1_000_000);

    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
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
    assert_eq!(s.token.balance(&s.client), 1_000);
}

#[test]
fn test_expire_unfunded_pause_gate_and_status_checks() {
    let s = setup();
    set_now(&s.env, 1_000_000);

    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    let fund_by = s.escrow.get_fund_by(&id);
    set_now(&s.env, fund_by + 1);

    // Pause blocks expire_unfunded
    s.escrow.set_paused(&true);
    assert_eq!(
        s.escrow.try_expire_unfunded(&id),
        Err(Ok(EscrowError::Paused))
    );

    // Unpause allows expire_unfunded
    s.escrow.set_paused(&false);
    assert_eq!(s.escrow.try_expire_unfunded(&id), Ok(Ok(())));

    // Re-expiring an already cancelled escrow fails with InvalidStatus
    assert_eq!(
        s.escrow.try_expire_unfunded(&id),
        Err(Ok(EscrowError::InvalidStatus))
    );

    // Depositing into cancelled escrow fails with InvalidStatus
    assert_eq!(
        s.escrow.try_deposit_funds(&id, &50),
        Err(Ok(EscrowError::InvalidStatus))
    );

    // Missing storage for get_fund_by returns 0 default
    assert_eq!(s.escrow.get_fund_by(&999), 0);
}

#[test]
fn dispute_outcomes_freelancer_client_and_exact_threshold() {
    let s = setup();
    // 1. Freelancer-favoured resolution (> 5_000 bps): 7_500 bps
    let id1 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 200i128],
    );
    s.escrow.deposit_funds(&id1, &200);
    s.escrow.open_dispute(&id1, &s.client);
    s.escrow.resolve_dispute(&id1, &7_500);
    let client_outcomes1 = s.escrow.get_outcomes(&s.client);
    let freelancer_outcomes1 = s.escrow.get_outcomes(&s.freelancer);
    assert_eq!(client_outcomes1.disputes_opened, 1);
    assert_eq!(client_outcomes1.disputes_won, 0);
    assert_eq!(client_outcomes1.disputes_lost, 1);
    assert_eq!(freelancer_outcomes1.disputes_opened, 0);
    assert_eq!(freelancer_outcomes1.disputes_won, 1);
    assert_eq!(freelancer_outcomes1.disputes_lost, 0);
    // 2. Client-favoured resolution (< 5_000 bps): 2_000 bps
    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 200i128],
    );
    s.escrow.deposit_funds(&id2, &200);
    s.escrow.open_dispute(&id2, &s.freelancer);
    s.escrow.resolve_dispute(&id2, &2_000);
    let client_outcomes2 = s.escrow.get_outcomes(&s.client);
    let freelancer_outcomes2 = s.escrow.get_outcomes(&s.freelancer);
    assert_eq!(client_outcomes2.disputes_opened, 1);
    assert_eq!(client_outcomes2.disputes_won, 1);
    assert_eq!(client_outcomes2.disputes_lost, 1);
    assert_eq!(freelancer_outcomes2.disputes_opened, 1);
    assert_eq!(freelancer_outcomes2.disputes_won, 1);
    assert_eq!(freelancer_outcomes2.disputes_lost, 1);
    // 3. Exactly 5_000 bps threshold: counts as a win for freelancer, loss for client
    let id3 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 200i128],
    );
    s.escrow.deposit_funds(&id3, &200);
    s.escrow.open_dispute(&id3, &s.client);
    s.escrow.resolve_dispute(&id3, &5_000);
    let client_outcomes3 = s.escrow.get_outcomes(&s.client);
    let freelancer_outcomes3 = s.escrow.get_outcomes(&s.freelancer);
    assert_eq!(client_outcomes3.disputes_opened, 2);
    assert_eq!(client_outcomes3.disputes_won, 1);
    assert_eq!(client_outcomes3.disputes_lost, 2);
    assert_eq!(freelancer_outcomes3.disputes_opened, 1);
    assert_eq!(freelancer_outcomes3.disputes_won, 2);
    assert_eq!(freelancer_outcomes3.disputes_lost, 1);
}
#[test]
fn rate_resolved_escrow_succeeds_once_and_cancelled_rejected() {
    let s = setup();
    // Create and resolve dispute
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.open_dispute(&id, &s.client);
    s.escrow.resolve_dispute(&id, &5_000);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Resolved);
    // Rating a Resolved escrow succeeds once
    s.escrow.issue_reputation(&id, &4);
    let rep = s.escrow.get_reputation(&s.freelancer);
    assert_eq!(rep.completed_contracts, 1);
    assert_eq!(rep.total_rating, 4);
    // Second rating attempt is rejected
    assert_eq!(
        s.escrow.try_issue_reputation(&id, &4),
        Err(Ok(EscrowError::ReputationAlreadyIssued))
    );
    // Cancelled escrow rating is rejected
    let id_cancel = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50i128],
    );
    s.escrow.deposit_funds(&id_cancel, &50);
    s.escrow.cancel_contract(&id_cancel);
    assert_eq!(
        s.escrow.get_contract(&id_cancel).status,
        ContractStatus::Cancelled
    );
    assert_eq!(
        s.escrow.try_issue_reputation(&id_cancel, &5),
        Err(Ok(EscrowError::InvalidStatus))
    );
}
#[test]
fn cancel_unfunded_escrow_does_not_count_cancellation() {
    let s = setup();
    // 1. Unfunded cancellation -> cancellations remains 0
    let id1 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.cancel_contract(&id1);
    assert_eq!(s.escrow.get_outcomes(&s.client).cancellations, 0);
    // 2. Funded cancellation -> cancellations increments to 1
    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id2, &100);
    s.escrow.cancel_contract(&id2);
    assert_eq!(s.escrow.get_outcomes(&s.client).cancellations, 1);
}

#[test]
fn rate_then_void_restores_reputation_and_prevents_rerating() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.release_milestone(&id, &0);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);

    // Initial reputation is 0
    let rep0 = s.escrow.get_reputation(&s.freelancer);
    assert_eq!(rep0.completed_contracts, 0);
    assert_eq!(rep0.total_rating, 0);

    // Issue rating 5
    s.escrow.issue_reputation(&id, &5);
    let rep1 = s.escrow.get_reputation(&s.freelancer);
    assert_eq!(rep1.completed_contracts, 1);
    assert_eq!(rep1.total_rating, 5);

    // Verify rating record is stored
    let rating_rec = s.escrow.get_rating(&id).expect("rating record must exist");
    assert_eq!(rating_rec.freelancer, s.freelancer);
    assert_eq!(rating_rec.rating, 5);
    assert!(!rating_rec.voided);

    // Admin voids rating
    let reason = symbol_short!("fraud");
    s.escrow.void_rating(&id, &reason);

    // Reputation is restored to exact previous values
    let rep2 = s.escrow.get_reputation(&s.freelancer);
    assert_eq!(rep2.completed_contracts, 0);
    assert_eq!(rep2.total_rating, 0);

    // Rating record reflects voided state
    let rating_rec_voided = s.escrow.get_rating(&id).expect("rating record must exist");
    assert!(rating_rec_voided.voided);

    // Voiding twice is rejected with RatingAlreadyVoided
    assert_eq!(
        s.escrow.try_void_rating(&id, &reason),
        Err(Ok(EscrowError::RatingAlreadyVoided))
    );

    // Voided escrow cannot be re-rated (reputation_already_issued remains)
    assert_eq!(
        s.escrow.try_issue_reputation(&id, &5),
        Err(Ok(EscrowError::ReputationAlreadyIssued))
    );
}

#[test]
fn void_unrated_escrow_fails_with_rating_not_found() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    assert_eq!(s.escrow.get_rating(&id), None);

    let reason = symbol_short!("fraud");
    assert_eq!(
        s.escrow.try_void_rating(&id, &reason),
        Err(Ok(EscrowError::RatingNotFound))
    );
}

#[test]
fn void_one_of_three_ratings_preserves_others() {
    let s = setup();

    // Create 3 escrows
    let id1 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50i128],
    );
    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50i128],
    );
    let id3 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50i128],
    );

    // Deposit and complete each
    s.escrow.deposit_funds(&id1, &50);
    s.escrow.release_milestone(&id1, &0);
    s.escrow.issue_reputation(&id1, &5);

    s.escrow.deposit_funds(&id2, &50);
    s.escrow.release_milestone(&id2, &0);
    s.escrow.issue_reputation(&id2, &4);

    s.escrow.deposit_funds(&id3, &50);
    s.escrow.release_milestone(&id3, &0);
    s.escrow.issue_reputation(&id3, &3);

    // Total should be 3 contracts, 12 rating
    let rep_before = s.escrow.get_reputation(&s.freelancer);
    assert_eq!(rep_before.completed_contracts, 3);
    assert_eq!(rep_before.total_rating, 12);

    // Void rating for escrow 2 (rating 4)
    s.escrow.void_rating(&id2, &symbol_short!("collusion"));

    // Total should now be 2 contracts, 8 rating (5 + 3)
    let rep_after = s.escrow.get_reputation(&s.freelancer);
    assert_eq!(rep_after.completed_contracts, 2);
    assert_eq!(rep_after.total_rating, 8);

    // Check individual records
    assert!(!s.escrow.get_rating(&id1).unwrap().voided);
    assert!(s.escrow.get_rating(&id2).unwrap().voided);
    assert!(!s.escrow.get_rating(&id3).unwrap().voided);
}

#[test]
fn ratings_issued_before_upgrade_return_rating_not_found() {
    let s = setup();
    let unrecorded_id = 42;
    assert_eq!(s.escrow.get_rating(&unrecorded_id), None);
    assert_eq!(
        s.escrow.try_void_rating(&unrecorded_id, &symbol_short!("legacy")),
        Err(Ok(EscrowError::RatingNotFound))
    );
}

#[test]
fn non_admin_cannot_void_rating() {
    let s = setup();
    assert_eq!(s.escrow.get_admin(), s.admin);

    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.release_milestone(&id, &0);
    s.escrow.issue_reputation(&id, &5);

    let non_admin = Address::generate(&s.env);
    s.env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &non_admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &s.escrow.address,
            fn_name: "void_rating",
            args: soroban_sdk::IntoVal::into_val(&(&id, symbol_short!("fraud")), &s.env),
            sub_invokes: &[],
        },
    }]);

    // Calling void_rating as non-admin must fail
}
