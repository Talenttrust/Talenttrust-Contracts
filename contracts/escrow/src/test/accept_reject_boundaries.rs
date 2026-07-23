#![cfg(test)]

use soroban_sdk::{
    symbol_short, testutils::Address as _, testutils::Events as _, vec, Address, Env, Symbol,
    TryFromVal,
};

use super::{
    assert_contract_error, default_milestones, generated_participants, register_client,
    total_milestone_amount, EscrowFixture,
};
use crate::{
    types::{ContractStatus, DataKey},
    Contract, Error, EscrowError, ReleaseAuthorization,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn inject_state(env: &Env, escrow_addr: &Address, id: u32, status: ContractStatus) {
    env.as_contract(escrow_addr, || {
        let key = DataKey::Contract(id);
        let mut contract: Contract = env.storage().persistent().get(&key).unwrap();
        contract.status = status;
        env.storage().persistent().set(&key, &contract);
    });
}

fn inject_funded(env: &Env, escrow_addr: &Address, id: u32) {
    env.as_contract(escrow_addr, || {
        let key = DataKey::Contract(id);
        let mut contract: Contract = env.storage().persistent().get(&key).unwrap();
        contract.status = ContractStatus::Funded;
        contract.funded_amount = total_milestone_amount();
        env.storage().persistent().set(&key, &contract);
    });
}

fn has_event_with_topic(env: &Env, topic: &Symbol) -> bool {
    env.events().all().iter().any(|event| {
        let topics = &event.1;
        !topics.is_empty()
            && Symbol::try_from_val(env, &topics.get(0).unwrap())
                .ok()
                .as_ref()
                == Some(topic)
    })
}

// ═══════════════════════════════════════════════════════════════════════════════
//  APPROVE (ACCEPT) BOUNDARIES
// ═══════════════════════════════════════════════════════════════════════════════

// ── State guards ─────────────────────────────────────────────────────────────

#[test]
fn approve_rejects_created_state() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let milestones = default_milestones(&env);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_contract_error(
        client.try_approve_milestone_release(&id, &client_addr, &0),
        Error::InvalidState,
    );
}

#[test]
fn approve_rejects_completed_state() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    // Release all 3 milestones to reach Completed status.
    for i in 0..3_u32 {
        assert!(escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &i));
        assert!(escrow.release_milestone(&fixture.escrow_id, &fixture.client, &i));
    }
    assert_contract_error(
        escrow.try_approve_milestone_release(&fixture.escrow_id, &fixture.client, &0),
        Error::InvalidState,
    );
}

#[test]
fn approve_rejects_cancelled_state() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let milestones = default_milestones(&env);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    inject_funded(&env, &client.address, id);
    inject_state(&env, &client.address, id, ContractStatus::Cancelled);
    assert_contract_error(
        client.try_approve_milestone_release(&id, &client_addr, &0),
        Error::InvalidState,
    );
}

#[test]
fn approve_rejects_refunded_state() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let milestones = default_milestones(&env);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    inject_funded(&env, &client.address, id);
    inject_state(&env, &client.address, id, ContractStatus::Refunded);
    assert_contract_error(
        client.try_approve_milestone_release(&id, &client_addr, &0),
        Error::InvalidState,
    );
}

#[test]
fn approve_rejects_disputed_state() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let milestones = default_milestones(&env);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    inject_funded(&env, &client.address, id);
    inject_state(&env, &client.address, id, ContractStatus::Disputed);
    assert_contract_error(
        client.try_approve_milestone_release(&id, &client_addr, &0),
        Error::InvalidState,
    );
}

// ── Boundary: index ──────────────────────────────────────────────────────────

#[test]
fn approve_rejects_out_of_bounds_index() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let milestones = default_milestones(&env);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    inject_funded(&env, &client.address, id);
    assert_contract_error(
        client.try_approve_milestone_release(&id, &client_addr, &99),
        Error::IndexOutOfBounds,
    );
}

#[test]
fn approve_rejects_index_at_len() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let milestones = default_milestones(&env);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    inject_funded(&env, &client.address, id);
    assert_contract_error(
        client.try_approve_milestone_release(&id, &client_addr, &3),
        Error::IndexOutOfBounds,
    );
}

// ── Boundary: already released / duplicate ────────────────────────────────────

#[test]
fn approve_rejects_already_released_milestone() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    assert!(escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0));
    assert!(escrow.release_milestone(&fixture.escrow_id, &fixture.client, &0));
    assert_contract_error(
        escrow.try_approve_milestone_release(&fixture.escrow_id, &fixture.client, &0),
        Error::MilestoneAlreadyReleased,
    );
}

