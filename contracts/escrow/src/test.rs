extern crate std;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _, MockAuth, MockAuthInvoke},
    token::StellarAssetClient,
    token::TokenClient,
    vec, IntoVal, TryFromVal, TryIntoVal,
};

/// One token amount unit in tests: the minimum milestone amount, so every
/// milestone used below is >= `MIN_MILESTONE_AMOUNT`.
const U: i128 = MIN_MILESTONE_AMOUNT;

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
    StellarAssetClient::new(&env, &sac.address()).mint(&client, &(1_000 * U));
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
fn dispute_split_and_cancel() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 400 * U],
    );
    s.escrow.deposit_funds(&id, &(400 * U));
    s.escrow.open_dispute(&id, &s.freelancer);
    s.escrow.resolve_dispute(&s.admin, &id, &2_500);
    assert_eq!(s.token.balance(&s.freelancer), 100 * U);
    assert_eq!(s.token.balance(&s.client), 900 * U);

    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50 * U],
    );
    s.escrow.deposit_funds(&id2, &(50 * U));
    s.escrow.cancel_contract(&id2);
    assert_eq!(s.token.balance(&s.client), 900 * U);
    assert_eq!(
        s.escrow.get_contract(&id2).status,
        ContractStatus::Cancelled
    );
}

#[test]
fn pause_blocks_mutations() {
    let s = setup();
    s.escrow.set_paused(&s.admin, &true);
    assert_eq!(
        s.escrow.try_create_contract(
            &s.client,
            &s.freelancer,
            &s.token.address,
            &vec![&s.env, U]
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
        &vec![&s.env, 200 * U],
    );
    s.escrow.deposit_funds(&id1, &(200 * U));
    s.escrow.open_dispute(&id1, &s.client);
    s.escrow.resolve_dispute(&s.admin, &id1, &7_500);

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
        &vec![&s.env, 200 * U],
    );
    s.escrow.deposit_funds(&id2, &(200 * U));
    s.escrow.open_dispute(&id2, &s.freelancer);
    s.escrow.resolve_dispute(&s.admin, &id2, &2_000);

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
        &vec![&s.env, 200 * U],
    );
    s.escrow.deposit_funds(&id3, &(200 * U));
    s.escrow.open_dispute(&id3, &s.client);
    s.escrow.resolve_dispute(&s.admin, &id3, &5_000);

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
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.open_dispute(&id, &s.client);
    s.escrow.resolve_dispute(&s.admin, &id, &5_000);

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
        &vec![&s.env, 50 * U],
    );
    s.escrow.deposit_funds(&id_cancel, &(50 * U));
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
fn rate_then_void_restores_reputation_and_prevents_rerating() {
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
        &vec![&s.env, 100 * U],
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
        &vec![&s.env, 50 * U],
    );
    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50 * U],
    );
    let id3 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50 * U],
    );

    // Deposit and complete each
    s.escrow.deposit_funds(&id1, &(50 * U));
    s.escrow.release_milestone(&id1, &0);
    s.escrow.issue_reputation(&id1, &5);

    s.escrow.deposit_funds(&id2, &(50 * U));
    s.escrow.release_milestone(&id2, &0);
    s.escrow.issue_reputation(&id2, &4);

    s.escrow.deposit_funds(&id3, &(50 * U));
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
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
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
                    s.escrow.try_resolve_dispute(&s.admin, &target_id, &bps)
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
    s.escrow.open_dispute(&id, &s.freelancer);
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
fn bonus_blocked_when_paused() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.set_paused(&s.admin, &true);
    assert_eq!(
        s.escrow.try_pay_bonus(&id, &(10 * U)),
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

// ---------------------------------------------------------------------------
// Payout address feature
// ---------------------------------------------------------------------------

/// Helper: create + fully fund an escrow, returning its id.
fn funded_escrow(s: &Setup) -> u64 {
    let id = s
        .escrow
        .create_contract(&s.client, &s.freelancer, &s.token.address, &vec![&s.env, 100 * U]);
    s.escrow.deposit_funds(&id, &(100 * U));
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
    s.escrow.open_dispute(&id, &s.freelancer);
    // set_payout_address must be rejected while disputed
    let other = Address::generate(&s.env);
    assert_eq!(
        s.escrow.try_set_payout_address(&id, &other),
        Err(Ok(EscrowError::InvalidStatus))
    );
    s.escrow.resolve_dispute(&s.admin, &id, &10_000); // full share to freelancer side

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
        &vec![&s.env, 100 * U, 200 * U],
    );
    let id_b = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U, 200 * U],
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
        s.escrow.try_deposit_funds(&id_a, &(300 * U)),
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
    assert_eq!(s.escrow.try_deposit_funds(&id_b, &(300 * U)), Ok(Ok(())));
    assert_eq!(s.escrow.try_release_milestone(&id_b, &0), Ok(Ok(())));
    assert_eq!(s.token.balance(&s.freelancer), freelancer_bal_before + 100 * U);

    // Unfreeze Escrow A and verify normal operation is restored
    s.escrow.unfreeze_escrow(&id_a);
    assert!(!s.escrow.is_frozen(&id_a));
    assert_eq!(s.escrow.try_deposit_funds(&id_a, &(300 * U)), Ok(Ok(())));
}

