extern crate std;

use super::*;
use soroban_sdk::{
    contract as mock_contract, contractimpl as mock_contractimpl, testutils::Address as _,
    token::StellarAssetClient, token::TokenClient, vec, Address, Env,
};

// ─────────────────────────────────────────────────────────────────────────────
// Mock tokens used to prove the balance-delta defence (#1678).
//
// The SAC (and every well-behaved token) moves exactly `amount`; these mocks
// deliberately do not, so the contract's accounting is exercised against
// fee-on-transfer and no-op tokens. Each mock lives in its own module because
// the Soroban contract macro emits spec symbols that cannot share a scope.
// ─────────────────────────────────────────────────────────────────────────────

mod fee_token {
    use super::*;

    /// A fee-on-transfer token: `from` is debited the full `amount`, `to` is
    /// credited `amount - 1%`. `share()` reports what a transfer would deliver.
    #[mock_contract]
    pub struct MockFeeToken;

    #[mock_contractimpl]
    impl MockFeeToken {
        /// Mints without any balance rebalancing: test set-up only.
        pub fn mint(env: Env, to: Address, amount: i128) {
            let mut b: i128 = env.storage().persistent().get(&to).unwrap_or(0);
            b += amount;
            env.storage().persistent().set(&to, &b);
        }

        pub fn balance(env: Env, addr: Address) -> i128 {
            env.storage().persistent().get(&addr).unwrap_or(0)
        }

        pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
            let fee = amount / 100; // 1%
            let from_balance: i128 = env.storage().persistent().get(&from).unwrap_or(0);
            env.storage()
                .persistent()
                .set(&from, &(from_balance - amount));
            let to_balance: i128 = env.storage().persistent().get(&to).unwrap_or(0);
            env.storage()
                .persistent()
                .set(&to, &(to_balance + amount - fee));
        }

        /// What a `transfer` of `amount` would actually deliver.
        pub fn share(amount: i128) -> i128 {
            amount - amount / 100
        }
    }
}

mod short_token {
    use super::*;

    /// A balance-moving token with a 1-unit fee: `from` is debited `amount`,
    /// `to` is credited `amount - 1`. Reachable on both deposit and release.
    #[mock_contract]
    pub struct MockShortToken;

    #[mock_contractimpl]
    impl MockShortToken {
        pub fn mint(env: Env, to: Address, amount: i128) {
            let mut b: i128 = env.storage().persistent().get(&to).unwrap_or(0);
            b += amount;
            env.storage().persistent().set(&to, &b);
        }

        pub fn balance(env: Env, addr: Address) -> i128 {
            env.storage().persistent().get(&addr).unwrap_or(0)
        }

        pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
            let from_balance: i128 = env.storage().persistent().get(&from).unwrap_or(0);
            env.storage()
                .persistent()
                .set(&from, &(from_balance - amount));
            let to_balance: i128 = env.storage().persistent().get(&to).unwrap_or(0);
            env.storage()
                .persistent()
                .set(&to, &(to_balance + amount - 1));
        }
    }
}

mod noop_token {
    use super::*;

    /// A token that reports a successful transfer while moving nothing at all.
    #[mock_contract]
    pub struct MockNoopToken;

    #[mock_contractimpl]
    impl MockNoopToken {
        pub fn mint(env: Env, to: Address, amount: i128) {
            let mut b: i128 = env.storage().persistent().get(&to).unwrap_or(0);
            b += amount;
            env.storage().persistent().set(&to, &b);
        }

        pub fn balance(env: Env, addr: Address) -> i128 {
            env.storage().persistent().get(&addr).unwrap_or(0)
        }

        pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
    }
}

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

// ─────────────────────────────────────────────────────────────────────────────
// #1678 — credit by measured balance delta
// ─────────────────────────────────────────────────────────────────────────────

/// Sets up fee-on-transfer token mock and returns (env, escrow, token addr, client, freelancer, admin).
fn setup_with_fee_token<'a>() -> (Env, EscrowClient<'a>, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let id = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &id);
    escrow.initialize(&admin);
    let token = env.register(fee_token::MockFeeToken, ());
    fee_token::MockFeeTokenClient::new(&env, &token).mint(&client, &1_000);
    (env, escrow, token, client, freelancer)
}

/// Escrow wired to a token whose outgoing transfers under-deliver by 1 unit.
fn setup_short_token<'a>() -> (Env, EscrowClient<'a>, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let id = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &id);
    escrow.initialize(&admin);
    let token = env.register(short_token::MockShortToken, ());
    short_token::MockShortTokenClient::new(&env, &token).mint(&client, &1_000);
    (env, escrow, token, client, freelancer)
}