#[test]
fn approve_rejects_duplicate_client_approval() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let milestones = default_milestones(&env);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    inject_funded(&env, &client.address, id);
    assert!(client.approve_milestone_release(&id, &client_addr, &0));
    assert_contract_error(
        client.try_approve_milestone_release(&id, &client_addr, &0),
        Error::AlreadyApproved,
    );
}

#[test]
fn approve_rejects_duplicate_arbiter_approval() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = default_milestones(&env);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ArbiterOnly,
    );
    inject_funded(&env, &client.address, id);
    assert!(client.approve_milestone_release(&id, &arbiter_addr, &0));
    assert_contract_error(
        client.try_approve_milestone_release(&id, &arbiter_addr, &0),
        Error::AlreadyApproved,
    );
}

// ── Authorization ────────────────────────────────────────────────────────────

#[test]
fn approve_rejects_freelancer_in_client_only() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let milestones = default_milestones(&env);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    inject_funded(&env, &client.address, id);
    assert_contract_error(
        client.try_approve_milestone_release(&id, &freelancer_addr, &0),
        Error::UnauthorizedRole,
    );
}

#[test]
fn approve_rejects_client_in_arbiter_only() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = default_milestones(&env);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ArbiterOnly,
    );
    inject_funded(&env, &client.address, id);
    assert_contract_error(
        client.try_approve_milestone_release(&id, &client_addr, &0),
        Error::UnauthorizedRole,
    );
}

#[test]
fn approve_rejects_arbiter_in_client_only() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = default_milestones(&env);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    inject_funded(&env, &client.address, id);
    assert_contract_error(
        client.try_approve_milestone_release(&id, &arbiter_addr, &0),
        Error::UnauthorizedRole,
    );
}

#[test]
fn approve_rejects_arbiter_in_multisig() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = default_milestones(&env);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::MultiSig,
    );
    inject_funded(&env, &client.address, id);
    assert_contract_error(
        client.try_approve_milestone_release(&id, &arbiter_addr, &0),
        Error::UnauthorizedRole,
    );
}

#[test]
fn approve_rejects_stranger() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let milestones = default_milestones(&env);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    inject_funded(&env, &client.address, id);
    let stranger = Address::generate(&env);
    assert_contract_error(
        client.try_approve_milestone_release(&id, &stranger, &0),
        Error::UnauthorizedRole,
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
//  REFUND (REJECT) BOUNDARIES
// ═══════════════════════════════════════════════════════════════════════════════

// ─── Input validation ────────────────────────────────────────────────────────

#[test]
fn refund_rejects_empty_request() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    let empty = vec![&fixture.env];
    assert_contract_error(
        escrow.try_refund_unreleased_milestones(&fixture.escrow_id, &empty),
        EscrowError::EmptyRefundRequest,
    );
}

#[test]
fn refund_rejects_duplicate_indices() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    let dup = vec![&fixture.env, 0_u32, 0_u32];
    assert_contract_error(
        escrow.try_refund_unreleased_milestones(&fixture.escrow_id, &dup),
        EscrowError::DuplicateMilestoneInRefund,
    );
}

#[test]
fn refund_rejects_out_of_bounds_index() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    let ids = vec![&fixture.env, 99_u32];
    assert_contract_error(
        escrow.try_refund_unreleased_milestones(&fixture.escrow_id, &ids),
        Error::IndexOutOfBounds,
    );
}

#[test]
fn refund_rejects_index_at_len() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    let ids = vec![&fixture.env, 3_u32];
    assert_contract_error(
        escrow.try_refund_unreleased_milestones(&fixture.escrow_id, &ids),
        Error::IndexOutOfBounds,
    );
}

// ── Already released / refunded ──────────────────────────────────────────────

#[test]
fn refund_rejects_already_released_milestone() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    assert!(escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0));
    assert!(escrow.release_milestone(&fixture.escrow_id, &fixture.client, &0));
    let ids = vec![&fixture.env, 0_u32];
    assert_contract_error(
        escrow.try_refund_unreleased_milestones(&fixture.escrow_id, &ids),
        Error::AlreadyReleased,
    );
}

#[test]
fn refund_rejects_already_refunded_milestone() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    let ids = vec![&fixture.env, 0_u32];
    assert!(escrow.refund_unreleased_milestones(&fixture.escrow_id, &ids) > 0);
    assert_contract_error(
        escrow.try_refund_unreleased_milestones(&fixture.escrow_id, &ids),
        EscrowError::AlreadyRefunded,
    );
}

