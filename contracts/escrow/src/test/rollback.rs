use crate::{
    Contract, ContractStatus, DataKey, DisputeResolution, Error, Escrow, EscrowClient,
    ReleaseAuthorization,
};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events},
    token, vec, Address, Env, Symbol, TryFromVal,
};

struct RollbackContext {
    env: Env,
    escrow_address: Address,
    admin: Address,
    client: Address,
    freelancer: Address,
    arbiter: Address,
    contract_id: u32,
    token: Address,
}

impl RollbackContext {
    fn escrow(&self) -> EscrowClient<'_> {
        EscrowClient::new(&self.env, &self.escrow_address)
    }
}

fn setup(deposit: i128) -> RollbackContext {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let client_address = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let escrow_address = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &escrow_address);
    escrow.initialize(&admin);

    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    escrow.bind_settlement_token(&admin, &token);

    let contract_id = escrow.create_contract(
        &client_address,
        &freelancer,
        &Some(arbiter.clone()),
        &vec![&env, 100_i128, 200_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    token::StellarAssetClient::new(&env, &token).mint(&client_address, &300_i128);
    escrow.deposit_funds(&contract_id, &client_address, &deposit);

    RollbackContext {
        env,
        escrow_address,
        admin,
        client: client_address,
        freelancer,
        arbiter,
        contract_id,
        token,
    }
}

fn set_status(context: &RollbackContext, status: ContractStatus) {
    context.env.as_contract(&context.escrow_address, || {
        let key = DataKey::Contract(context.contract_id);
        let mut contract: Contract = context.env.storage().persistent().get(&key).unwrap();
        contract.status = status;
        context.env.storage().persistent().set(&key, &contract);
    });
}

fn has_rollback_record(context: &RollbackContext) -> bool {
    context.env.as_contract(&context.escrow_address, || {
        context
            .env
            .storage()
            .persistent()
            .has(&DataKey::DisputeRollback(context.contract_id))
    })
}

fn rollback_event_count(context: &RollbackContext) -> usize {
    let topic = symbol_short!("rollback");
    context
        .env
        .events()
        .all()
        .iter()
        .filter(|event| {
            event.0 == context.escrow_address
                && Symbol::try_from_val(&context.env, &event.1.get(0).unwrap()).ok()
                    == Some(topic.clone())
        })
        .count()
}

#[test]
fn rollback_restores_funded_state_without_changing_value() {
    let context = setup(300);
    let escrow = context.escrow();
    let token = token::Client::new(&context.env, &context.token);

    let contract_before = escrow.get_contract(&context.contract_id);
    let milestones_before = escrow.get_milestones(&context.contract_id);
    let escrow_balance = token.balance(&context.escrow_address);
    let client_balance = token.balance(&context.client);
    let freelancer_balance = token.balance(&context.freelancer);

    escrow.raise_dispute(&context.contract_id, &context.client);
    assert!(has_rollback_record(&context));
    assert!(escrow.rollback_dispute(&context.contract_id));
    let auths = context.env.auths();
    assert_eq!(auths.len(), 1);
    assert_eq!(auths[0].0, context.admin);

    assert_eq!(escrow.get_contract(&context.contract_id), contract_before);
    assert_eq!(
        escrow.get_milestones(&context.contract_id),
        milestones_before
    );
    assert_eq!(token.balance(&context.escrow_address), escrow_balance);
    assert_eq!(token.balance(&context.client), client_balance);
    assert_eq!(token.balance(&context.freelancer), freelancer_balance);
    assert!(!has_rollback_record(&context));
}

#[test]
fn rollback_restores_partially_funded_state() {
    let context = setup(100);
    let escrow = context.escrow();

    assert_eq!(
        escrow.get_contract(&context.contract_id).status,
        ContractStatus::PartiallyFunded
    );
    escrow.raise_dispute(&context.contract_id, &context.freelancer);
    escrow.rollback_dispute(&context.contract_id);

    assert_eq!(
        escrow.get_contract(&context.contract_id).status,
        ContractStatus::PartiallyFunded
    );
}

#[test]
fn rollback_requires_admin_authorization() {
    let context = setup(300);
    let escrow = context.escrow();
    escrow.raise_dispute(&context.contract_id, &context.client);
    context.env.mock_auths(&[]);

    assert!(escrow.try_rollback_dispute(&context.contract_id).is_err());
    assert_eq!(
        escrow.get_contract(&context.contract_id).status,
        ContractStatus::Disputed
    );
    assert!(has_rollback_record(&context));
}

#[test]
fn rollback_rejects_missing_contract_and_non_disputed_states() {
    let context = setup(300);
    let escrow = context.escrow();

    super::assert_contract_error(
        escrow.try_rollback_dispute(&999_u32),
        Error::ContractNotFound,
    );

    for status in [
        ContractStatus::Created,
        ContractStatus::Funded,
        ContractStatus::Completed,
        ContractStatus::Cancelled,
        ContractStatus::Refunded,
    ] {
        set_status(&context, status);
        super::assert_contract_error(
            escrow.try_rollback_dispute(&context.contract_id),
            Error::RollbackNotAllowed,
        );
    }
}

#[test]
fn rollback_rejects_changed_state() {
    let context = setup(300);
    let escrow = context.escrow();
    escrow.raise_dispute(&context.contract_id, &context.client);

    context.env.as_contract(&context.escrow_address, || {
        let key = DataKey::Contract(context.contract_id);
        let mut contract: Contract = context.env.storage().persistent().get(&key).unwrap();
        contract.refunded_amount = 1;
        context.env.storage().persistent().set(&key, &contract);
    });

    super::assert_contract_error(
        escrow.try_rollback_dispute(&context.contract_id),
        Error::RollbackStateChanged,
    );
    assert_eq!(rollback_event_count(&context), 0);
}

#[test]
fn refund_closes_rollback_window() {
    let context = setup(300);
    let escrow = context.escrow();
    escrow.raise_dispute(&context.contract_id, &context.client);
    escrow.refund_unreleased_milestones(&context.contract_id, &vec![&context.env, 0_u32]);

    assert!(!has_rollback_record(&context));
    super::assert_contract_error(
        escrow.try_rollback_dispute(&context.contract_id),
        Error::RollbackNotAllowed,
    );
}

#[test]
fn resolution_and_finalization_close_rollback_window() {
    let resolved = setup(300);
    let resolved_escrow = resolved.escrow();
    resolved_escrow.raise_dispute(&resolved.contract_id, &resolved.client);
    resolved_escrow.resolve_dispute(
        &resolved.contract_id,
        &resolved.arbiter,
        &DisputeResolution::FullRefund,
    );
    assert!(!has_rollback_record(&resolved));
    super::assert_contract_error(
        resolved_escrow.try_rollback_dispute(&resolved.contract_id),
        Error::RollbackNotAllowed,
    );

    let finalized = setup(300);
    let finalized_escrow = finalized.escrow();
    finalized_escrow.raise_dispute(&finalized.contract_id, &finalized.client);
    finalized_escrow.finalize_contract(&finalized.contract_id, &finalized.client);
    assert!(!has_rollback_record(&finalized));
    super::assert_contract_error(
        finalized_escrow.try_rollback_dispute(&finalized.contract_id),
        Error::AlreadyFinalized,
    );
}

#[test]
fn rollback_is_single_use_and_emits_expected_event() {
    let context = setup(300);
    let escrow = context.escrow();
    escrow.raise_dispute(&context.contract_id, &context.client);
    escrow.rollback_dispute(&context.contract_id);

    let topic = symbol_short!("rollback");
    let event = context
        .env
        .events()
        .all()
        .iter()
        .find(|event| {
            event.0 == context.escrow_address
                && Symbol::try_from_val(&context.env, &event.1.get(0).unwrap()).ok()
                    == Some(topic.clone())
        })
        .unwrap();
    assert_eq!(event.1.len(), 2);
    assert_eq!(
        u32::try_from_val(&context.env, &event.1.get(1).unwrap()).unwrap(),
        context.contract_id
    );
    let data =
        <(Address, ContractStatus, ContractStatus, u64)>::try_from_val(&context.env, &event.2)
            .unwrap();
    assert_eq!(
        data,
        (
            context.admin.clone(),
            ContractStatus::Disputed,
            ContractStatus::Funded,
            context.env.ledger().timestamp(),
        )
    );
    assert_eq!(rollback_event_count(&context), 1);

    super::assert_contract_error(
        escrow.try_rollback_dispute(&context.contract_id),
        Error::RollbackNotAllowed,
    );
}

#[test]
fn pause_blocks_rollback() {
    let context = setup(300);
    let escrow = context.escrow();
    escrow.raise_dispute(&context.contract_id, &context.client);
    escrow.pause();

    super::assert_contract_error(
        escrow.try_rollback_dispute(&context.contract_id),
        Error::ContractPaused,
    );
    assert!(has_rollback_record(&context));
}