#[test]
fn test_resolve_dispute_allowed_on_frozen_escrow() {
    let s = setup();

    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 400 * U],
    );
    s.escrow.deposit_funds(&id, &(400 * U));

    // Open dispute before freezing
    s.escrow.open_dispute(&id, &s.freelancer);

    // Admin freezes escrow
    let reason = symbol_short!("suspect");
    s.escrow.freeze_escrow(&id, &reason);
    assert!(s.escrow.is_frozen(&id));

    // Admin resolves dispute on frozen escrow (must succeed)
    assert_eq!(s.escrow.try_resolve_dispute(&s.admin, &id, &2_500), Ok(Ok(())));
    assert_eq!(s.token.balance(&s.freelancer), 100 * U);
    assert_eq!(s.token.balance(&s.client), 900 * U);
}

#[test]
fn test_freeze_error_conditions() {
    let s = setup();

    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
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
        &vec![&s.env, 100 * U],
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
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.freeze_escrow(&id, &symbol_short!("fraud"));
    assert_eq!(
        s.escrow.try_pay_bonus(&id, &(10 * U)),
        Err(Ok(EscrowError::EscrowFrozen))
    );
    let wallet = Address::generate(&s.env);
    assert_eq!(
        s.escrow.try_set_payout_address(&id, &wallet),
        Err(Ok(EscrowError::EscrowFrozen))
    );
}

// ---------------------------------------------------------------------------
// create_contract input validation
// ---------------------------------------------------------------------------

/// Helper that builds a valid milestone vector for a given amount.
fn milestones(s: &Setup, amount: i128) -> Vec<i128> {
    vec![&s.env, amount]
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

// ---------------------------------------------------------------------------
// Terminal escrow compaction
// ---------------------------------------------------------------------------

#[test]
fn compact_cancelled_escrow() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.cancel_contract(&id);

    // Should be able to compact a cancelled escrow
    s.escrow.compact_escrow(&id);
    assert_eq!(
        s.escrow.try_get_contract(&id),
        Err(Ok(EscrowError::EscrowCompacted))
    );

    // Receipt should exist and match the escrow
    let receipt = s.escrow.get_receipt(&id);
    assert!(receipt.is_some());
    let r = receipt.unwrap();
    assert_eq!(r.id, id);
    assert_eq!(r.status, ContractStatus::Cancelled);
    assert_eq!(r.total_amount, 100 * U);
    assert_eq!(r.released_amount, 0);
    assert_eq!(r.refunded_amount, 100 * U);
    assert_eq!(r.milestone_count, 1);
    assert!(!r.reputation_issued);
}

#[test]
fn compact_resolved_escrow() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 400 * U],
    );
    s.escrow.deposit_funds(&id, &(400 * U));
    s.escrow.open_dispute(&id, &s.freelancer);
    s.escrow.resolve_dispute(&s.admin, &id, &2_500); // 25% to freelancer

    // Resolved escrows can still be rated, so compaction waits for the rating
    assert_eq!(
        s.escrow.try_compact_escrow(&id),
        Err(Ok(EscrowError::NotCompactable))
    );
    s.escrow.issue_reputation(&id, &4);

    // Should be able to compact a rated resolved escrow
    s.escrow.compact_escrow(&id);
    assert_eq!(
        s.escrow.try_get_contract(&id),
        Err(Ok(EscrowError::EscrowCompacted))
    );

    // Receipt should exist and match the escrow
    let receipt = s.escrow.get_receipt(&id);
    assert!(receipt.is_some());
    let r = receipt.unwrap();
    assert_eq!(r.id, id);
    assert_eq!(r.status, ContractStatus::Resolved);
    assert_eq!(r.total_amount, 400 * U);
    assert_eq!(r.released_amount, 100 * U); // 25% of 400
    assert_eq!(r.refunded_amount, 300 * U); // 75% of 400
    assert_eq!(r.milestone_count, 1);
    assert!(r.reputation_issued);
}

#[test]
fn compact_completed_escrow_after_rating() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U, 200 * U],
    );
    s.escrow.deposit_funds(&id, &(300 * U));
    s.escrow.release_milestone(&id, &0);
    s.escrow.release_milestone(&id, &1);
    s.escrow.issue_reputation(&id, &5);

    // Should be able to compact a completed escrow after rating
    s.escrow.compact_escrow(&id);
    assert_eq!(
        s.escrow.try_get_contract(&id),
        Err(Ok(EscrowError::EscrowCompacted))
    );

    // Receipt should exist and match the escrow
    let receipt = s.escrow.get_receipt(&id);
    assert!(receipt.is_some());
    let r = receipt.unwrap();
    assert_eq!(r.id, id);
    assert_eq!(r.status, ContractStatus::Completed);
    assert_eq!(r.total_amount, 300 * U);
    assert_eq!(r.released_amount, 300 * U);
    assert_eq!(r.refunded_amount, 0);
    assert_eq!(r.milestone_count, 2);
    assert!(r.reputation_issued);
}

#[test]
fn compact_completed_escrow_after_30_days() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.release_milestone(&id, &0);

    // Cannot compact immediately without rating
    assert_eq!(
        s.escrow.try_compact_escrow(&id),
        Err(Ok(EscrowError::NotCompactable))
    );

    // Advance 30 days in ledgers
    s.env.ledger().set_sequence_number(518_400u32);

    // Now should be able to compact
    s.escrow.compact_escrow(&id);
    assert_eq!(
        s.escrow.try_get_contract(&id),
        Err(Ok(EscrowError::EscrowCompacted))
    );

    let receipt = s.escrow.get_receipt(&id);
    assert!(receipt.is_some());
    let r = receipt.unwrap();
    assert_eq!(r.status, ContractStatus::Completed);
    assert!(!r.reputation_issued);
}

