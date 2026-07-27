#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env};

use crate::{
    BatchContractResult, ContractItem, Escrow, EscrowClient, EscrowError, ReleaseAuthorization,
};

fn setup() -> (Env, Address, EscrowClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let escrow_address = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &escrow_address);
    escrow.initialize(&admin);
    (env, admin, escrow)
}

fn make_item(client: &Address, freelancer: &Address) -> ContractItem {
    ContractItem {
        client: client.clone(),
        freelancer: freelancer.clone(),
        arbiter: None,
        milestones: soroban_sdk::vec![&Env::default(), 100_0000000i128, 200_0000000i128],
        release_authorization: ReleaseAuthorization::ClientOnly,
    }
}

fn make_item_with_env(env: &Env, client: &Address, freelancer: &Address) -> ContractItem {
    ContractItem {
        client: client.clone(),
        freelancer: freelancer.clone(),
        arbiter: None,
        milestones: soroban_sdk::vec![env, 100_0000000i128, 200_0000000i128],
        release_authorization: ReleaseAuthorization::ClientOnly,
    }
}

// ── Empty batch ──────────────────────────────────────────────────────────────

#[test]
fn batch_empty_returns_empty_results() {
    let (env, _, escrow) = setup();
    let caller = Address::generate(&env);
    let items = soroban_sdk::vec![&env];

    let results = escrow.create_contracts_batch(&caller, &items);
    assert_eq!(results.len(), 0);
}

// ── Over-cap batch ───────────────────────────────────────────────────────────

#[test]
#[should_panic(expected = "#44")]
fn batch_over_cap_panics() {
    let (env, _, escrow) = setup();
    let caller = Address::generate(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);

    let mut items: soroban_sdk::Vec<ContractItem> = soroban_sdk::vec![&env];
    let mut i: u32 = 0;
    while i < 11 {
        items.push_back(make_item_with_env(&env, &a, &b));
        i += 1;
    }
    escrow.create_contracts_batch(&caller, &items);
}

// ── At-cap batch (10 items) ──────────────────────────────────────────────────

#[test]
fn batch_at_cap_succeeds() {
    let (env, _, escrow) = setup();
    let caller = Address::generate(&env);
    let mut items: soroban_sdk::Vec<ContractItem> = soroban_sdk::vec![&env];
    let mut i: u32 = 0;
    while i < 10 {
        let a = Address::generate(&env);
        let b = Address::generate(&env);
        items.push_back(make_item_with_env(&env, &a, &b));
        i += 1;
    }

    let results = escrow.create_contracts_batch(&caller, &items);
    assert_eq!(results.len(), 10);

    // All should succeed with sequential IDs
    let mut j: u32 = 0;
    while j < 10 {
        let result: BatchContractResult = results.get(j).unwrap();
        assert_eq!(result.index, j);
        assert!(result.contract_id.is_some(), "item {} should succeed", j);
        assert!(result.error_code.is_none());
        j += 1;
    }
}

// ── Per-item validation errors ───────────────────────────────────────────────

#[test]
fn batch_invalid_participant_returns_error_code() {
    let (env, _, escrow) = setup();
    let caller = Address::generate(&env);
    let same = Address::generate(&env);

    let item = ContractItem {
        client: same.clone(),
        freelancer: same.clone(),
        arbiter: None,
        milestones: soroban_sdk::vec![&env, 100_0000000i128],
        release_authorization: ReleaseAuthorization::ClientOnly,
    };
    let items = soroban_sdk::vec![&env, item];

    let results = escrow.create_contracts_batch(&caller, &items);
    assert_eq!(results.len(), 1);
    let result = results.get(0).unwrap();
    assert_eq!(result.index, 0);
    assert!(result.contract_id.is_none());
    assert_eq!(
        result.error_code,
        Some(EscrowError::InvalidParticipant as u32)
    );
}

#[test]
fn batch_empty_milestones_returns_error_code() {
    let (env, _, escrow) = setup();
    let caller = Address::generate(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);

    let item = ContractItem {
        client: a,
        freelancer: b,
        arbiter: None,
        milestones: soroban_sdk::vec![&env],
        release_authorization: ReleaseAuthorization::ClientOnly,
    };
    let items = soroban_sdk::vec![&env, item];

    let results = escrow.create_contracts_batch(&caller, &items);
    assert_eq!(results.len(), 1);
    let result = results.get(0).unwrap();
    assert!(result.contract_id.is_none());
    assert_eq!(result.error_code, Some(EscrowError::EmptyMilestones as u32));
}

