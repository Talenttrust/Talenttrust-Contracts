use super::{EscrowFixture, MILESTONE_ONE};
use crate::{ContractStatus, Error, Escrow, EscrowError, ReleaseAuthorization, SimulatedRelease};
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, vec, Address, Env};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn assert_simulation_ok(result: &SimulatedRelease) {
    assert!(
        result.would_succeed,
        "expected successful simulation, got error_code={:?}",
        result.error_code
    );
    assert!(result.error_code.is_none());
}

fn assert_simulation_err(result: &SimulatedRelease, expected_code: u32) {
    assert!(!result.would_succeed, "expected simulation to fail");
    assert_eq!(result.error_code, Some(expected_code));
}

// ── Happy path ────────────────────────────────────────────────────────────────

/// Simulating a milestone release returns the same amounts that the real release
/// would produce.
#[test]
fn simulate_matches_real_release_outcome() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    // Approve milestone 0
    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0);

    // Simulate before releasing
    let sim = escrow.simulate_release_milestone(&fixture.escrow_id, &fixture.client, &0);
    assert_simulation_ok(&sim);

    // Now do the real release
    assert!(escrow.release_milestone(&fixture.escrow_id, &fixture.client, &0));

    let contract = escrow.get_contract(&fixture.escrow_id);

    // Verify simulation matched reality
    assert_eq!(sim.gross_amount, MILESTONE_ONE);
    assert_eq!(sim.net_amount, MILESTONE_ONE - sim.protocol_fee);
    assert_eq!(sim.projected_released_amount, contract.released_amount);

    // No protocol fee set in default fixture, so fee should be 0
    assert_eq!(sim.protocol_fee, 0);
}

/// Simulation correctly detects contract completion when the last milestone
/// would be released.
#[test]
fn simulate_detects_contract_completion() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    // Simulate releasing all 3 milestones should eventually complete
    for i in 0..3u32 {
        escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &i);
        let sim = escrow.simulate_release_milestone(&fixture.escrow_id, &fixture.client, &i);
        assert_simulation_ok(&sim);

        // Only the third release should trigger completion
        let expected_completion = i == 2;
        assert_eq!(
            sim.would_complete_contract, expected_completion,
            "milestone {} completion mismatch",
            i
        );

        // Actually release so we can test the next one
        escrow.release_milestone(&fixture.escrow_id, &fixture.client, &i);
    }
}

/// Simulate matches real release for each milestone in a multi-milestone contract.
#[test]
fn simulate_sequential_releases_match_real() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    for i in 0..3u32 {
        escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &i);
        let sim = escrow.simulate_release_milestone(&fixture.escrow_id, &fixture.client, &i);
        assert_simulation_ok(&sim);

        assert!(escrow.release_milestone(&fixture.escrow_id, &fixture.client, &i));

        let contract = escrow.get_contract(&fixture.escrow_id);
        assert_eq!(sim.projected_released_amount, contract.released_amount);
    }
}

/// Simulation produces the same result whether called before or after the real
/// release (i.e. the already-released check is consistent).
#[test]
fn simulate_rejects_already_released_milestone() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0);
    escrow.release_milestone(&fixture.escrow_id, &fixture.client, &0);

    // Simulate again on the same milestone — should report AlreadyReleased
    let sim = escrow.simulate_release_milestone(&fixture.escrow_id, &fixture.client, &0);
    assert_simulation_err(&sim, Error::MilestoneAlreadyReleased as u32);
}

// ── Error-path coverage — each check that release_milestone panics with ───────

/// ContractNotFound when contract_id does not exist.
#[test]
fn simulate_contract_not_found() {
    let fixture = EscrowFixture::builder().build();
    let escrow = fixture.escrow();

    let sim = escrow.simulate_release_milestone(&9999, &fixture.client, &0);
    assert_simulation_err(&sim, EscrowError::ContractNotFound as u32);
}

/// InvalidState when contract is not Funded (e.g. just Created).
#[test]
fn simulate_not_funded() {
    let fixture = EscrowFixture::builder().build();
    let escrow = fixture.escrow();

    let sim = escrow.simulate_release_milestone(&fixture.escrow_id, &fixture.client, &0);
    assert_simulation_err(&sim, Error::InvalidState as u32);
}

/// UnauthorizedRole when caller is not the authorized releaser.
#[test]
fn simulate_unauthorized_caller() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    let stranger = Address::generate(&fixture.env);

    let sim = escrow.simulate_release_milestone(&fixture.escrow_id, &stranger, &0);
    assert_simulation_err(&sim, EscrowError::UnauthorizedRole as u32);
}

/// IndexOutOfBounds for an invalid milestone index.
#[test]
fn simulate_index_out_of_bounds() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    let sim = escrow.simulate_release_milestone(&fixture.escrow_id, &fixture.client, &99);
    assert_simulation_err(&sim, Error::IndexOutOfBounds as u32);
}

/// Already refunded milestone cannot be released.
#[test]
fn simulate_already_refunded() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    // Refund only milestone 0 so contract stays Funded
    escrow.refund_unreleased_milestones(&fixture.escrow_id, &vec![&fixture.env, 0u32]);

    let sim = escrow.simulate_release_milestone(&fixture.escrow_id, &fixture.client, &0);
    assert_simulation_err(&sim, EscrowError::AlreadyRefunded as u32);
}

/// InsufficientApprovals when no approval record exists.
#[test]
fn simulate_insufficient_approvals() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    // No approval recorded
    let sim = escrow.simulate_release_milestone(&fixture.escrow_id, &fixture.client, &0);
    assert_simulation_err(&sim, Error::InsufficientApprovals as u32);
}

