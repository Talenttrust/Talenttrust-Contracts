extern crate std;

use super::*;
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, token::TokenClient, vec};

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

// ---------------------------------------------------------------------------
// Existing tests (unchanged)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Platform fee tests (issue #1682)
// ---------------------------------------------------------------------------

/// Helper: set up a fresh environment with a treasury account and a known
/// starting balance for the client (1_000 units).
struct FeeSetup<'a> {
    env: Env,
    admin: Address,
    treasury: Address,
    escrow: EscrowClient<'a>,
    token: TokenClient<'a>,
    client: Address,
    freelancer: Address,
}

fn fee_setup<'a>() -> FeeSetup<'a> {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let treasury = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    StellarAssetClient::new(&env, &sac.address()).mint(&client, &1_000);
    let contract_id = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &contract_id);
    escrow.initialize(&admin);
    let token = TokenClient::new(&env, &sac.address());
    FeeSetup {
        env,
        admin,
        treasury,
        escrow,
        token,
        client,
        freelancer,
    }
}

/// AC-1: Escrow created at 100 bps; admin raises to 300 bps; release still
/// charges 100 bps. Balances of freelancer, treasury, and contract are asserted.
/// Conservation: freelancer + treasury == escrow total.
#[test]
fn fee_snapshot_is_immutable_after_admin_raises_rate() {
    let s = fee_setup();

    // Set fee to 100 bps at creation time.
    s.escrow.set_fee(&100u32, &s.treasury);
    assert_eq!(s.escrow.get_fee_config(), (100u32, Some(s.treasury.clone())));

    let amount: i128 = 1_000;
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, amount],
    );
    // Snapshot must reflect 100 bps.
    assert_eq!(s.escrow.get_escrow_fee(&id), 100u32);

    // Admin raises fee to 300 bps — must NOT affect this escrow.
    s.escrow.set_fee(&300u32, &s.treasury);

    s.escrow.deposit_funds(&id, &amount);
    let contract_before = s.token.balance(&s.escrow.address);
    assert_eq!(contract_before, amount);

    s.escrow.release_milestone(&id, &0);

    // fee = 1_000 * 100 / 10_000 = 10
    let expected_fee: i128 = 10;
    let expected_freelancer: i128 = amount - expected_fee;

    assert_eq!(s.token.balance(&s.freelancer), expected_freelancer);
    assert_eq!(s.token.balance(&s.treasury), expected_fee);
    assert_eq!(s.token.balance(&s.escrow.address), 0);

    // Conservation: freelancer received + treasury received == total deposited.
    assert_eq!(expected_freelancer + expected_fee, amount);
}

/// AC-2: Fee rounding on a 1-unit amount with nonzero bps.
/// 1 * 100 / 10_000 = 0 (integer division) → fee is 0, freelancer gets everything.
#[test]
fn fee_rounds_down_to_zero_on_unit_amount() {
    let s = fee_setup();
    s.escrow.set_fee(&100u32, &s.treasury);

    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 1i128],
    );
    s.escrow.deposit_funds(&id, &1);
    s.escrow.release_milestone(&id, &0);

    // fee = 1 * 100 / 10_000 = 0 (rounds down)
    assert_eq!(s.token.balance(&s.freelancer), 1);
    assert_eq!(s.token.balance(&s.treasury), 0);
    // Conservation
    assert_eq!(s.token.balance(&s.freelancer) + s.token.balance(&s.treasury), 1);
}

/// AC-3a: Dispute split charges fee only on the freelancer's gross share;
/// the client's refund is untouched.
/// Conservation: freelancer_net + treasury + client_refund == total held.
#[test]
fn dispute_fee_applies_only_to_freelancer_share() {
    let s = fee_setup();
    s.escrow.set_fee(&200u32, &s.treasury); // 2 %

    let amount: i128 = 1_000;
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, amount],
    );
    s.escrow.deposit_funds(&id, &amount);
    s.escrow.open_dispute(&id, &s.freelancer);

    // freelancer_bps = 5_000 → gross_freelancer = 500, client_refund = 500
    // fee on freelancer = 500 * 200 / 10_000 = 10
    // to_freelancer = 490, treasury = 10, to_client = 500
    s.escrow.resolve_dispute(&id, &5_000);

    let gross_freelancer: i128 = 500;
    let fee: i128 = 10;
    let to_freelancer = gross_freelancer - fee;
    let to_client: i128 = 500;

    assert_eq!(s.token.balance(&s.freelancer), to_freelancer);
    assert_eq!(s.token.balance(&s.treasury), fee);
    // client started with 1_000, deposited 1_000, got 500 back
    assert_eq!(s.token.balance(&s.client), to_client);
    assert_eq!(s.token.balance(&s.escrow.address), 0);

    // Conservation
    assert_eq!(to_freelancer + fee + to_client, amount);
}

