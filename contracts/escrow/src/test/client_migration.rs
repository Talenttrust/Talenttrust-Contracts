#![cfg(test)]

use crate::migration::PendingClientMigration;
use crate::ttl::PENDING_MIGRATION_TTL_LEDGERS;
use crate::{
    types::{ContractStatus, DataKey},
    Contract, Escrow, EscrowClient, EscrowError,
};
use soroban_sdk::{
    testutils::Address as _, testutils::Events, testutils::Ledger as _, testutils::LedgerInfo,
    Address, Env, IntoVal, Symbol, TryFromVal, Val,
};

use super::{assert_contract_error, create_contract, register_client, total_milestone_amount};

// ---------------------------------------------------------------------------
// Helper: forcibly inject a contract status via env.as_contract.
// Used for terminal states that have no convenient public entrypoint.
// ---------------------------------------------------------------------------

fn set_escrow_status(env: &Env, escrow_addr: &Address, id: u32, status: ContractStatus) {
    env.as_contract(escrow_addr, || {
        let key = DataKey::Contract(id);
        let mut contract: Contract = env.storage().persistent().get(&key).unwrap();
        contract.status = status;
        env.storage().persistent().set(&key, &contract);
    });
}

// ---------------------------------------------------------------------------
// Helper: check whether any emitted event has a given Symbol as its first topic.
//
// env.events().all() returns Vec<(Address, Vec<Val>, Val)>:
//   tuple.0 = the contract Address that emitted the event
//   tuple.1 = the topics Vec<Val>  ← Symbol is topics[0]
//   tuple.2 = the data Val
// ---------------------------------------------------------------------------