#[test]
fn cannot_compact_live_escrow() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );

    // Created status cannot be compacted
    assert_eq!(
        s.escrow.try_compact_escrow(&id),
        Err(Ok(EscrowError::NotCompactable))
    );

    s.escrow.deposit_funds(&id, &(100 * U));

    // Funded status cannot be compacted
    assert_eq!(
        s.escrow.try_compact_escrow(&id),
        Err(Ok(EscrowError::NotCompactable))
    );

    s.escrow.open_dispute(&id, &s.freelancer);

    // Disputed status cannot be compacted
    assert_eq!(
        s.escrow.try_compact_escrow(&id),
        Err(Ok(EscrowError::NotCompactable))
    );
}

#[test]
fn cannot_compact_completed_before_30_days_without_rating() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.release_milestone(&id, &0);

    // Cannot compact without rating and before 30 days
    assert_eq!(
        s.escrow.try_compact_escrow(&id),
        Err(Ok(EscrowError::NotCompactable))
    );

    // Advance 29 days (just under the threshold)
    s.env.ledger().set_sequence_number(518_400u32 - 1);

    // Still cannot compact
    assert_eq!(
        s.escrow.try_compact_escrow(&id),
        Err(Ok(EscrowError::NotCompactable))
    );
}

#[test]
fn cannot_double_compact() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.cancel_contract(&id);

    // First compaction succeeds
    s.escrow.compact_escrow(&id);

    // Second compaction fails - receipt already exists
    assert_eq!(
        s.escrow.try_compact_escrow(&id),
        Err(Ok(EscrowError::NotCompactable))
    );
}

#[test]
fn receipt_fields_match_escrow_before_compaction() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U, 200 * U, 300 * U],
    );
    s.escrow.deposit_funds(&id, &(600 * U));
    s.escrow.release_milestone(&id, &0);
    s.escrow.release_milestone(&id, &1);

    // Cancel to make it terminal
    s.escrow.cancel_contract(&id);

    // Get the escrow after cancellation (it has the final state)
    let escrow = s.escrow.get_contract(&id);

    // Compact
    s.escrow.compact_escrow(&id);

    // Check receipt matches
    let receipt = s.escrow.get_receipt(&id).unwrap();
    assert_eq!(receipt.id, escrow.id);
    assert_eq!(receipt.client, escrow.client);
    assert_eq!(receipt.freelancer, escrow.freelancer);
    assert_eq!(receipt.token, escrow.token);
    assert_eq!(receipt.total_amount, escrow.total_amount);
    assert_eq!(receipt.released_amount, escrow.released_amount);
    assert_eq!(receipt.refunded_amount, escrow.refunded_amount);
    assert_eq!(receipt.status, escrow.status);
    assert_eq!(receipt.milestone_count, escrow.milestones.len() as u32);
    assert_eq!(receipt.reputation_issued, escrow.reputation_issued);
}

#[test]
fn issue_reputation_on_compacted_unrated_escrow_before_30_days() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.release_milestone(&id, &0);

    // Try to compact before 30 days - should fail
    assert_eq!(
        s.escrow.try_compact_escrow(&id),
        Err(Ok(EscrowError::NotCompactable))
    );

    // Should still be able to issue reputation
    s.escrow.issue_reputation(&id, &5);

    // Now compaction should succeed
    s.escrow.compact_escrow(&id);
    assert_eq!(
        s.escrow.try_get_contract(&id),
        Err(Ok(EscrowError::EscrowCompacted))
    );
}

#[test]
fn get_contract_returns_escrow_compacted() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.cancel_contract(&id);
    s.escrow.compact_escrow(&id);

    // get_contract should return EscrowCompacted error
    assert_eq!(
        s.escrow.try_get_contract(&id),
        Err(Ok(EscrowError::EscrowCompacted))
    );
}

#[test]
fn get_receipt_returns_none_for_non_compacted() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );

    // Receipt should not exist for non-compacted escrow
    let receipt = s.escrow.get_receipt(&id);
    assert!(receipt.is_none());
}


#[test]
fn compact_resolved_unrated_escrow_after_30_days() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.open_dispute(&id, &s.freelancer);
    s.escrow.resolve_dispute(&s.admin, &id, &5_000);

    s.env.ledger().set_sequence_number(THIRTY_DAYS_IN_LEDGERS - 1);
    assert_eq!(
        s.escrow.try_compact_escrow(&id),
        Err(Ok(EscrowError::NotCompactable))
    );
    s.env.ledger().set_sequence_number(THIRTY_DAYS_IN_LEDGERS);
    s.escrow.compact_escrow(&id);
    assert_eq!(
        s.escrow.try_issue_reputation(&id, &5),
        Err(Ok(EscrowError::EscrowCompacted))
    );
    assert_eq!(s.escrow.get_receipt(&id).unwrap().closed_at, 0);
}

// ---------------------------------------------------------------------------
// Team payout splits
// ---------------------------------------------------------------------------

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
    s.escrow.open_dispute(&id, &s.freelancer);
    s.escrow.resolve_dispute(&s.admin, &id, &2_500);

    // 25% of 4_000_004 is 1_000_001; the split floors to 500_000/300_000/200_000
    // and the lead gets the 1-unit remainder.
    assert_eq!(s.token.balance(&s.freelancer), 500_001);
    assert_eq!(s.token.balance(&teammate_a), 300_000);
    assert_eq!(s.token.balance(&teammate_b), 200_000);
    assert_eq!(s.token.balance(&s.client), 1_000 * U - 1_000_001);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Resolved);
}


