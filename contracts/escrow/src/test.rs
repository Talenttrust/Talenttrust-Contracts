extern crate std;

use super::*;
use soroban_sdk::{
    testutils::Address as _, token::StellarAssetClient, token::TokenClient, vec, InvokeError,
};

struct Setup<'a> {
    env: Env,
    escrow: EscrowClient<'a>,
    token: TokenClient<'a>,
    client: Address,
    freelancer: Address,
    contract_id: Address,
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
        contract_id: id,
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

/// Every pause category bit, in the order used by the table-driven test.
const PAUSE_CATEGORIES: [u32; 6] = [
    PAUSE_CREATE,
    PAUSE_DEPOSIT,
    PAUSE_RELEASE,
    PAUSE_DISPUTE,
    PAUSE_CANCEL,
    PAUSE_REPUTATION,
];

/// Asserts that a mutating call is blocked with `Paused` when `expect_paused`
/// is true and succeeds otherwise.
fn assert_gate<T: std::fmt::Debug, E: std::fmt::Debug>(
    label: &str,
    res: Result<Result<T, E>, Result<EscrowError, InvokeError>>,
    expect_paused: bool,
) {
    match res {
        Err(Ok(EscrowError::Paused)) => {
            assert!(expect_paused, "{label}: unexpectedly blocked with Paused")
        }
        Ok(Ok(_)) => assert!(!expect_paused, "{label}: unexpectedly succeeded"),
        other => panic!("{label}: unexpected result {other:?}"),
    }
}

/// Creates a contract with a single milestone and funds it.
fn funded_contract(s: &Setup<'_>, amount: i128) -> u64 {
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, amount],
    );
    s.escrow.deposit_funds(&id, &amount);
    id
}

/// Creates a contract with two milestones and funds only the first one, so a
/// further deposit is still possible.
fn partially_funded_contract(s: &Setup<'_>, milestone: i128, funded: i128) -> u64 {
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, milestone, milestone],
    );
    s.escrow.deposit_funds(&id, &funded);
    id
}

/// Creates a contract, funds it and releases its only milestone.
fn completed_contract(s: &Setup<'_>, amount: i128) -> u64 {
    let id = funded_contract(s, amount);
    s.escrow.release_milestone(&id, &0);
    id
}

#[test]
fn pause_blocks_issue_reputation() {
    let s = setup();
    let id = completed_contract(&s, 100);

    s.escrow.set_paused(&true);
    assert_eq!(
        s.escrow.try_issue_reputation(&id, &5),
        Err(Ok(EscrowError::Paused))
    );
    // No reputation was written while paused.
    assert_eq!(
        s.escrow.get_reputation(&s.freelancer).completed_contracts,
        0
    );

    // Unpausing restores the reputation write.
    s.escrow.set_paused(&false);
    s.escrow.issue_reputation(&id, &5);
    assert_eq!(
        s.escrow.get_reputation(&s.freelancer).completed_contracts,
        1
    );
}

#[test]
fn pause_flags_gate_only_their_own_category() {
    for flag in PAUSE_CATEGORIES {
        let s = setup();

        // State prepared before the pause is applied.
        let deposit_id = partially_funded_contract(&s, 100, 100);
        let release_id = funded_contract(&s, 100);
        let dispute_id = funded_contract(&s, 100);
        let cancel_id = funded_contract(&s, 100);
        let reputation_id = completed_contract(&s, 100);

        s.escrow.set_pause_flags(&flag);
        assert_eq!(s.escrow.get_pause_flags(), flag);
        assert!(s.escrow.is_paused());

        assert_gate(
            "create_contract",
            s.escrow.try_create_contract(
                &s.client,
                &s.freelancer,
                &s.token.address,
                &vec![&s.env, 10i128],
            ),
            flag == PAUSE_CREATE,
        );
        assert_gate(
            "deposit_funds",
            s.escrow.try_deposit_funds(&deposit_id, &100),
            flag == PAUSE_DEPOSIT,
        );
        assert_gate(
            "release_milestone",
            s.escrow.try_release_milestone(&release_id, &0),
            flag == PAUSE_RELEASE,
        );
        assert_gate(
            "open_dispute",
            s.escrow.try_open_dispute(&dispute_id, &s.client),
            flag == PAUSE_DISPUTE,
        );
        assert_gate(
            "cancel_contract",
            s.escrow.try_cancel_contract(&cancel_id),
            flag == PAUSE_CANCEL,
        );
        assert_gate(
            "issue_reputation",
            s.escrow.try_issue_reputation(&reputation_id, &5),
            flag == PAUSE_REPUTATION,
        );
    }
}