/// Simulation does not mutate any contract state.
#[test]
fn simulate_does_not_mutate_state() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0);

    let before_contract = escrow.get_contract(&fixture.escrow_id);
    let before_milestones = escrow.get_milestones(&fixture.escrow_id);

    // Run simulation
    let sim = escrow.simulate_release_milestone(&fixture.escrow_id, &fixture.client, &0);
    assert_simulation_ok(&sim);

    // Verify state is unchanged
    let after_contract = escrow.get_contract(&fixture.escrow_id);
    let after_milestones = escrow.get_milestones(&fixture.escrow_id);

    assert_eq!(before_contract, after_contract);
    assert_eq!(before_milestones, after_milestones);
}

/// Contract status is not affected by simulation (no accidental completion).
#[test]
fn simulate_does_not_complete_contract() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    // Approve and release first 2 milestones so the 3rd would complete
    for i in 0..2u32 {
        escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &i);
        escrow.release_milestone(&fixture.escrow_id, &fixture.client, &i);
    }

    // Simulate releasing the last milestone — would complete contract
    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &2);
    let sim = escrow.simulate_release_milestone(&fixture.escrow_id, &fixture.client, &2);
    assert_simulation_ok(&sim);
    assert!(sim.would_complete_contract);

    // But contract should still be Funded (not Completed)
    let contract = escrow.get_contract(&fixture.escrow_id);
    assert_eq!(contract.status, ContractStatus::Funded);
}

/// Simulation works with different release authorization modes.
#[test]
fn simulate_arbiter_only_authorization() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let client = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let arbiter = Address::generate(&env);

    let escrow_address = env.register(Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_address);
    escrow.initialize(&admin);

    // Register and bind token
    let token = env.register_stellar_asset_contract(admin.clone());
    escrow.bind_settlement_token(&admin, &token);

    // Create contract with ArbiterOnly
    let milestones = vec![&env, MILESTONE_ONE];
    let cid = escrow.create_contract(
        &client,
        &freelancer,
        &Some(arbiter.clone()),
        &milestones,
        &ReleaseAuthorization::ArbiterOnly,
    );

    // Fund the contract
    let sac = StellarAssetClient::new(&env, &token);
    sac.mint(&client, &MILESTONE_ONE);
    escrow.deposit_funds(&cid, &client, &MILESTONE_ONE);

    // Client should NOT be authorized
    let sim = escrow.simulate_release_milestone(&cid, &client, &0);
    assert_simulation_err(&sim, EscrowError::UnauthorizedRole as u32);

    // Arbiter should be authorized
    escrow.approve_milestone_release(&cid, &arbiter, &0);
    let sim = escrow.simulate_release_milestone(&cid, &arbiter, &0);
    assert_simulation_ok(&sim);
}

/// Simulation with pending contract (Created state) returns InvalidState.
#[test]
fn simulate_created_state_rejected() {
    let fixture = EscrowFixture::builder().build();
    let escrow = fixture.escrow();

    let sim = escrow.simulate_release_milestone(&fixture.escrow_id, &fixture.client, &0);
    assert_simulation_err(&sim, Error::InvalidState as u32);
}

/// Simulation works correctly with protocol fees configured.
#[test]
fn simulate_with_protocol_fees() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    // Set a 10% protocol fee
    escrow.set_protocol_fee_bps(&1_000);

    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0);

    let sim = escrow.simulate_release_milestone(&fixture.escrow_id, &fixture.client, &0);
    assert_simulation_ok(&sim);

    // 10% of MILESTONE_ONE (200_0000000) = 20_0000000
    assert!(sim.protocol_fee > 0);
    assert_eq!(sim.net_amount, sim.gross_amount - sim.protocol_fee);

    // Verify against the real release
    assert!(escrow.release_milestone(&fixture.escrow_id, &fixture.client, &0));
    let contract = escrow.get_contract(&fixture.escrow_id);
    assert_eq!(sim.projected_released_amount, contract.released_amount);
}

/// AlreadyFinalized contract rejects simulation.
#[test]
fn simulate_finalized_contract_rejected() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    // Complete all milestones
    for i in 0..3u32 {
        escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &i);
        escrow.release_milestone(&fixture.escrow_id, &fixture.client, &i);
    }

    // Finalize
    escrow.finalize_contract(&fixture.escrow_id, &fixture.client);

    // Simulate should be rejected
    let sim = escrow.simulate_release_milestone(&fixture.escrow_id, &fixture.client, &0);
    assert_simulation_err(&sim, Error::AlreadyFinalized as u32);
}

/// Partially funded contract — status is PartiallyFunded, not Funded,
/// so release_milestone rejects with InvalidState before any fund check.
#[test]
fn simulate_partially_funded_contract_rejected() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let client = Address::generate(&env);
    let freelancer = Address::generate(&env);

    let escrow_address = env.register(Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_address);
    escrow.initialize(&admin);

    // Register and bind token
    let token = env.register_stellar_asset_contract(admin.clone());
    escrow.bind_settlement_token(&admin, &token);

    // Create contract with a 1000-unit milestone
    let milestones = vec![&env, 1000i128];
    let cid = escrow.create_contract(
        &client,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // Deposit only 1 unit (far below the 1000 milestone amount)
    let sac = StellarAssetClient::new(&env, &token);
    sac.mint(&client, &1);
    escrow.deposit_funds(&cid, &client, &1);

    // Contract is now PartiallyFunded, not Funded
    escrow.approve_milestone_release(&cid, &client, &0);

    let sim = escrow.simulate_release_milestone(&cid, &client, &0);
    assert!(!sim.would_succeed);
    assert_eq!(
        sim.error_code,
        Some(Error::InvalidState as u32),
        "expected InvalidState(16), got error_code={:?}",
        sim.error_code
    );
}
