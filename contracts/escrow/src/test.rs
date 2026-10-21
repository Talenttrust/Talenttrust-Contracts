extern crate std;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events, MockAuth, MockAuthInvoke},
    token::StellarAssetClient,
    token::TokenClient,
    vec, IntoVal, Val,
};

struct Setup<'a> {
    env: Env,
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

// ---------------------------------------------------------------------------
// #1670 — two-step admin transfer
// ---------------------------------------------------------------------------

fn setup_admin<'a>() -> (Env, EscrowClient<'a>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let id = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &id);
    escrow.initialize(&admin);
    (env, escrow, admin)
}

#[test]
fn propose_then_accept_transfers_admin() {
    let (env, escrow, old_admin) = setup_admin();
    let new_admin = Address::generate(&env);

    // No pending admin initially.
    assert_eq!(escrow.get_pending_admin(), None);

    escrow.propose_admin(&new_admin);
    assert_eq!(escrow.get_pending_admin(), Some(new_admin.clone()));
    // Control has NOT moved yet.
    assert_eq!(escrow.get_admin(), old_admin);

    escrow.accept_admin();
    assert_eq!(escrow.get_admin(), new_admin.clone());
    assert_eq!(escrow.get_pending_admin(), None);

    // New admin can pause; the old admin can no longer.
    escrow.set_paused(&true);
    assert!(escrow.is_paused());

    // Explicitly authorize the OLD admin for set_paused: must now fail.
    env.set_auths(&[]);
    escrow.mock_auths(&[MockAuth {
        address: &old_admin,
        invoke: &MockAuthInvoke {
            contract: &escrow.address,
            fn_name: "set_paused",
            args: (false,).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(escrow.try_set_paused(&false).is_err());
}

#[test]
fn propose_rejects_self_and_non_admin() {
    let (env, escrow, admin) = setup_admin();
    // Proposing the current admin is rejected.
    assert_eq!(
        escrow.try_propose_admin(&admin),
        Err(Ok(EscrowError::InvalidAdmin))
    );

    // A non-admin proposer fails auth. Only `stranger` is authorized here.
    let stranger = Address::generate(&env);
    let nominee = Address::generate(&env);
    env.set_auths(&[]);
    escrow.mock_auths(&[MockAuth {
        address: &stranger,
        invoke: &MockAuthInvoke {
            contract: &escrow.address,
            fn_name: "propose_admin",
            args: (nominee.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(escrow.try_propose_admin(&nominee).is_err());
    assert_eq!(escrow.get_pending_admin(), None);
}

#[test]
fn accept_without_pending_fails() {
    let (_env, escrow, _admin) = setup_admin();
    assert_eq!(
        escrow.try_accept_admin(),
        Err(Ok(EscrowError::NoPendingAdmin))
    );
}

#[test]
fn accept_by_non_nominee_fails() {
    let (env, escrow, _admin) = setup_admin();
    let nominee = Address::generate(&env);
    let imposter = Address::generate(&env);
    escrow.propose_admin(&nominee);

    // Only the imposter is authorized: accept must fail auth and leave the
    // nominee pending and the admin unchanged.
    env.set_auths(&[]);
    escrow.mock_auths(&[MockAuth {
        address: &imposter,
        invoke: &MockAuthInvoke {
            contract: &escrow.address,
            fn_name: "accept_admin",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(escrow.try_accept_admin().is_err());
    assert_eq!(escrow.get_pending_admin(), Some(nominee));
}

#[test]
fn cancel_without_pending_fails_and_cancel_clears_pending() {
    let (env, escrow, _admin) = setup_admin();
    assert_eq!(
        escrow.try_cancel_admin_proposal(),
        Err(Ok(EscrowError::NoPendingAdmin))
    );

    let nominee = Address::generate(&env);
    escrow.propose_admin(&nominee);
    escrow.cancel_admin_proposal();
    assert_eq!(escrow.get_pending_admin(), None);
    // Nominee can no longer accept.
    assert_eq!(
        escrow.try_accept_admin(),
        Err(Ok(EscrowError::NoPendingAdmin))
    );
}

#[test]
fn second_proposal_replaces_first_nominee() {
    let (env, escrow, _admin) = setup_admin();
    let first = Address::generate(&env);
    let second = Address::generate(&env);

    escrow.propose_admin(&first);
    escrow.propose_admin(&second);
    assert_eq!(escrow.get_pending_admin(), Some(second.clone()));

    escrow.accept_admin();
    assert_eq!(escrow.get_admin(), second);
}

#[test]
fn admin_transfer_works_while_paused() {
    let (env, escrow, _admin) = setup_admin();
    escrow.set_paused(&true);
    assert!(escrow.is_paused());

    let new_admin = Address::generate(&env);
    escrow.propose_admin(&new_admin);
    escrow.accept_admin();
    assert_eq!(escrow.get_admin(), new_admin);
    assert_eq!(escrow.get_pending_admin(), None);
    // Still paused: admin transfer is a recovery action, not a state change.
    assert!(escrow.is_paused());
}

#[test]
fn admin_transfer_events_have_topic_and_payload() {
    use soroban_sdk::FromVal;
    let (env, escrow, old_admin) = setup_admin();
    let new_admin = Address::generate(&env);

    escrow.propose_admin(&new_admin);
    let prop = env.events().all().last().unwrap();
    assert_eq!(prop.0, escrow.address);
    let prop_topics: Vec<Val> = (symbol_short!("adm_prop"),).into_val(&env);
    assert_eq!(prop.1, prop_topics);
    let prop_data: (Address, Address) = FromVal::from_val(&env, &prop.2);
    assert_eq!(prop_data, (old_admin.clone(), new_admin.clone()));

    escrow.accept_admin();
    let acc = env.events().all().last().unwrap();
    let acc_topics: Vec<Val> = (symbol_short!("adm_acc"),).into_val(&env);
    assert_eq!(acc.1, acc_topics);
    let acc_data: (Address, Address) = FromVal::from_val(&env, &acc.2);
    assert_eq!(acc_data, (old_admin.clone(), new_admin.clone()));

    escrow.propose_admin(&old_admin);
    escrow.cancel_admin_proposal();
    let cncl = env.events().all().last().unwrap();
    let cncl_topics: Vec<Val> = (symbol_short!("adm_cncl"),).into_val(&env);
    assert_eq!(cncl.1, cncl_topics);
    let cncl_data: Address = FromVal::from_val(&env, &cncl.2);
    assert_eq!(cncl_data, old_admin);
}
