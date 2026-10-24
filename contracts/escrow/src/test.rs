extern crate std;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _, MockAuth, MockAuthInvoke},
    token::StellarAssetClient,
    token::TokenClient,
    vec, TryFromVal, TryIntoVal,
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
    assert!(s.escrow.try_void_rating(&id, &symbol_short!("fraud")).is_err());
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
fn deposit_funds_on_funded_returns_invalid_status() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Funded);

    // Attempting to deposit on an already Funded escrow returns InvalidStatus immediately
    assert_eq!(
        s.escrow.try_deposit_funds(&id, &50),
        Err(Ok(EscrowError::InvalidStatus))
    );
}

struct Lcg {
    state: u64,
}
impl Lcg {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }
    fn next_u32(&mut self) -> u32 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.state >> 32) as u32
    }
    fn next_bounded(&mut self, bound: u32) -> u32 {
        if bound == 0 {
            0
        } else {
            self.next_u32() % bound
        }
    }
}

#[test]
fn randomized_lifecycle_property_test() {
    const SEED: u64 = 0x5EED_2026_1019;
    let mut rng = Lcg::new(SEED);

    let s = setup();
    // Mint ample tokens to client for all iterations
    StellarAssetClient::new(&s.env, &s.token.address).mint(&s.client, &10_000_000);

    let mut id1 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50, 50],
    );
    let mut id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 40, 60],
    );

    let mut c1 = s.escrow.get_contract(&id1);
    let mut c2 = s.escrow.get_contract(&id2);
    let mut bal_contract = s.token.balance(&s.escrow.address);

    for iteration in 0..500 {
        if matches!(
            c1.status,
            ContractStatus::Completed | ContractStatus::Resolved | ContractStatus::Cancelled
        ) {
            let m0 = (rng.next_bounded(50) + 10) as i128;
            let m1 = (rng.next_bounded(50) + 10) as i128;
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
            let m0 = (rng.next_bounded(40) + 15) as i128;
            let m1 = (rng.next_bounded(40) + 15) as i128;
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

            let op_res = match op {
                0 => {
                    let amt = (rng.next_bounded(120) + 1) as i128;
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
                    s.escrow.try_resolve_dispute(&target_id, &bps)
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
                assert_eq!(
                    new_bal,
                    bal_contract + delta_held,
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
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.release_milestone(&id, &0);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);

    let contract_before = s.token.balance(&s.escrow.address);
    s.escrow.pay_bonus(&id, &25);
    assert_eq!(s.escrow.get_bonus(&id), 25);
    assert_eq!(s.escrow.get_bonus_total(&s.freelancer), 25);
    assert_eq!(s.token.balance(&s.freelancer), 125);
    assert_eq!(s.token.balance(&s.client), 875);
    assert_eq!(s.token.balance(&s.escrow.address), contract_before);
    let c = s.escrow.get_contract(&id);
    assert_eq!(c.funded_amount, 100);
    assert_eq!(c.released_amount, 100);
}

#[test]
fn bonus_on_funded_escrow() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Funded);

    let contract_before = s.token.balance(&s.escrow.address);
    s.escrow.pay_bonus(&id, &10);
    assert_eq!(s.escrow.get_bonus(&id), 10);
    assert_eq!(s.token.balance(&s.freelancer), 10);
    assert_eq!(s.token.balance(&s.escrow.address), contract_before);
    let c = s.escrow.get_contract(&id);
    assert_eq!(c.funded_amount, 100);
    assert_eq!(c.released_amount, 0);
}

#[test]
fn bonus_rejected_on_disputed_and_cancelled() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.open_dispute(&id, &s.freelancer);
    assert!(s.escrow.try_pay_bonus(&id, &10).is_err());

    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50i128],
    );
    s.escrow.deposit_funds(&id2, &50);
    s.escrow.cancel_contract(&id2);
    assert!(s.escrow.try_pay_bonus(&id2, &10).is_err());
}