fn has_event_with_topic(env: &Env, topic: &Symbol) -> bool {
    env.events().all().iter().any(|event| {
        let topics = &event.1;
        topics.len() > 0 && {
            if let Ok(sym) = Symbol::try_from_val(env, &topics.get(0).unwrap()) {
                sym == *topic
            } else {
                false
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Test 1 – propose → accept updates contract.client and emits expected events
// ---------------------------------------------------------------------------

/// A successful two-step migration must:
///   1. Store a live `PendingClientMigration` record in temporary storage.
///   2. Emit a `client_migration_proposed` event on proposal.
///   3. Update `contract.client` to the new address on acceptance.
///   4. Clear the pending record after acceptance.
///   5. Emit a `client_migration_accepted` event on acceptance.
#[test]
#[ignore]
fn propose_and_accept_updates_client_and_emits_events() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();

    // Set max_entry_ttl high enough so the migration proposal's
    // temporary storage entry doesn't get rejected by the host.
    let initial = env.ledger().get();
    env.ledger().set(LedgerInfo {
        sequence_number: initial.sequence_number,
        timestamp: initial.timestamp,
        protocol_version: initial.protocol_version,
        network_id: initial.network_id.clone(),
        base_reserve: initial.base_reserve,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: PENDING_MIGRATION_TTL_LEDGERS * 4,
        max_entry_ttl: PENDING_MIGRATION_TTL_LEDGERS * 4,
    });

    let client = register_client(&env);

    let (client_addr, _freelancer_addr, id) = create_contract(&env, &client);
    let new_client = Address::generate(&env);

    // --- Proposal ---
    assert!(client.propose_client_migration(&id, &client_addr, &new_client));

    // Capture events immediately after proposal before any other SDK calls
    let events_after_proposal = env.events().all();
    assert!(
        !events_after_proposal.is_empty(),
        "at least one event must be emitted after proposal"
    );
    assert!(
        has_event_with_topic(&env, &Symbol::new(&env, "client_migration_proposed")),
        "client_migration_proposed event not found"
    );

    assert!(client.has_pending_client_migration(&id));

    // Pending record fields are correct
    let pending: PendingClientMigration = client.get_pending_client_migration(&id);
    assert_eq!(pending.current_client, client_addr);
    assert_eq!(pending.proposed_client, new_client);
    assert!(
        pending.expires_at_ledger > env.ledger().sequence(),
        "expires_at_ledger must be in the future"
    );

    // --- Acceptance ---
    assert!(client.accept_client_migration(&id, &new_client));

    // Capture events from acceptance before other SDK calls
    assert!(
        has_event_with_topic(&env, &Symbol::new(&env, "client_migration_accepted")),
        "client_migration_accepted event not found"
    );

    // contract.client is now the new address
    let contract = client.get_contract(&id);
    assert_eq!(contract.client, new_client);

    // Pending record is cleared
    assert!(!client.has_pending_client_migration(&id));
}

// ---------------------------------------------------------------------------
// Test 2 – only the proposed address may accept
// ---------------------------------------------------------------------------

/// Acceptance by any address other than the one named in the proposal must
/// fail with `UnauthorizedRole`.  This covers the original client, the
/// freelancer, and random third parties.
#[test]
fn non_proposed_address_cannot_accept_migration() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let client = register_client(&env);

    let (client_addr, freelancer_addr, id) = create_contract(&env, &client);
    let new_client = Address::generate(&env);
    let attacker = Address::generate(&env);

    assert!(client.propose_client_migration(&id, &client_addr, &new_client));

    // Random attacker is rejected
    assert_contract_error(
        client.try_accept_client_migration(&id, &attacker),
        EscrowError::UnauthorizedRole,
    );

    // The original client is also rejected (only proposed address may accept)
    assert_contract_error(
        client.try_accept_client_migration(&id, &client_addr),
        EscrowError::UnauthorizedRole,
    );

    // Freelancer is rejected
    assert_contract_error(
        client.try_accept_client_migration(&id, &freelancer_addr),
        EscrowError::UnauthorizedRole,
    );
}

// ---------------------------------------------------------------------------
// Test 3 – advancing ledgers past PENDING_MIGRATION_TTL_LEDGERS kills proposal
// ---------------------------------------------------------------------------

/// Once the migration TTL window lapses the temporary storage entry is
/// auto-evicted by Soroban.  Both `accept_client_migration` and
/// `has_pending_client_migration` must reflect the eviction.
#[test]
fn expired_proposal_cannot_be_accepted() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();

    // Set max_entry_ttl high enough so the proposal can be stored without hitting the cap.
    let initial = env.ledger().get();
    env.ledger().set(LedgerInfo {
        sequence_number: initial.sequence_number,
        timestamp: initial.timestamp,
        protocol_version: initial.protocol_version,
        network_id: initial.network_id.clone(),
        base_reserve: initial.base_reserve,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: PENDING_MIGRATION_TTL_LEDGERS * 4,
        max_entry_ttl: PENDING_MIGRATION_TTL_LEDGERS * 4,
    });

    let client = register_client(&env);
    let (client_addr, _freelancer_addr, id) = create_contract(&env, &client);
    let new_client = Address::generate(&env);

    assert!(client.propose_client_migration(&id, &client_addr, &new_client));
    assert!(
        client.has_pending_client_migration(&id),
        "proposal must exist before expiry"
    );

    // Advance the ledger past the TTL. Soroban evicts temporary entries beyond max_entry_ttl.
    let current = env.ledger().get();
    env.ledger().set(LedgerInfo {
        sequence_number: current.sequence_number + PENDING_MIGRATION_TTL_LEDGERS + 1,
        timestamp: current.timestamp + u64::from(PENDING_MIGRATION_TTL_LEDGERS) * 5,
        protocol_version: current.protocol_version,
        network_id: [0u8; 32].into(),
        base_reserve: current.base_reserve,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 65_536,
    });

    // has_pending returns false — entry is evicted
    assert!(
        !client.has_pending_client_migration(&id),
        "proposal must be gone after TTL expiry"
    );

    // accept panics with InvalidState because no live record exists
    assert_contract_error(
        client.try_accept_client_migration(&id, &new_client),
        EscrowError::InvalidState,
    );
}

// ---------------------------------------------------------------------------
// Test 3b – acceptance at the last live ledger inside the TTL window succeeds
// ---------------------------------------------------------------------------

/// Complements `expired_proposal_cannot_be_accepted`: a proposal accepted on
/// the final ledger *before* `expires_at_ledger` must still succeed and
/// transfer client rights.  This pins the inclusive boundary of the migration
/// window so an off-by-one in the TTL handling (evicting one ledger early)
/// would be caught.
#[test]
fn proposal_accepted_at_window_boundary_succeeds() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();

    // Give temporary entries enough head-room to live for the full TTL.
    let initial = env.ledger().get();
    env.ledger().set(LedgerInfo {
        sequence_number: initial.sequence_number,
        timestamp: initial.timestamp,
        protocol_version: initial.protocol_version,
        network_id: initial.network_id.clone(),
        base_reserve: initial.base_reserve,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: PENDING_MIGRATION_TTL_LEDGERS * 4,
        max_entry_ttl: PENDING_MIGRATION_TTL_LEDGERS * 4,
    });

    let client = register_client(&env);
    let (client_addr, _freelancer_addr, id) = create_contract(&env, &client);
    let new_client = Address::generate(&env);

    assert!(client.propose_client_migration(&id, &client_addr, &new_client));

    let pending: PendingClientMigration = client.get_pending_client_migration(&id);
    let expires_at = pending.expires_at_ledger;

    // Advance to the last ledger that is still strictly inside the window.
    let current = env.ledger().get();
    env.ledger().set(LedgerInfo {
        sequence_number: expires_at - 1,
        timestamp: current.timestamp + 5,
        protocol_version: current.protocol_version,
        network_id: current.network_id.clone(),
        base_reserve: current.base_reserve,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: PENDING_MIGRATION_TTL_LEDGERS * 4,
        max_entry_ttl: PENDING_MIGRATION_TTL_LEDGERS * 4,
    });

    // Still live at the boundary.
    assert!(
        client.has_pending_client_migration(&id),
        "proposal must remain live on the last ledger before expiry"
    );

    // Acceptance at the boundary succeeds and updates contract.client.
    assert!(client.accept_client_migration(&id, &new_client));
    assert_eq!(client.get_contract(&id).client, new_client);
    assert!(
        !client.has_pending_client_migration(&id),
        "pending record must be cleared after a boundary acceptance"
    );
}

// ---------------------------------------------------------------------------
// Test 4 – migration blocked on all four terminal statuses
// ---------------------------------------------------------------------------

/// `require_migration_allowed` in migration.rs blocks proposals when the
/// escrow is in a terminal state.  All four terminal states are tested.
/// Completed contract blocks proposal.
#[test]
fn migration_blocked_on_completed_contract() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let client = register_client(&env);

    let (client_addr, _freelancer_addr, id) = create_contract(&env, &client);
    let new_client = Address::generate(&env);

    // Inject Completed status directly — the complete_contract helper in mod.rs
    // has a pre-existing double-release bug so we use set_escrow_status instead.
    let escrow_addr = client.address.clone();
    set_escrow_status(&env, &escrow_addr, id, ContractStatus::Completed);
    assert_eq!(client.get_contract(&id).status, ContractStatus::Completed);

    assert_contract_error(
        client.try_propose_client_migration(&id, &client_addr, &new_client),
        EscrowError::InvalidStatusTransition,
    );
}