#[test]
fn team_split_rejected_on_frozen_escrow() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.freeze_escrow(&id, &symbol_short!("fraud"));
    let valid = vec![&s.env, (s.freelancer.clone(), 10_000u32)];
    assert_eq!(
        s.escrow.try_set_team_split(&id, &valid),
        Err(Ok(EscrowError::EscrowFrozen))
    );
}

// ---------------------------------------------------------------------------
// Milestone and summary read APIs
// ---------------------------------------------------------------------------

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
    s.escrow.open_dispute(&resolved, &s.freelancer);
    s.escrow.resolve_dispute(&s.admin, &resolved, &2_500);
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

// ---------------------------------------------------------------------------
// Freelancers rate clients
// ---------------------------------------------------------------------------

#[test]
fn test_rate_client_completed_success_and_event() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 150 * U],
    );
    s.escrow.deposit_funds(&id, &(150 * U));
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
        &vec![&s.env, 200 * U],
    );
    s.escrow.deposit_funds(&id, &(200 * U));
    s.escrow.open_dispute(&id, &s.freelancer);
    s.escrow.resolve_dispute(&s.admin, &id, &5_000);
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
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
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
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
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
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
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
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id_funded, &(100 * U));
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
        &vec![&s.env, 50 * U],
    );
    s.escrow.deposit_funds(&id_cancelled, &(50 * U));
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
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id1, &(100 * U));
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
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id2, &(100 * U));
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
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.release_milestone(&id, &0);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);

    // Pause contract
    s.escrow.set_paused(&s.admin, &true);
    assert_eq!(
        s.escrow.try_rate_client(&id, &5),
        Err(Ok(EscrowError::Paused))
    );

    // Unpause contract and verify rating succeeds
    s.escrow.set_paused(&s.admin, &false);
    assert_eq!(s.escrow.try_rate_client(&id, &5), Ok(Ok(())));
    let rep = s.escrow.get_client_reputation(&s.client);
    assert_eq!(rep.completed_contracts, 1);
    assert_eq!(rep.total_rating, 5);
}

// ---------------------------------------------------------------------------
// Sequential milestone releases
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Batch milestone releases
// ---------------------------------------------------------------------------

fn three_milestone_escrow(s: &Setup, sequential: bool) -> u64 {
    let amounts = vec![&s.env, 100 * U, 200 * U, 300 * U];
    let id = if sequential {
        s.escrow
            .create_contract_sequential(&s.client, &s.freelancer, &s.token.address, &amounts)
    } else {
        s.escrow
            .create_contract(&s.client, &s.freelancer, &s.token.address, &amounts)
    };
    s.escrow.deposit_funds(&id, &(600 * U));
    id
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

// ---------------------------------------------------------------------------
// Value-weighted reputation
// ---------------------------------------------------------------------------

fn completed_escrow(s: &Setup, amount: i128) -> u64 {
    StellarAssetClient::new(&s.env, &s.token.address).mint(&s.client, &amount);
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, amount],
    );
    s.escrow.deposit_funds(&id, &amount);
    s.escrow.release_milestone(&id, &0);
    id
}

#[test]
fn dust_release_rating_returns_below_threshold() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.open_dispute(&id, &s.client);
    // 1 bps of the held balance reaches the freelancer: far below the threshold
    s.escrow.resolve_dispute(&s.admin, &id, &1);
    assert!(s.escrow.get_contract(&id).released_amount < MIN_REPUTATION_VALUE);
    assert_eq!(
        s.escrow.try_issue_reputation(&id, &5),
        Err(Ok(EscrowError::BelowReputationThreshold))
    );
    assert_eq!(s.escrow.get_reputation(&s.freelancer), Reputation::default());
    assert_eq!(s.escrow.get_reputation_score(&s.freelancer), None);
}

#[test]
fn weighted_score_large_and_small_job() {
    let s = setup();
    // 1 token unit = 10 * U; a 5-star 1,000-unit job and a 1-star 10-unit job.
    let unit = MIN_REPUTATION_VALUE;
    let big = completed_escrow(&s, 1_000 * unit);
    let small = completed_escrow(&s, 10 * unit);
    s.escrow.issue_reputation(&big, &5);
    s.escrow.issue_reputation(&small, &1);
    // (5 * 1000 + 1 * 10) / 1010 = 4.9603 -> 496
    assert_eq!(s.escrow.get_reputation_score(&s.freelancer), Some(496));
    let w = s.escrow.get_weighted_reputation(&s.freelancer);
    assert_eq!(w.rated_contracts, 2);
    assert_eq!(w.weight_total, 1_010 * unit);
    assert_eq!(w.weighted_rating_sum, 5_010 * unit);
    // Legacy reputation keeps incrementing as before
    let rep = s.escrow.get_reputation(&s.freelancer);
    assert_eq!(rep.completed_contracts, 2);
    assert_eq!(rep.total_rating, 6);
}

#[test]
fn weighted_score_rounds_half_up() {
    let s = setup();
    let unit = MIN_REPUTATION_VALUE;
    let a = completed_escrow(&s, 2 * unit);
    let b = completed_escrow(&s, unit);
    s.escrow.issue_reputation(&a, &5);
    s.escrow.issue_reputation(&b, &4);
    // (5 * 2 + 4 * 1) / 3 = 4.6667 -> 467 (truncation would give 466)
    assert_eq!(s.escrow.get_reputation_score(&s.freelancer), Some(467));
}