#[test]
fn bonus_rejects_zero_and_negative() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
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
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    // Only the freelancer authorizes: the client's auth is missing, so it must fail.
    s.env.mock_auths(&[MockAuth {
        address: &s.freelancer,
        invoke: &MockAuthInvoke {
            contract: &s.escrow.address,
            fn_name: "pay_bonus",
            args: (id, 10i128).into_val(&s.env),
            sub_invokes: &[],
        },
    }]);
    assert!(s.escrow.try_pay_bonus(&id, &10).is_err());
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
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id1, &100);
    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50i128],
    );
    s.escrow.deposit_funds(&id2, &50);

    s.escrow.pay_bonus(&id1, &15);
    s.escrow.pay_bonus(&id2, &25);
    assert_eq!(s.escrow.get_bonus(&id1), 15);
    assert_eq!(s.escrow.get_bonus(&id2), 25);
    assert_eq!(s.escrow.get_bonus_total(&s.freelancer), 40);
    assert_eq!(s.token.balance(&s.freelancer), 40);
}

#[test]
fn bonus_blocked_when_paused() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.set_paused(&true);
    assert_eq!(
        s.escrow.try_pay_bonus(&id, &10),
        Err(Ok(EscrowError::Paused))
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
    token_admin_client.mint(&client, &1000);

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
    token_admin_client.mint(&contract_id, &50);

    // Test non-admin sweep rejection
    let non_admin = Address::generate(&env);
    // Without the admin's authorization the sweep must be rejected.
    env.mock_auths(&[]);
    let sweep_result = escrow_client.try_sweep_surplus(&token_id, &non_admin);
    assert!(sweep_result.is_err());
    env.mock_all_auths();

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

    token_admin_client.mint(&client, &100_000);

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

        // Assert balance >= liability after every step
        let balance = token_client.balance(&contract_id);
        let liability = escrow_client.get_liability(&token_id);

        assert!(balance >= liability);
    }
}

// ---------------------------------------------------------------------------
// Payout address feature
// ---------------------------------------------------------------------------

/// Helper: create + fully fund an escrow, returning its id.
fn funded_escrow(s: &Setup) -> u64 {
    let id = s
        .escrow
        .create_contract(&s.client, &s.freelancer, &s.token.address, &vec![&s.env, 100i128]);
    s.escrow.deposit_funds(&id, &100);
    id
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

    assert_eq!(s.token.balance(&payout_wallet), 100);
    assert_eq!(s.token.balance(&s.freelancer), 0);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);
}

#[test]
fn resolve_dispute_credits_payout_address_not_freelancer() {
    let s = setup();
    let id = funded_escrow(&s);
    let payout_wallet = Address::generate(&s.env);

    s.escrow.set_payout_address(&id, &payout_wallet);
    s.escrow.open_dispute(&id, &s.freelancer);
    // set_payout_address must be rejected while disputed
    let other = Address::generate(&s.env);
    assert_eq!(
        s.escrow.try_set_payout_address(&id, &other),
        Err(Ok(EscrowError::InvalidStatus))
    );
    s.escrow.resolve_dispute(&id, &10_000); // full share to freelancer side

    assert_eq!(s.token.balance(&payout_wallet), 100);
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

    s.escrow.open_dispute(&id, &s.freelancer);
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
fn test_per_escrow_freeze_isolation_and_unfreeze() {
    let s = setup();

    // Create two separate escrows
    let id_a = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128, 200i128],
    );
    let id_b = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128, 200i128],
    );

    // Freeze escrow A
    let reason = symbol_short!("fraud");
    s.escrow.freeze_escrow(&id_a, &reason);
    assert!(s.escrow.is_frozen(&id_a));
    assert!(!s.escrow.is_frozen(&id_b));

    // Record initial token balances
    let client_bal_before = s.token.balance(&s.client);
    let freelancer_bal_before = s.token.balance(&s.freelancer);
    let contract_bal_before = s.token.balance(&s.escrow.address);

    // All listed mutations on A must return EscrowFrozen and leave balances unchanged
    assert_eq!(
        s.escrow.try_deposit_funds(&id_a, &300),
        Err(Ok(EscrowError::EscrowFrozen))
    );
    assert_eq!(
        s.escrow.try_release_milestone(&id_a, &0),
        Err(Ok(EscrowError::EscrowFrozen))
    );
    assert_eq!(
        s.escrow.try_open_dispute(&id_a, &s.client),
        Err(Ok(EscrowError::EscrowFrozen))
    );
    assert_eq!(
        s.escrow.try_cancel_contract(&id_a),
        Err(Ok(EscrowError::EscrowFrozen))
    );

    // Verify token balances are unchanged
    assert_eq!(s.token.balance(&s.client), client_bal_before);
    assert_eq!(s.token.balance(&s.freelancer), freelancer_bal_before);
    assert_eq!(s.token.balance(&s.escrow.address), contract_bal_before);

    // Escrow B operations succeed normally
    assert_eq!(s.escrow.try_deposit_funds(&id_b, &300), Ok(Ok(())));
    assert_eq!(s.escrow.try_release_milestone(&id_b, &0), Ok(Ok(())));
    assert_eq!(s.token.balance(&s.freelancer), freelancer_bal_before + 100);

    // Unfreeze Escrow A and verify normal operation is restored
    s.escrow.unfreeze_escrow(&id_a);
    assert!(!s.escrow.is_frozen(&id_a));
    assert_eq!(s.escrow.try_deposit_funds(&id_a, &300), Ok(Ok(())));
}

