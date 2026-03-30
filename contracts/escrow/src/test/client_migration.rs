use crate::ContractStatus;

use super::{
    assert_panics, create_sample_contract, full_funding_amount, register_escrow, setup_env,
};

#[test]
fn migration_requires_acceptance_before_finalization() {
    let env = setup_env();
    let client = register_escrow(&env);
    let (parties, contract_id) = create_sample_contract(&env, &client);

    assert!(client.request_client_migration(&contract_id, &parties.replacement_client));

    let pending = client.get_pending_client_migration(&contract_id);
    assert_eq!(pending.current_client, parties.client);
    assert_eq!(pending.proposed_client, parties.replacement_client);
    assert!(!pending.proposed_client_confirmed);
    assert!(client.has_pending_client_migration(&contract_id));

    assert_panics(|| {
        client.finalize_client_migration(&contract_id);
    });

    assert!(client.confirm_client_migration(&contract_id));
    let confirmed = client.get_pending_client_migration(&contract_id);
    assert!(confirmed.proposed_client_confirmed);

    assert!(client.finalize_client_migration(&contract_id));
    let contract = client.get_contract(&contract_id);
    assert_eq!(contract.client, parties.replacement_client);
    assert_eq!(contract.status, ContractStatus::Created);
    assert!(!client.has_pending_client_migration(&contract_id));
}

#[test]
fn confirmed_migration_transfers_client_authority() {
    let env = setup_env();
    let client = register_escrow(&env);
    let (parties, contract_id) = create_sample_contract(&env, &client);

    assert!(client.request_client_migration(&contract_id, &parties.replacement_client));
    assert!(client.confirm_client_migration(&contract_id));
    assert!(client.finalize_client_migration(&contract_id));

    assert!(client.deposit_funds(&contract_id, &full_funding_amount()));
    let auths = env.auths();

    assert_eq!(auths.len(), 1);
    assert_eq!(auths[0].0, parties.replacement_client);
    assert!(auths[0].1.sub_invocations.is_empty());
}

#[test]
fn cancel_client_migration_clears_pending_state() {
    let env = setup_env();
    let client = register_escrow(&env);
    let (parties, contract_id) = create_sample_contract(&env, &client);

    assert!(client.request_client_migration(&contract_id, &parties.replacement_client));
    assert!(client.cancel_client_migration(&contract_id));
    assert!(!client.has_pending_client_migration(&contract_id));
    assert_panics(|| {
        client.get_pending_client_migration(&contract_id);
    });
}