#[test]
fn reputation_score_is_overflow_safe() {
    assert_eq!(reputation_score(i128::MAX, i128::MAX), Some(100));
    assert_eq!(reputation_score(5 * (i128::MAX / 5), i128::MAX / 5), Some(500));
    let near = reputation_score(i128::MAX, i128::MAX / 2 + 1).unwrap();
    assert!((199..=200).contains(&near));
    assert_eq!(reputation_score(0, 0), None);
    // A rating at the maximum escrow value does not overflow the contract path
    let s = setup();
    StellarAssetClient::new(&s.env, &s.token.address).mint(&s.client, &MAX_ESCROW_TOTAL);
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, MAX_ESCROW_TOTAL],
    );
    s.escrow.deposit_funds(&id, &MAX_ESCROW_TOTAL);
    s.escrow.release_milestone(&id, &0);
    s.escrow.issue_reputation(&id, &5);
    assert_eq!(s.escrow.get_reputation_score(&s.freelancer), Some(500));
}

#[test]
fn voiding_rating_reverses_weighted_reputation() {
    let s = setup();
    let unit = MIN_REPUTATION_VALUE;
    let a = completed_escrow(&s, 1_000 * unit);
    let b = completed_escrow(&s, 10 * unit);
    s.escrow.issue_reputation(&a, &5);
    s.escrow.issue_reputation(&b, &1);
    s.escrow.void_rating(&a, &symbol_short!("fraud"));
    assert_eq!(s.escrow.get_reputation_score(&s.freelancer), Some(100));
    let w = s.escrow.get_weighted_reputation(&s.freelancer);
    assert_eq!(w.rated_contracts, 1);
    assert_eq!(w.weight_total, 10 * unit);
}

#[test]
fn rep_event_includes_weight() {
    let s = setup();
    let id = completed_escrow(&s, 20 * U);
    s.escrow.issue_reputation(&id, &4);
    let events = s.env.events().all();
    let last = events.last().unwrap();
    assert_eq!(
        Symbol::try_from_val(&s.env, &last.1.get(0).unwrap()).unwrap(),
        symbol_short!("rep")
    );
    let (who, rating, weight) = <(Address, u32, i128)>::try_from_val(&s.env, &last.2).unwrap();
    assert_eq!(who, s.freelancer);
    assert_eq!(rating, 4);
    assert_eq!(weight, 20 * U);
}

// ---------------------------------------------------------------------------
// M-of-N arbiter panel
// ---------------------------------------------------------------------------

fn disputed_high_value_escrow(s: &Setup, amount: i128) -> u64 {
    StellarAssetClient::new(&s.env, &s.token.address).mint(&s.client, &amount);
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, amount],
    );
    s.escrow.deposit_funds(&id, &amount);
    s.escrow.open_dispute(&id, &s.client);
    id
}

#[test]
fn panel_two_of_three_different_votes_do_not_settle_until_matching() {
    let s = setup();
    let m1 = Address::generate(&s.env);
    let m2 = Address::generate(&s.env);
    let m3 = Address::generate(&s.env);
    s.escrow
        .set_panel(&vec![&s.env, m1.clone(), m2.clone(), m3.clone()], &2);
    let id = disputed_high_value_escrow(&s, PANEL_THRESHOLD_AMOUNT);
    // A high-value escrow cannot be settled by a single admin call.
    assert_eq!(
        s.escrow.try_resolve_dispute(&s.admin, &id, &5_000),
        Err(Ok(EscrowError::PanelRequired))
    );
    // Two different splits do not settle.
    s.escrow.vote_resolution(&m1, &id, &4_000);
    s.escrow.vote_resolution(&m2, &id, &6_000);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Disputed);
    assert_eq!(s.token.balance(&s.freelancer), 0);
    assert_eq!(s.token.balance(&s.client), 1_000 * U);
    // The second matching vote (m3 joins m1 at 4_000) settles the escrow.
    s.escrow.vote_resolution(&m3, &id, &4_000);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Resolved);
    let to_freelancer = PANEL_THRESHOLD_AMOUNT * 4_000 / BPS_DENOMINATOR;
    assert_eq!(s.token.balance(&s.freelancer), to_freelancer);
    assert_eq!(
        s.token.balance(&s.client),
        1_000 * U + PANEL_THRESHOLD_AMOUNT - to_freelancer
    );
    // Votes are cleared once the escrow settles.
    assert_eq!(s.escrow.get_votes(&id).len(), 0);
}