#[test]
fn batch_missing_arbiter_returns_error_code() {
    let (env, _, escrow) = setup();
    let caller = Address::generate(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);

    let item = ContractItem {
        client: a,
        freelancer: b,
        arbiter: None,
        milestones: soroban_sdk::vec![&env, 100_0000000i128],
        release_authorization: ReleaseAuthorization::ArbiterOnly,
    };
    let items = soroban_sdk::vec![&env, item];

    let results = escrow.create_contracts_batch(&caller, &items);
    assert_eq!(results.len(), 1);
    let result = results.get(0).unwrap();
    assert!(result.contract_id.is_none());
    assert_eq!(result.error_code, Some(EscrowError::MissingArbiter as u32));
}

#[test]
fn batch_invalid_arbiter_returns_error_code() {
    let (env, _, escrow) = setup();
    let caller = Address::generate(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);

    let item = ContractItem {
        client: a.clone(),
        freelancer: b,
        arbiter: Some(a),
        milestones: soroban_sdk::vec![&env, 100_0000000i128],
        release_authorization: ReleaseAuthorization::ArbiterOnly,
    };
    let items = soroban_sdk::vec![&env, item];

    let results = escrow.create_contracts_batch(&caller, &items);
    assert_eq!(results.len(), 1);
    let result = results.get(0).unwrap();
    assert!(result.contract_id.is_none());
    assert_eq!(result.error_code, Some(EscrowError::InvalidArbiter as u32));
}

// ── Mixed success and failure ────────────────────────────────────────────────

#[test]
fn batch_mixed_valid_and_invalid_items() {
    let (env, _, escrow) = setup();
    let caller = Address::generate(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let same = Address::generate(&env);

    let valid = make_item_with_env(&env, &a, &b);
    let invalid = ContractItem {
        client: same.clone(),
        freelancer: same,
        arbiter: None,
        milestones: soroban_sdk::vec![&env, 100_0000000i128],
        release_authorization: ReleaseAuthorization::ClientOnly,
    };

    let items = soroban_sdk::vec![&env, valid, invalid];
    let results = escrow.create_contracts_batch(&caller, &items);
    assert_eq!(results.len(), 2);

    // First succeeds
    let r0 = results.get(0).unwrap();
    assert!(r0.contract_id.is_some());
    assert!(r0.error_code.is_none());

    // Second fails
    let r1 = results.get(1).unwrap();
    assert!(r1.contract_id.is_none());
    assert_eq!(r1.error_code, Some(EscrowError::InvalidParticipant as u32));
}

// ── Per-item events ──────────────────────────────────────────────────────────

#[test]
fn batch_emits_creation_event_per_item() {
    let (env, _, escrow) = setup();
    let caller = Address::generate(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);
    let d = Address::generate(&env);

    let item1 = make_item_with_env(&env, &a, &b);
    let item2 = make_item_with_env(&env, &c, &d);
    let items = soroban_sdk::vec![&env, item1, item2];

    let results = escrow.create_contracts_batch(&caller, &items);
    assert_eq!(results.len(), 2);

    let id1 = results.get(0).unwrap().contract_id.unwrap();
    let id2 = results.get(1).unwrap().contract_id.unwrap();

    // Each created contract gets sequential IDs
    assert_eq!(id2, id1 + 1);

    // Verify contracts exist via get_contract
    let c1 = escrow.get_contract(&id1);
    assert_eq!(c1.client, a);
    assert_eq!(c1.freelancer, b);

    let c2 = escrow.get_contract(&id2);
    assert_eq!(c2.client, c);
    assert_eq!(c2.freelancer, d);
}

// ── Single item batch ────────────────────────────────────────────────────────

#[test]
fn batch_single_item_works() {
    let (env, _, escrow) = setup();
    let caller = Address::generate(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);

    let item = make_item_with_env(&env, &a, &b);
    let items = soroban_sdk::vec![&env, item];

    let results = escrow.create_contracts_batch(&caller, &items);
    assert_eq!(results.len(), 1);

    let result = results.get(0).unwrap();
    assert_eq!(result.index, 0);
    assert!(result.contract_id.is_some());
    assert!(result.error_code.is_none());
}

// ── Batch with arbiter required but provided ─────────────────────────────────

#[test]
fn batch_valid_arbiter_succeeds() {
    let (env, _, escrow) = setup();
    let caller = Address::generate(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let arb = Address::generate(&env);

    let item = ContractItem {
        client: a,
        freelancer: b,
        arbiter: Some(arb),
        milestones: soroban_sdk::vec![&env, 100_0000000i128],
        release_authorization: ReleaseAuthorization::ArbiterOnly,
    };
    let items = soroban_sdk::vec![&env, item];

    let results = escrow.create_contracts_batch(&caller, &items);
    assert_eq!(results.len(), 1);
    let result = results.get(0).unwrap();
    assert!(result.contract_id.is_some());
    assert!(result.error_code.is_none());
}