#[test]
fn inflow_pause_still_allows_cancel_refund() {
    let s = setup();
    let cancel_id = funded_contract(&s, 300);
    let dispute_id = funded_contract(&s, 200);
    let balance_before = s.token.balance(&s.client);

    s.escrow.set_pause_flags(&(PAUSE_CREATE | PAUSE_DEPOSIT));
    assert_eq!(s.escrow.get_pause_flags(), PAUSE_CREATE | PAUSE_DEPOSIT);

    // New inflows are frozen.
    assert_eq!(
        s.escrow.try_create_contract(
            &s.client,
            &s.freelancer,
            &s.token.address,
            &vec![&s.env, 10i128]
        ),
        Err(Ok(EscrowError::Paused))
    );
    assert_eq!(
        s.escrow.try_deposit_funds(&cancel_id, &10),
        Err(Ok(EscrowError::Paused))
    );

    // Users can still exit: cancel refunds the client and dispute still opens.
    s.escrow.cancel_contract(&cancel_id);
    assert_eq!(s.token.balance(&s.client), balance_before + 300);
    assert_eq!(
        s.escrow.get_contract(&cancel_id).status,
        ContractStatus::Cancelled
    );
    s.escrow.open_dispute(&dispute_id, &s.client);
    assert_eq!(
        s.escrow.get_contract(&dispute_id).status,
        ContractStatus::Disputed
    );
}

#[test]
fn set_pause_flags_rejects_unknown_bits() {
    let s = setup();

    assert_eq!(
        s.escrow.try_set_pause_flags(&64),
        Err(Ok(EscrowError::InvalidAmount))
    );
    assert_eq!(
        s.escrow.try_set_pause_flags(&(PAUSE_ALL | 1 << 10)),
        Err(Ok(EscrowError::InvalidAmount))
    );
    assert_eq!(
        s.escrow.try_set_pause_flags(&u32::MAX),
        Err(Ok(EscrowError::InvalidAmount))
    );

    // A rejected update leaves the pause state untouched.
    assert_eq!(s.escrow.get_pause_flags(), 0);
    assert!(!s.escrow.is_paused());

    // Every in-range bit is accepted.
    s.escrow.set_pause_flags(&PAUSE_ALL);
    assert_eq!(s.escrow.get_pause_flags(), PAUSE_ALL);
}

#[test]
fn default_state_is_unpaused_and_set_paused_toggles_all_flags() {
    let s = setup();

    // Default (no flags stored) leaves everything unpaused.
    assert!(!s.escrow.is_paused());
    assert_eq!(s.escrow.get_pause_flags(), 0);

    // set_paused(true) pauses every category.
    s.escrow.set_paused(&true);
    assert!(s.escrow.is_paused());
    assert_eq!(s.escrow.get_pause_flags(), PAUSE_ALL);

    // set_paused(false) clears every category.
    s.escrow.set_paused(&false);
    assert!(!s.escrow.is_paused());
    assert_eq!(s.escrow.get_pause_flags(), 0);

    // Mutations work again after the pause is lifted.
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Funded);
}

#[test]
fn legacy_pause_flag_is_treated_as_full_pause() {
    let s = setup();

    // Simulate a deployment created before granular pause flags existed.
    s.env.as_contract(&s.contract_id, || {
        s.env.storage().instance().set(&DataKey::Paused, &true);
    });

    assert_eq!(s.escrow.get_pause_flags(), PAUSE_ALL);
    assert!(s.escrow.is_paused());
    assert_eq!(
        s.escrow.try_create_contract(
            &s.client,
            &s.freelancer,
            &s.token.address,
            &vec![&s.env, 10i128]
        ),
        Err(Ok(EscrowError::Paused))
    );
    assert_eq!(
        s.escrow.try_issue_reputation(&1, &5),
        Err(Ok(EscrowError::Paused))
    );

    // A stored bitmask takes precedence over the legacy boolean.
    s.escrow.set_pause_flags(&PAUSE_CANCEL);
    assert_eq!(s.escrow.get_pause_flags(), PAUSE_CANCEL);
    assert!(s.escrow.is_paused());
    assert!(s
        .escrow
        .try_create_contract(
            &s.client,
            &s.freelancer,
            &s.token.address,
            &vec![&s.env, 10i128]
        )
        .is_ok());
    assert_eq!(
        s.escrow.try_cancel_contract(&1),
        Err(Ok(EscrowError::Paused))
    );
}