#[test]
fn vote_change_replaces_vote_and_requires_membership() {
    let s = setup();
    let m1 = Address::generate(&s.env);
    let m2 = Address::generate(&s.env);
    let m3 = Address::generate(&s.env);
    s.escrow
        .set_panel(&vec![&s.env, m1.clone(), m2.clone(), m3.clone()], &2);
    let id = disputed_high_value_escrow(&s, PANEL_THRESHOLD_AMOUNT);

    // A non-member cannot vote.
    let outsider = Address::generate(&s.env);
    assert_eq!(
        s.escrow.try_vote_resolution(&outsider, &id, &5_000),
        Err(Ok(EscrowError::Unauthorized))
    );
    // A split above 100% is rejected.
    assert_eq!(
        s.escrow.try_vote_resolution(&m1, &id, &10_001),
        Err(Ok(EscrowError::InvalidSplit))
    );
    // Parties to the escrow cannot vote, even if on the panel.
    s.escrow
        .set_panel(&vec![&s.env, s.client.clone(), m1.clone(), m2.clone()], &2);
    assert_eq!(
        s.escrow.try_vote_resolution(&s.client, &id, &5_000),
        Err(Ok(EscrowError::Unauthorized))
    );
    assert_eq!(
        s.escrow.try_vote_resolution(&s.freelancer, &id, &5_000),
        Err(Ok(EscrowError::Unauthorized))
    );

    // Changing a vote replaces it: the first vote no longer counts.
    s.escrow.vote_resolution(&m1, &id, &5_000);
    s.escrow.vote_resolution(&m1, &id, &7_000);
    let votes = s.escrow.get_votes(&id);
    assert_eq!(votes.len(), 1);
    assert_eq!(votes.get(m1.clone()), Some(7_000));
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Disputed);

    // m2 matching m1's new vote settles at 7_000.
    s.escrow.vote_resolution(&m2, &id, &7_000);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Resolved);
    let to_freelancer = PANEL_THRESHOLD_AMOUNT * 7_000 / BPS_DENOMINATOR;
    assert_eq!(s.token.balance(&s.freelancer), to_freelancer);
    assert_eq!(
        s.token.balance(&s.client),
        1_000 * U + PANEL_THRESHOLD_AMOUNT - to_freelancer
    );
}

#[test]
fn panel_change_mid_dispute_ignores_removed_members_votes() {
    let s = setup();
    let m1 = Address::generate(&s.env);
    let m2 = Address::generate(&s.env);
    let m3 = Address::generate(&s.env);
    let m4 = Address::generate(&s.env);
    s.escrow
        .set_panel(&vec![&s.env, m1.clone(), m2.clone(), m3.clone()], &2);
    let id = disputed_high_value_escrow(&s, PANEL_THRESHOLD_AMOUNT);
    s.escrow.vote_resolution(&m1, &id, &3_000);
    // Rule: when the panel changes, votes from removed members stop counting.
    s.escrow
        .set_panel(&vec![&s.env, m2.clone(), m3.clone(), m4.clone()], &2);
    s.escrow.vote_resolution(&m2, &id, &3_000);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Disputed);
    assert_eq!(s.token.balance(&s.freelancer), 0);
    // A second vote from a current member reaches the threshold.
    s.escrow.vote_resolution(&m3, &id, &3_000);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Resolved);
    let to_freelancer = PANEL_THRESHOLD_AMOUNT * 3_000 / BPS_DENOMINATOR;
    assert_eq!(s.token.balance(&s.freelancer), to_freelancer);
    assert_eq!(
        s.token.balance(&s.client),
        1_000 * U + PANEL_THRESHOLD_AMOUNT - to_freelancer
    );
}

#[test]
fn below_threshold_escrow_still_settles_via_resolve_dispute() {
    let s = setup();
    let m1 = Address::generate(&s.env);
    let m2 = Address::generate(&s.env);
    s.escrow.set_panel(&vec![&s.env, m1, m2], &2);
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 400 * U],
    );
    s.escrow.deposit_funds(&id, &(400 * U));
    s.escrow.open_dispute(&id, &s.freelancer);
    s.escrow.resolve_dispute(&s.admin, &id, &2_500);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Resolved);
    assert_eq!(s.token.balance(&s.freelancer), 100 * U);
    assert_eq!(s.token.balance(&s.client), 900 * U);
}

#[test]
fn set_panel_validates_threshold_and_members() {
    let s = setup();
    let m1 = Address::generate(&s.env);
    let m2 = Address::generate(&s.env);
    assert_eq!(
        s.escrow
            .try_set_panel(&vec![&s.env, m1.clone(), m2.clone()], &0),
        Err(Ok(EscrowError::InvalidPanel))
    );
    assert_eq!(
        s.escrow.try_set_panel(&vec![&s.env, m1.clone()], &2),
        Err(Ok(EscrowError::InvalidPanel))
    );
    let too_many = vec![
        &s.env,
        Address::generate(&s.env),
        Address::generate(&s.env),
        Address::generate(&s.env),
        Address::generate(&s.env),
        Address::generate(&s.env),
        Address::generate(&s.env),
        Address::generate(&s.env),
        Address::generate(&s.env),
    ];
    assert_eq!(
        s.escrow.try_set_panel(&too_many, &2),
        Err(Ok(EscrowError::InvalidPanel))
    );
    assert_eq!(
        s.escrow
            .try_set_panel(&vec![&s.env, m1.clone(), m2.clone(), m1.clone()], &2),
        Err(Ok(EscrowError::InvalidPanel))
    );
    s.escrow
        .set_panel(&vec![&s.env, m1.clone(), m2.clone()], &2);
    let panel = s.escrow.get_panel().unwrap();
    assert_eq!(panel.threshold, 2);
    assert_eq!(panel.members.len(), 2);
}


#[test]
fn panel_settlement_records_outcomes_and_liability() {
    let s = setup();
    let m1 = Address::generate(&s.env);
    let m2 = Address::generate(&s.env);
    s.escrow.set_panel(&vec![&s.env, m1.clone(), m2.clone()], &2);
    let id = disputed_high_value_escrow(&s, PANEL_THRESHOLD_AMOUNT);
    s.escrow.vote_resolution(&m1, &id, &6_000);
    s.escrow.vote_resolution(&m2, &id, &6_000);
    let c = s.escrow.get_contract(&id);
    assert_eq!(c.status, ContractStatus::Resolved);
    check_invariants(&c);
    assert_eq!(s.escrow.get_liability(&s.token.address), 0);
    assert_eq!(s.escrow.get_outcomes(&s.freelancer).disputes_won, 1);
    assert_eq!(s.escrow.get_outcomes(&s.client).disputes_lost, 1);
}