/// Cancelled contract blocks proposal.
#[test]
fn migration_blocked_on_cancelled_contract() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let client = register_client(&env);

    let (client_addr, _freelancer_addr, id) = create_contract(&env, &client);
    let new_client = Address::generate(&env);

    client.cancel_contract(&id, &client_addr);
    assert_eq!(client.get_contract(&id).status, ContractStatus::Cancelled);

    assert_contract_error(
        client.try_propose_client_migration(&id, &client_addr, &new_client),
        EscrowError::InvalidStatusTransition,
    );
}

/// Refunded contract blocks proposal.
#[test]
fn migration_blocked_on_refunded_contract() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let client = register_client(&env);

    let (client_addr, _freelancer_addr, id) = create_contract(&env, &client);
    let new_client = Address::generate(&env);

    client.deposit_funds(&id, &client_addr, &total_milestone_amount());
    let all_indices = soroban_sdk::vec![&env, 0u32, 1u32, 2u32];
    client.refund_unreleased_milestones(&id, &all_indices);
    assert_eq!(client.get_contract(&id).status, ContractStatus::Refunded);

    assert_contract_error(
        client.try_propose_client_migration(&id, &client_addr, &new_client),
        EscrowError::InvalidStatusTransition,
    );
}

/// Disputed contract blocks proposal (status injected via env.as_contract).
#[test]
fn migration_blocked_on_disputed_contract() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let client = register_client(&env);

    let (client_addr, _freelancer_addr, id) = create_contract(&env, &client);
    let new_client = Address::generate(&env);

    let escrow_addr = client.address.clone();
    set_escrow_status(&env, &escrow_addr, id, ContractStatus::Disputed);
    assert_eq!(client.get_contract(&id).status, ContractStatus::Disputed);

    assert_contract_error(
        client.try_propose_client_migration(&id, &client_addr, &new_client),
        EscrowError::InvalidStatusTransition,
    );
}