/// AC-3b: cancel_contract refunds carry no fee.
#[test]
fn cancel_charges_no_fee() {
    let s = fee_setup();
    s.escrow.set_fee(&300u32, &s.treasury); // 3 %

    let amount: i128 = 500;
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, amount],
    );
    s.escrow.deposit_funds(&id, &amount);

    let client_before = s.token.balance(&s.client);
    s.escrow.cancel_contract(&id);

    // Full refund — no fee deducted.
    assert_eq!(s.token.balance(&s.client), client_before + amount);
    assert_eq!(s.token.balance(&s.treasury), 0);
    assert_eq!(s.token.balance(&s.escrow.address), 0);
}

/// AC-4a: set_fee above MAX_FEE_BPS (500) returns FeeTooHigh.
#[test]
fn set_fee_above_cap_returns_fee_too_high() {
    let s = fee_setup();
    assert_eq!(
        s.escrow.try_set_fee(&501u32, &s.treasury),
        Err(Ok(EscrowError::FeeTooHigh))
    );
    // Exactly at the cap must succeed.
    s.escrow.set_fee(&500u32, &s.treasury);
    assert_eq!(s.escrow.get_fee_config(), (500u32, Some(s.treasury.clone())));
}

/// AC-4b: non-admin calling set_fee is rejected.
#[test]
fn non_admin_cannot_set_fee() {
    let s = fee_setup();
    let non_admin = Address::generate(&s.env);
    s.env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &non_admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &s.escrow.address,
            fn_name: "set_fee",
            args: soroban_sdk::IntoVal::into_val(&(100u32, s.treasury.clone()), &s.env),
            sub_invokes: &[],
        },
    }]);
    assert!(s.escrow.try_set_fee(&100u32, &s.treasury).is_err());
}

/// AC-5 (conservation): release across two milestones — freelancer + treasury
/// + remaining contract balance == original deposit.
#[test]
fn conservation_multi_milestone_release() {
    let s = fee_setup();
    s.escrow.set_fee(&250u32, &s.treasury); // 2.5 %

    // Two milestones: 400 and 600
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 400i128, 600i128],
    );
    s.escrow.deposit_funds(&id, &1_000);

    s.escrow.release_milestone(&id, &0);
    s.escrow.release_milestone(&id, &1);

    // fee per milestone: 400*250/10_000=10, 600*250/10_000=15
    let expected_fee: i128 = 10 + 15;
    let expected_freelancer: i128 = (400 - 10) + (600 - 15);

    assert_eq!(s.token.balance(&s.freelancer), expected_freelancer);
    assert_eq!(s.token.balance(&s.treasury), expected_fee);
    assert_eq!(s.token.balance(&s.escrow.address), 0);
    assert_eq!(expected_freelancer + expected_fee, 1_000);
}

/// Zero-fee (default): existing behaviour is preserved — freelancer gets 100 %,
/// treasury receives nothing.
#[test]
fn zero_fee_default_preserves_existing_behaviour() {
    let s = fee_setup();
    // No set_fee call — default is 0.
    let (bps, treasury) = s.escrow.get_fee_config();
    assert_eq!(bps, 0u32);
    assert!(treasury.is_none());

    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 300i128],
    );
    s.escrow.deposit_funds(&id, &300);
    s.escrow.release_milestone(&id, &0);

    assert_eq!(s.token.balance(&s.freelancer), 300);
}

/// get_escrow_fee defaults to 0 for escrows created before set_fee was called.
#[test]
fn get_escrow_fee_defaults_to_zero() {
    let s = fee_setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50i128],
    );
    assert_eq!(s.escrow.get_escrow_fee(&id), 0u32);

    // Now raise fee; snapshot for old escrow must remain 0.
    s.escrow.set_fee(&100u32, &s.treasury);
    assert_eq!(s.escrow.get_escrow_fee(&id), 0u32);
}