// ---------------------------------------------------------------------------
// Pauser and resolver roles
// ---------------------------------------------------------------------------

#[test]
fn pauser_can_pause_but_cannot_resolve_disputes() {
    let s = setup();
    let pauser = Address::generate(&s.env);
    s.escrow.grant_role(&Role::Pauser, &pauser);
    assert!(s.escrow.has_role(&Role::Pauser, &pauser));

    s.escrow
        .mock_auths(&[MockAuth {
            address: &pauser,
            invoke: &MockAuthInvoke {
                contract: &s.escrow.address,
                fn_name: "set_paused",
                args: (&pauser, true).into_val(&s.env),
                sub_invokes: &[],
            },
        }])
        .set_paused(&pauser, &true);
    assert!(s.escrow.is_paused());

    s.escrow
        .mock_auths(&[MockAuth {
            address: &pauser,
            invoke: &MockAuthInvoke {
                contract: &s.escrow.address,
                fn_name: "set_paused",
                args: (&pauser, false).into_val(&s.env),
                sub_invokes: &[],
            },
        }])
        .set_paused(&pauser, &false);
    assert!(!s.escrow.is_paused());

    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.open_dispute(&id, &s.freelancer);

    let result = s
        .escrow
        .mock_auths(&[MockAuth {
            address: &pauser,
            invoke: &MockAuthInvoke {
                contract: &s.escrow.address,
                fn_name: "resolve_dispute",
                args: (&pauser, id, 5_000u32).into_val(&s.env),
                sub_invokes: &[],
            },
        }])
        .try_resolve_dispute(&pauser, &id, &5_000);
    assert_eq!(result, Err(Ok(EscrowError::Unauthorized)));
}

#[test]
fn resolver_can_resolve_but_cannot_pause() {
    let s = setup();
    let resolver = Address::generate(&s.env);
    s.escrow.grant_role(&Role::Resolver, &resolver);
    assert!(s.escrow.has_role(&Role::Resolver, &resolver));

    let pause_result = s
        .escrow
        .mock_auths(&[MockAuth {
            address: &resolver,
            invoke: &MockAuthInvoke {
                contract: &s.escrow.address,
                fn_name: "set_paused",
                args: (&resolver, true).into_val(&s.env),
                sub_invokes: &[],
            },
        }])
        .try_set_paused(&resolver, &true);
    assert_eq!(pause_result, Err(Ok(EscrowError::Unauthorized)));

    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.open_dispute(&id, &s.freelancer);

    s.escrow
        .mock_auths(&[MockAuth {
            address: &resolver,
            invoke: &MockAuthInvoke {
                contract: &s.escrow.address,
                fn_name: "resolve_dispute",
                args: (&resolver, id, 10_000u32).into_val(&s.env),
                sub_invokes: &[],
            },
        }])
        .resolve_dispute(&resolver, &id, &10_000);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Resolved);
    assert_eq!(s.token.balance(&s.freelancer), 100 * U);
}

#[test]
fn revoking_pauser_role_takes_effect_immediately() {
    let s = setup();
    let pauser = Address::generate(&s.env);
    s.escrow.grant_role(&Role::Pauser, &pauser);
    s.escrow.revoke_role(&Role::Pauser, &pauser);
    assert!(!s.escrow.has_role(&Role::Pauser, &pauser));

    let result = s
        .escrow
        .mock_auths(&[MockAuth {
            address: &pauser,
            invoke: &MockAuthInvoke {
                contract: &s.escrow.address,
                fn_name: "set_paused",
                args: (&pauser, true).into_val(&s.env),
                sub_invokes: &[],
            },
        }])
        .try_set_paused(&pauser, &true);
    assert_eq!(result, Err(Ok(EscrowError::Unauthorized)));
}

#[test]
fn resolver_who_is_a_party_cannot_resolve_that_escrow() {
    let s = setup();
    s.escrow.grant_role(&Role::Resolver, &s.client);

    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.open_dispute(&id, &s.freelancer);

    let result = s
        .escrow
        .mock_auths(&[MockAuth {
            address: &s.client,
            invoke: &MockAuthInvoke {
                contract: &s.escrow.address,
                fn_name: "resolve_dispute",
                args: (&s.client, id, 5_000u32).into_val(&s.env),
                sub_invokes: &[],
            },
        }])
        .try_resolve_dispute(&s.client, &id, &5_000);
    assert_eq!(result, Err(Ok(EscrowError::Unauthorized)));
}


#[test]
fn grant_and_revoke_role_unchanged_and_admin_only() {
    let s = setup();
    let account = Address::generate(&s.env);
    assert!(s.escrow.has_role(&Role::Pauser, &s.admin));
    assert!(s.escrow.has_role(&Role::Resolver, &s.admin));
    assert!(!s.escrow.has_role(&Role::Pauser, &account));
    assert_eq!(
        s.escrow.try_revoke_role(&Role::Pauser, &account),
        Err(Ok(EscrowError::RoleUnchanged))
    );
    s.escrow.grant_role(&Role::Pauser, &account);
    assert_eq!(
        s.escrow.try_grant_role(&Role::Pauser, &account),
        Err(Ok(EscrowError::RoleUnchanged))
    );
    // Roles are independent
    assert!(!s.escrow.has_role(&Role::Resolver, &account));
    // A role holder cannot grant roles: without the admin's signature it fails
    s.env.mock_auths(&[]);
    assert!(s
        .escrow
        .try_grant_role(&Role::Resolver, &account)
        .is_err());
    s.env.mock_all_auths();
    assert!(!s.escrow.has_role(&Role::Resolver, &account));
}

