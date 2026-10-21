//! Tests for issue #1707: terms and per-milestone deliverable hashes.

use crate::{
    Escrow, EscrowClient, EscrowError, Error, ReleaseAuthorization,
};
use soroban_sdk::{
    testutils::Address as _, Address, BytesN, Env, Vec,
};

/// Helper: create an Env + registered escrow + initialized client.
/// Returns (env, escrow_client, admin_addr).
fn setup() -> (Env, EscrowClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    (env, client, admin)
}

/// Helper: generate a deterministic 32-byte hash.
fn hash32(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

/// Helper: create a 2-milestone Created contract and return (id, client_addr, freelancer_addr).
fn create_two_milestone_contract(
    env: &Env,
    client: &EscrowClient,
) -> (u32, Address, Address) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let milestones = soroban_sdk::vec![env, 100_i128, 200_i128];
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    (id, client_addr, freelancer_addr)
}

/// Helper: bind a settlement token, mint, and deposit full amount.
fn fund_contract(
    env: &Env,
    client: &EscrowClient,
    admin: &Address,
    id: u32,
    client_addr: &Address,
    total: i128,
) -> Address {
    let token = env.register_stellar_asset_contract(admin.clone());
    client.bind_settlement_token(admin, &token);
    use soroban_sdk::token::StellarAssetClient;
    StellarAssetClient::new(env, &token).mint(client_addr, &total);
    client.deposit_funds(&id, client_addr, &total);
    token
}

// ── set_terms ──────────────────────────────────────────────────────────────

#[test]
fn set_terms_then_fund_succeeds() {
    let (env, client, _admin) = setup();
    let (id, client_addr, _freelancer_addr) = create_two_milestone_contract(&env, &client);

    let terms_hash = hash32(&env, 0x01);
    let milestone_hashes = Vec::new(&env);

    // set_terms before deposit should succeed.
    client.set_terms(&id, &client_addr, &terms_hash, &milestone_hashes);

    // Verify terms were stored.
    let stored = client.get_terms(&id);
    assert!(stored.is_some());
    let (stored_hash, stored_specs) = stored.unwrap();
    assert_eq!(stored_hash, terms_hash);
    assert_eq!(stored_specs.len(), 0);
}

#[test]
fn set_terms_with_milestone_hashes_succeeds() {
    let (env, client, _admin) = setup();
    let (id, client_addr, _freelancer_addr) = create_two_milestone_contract(&env, &client);

    let terms_hash = hash32(&env, 0x01);
    let milestone_hashes = soroban_sdk::vec![
        &env,
        hash32(&env, 0x10),
        hash32(&env, 0x20),
    ];

    client.set_terms(&id, &client_addr, &terms_hash, &milestone_hashes);

    let stored = client.get_terms(&id).unwrap();
    assert_eq!(stored.0, terms_hash);
    assert_eq!(stored.1.len(), 2);
    assert_eq!(stored.1.get(0).unwrap(), hash32(&env, 0x10));
    assert_eq!(stored.1.get(1).unwrap(), hash32(&env, 0x20));
}

#[test]
fn set_terms_after_deposit_rejected() {
    let (env, client, admin) = setup();
    let (id, client_addr, _freelancer_addr) = create_two_milestone_contract(&env, &client);

    fund_contract(&env, &client, &admin, id, &client_addr, 300_i128);

    // set_terms should now fail — terms are frozen after deposit.
    let terms_hash = hash32(&env, 0x01);
    let milestone_hashes = Vec::new(&env);
    let result = client.try_set_terms(&id, &client_addr, &terms_hash, &milestone_hashes);
    crate::test::assert_contract_error(result, EscrowError::InvalidState);
}

#[test]
fn set_terms_length_mismatch_rejected() {
    let (env, client, _admin) = setup();
    let (id, client_addr, _freelancer_addr) = create_two_milestone_contract(&env, &client);

    let terms_hash = hash32(&env, 0x01);
    // Wrong length: 1 hash for 2 milestones.
    let milestone_hashes = soroban_sdk::vec![&env, hash32(&env, 0x10)];

    let result = client.try_set_terms(&id, &client_addr, &terms_hash, &milestone_hashes);
    crate::test::assert_contract_error(result, EscrowError::InvalidMetadata);
}

#[test]
fn set_terms_non_client_rejected() {
    let (env, client, _admin) = setup();
    let (id, _client_addr, freelancer_addr) = create_two_milestone_contract(&env, &client);

    let terms_hash = hash32(&env, 0x01);
    let milestone_hashes = Vec::new(&env);

    // Freelancer cannot set terms.
    let result = client.try_set_terms(&id, &freelancer_addr, &terms_hash, &milestone_hashes);
    crate::test::assert_contract_error(result, EscrowError::UnauthorizedRole);
}

#[test]
fn get_terms_returns_none_when_unset() {
    let (env, client, _admin) = setup();
    let (id, _client_addr, _freelancer_addr) = create_two_milestone_contract(&env, &client);

    assert!(client.get_terms(&id).is_none());
}

// ── deliver ────────────────────────────────────────────────────────────────