// ---------------------------------------------------------------------------
// Test 5 – invalid participant addresses rejected at proposal time
// ---------------------------------------------------------------------------

/// Proposing the freelancer collapses the two roles and must be rejected
/// with `InvalidParticipant`.
#[test]
fn cannot_propose_freelancer_as_new_client() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let client = register_client(&env);

    let (client_addr, freelancer_addr, id) = create_contract(&env, &client);

    assert_contract_error(
        client.try_propose_client_migration(&id, &client_addr, &freelancer_addr),
        EscrowError::InvalidParticipant,
    );
}

/// Proposing the current client as themselves must be rejected with
/// `InvalidParticipant`.
#[test]
fn cannot_propose_current_client_as_new_client() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let client = register_client(&env);

    let (client_addr, _freelancer_addr, id) = create_contract(&env, &client);

    assert_contract_error(
        client.try_propose_client_migration(&id, &client_addr, &client_addr),
        EscrowError::InvalidParticipant,
    );
}

// ---------------------------------------------------------------------------
// Test 6 – only the current client may propose
// ---------------------------------------------------------------------------

/// A third party (non-client) attempting to propose must be rejected with
/// `UnauthorizedRole`.
#[test]
fn only_current_client_may_propose_migration() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let client = register_client(&env);

    let (_client_addr, freelancer_addr, id) = create_contract(&env, &client);
    let new_client = Address::generate(&env);
    let attacker = Address::generate(&env);

    // Freelancer as proposer is rejected
    assert_contract_error(
        client.try_propose_client_migration(&id, &freelancer_addr, &new_client),
        EscrowError::UnauthorizedRole,
    );

    // Random attacker as proposer is rejected
    assert_contract_error(
        client.try_propose_client_migration(&id, &attacker, &new_client),
        EscrowError::UnauthorizedRole,
    );
}

// ---------------------------------------------------------------------------
// Test 7 – duplicate proposal while one is already pending is rejected
// ---------------------------------------------------------------------------

/// A second proposal while a live one already exists must fail with
/// `InvalidState`.  In-flight proposals must not be overwritten.
#[test]
fn duplicate_proposal_while_pending_is_rejected() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let client = register_client(&env);

    let (client_addr, _freelancer_addr, id) = create_contract(&env, &client);
    let new_client1 = Address::generate(&env);
    let new_client2 = Address::generate(&env);

    assert!(client.propose_client_migration(&id, &client_addr, &new_client1));

    assert_contract_error(
        client.try_propose_client_migration(&id, &client_addr, &new_client2),
        EscrowError::InvalidState,
    );
}