#[test]
fn test_resolve_dispute_allowed_on_frozen_escrow() {
    let s = setup();

    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 400i128],
    );
    s.escrow.deposit_funds(&id, &400);

    // Open dispute before freezing
    s.escrow.open_dispute(&id, &s.freelancer);

    // Admin freezes escrow
    let reason = symbol_short!("suspect");
    s.escrow.freeze_escrow(&id, &reason);
    assert!(s.escrow.is_frozen(&id));

    // Admin resolves dispute on frozen escrow (must succeed)
    assert_eq!(s.escrow.try_resolve_dispute(&id, &2_500), Ok(Ok(())));
    assert_eq!(s.token.balance(&s.freelancer), 100);
    assert_eq!(s.token.balance(&s.client), 900);
}

#[test]
fn test_freeze_error_conditions() {
    let s = setup();

    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );

    // ContractNotFound for unknown IDs
    assert_eq!(
        s.escrow.try_freeze_escrow(&999, &symbol_short!("none")),
        Err(Ok(EscrowError::ContractNotFound))
    );
    assert_eq!(
        s.escrow.try_unfreeze_escrow(&999),
        Err(Ok(EscrowError::ContractNotFound))
    );

    // Unfreezing non-frozen escrow returns InvalidStatus
    assert_eq!(
        s.escrow.try_unfreeze_escrow(&id),
        Err(Ok(EscrowError::InvalidStatus))
    );

    // Freeze valid escrow
    s.escrow.freeze_escrow(&id, &symbol_short!("reason"));

    // Freezing an already-frozen escrow returns EscrowFrozen
    assert_eq!(
        s.escrow.try_freeze_escrow(&id, &symbol_short!("again")),
        Err(Ok(EscrowError::EscrowFrozen))
    );
}

#[test]
fn test_non_admin_freeze_rejected() {
    let s = setup();
    let contract_id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );

    // No authorization mocked: the admin has not signed, so the freeze must fail.
    s.env.mock_auths(&[]);
    let res = s
        .escrow
        .try_freeze_escrow(&contract_id, &symbol_short!("fraud"));
    assert!(res.is_err());
    s.env.mock_all_auths();
    assert!(!s.escrow.is_frozen(&contract_id));
}

#[test]
fn test_freeze_blocks_bonus_and_payout_change() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.freeze_escrow(&id, &symbol_short!("fraud"));
    assert_eq!(
        s.escrow.try_pay_bonus(&id, &10),
        Err(Ok(EscrowError::EscrowFrozen))
    );
    let wallet = Address::generate(&s.env);
    assert_eq!(
        s.escrow.try_set_payout_address(&id, &wallet),
        Err(Ok(EscrowError::EscrowFrozen))
    );
}