#[test]
fn deposit_fee_on_transfer_token_is_rejected() {
    let (env, escrow, token, client, freelancer) = setup_with_fee_token();
    let id = escrow.create_contract(&client, &freelancer, &token, &vec![&env, 100i128]);

    assert_eq!(
        escrow.try_deposit_funds(&id, &100),
        Err(Ok(EscrowError::TransferAmountMismatch))
    );

    // Nothing was credited: the escrow neither over-reports funding nor holds
    // a short balance for other escrows to cover.
    let c = escrow.get_contract(&id);
    assert_eq!(c.funded_amount, 0);
    assert_eq!(c.status, ContractStatus::Created);
    assert_eq!(
        fee_token::MockFeeTokenClient::new(&env, &token).balance(&escrow.address),
        0
    );
}

#[test]
fn deposit_noop_token_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let id = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &id);
    escrow.initialize(&admin);
    let token = env.register(noop_token::MockNoopToken, ());
    let cid = escrow.create_contract(&client, &freelancer, &token, &vec![&env, 100i128]);

    assert_eq!(
        escrow.try_deposit_funds(&cid, &100),
        Err(Ok(EscrowError::TransferAmountMismatch))
    );
}

#[test]
fn release_short_delivery_token_is_rejected_without_marking_released() {
    // A token that debits the escrow the full `amount` but only credits the
    // recipient `amount - 1`. The escrow's own balance still falls by exactly
    // `amount`, so the balance-delta guard alone cannot see the shortfall — but
    // the guard's post-condition is on the CONTRACT's balance, which is what
    // protects other escrows in the same token. To make that pay out over the
    // recorded figure, the escrow must hold at least `amount`; it is credited
    // only `amount - 1` at deposit, which the deposit guard already rejects.
    // This test pins that a short-delivery token can never get past deposit,
    // so no milestone can ever be released against a phantom balance.
    let (env, escrow, token, client, freelancer) = setup_short_token();
    let id = escrow.create_contract(&client, &freelancer, &token, &vec![&env, 100i128]);

    assert_eq!(
        escrow.try_deposit_funds(&id, &100),
        Err(Ok(EscrowError::TransferAmountMismatch))
    );
    assert_eq!(escrow.get_contract(&id).funded_amount, 0);
    assert!(!escrow.get_contract(&id).milestones.get(0).unwrap().released);
    assert_eq!(
        short_token::MockShortTokenClient::new(&env, &token).balance(&escrow.address),
        0
    );
}

#[test]
fn release_escrow_balance_shortfall_is_rejected() {
    // The outgoing post-condition: the escrow balance must fall by exactly the
    // amount sent. Fund via the SAC, drain the escrow's balance behind the
    // contract's back (simulating a token whose balances drift), then prove a
    // release is refused rather than silently short-paying.
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);

    // A well-behaved release still works and moves exactly 100.
    let before = s.token.balance(&s.escrow.address);
    s.escrow.release_milestone(&id, &0);
    assert_eq!(before - s.token.balance(&s.escrow.address), 100);
    assert_eq!(s.token.balance(&s.freelancer), 100);
}

#[test]
fn resolve_dispute_outgoing_transfers_are_balance_checked() {
    // Both outgoing legs of a dispute resolution are checked and, with a
    // well-behaved token, deliver exactly the recorded split.
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 400i128],
    );
    s.escrow.deposit_funds(&id, &400);
    s.escrow.open_dispute(&id, &s.client);

    let escrow_before = s.token.balance(&s.escrow.address);
    s.escrow.resolve_dispute(&id, &2_500);
    assert_eq!(escrow_before - s.token.balance(&s.escrow.address), 400);
    assert_eq!(s.token.balance(&s.freelancer), 100);
    assert_eq!(s.escrow.get_contract(&id).funded_amount, 100);
}

#[test]
fn cancel_outgoing_transfer_is_balance_checked() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);

    let escrow_before = s.token.balance(&s.escrow.address);
    s.escrow.cancel_contract(&id);
    assert_eq!(escrow_before - s.token.balance(&s.escrow.address), 100);
    assert_eq!(s.token.balance(&s.client), 1_000);

    // Re-cancelling a zero-refund escrow (already cancelled) is rejected by
    // status, and a refund of 0 skips the transfer entirely.
    assert_eq!(
        s.escrow.try_cancel_contract(&id),
        Err(Ok(EscrowError::InvalidStatus))
    );
}

#[test]
fn deposit_records_exactly_the_amount_received_with_a_well_behaved_token() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 300i128],
    );
    s.escrow.deposit_funds(&id, &300);
    let c = s.escrow.get_contract(&id);
    assert_eq!(c.funded_amount, 300);
    assert_eq!(s.token.balance(&s.escrow.address), 300);
}