// ---------------------------------------------------------------------------
// Test 8 – double-accept after the pending record is cleared fails
// ---------------------------------------------------------------------------

/// After a successful acceptance the pending record is removed.
/// A subsequent acceptance must fail with `InvalidState`.
#[test]
fn double_accept_after_migration_accepted_fails() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let client = register_client(&env);

    let (client_addr, _freelancer_addr, id) = create_contract(&env, &client);
    let new_client = Address::generate(&env);

    assert!(client.propose_client_migration(&id, &client_addr, &new_client));
    assert!(client.accept_client_migration(&id, &new_client));

    // Pending record is gone — second accept must fail
    assert_contract_error(
        client.try_accept_client_migration(&id, &new_client),
        EscrowError::InvalidState,
    );
}

// ---------------------------------------------------------------------------
// Test 9 – migration allowed on active (non-terminal) statuses
// ---------------------------------------------------------------------------

/// Created contract (default status) allows a proposal.
#[test]
fn migration_allowed_on_created_status() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let client = register_client(&env);

    let (client_addr, _freelancer_addr, id) = create_contract(&env, &client);
    assert_eq!(client.get_contract(&id).status, ContractStatus::Created);

    let new_client = Address::generate(&env);
    assert!(client.propose_client_migration(&id, &client_addr, &new_client));
}

/// PartiallyFunded contract allows a proposal.
#[test]
fn migration_allowed_on_partially_funded_status() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let client = register_client(&env);

    let (client_addr, _freelancer_addr, id) = create_contract(&env, &client);

    // Deposit less than the full milestone total → PartiallyFunded
    client.deposit_funds(&id, &client_addr, &super::MILESTONE_ONE);
    assert_eq!(
        client.get_contract(&id).status,
        ContractStatus::PartiallyFunded
    );

    let new_client = Address::generate(&env);
    assert!(client.propose_client_migration(&id, &client_addr, &new_client));
}

/// Fully funded contract allows a proposal.
#[test]
fn migration_allowed_on_funded_status() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let client = register_client(&env);

    let (client_addr, _freelancer_addr, id) = create_contract(&env, &client);
    client.deposit_funds(&id, &client_addr, &total_milestone_amount());
    assert_eq!(client.get_contract(&id).status, ContractStatus::Funded);

    let new_client = Address::generate(&env);
    assert!(client.propose_client_migration(&id, &client_addr, &new_client));
}

// ---------------------------------------------------------------------------
// Test 10 – PendingClientMigration expiry field matches TTL constant
// ---------------------------------------------------------------------------

/// The `expires_at_ledger` stored in the pending record must equal
/// `requested_at_ledger + PENDING_MIGRATION_TTL_LEDGERS`, matching the
/// `store_with_ttl` call in `propose_client_migration`.
#[test]
fn pending_migration_expiry_matches_ttl_constant() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let client = register_client(&env);

    let (client_addr, _freelancer_addr, id) = create_contract(&env, &client);
    let new_client = Address::generate(&env);

    let ledger_before = env.ledger().sequence();
    assert!(client.propose_client_migration(&id, &client_addr, &new_client));

    let pending: PendingClientMigration = client.get_pending_client_migration(&id);
    assert_eq!(
        pending.expires_at_ledger,
        ledger_before.saturating_add(PENDING_MIGRATION_TTL_LEDGERS),
        "expires_at_ledger must equal requested_at + PENDING_MIGRATION_TTL_LEDGERS"
    );
    assert_eq!(
        pending.requested_at_ledger, ledger_before,
        "requested_at_ledger must equal the ledger at proposal time"
    );
}