// ---------------------------------------------------------------------------
// Sponsor deposits with pro-rata refunds
// ---------------------------------------------------------------------------

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

    let client_before = token.balance(&client);
    let sponsor_before = token.balance(&sponsor);
    let freelancer_before = token.balance(&freelancer);
    let contract_before = token.balance(&contract_addr);

    // freelancer opens dispute, admin resolves 25% to freelancer
    escrow.open_dispute(&id, &freelancer);
    escrow.resolve_dispute(&admin, &id, &2_500);

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
fn deposit_from_respects_freeze_and_status() {
    let s = setup();
    let sponsor = Address::generate(&s.env);
    StellarAssetClient::new(&s.env, &s.token.address).mint(&sponsor, &(100 * U));
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50 * U],
    );
    s.escrow.freeze_escrow(&id, &symbol_short!("fraud"));
    assert_eq!(
        s.escrow.try_deposit_from(&id, &sponsor, &(10 * U)),
        Err(Ok(EscrowError::EscrowFrozen))
    );
    s.escrow.unfreeze_escrow(&id);
    s.escrow.deposit_from(&id, &sponsor, &(50 * U));
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Funded);
    assert_eq!(
        s.escrow.try_deposit_from(&id, &sponsor, &U),
        Err(Ok(EscrowError::InvalidStatus))
    );
    assert_eq!(s.escrow.get_liability(&s.token.address), 50 * U);
}

// ---------------------------------------------------------------------------
// Funding deadline and expiry
// ---------------------------------------------------------------------------

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
fn test_expire_unfunded_pause_gate_and_status_checks() {
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

    // Pause blocks expire_unfunded
    s.escrow.set_paused(&s.admin, &true);
    assert_eq!(
        s.escrow.try_expire_unfunded(&id),
        Err(Ok(EscrowError::Paused))
    );

    // Unpause allows expire_unfunded
    s.escrow.set_paused(&s.admin, &false);
    assert_eq!(s.escrow.try_expire_unfunded(&id), Ok(Ok(())));

    // Re-expiring an already cancelled escrow fails with InvalidStatus
    assert_eq!(
        s.escrow.try_expire_unfunded(&id),
        Err(Ok(EscrowError::InvalidStatus))
    );

    // Depositing into cancelled escrow fails with InvalidStatus
    assert_eq!(
        s.escrow.try_deposit_funds(&id, &(50 * U)),
        Err(Ok(EscrowError::InvalidStatus))
    );

    // Missing storage for get_fund_by returns 0 default
    assert_eq!(s.escrow.get_fund_by(&999), 0);
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

// ---------------------------------------------------------------------------
// Change orders
// ---------------------------------------------------------------------------

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
    s.escrow.open_dispute(&id, &s.client);
    assert_eq!(
        s.escrow.try_propose_change_order(&id, &vec![&s.env, 10 * U]),
        Err(Ok(EscrowError::InvalidStatus))
    );
}

// ---------------------------------------------------------------------------
// Partial milestone releases
// ---------------------------------------------------------------------------

fn assert_invariants(s: &Setup, c: &EscrowContract) {
    assert!(c.released_amount <= c.funded_amount);
    let mut total_paid: i128 = 0;
    for (i, m) in c.milestones.iter().enumerate() {
        let paid: i128 = s.escrow.get_milestone_paid(&c.id, &(i as u32));
        assert!(paid <= m.amount, "milestone {i} paid exceeds amount");
        if m.released {
            assert_eq!(paid, m.amount, "milestone {i} released but underpaid");
        }
        total_paid = total_paid.checked_add(paid).unwrap();
    }
    assert!(total_paid <= c.total_amount);
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

    s.escrow.open_dispute(&id, &s.freelancer);
    s.escrow.resolve_dispute(&s.admin, &id, &2_500);
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

// ---------------------------------------------------------------------------
// Atomic create_and_fund
// ---------------------------------------------------------------------------

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
    let mine: std::vec::Vec<_> = s
        .env
        .events()
        .all()
        .iter()
        .filter(|e| e.0 == s.escrow.address)
        .collect();
    let n = mine.len();
    assert!(n >= 2);
    let created = &mine[n - 2];
    let deposit = &mine[n - 1];
    assert_eq!(
        Symbol::try_from_val(&s.env, &created.1.get(0).unwrap()).unwrap(),
        symbol_short!("created")
    );
    assert_eq!(u64::try_from_val(&s.env, &created.1.get(1).unwrap()).unwrap(), id);
    let (c, f, total) = <(Address, Address, i128)>::try_from_val(&s.env, &created.2).unwrap();
    assert_eq!((c, f, total), (s.client.clone(), s.freelancer.clone(), 150 * U));
    assert_eq!(
        Symbol::try_from_val(&s.env, &deposit.1.get(0).unwrap()).unwrap(),
        symbol_short!("deposit")
    );
    let (funder, amount) = <(Address, i128)>::try_from_val(&s.env, &deposit.2).unwrap();
    assert_eq!((funder, amount), (s.client.clone(), 150 * U));
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