#[test]
fn deliver_recorded_and_overwritable() {
    let (env, client, _admin) = setup();
    let (id, _client_addr, freelancer_addr) = create_two_milestone_contract(&env, &client);

    // Initially no delivery hash.
    assert!(client.get_delivery(&id, &0).is_none());

    // Freelancer records a delivery hash.
    let hash1 = hash32(&env, 0xA0);
    client.deliver(&id, &freelancer_addr, &0, &hash1);
    assert_eq!(client.get_delivery(&id, &0).unwrap(), hash1);

    // Freelancer overwrites the delivery hash.
    let hash2 = hash32(&env, 0xB0);
    client.deliver(&id, &freelancer_addr, &0, &hash2);
    assert_eq!(client.get_delivery(&id, &0).unwrap(), hash2);
}

#[test]
fn deliver_after_release_rejected() {
    let (env, client, admin) = setup();
    let (id, client_addr, freelancer_addr) = create_two_milestone_contract(&env, &client);

    // Fund the contract.
    fund_contract(&env, &client, &admin, id, &client_addr, 300_i128);

    // Approve and release milestone 0.
    client.approve_milestone_release(&id, &client_addr, &0);
    client.release_milestone(&id, &client_addr, &0);

    // Delivering after release should fail.
    let hash = hash32(&env, 0xA0);
    let result = client.try_deliver(&id, &freelancer_addr, &0, &hash);
    crate::test::assert_contract_error(result, Error::MilestoneAlreadyReleased);
}

#[test]
fn deliver_client_cannot_deliver() {
    let (env, client, _admin) = setup();
    let (id, client_addr, _freelancer_addr) = create_two_milestone_contract(&env, &client);

    let hash = hash32(&env, 0xA0);
    // Client cannot deliver — only the freelancer can.
    let result = client.try_deliver(&id, &client_addr, &0, &hash);
    crate::test::assert_contract_error(result, EscrowError::UnauthorizedRole);
}

#[test]
fn deliver_index_out_of_bounds_rejected() {
    let (env, client, _admin) = setup();
    let (id, _client_addr, freelancer_addr) = create_two_milestone_contract(&env, &client);

    let hash = hash32(&env, 0xA0);
    // Index 5 exceeds milestone count of 2.
    let result = client.try_deliver(&id, &freelancer_addr, &5, &hash);
    crate::test::assert_contract_error(result, Error::IndexOutOfBounds);
}

#[test]
fn get_delivery_returns_none_when_unset() {
    let (env, client, _admin) = setup();
    let (id, _client_addr, _freelancer_addr) = create_two_milestone_contract(&env, &client);

    assert!(client.get_delivery(&id, &0).is_none());
}

// ── TTL assertions ─────────────────────────────────────────────────────────

/// Assert that `set_terms` bumps the persistent TTL for `DataKey::Terms`.
#[test]
fn set_terms_bumps_ttl_for_terms_key() {
    use soroban_sdk::testutils::storage::Persistent as _;

    let (env, client, _admin) = setup();
    let (id, client_addr, _freelancer_addr) = create_two_milestone_contract(&env, &client);

    let terms_hash = hash32(&env, 0x01);
    let milestone_hashes = Vec::new(&env);
    client.set_terms(&id, &client_addr, &terms_hash, &milestone_hashes);

    // TTL must be set (> 0) immediately after the write.
    let ttl: u32 = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get_ttl(&crate::DataKey::Terms(id))
    });
    assert!(
        ttl > 0,
        "set_terms must bump TTL for DataKey::Terms (got {})",
        ttl
    );
    assert!(
        ttl >= crate::ttl::PERSISTENT_BUMP_THRESHOLD,
        "TTL after set_terms must be at least PERSISTENT_BUMP_THRESHOLD (got {})",
        ttl
    );
}

/// Assert that `set_terms` with milestone hashes bumps TTL for `DataKey::MilestoneSpecs`.
#[test]
fn set_terms_bumps_ttl_for_milestone_specs_key() {
    use soroban_sdk::testutils::storage::Persistent as _;

    let (env, client, _admin) = setup();
    let (id, client_addr, _freelancer_addr) = create_two_milestone_contract(&env, &client);

    let terms_hash = hash32(&env, 0x01);
    let milestone_hashes = soroban_sdk::vec![&env, hash32(&env, 0x10), hash32(&env, 0x20)];
    client.set_terms(&id, &client_addr, &terms_hash, &milestone_hashes);

    let ttl: u32 = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get_ttl(&crate::DataKey::MilestoneSpecs(id))
    });
    assert!(
        ttl > 0,
        "set_terms must bump TTL for DataKey::MilestoneSpecs (got {})",
        ttl
    );
    assert!(
        ttl >= crate::ttl::PERSISTENT_BUMP_THRESHOLD,
        "TTL after set_terms must be at least PERSISTENT_BUMP_THRESHOLD for MilestoneSpecs (got {})",
        ttl
    );
}

/// Assert that `deliver` bumps the persistent TTL for `DataKey::Delivery`.
#[test]
fn deliver_bumps_ttl_for_delivery_key() {
    use soroban_sdk::testutils::storage::Persistent as _;

    let (env, client, _admin) = setup();
    let (id, _client_addr, freelancer_addr) = create_two_milestone_contract(&env, &client);

    let hash = hash32(&env, 0xA0);
    client.deliver(&id, &freelancer_addr, &0, &hash);

    let ttl: u32 = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get_ttl(&crate::DataKey::Delivery(id, 0))
    });
    assert!(
        ttl > 0,
        "deliver must bump TTL for DataKey::Delivery (got {})",
        ttl
    );
    assert!(
        ttl >= crate::ttl::PERSISTENT_BUMP_THRESHOLD,
        "TTL after deliver must be at least PERSISTENT_BUMP_THRESHOLD (got {})",
        ttl
    );
}