// ── Terminal state guards ────────────────────────────────────────────────────

#[test]
fn refund_rejects_completed_contract() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    for i in 0..3_u32 {
        assert!(escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &i));
        assert!(escrow.release_milestone(&fixture.escrow_id, &fixture.client, &i));
    }
    let ids = vec![&fixture.env, 0_u32];
    assert_contract_error(
        escrow.try_refund_unreleased_milestones(&fixture.escrow_id, &ids),
        EscrowError::InvalidState,
    );
}

#[test]
fn refund_rejects_cancelled_contract() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    assert!(escrow.cancel_contract(&fixture.escrow_id, &fixture.client));
    let ids = vec![&fixture.env, 0_u32];
    assert_contract_error(
        escrow.try_refund_unreleased_milestones(&fixture.escrow_id, &ids),
        EscrowError::InvalidState,
    );
}

#[test]
fn refund_rejects_refunded_contract() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    let ids = vec![&fixture.env, 0_u32, 1_u32, 2_u32];
    assert!(escrow.refund_unreleased_milestones(&fixture.escrow_id, &ids) > 0);
    let more_ids = vec![&fixture.env, 0_u32];
    assert_contract_error(
        escrow.try_refund_unreleased_milestones(&fixture.escrow_id, &more_ids),
        EscrowError::InvalidState,
    );
}

// ── Refund-to-completion ─────────────────────────────────────────────────────

#[test]
fn refund_all_milestones_transitions_to_refunded() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    let ids = vec![&fixture.env, 0_u32, 1_u32, 2_u32];
    let total = escrow.refund_unreleased_milestones(&fixture.escrow_id, &ids);
    assert_eq!(total, fixture.total_amount());
    let contract = escrow.get_contract(&fixture.escrow_id);
    assert_eq!(contract.status, ContractStatus::Refunded);
}

// ── Refund event ─────────────────────────────────────────────────────────────

#[test]
fn refund_emits_refunded_event() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    let ids = vec![&fixture.env, 0_u32, 1_u32, 2_u32];
    escrow.refund_unreleased_milestones(&fixture.escrow_id, &ids);
    let topic = symbol_short!("refunded");
    assert!(has_event_with_topic(&fixture.env, &topic));
}

// ═══════════════════════════════════════════════════════════════════════════════
//  CANCEL (REJECT) BOUNDARIES
// ═══════════════════════════════════════════════════════════════════════════════

// ── Unauthorized caller ──────────────────────────────────────────────────────

#[test]
fn cancel_rejects_freelancer() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    assert_contract_error(
        escrow.try_cancel_contract(&fixture.escrow_id, &fixture.freelancer),
        EscrowError::UnauthorizedRole,
    );
}

#[test]
fn cancel_rejects_stranger() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    let stranger = Address::generate(&fixture.env);
    assert_contract_error(
        escrow.try_cancel_contract(&fixture.escrow_id, &stranger),
        EscrowError::UnauthorizedRole,
    );
}

// ── Terminal state guards ────────────────────────────────────────────────────

#[test]
fn cancel_rejects_completed_contract() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    for i in 0..3_u32 {
        assert!(escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &i));
        assert!(escrow.release_milestone(&fixture.escrow_id, &fixture.client, &i));
    }
    assert_contract_error(
        escrow.try_cancel_contract(&fixture.escrow_id, &fixture.client),
        EscrowError::InvalidStatusTransition,
    );
}

#[test]
fn cancel_rejects_disputed_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = default_milestones(&env);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    inject_funded(&env, &client.address, id);
    inject_state(&env, &client.address, id, ContractStatus::Disputed);
    assert_contract_error(
        client.try_cancel_contract(&id, &client_addr),
        EscrowError::InvalidStatusTransition,
    );
}

#[test]
fn cancel_rejects_refunded_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let milestones = default_milestones(&env);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    inject_funded(&env, &client.address, id);
    inject_state(&env, &client.address, id, ContractStatus::Refunded);
    assert_contract_error(
        client.try_cancel_contract(&id, &client_addr),
        EscrowError::InvalidStatusTransition,
    );
}

// ── Event emission ───────────────────────────────────────────────────────────

#[test]
fn cancel_emits_cancelled_event() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    assert!(escrow.cancel_contract(&fixture.escrow_id, &fixture.client));
    let topic = symbol_short!("cancelled");
    assert!(has_event_with_topic(&fixture.env, &topic));
}
