//! Deterministic failure handling for the `simulate_*` dry runs (#1524).
//!
//! * `simulate_release_milestone` / `simulate_refund` never panic: every
//!   failure, including malformed persisted state, is an `error_code`.
//! * `simulate_deposit_funds` / `simulate_create_contract` fail with a typed
//!   contract error (observable through the `try_*` client methods).
//! * A dry run never writes, and the same inputs give the same result.
//! * `simulate_refund` projects each requested milestone exactly once.
//!
//! State is seeded directly with `env.as_contract`, so these tests depend only
//! on `simulate.rs`, `storage.rs`, and the types.
//!
//! ```sh
//! cargo test -p escrow --lib simulate_recovery
//! ```

use crate::{
    Contract, ContractStatus, DataKey, Error, Escrow, EscrowClient, GovernedParameters, Milestone,
    ReleaseAuthorization,
};
use soroban_sdk::{
    symbol_short, testutils::Address as _, vec, Address, Env, IntoVal, Symbol, Val, Vec,
};

const CONTRACT_ID: u32 = 1;

struct Fixture {
    env: Env,
    escrow: Address,
    client: Address,
}

impl Fixture {
    fn new() -> Self {
        let env = Env::default();
        let escrow = env.register(Escrow, ());
        let client = Address::generate(&env);
        let f = Fixture {
            env,
            escrow,
            client,
        };
        f.set(&DataKey::Initialized, &true);
        f
    }

    fn escrow_client(&self) -> EscrowClient<'_> {
        EscrowClient::new(&self.env, &self.escrow)
    }

    fn set<K: IntoVal<Env, Val>, V: IntoVal<Env, Val>>(&self, key: &K, value: &V) {
        self.env.as_contract(&self.escrow, || {
            self.env.storage().persistent().set(key, value);
        });
    }

    fn milestone_key(&self) -> (DataKey, Symbol) {
        (
            DataKey::Contract(CONTRACT_ID),
            Symbol::new(&self.env, "milestones"),
        )
    }

    fn seed(&self, status: ContractStatus, amounts: &[i128], funded: i128) {
        let contract = Contract {
            client: self.client.clone(),
            freelancer: Address::generate(&self.env),
            arbiter: None,
            status,
            total_deposited: funded,
            funded_amount: funded,
            released_amount: 0,
            refunded_amount: 0,
            release_authorization: ReleaseAuthorization::ClientOnly,
            reputation_issued: false,
        };
        let mut milestones = Vec::new(&self.env);
        for amount in amounts {
            milestones.push_back(Milestone {
                amount: *amount,
                funded_amount: *amount,
                released: false,
                refunded: false,
                work_evidence: None,
                refunded_amount: 0,
                deadline: None,
            });
        }
        self.set(&DataKey::Contract(CONTRACT_ID), &contract);
        self.set(&self.milestone_key(), &milestones);
    }

    fn update_milestone(&self, index: u32, f: impl FnOnce(&mut Milestone)) {
        self.env.as_contract(&self.escrow, || {
            let key = self.milestone_key();
            let mut milestones: Vec<Milestone> = self.env.storage().persistent().get(&key).unwrap();
            let mut m = milestones.get(index).unwrap();
            f(&mut m);
            milestones.set(index, m);
            self.env.storage().persistent().set(&key, &milestones);
        });
    }

    fn update_contract(&self, f: impl FnOnce(&mut Contract)) {
        self.env.as_contract(&self.escrow, || {
            let key = DataKey::Contract(CONTRACT_ID);
            let mut c: Contract = self.env.storage().persistent().get(&key).unwrap();
            f(&mut c);
            self.env.storage().persistent().set(&key, &c);
        });
    }
}

const CORRUPT: u32 = Error::StorageInvariantViolated as u32;

/// The panicking dry runs declare no typed error, so the generated `try_*`
/// client surfaces the contract error as a host `Error` carrying its code.
fn contract_error(error: Error) -> soroban_sdk::Error {
    soroban_sdk::Error::from_contract_error(error as u32)
}

// ── simulate_refund: projection correctness ──────────────────────────────────

/// Regression: the projected total used to be accumulated twice (once by
/// `refund::validate_milestones`, once more by a second loop).
#[test]
fn refund_projects_each_milestone_exactly_once() {
    let f = Fixture::new();
    f.seed(ContractStatus::Funded, &[100, 200, 300], 600);

    let one = f
        .escrow_client()
        .simulate_refund(&CONTRACT_ID, &vec![&f.env, 1u32]);
    assert!(one.would_succeed);
    assert_eq!(one.total_refund_amount, 200);
    assert_eq!(one.projected_refunded_amount, 200);
    assert_eq!(one.projected_status, ContractStatus::Funded);
    assert!(!one.would_complete_contract);

    let all = f
        .escrow_client()
        .simulate_refund(&CONTRACT_ID, &vec![&f.env, 0u32, 1, 2]);
    assert!(all.would_succeed);
    assert_eq!(all.total_refund_amount, 600);
    assert_eq!(all.projected_status, ContractStatus::Refunded);
    assert!(all.would_complete_contract);
}

/// Refunding exactly the remaining balance is valid; one stroop short is not.
#[test]
fn refund_balance_boundary() {
    let f = Fixture::new();
    f.seed(ContractStatus::Funded, &[100, 200], 300);
    let ok = f
        .escrow_client()
        .simulate_refund(&CONTRACT_ID, &vec![&f.env, 0u32, 1]);
    assert!(ok.would_succeed);

    f.update_contract(|c| c.funded_amount = 299);
    let short = f
        .escrow_client()
        .simulate_refund(&CONTRACT_ID, &vec![&f.env, 0u32, 1]);
    assert!(!short.would_succeed);
    assert_eq!(short.error_code, Some(Error::InsufficientFunds as u32));
}

/// A released milestone reports `MilestoneAlreadyReleased`, matching
/// `refund_unreleased_milestones` (it used to report `AlreadyRefunded`).
#[test]
fn refund_of_released_milestone_matches_entrypoint_error() {
    let f = Fixture::new();
    f.seed(ContractStatus::Funded, &[100, 200], 300);
    f.update_milestone(0, |m| m.released = true);

    let r = f
        .escrow_client()
        .simulate_refund(&CONTRACT_ID, &vec![&f.env, 0u32]);
    assert!(!r.would_succeed);
    assert_eq!(r.error_code, Some(Error::MilestoneAlreadyReleased as u32));
}

#[test]
fn refund_rejections_are_codes_not_panics() {
    let f = Fixture::new();
    f.seed(ContractStatus::Funded, &[100], 100);
    let c = f.escrow_client();

    let oob = c.simulate_refund(&CONTRACT_ID, &vec![&f.env, 5u32]);
    assert_eq!(oob.error_code, Some(Error::IndexOutOfBounds as u32));

    let empty = c.simulate_refund(&CONTRACT_ID, &Vec::new(&f.env));
    assert_eq!(empty.error_code, Some(Error::EmptyRefundRequest as u32));

    let dup = c.simulate_refund(&CONTRACT_ID, &vec![&f.env, 0u32, 0]);
    assert_eq!(
        dup.error_code,
        Some(Error::DuplicateMilestoneInRefund as u32)
    );

    let missing = c.simulate_refund(&7, &vec![&f.env, 0u32]);
    assert_eq!(missing.error_code, Some(Error::ContractNotFound as u32));

    let zero = c.simulate_refund(&0, &vec![&f.env, 0u32]);
    assert_eq!(zero.error_code, Some(Error::InvalidContractId as u32));
}

#[test]
fn refund_with_unrepresentable_accounting_reports_overflow() {
    let f = Fixture::new();
    f.seed(ContractStatus::Funded, &[100], 100);
    f.update_contract(|c| {
        c.funded_amount = i128::MIN;
        c.released_amount = 1;
    });
    let r = f
        .escrow_client()
        .simulate_refund(&CONTRACT_ID, &vec![&f.env, 0u32]);
    assert_eq!(r.error_code, Some(Error::PotentialOverflow as u32));
}

// ── Non-panicking dry runs under malformed storage ───────────────────────────

#[test]
fn refund_reports_malformed_contract_as_code() {
    let f = Fixture::new();
    f.seed(ContractStatus::Funded, &[100], 100);
    f.set(&DataKey::Contract(CONTRACT_ID), &symbol_short!("junk"));

    let r = f
        .escrow_client()
        .simulate_refund(&CONTRACT_ID, &vec![&f.env, 0u32]);
    assert!(!r.would_succeed);
    assert_eq!(r.error_code, Some(CORRUPT));
}

#[test]
fn refund_reports_malformed_milestone_as_code() {
    let f = Fixture::new();
    f.seed(ContractStatus::Funded, &[100], 100);
    let bad: Vec<Val> = vec![&f.env, 5u32.into_val(&f.env)];
    f.set(&f.milestone_key(), &bad);

    let r = f
        .escrow_client()
        .simulate_refund(&CONTRACT_ID, &vec![&f.env, 0u32]);
    assert_eq!(r.error_code, Some(CORRUPT));
}

#[test]
fn release_reports_malformed_contract_and_milestones_as_codes() {
    let f = Fixture::new();
    f.seed(ContractStatus::Funded, &[100], 100);
    let c = f.escrow_client();

    f.set(&f.milestone_key(), &true);
    let r = c.simulate_release_milestone(&CONTRACT_ID, &f.client, &0);
    assert!(!r.would_succeed);
    assert_eq!(r.error_code, Some(CORRUPT));

    f.set(&DataKey::Contract(CONTRACT_ID), &3i128);
    let r = c.simulate_release_milestone(&CONTRACT_ID, &f.client, &0);
    assert_eq!(r.error_code, Some(CORRUPT));
}

#[test]
fn release_reports_malformed_fee_accumulator_as_code() {
    let f = Fixture::new();
    f.seed(ContractStatus::Funded, &[100], 100);
    f.set(&DataKey::AccumulatedProtocolFees, &symbol_short!("junk"));

    let r = f
        .escrow_client()
        .simulate_release_milestone(&CONTRACT_ID, &f.client, &0);
    assert_eq!(r.error_code, Some(CORRUPT));
}

#[test]
fn malformed_safety_flags_fail_closed_in_dry_runs() {
    let f = Fixture::new();
    f.seed(ContractStatus::Funded, &[100], 100);
    let c = f.escrow_client();

    f.set(&DataKey::Paused, &1u32);
    assert_eq!(
        c.simulate_refund(&CONTRACT_ID, &vec![&f.env, 0u32])
            .error_code,
        Some(CORRUPT)
    );
    assert_eq!(
        c.simulate_release_milestone(&CONTRACT_ID, &f.client, &0)
            .error_code,
        Some(CORRUPT)
    );

    f.set(&DataKey::Paused, &false);
    f.set(&DataKey::Initialized, &symbol_short!("yes"));
    assert_eq!(
        c.simulate_refund(&CONTRACT_ID, &vec![&f.env, 0u32])
            .error_code,
        Some(CORRUPT)
    );
}

#[test]
fn pause_and_emergency_are_reported_distinctly() {
    let f = Fixture::new();
    f.seed(ContractStatus::Funded, &[100], 100);
    let c = f.escrow_client();

    f.set(&DataKey::Emergency, &true);
    assert_eq!(
        c.simulate_refund(&CONTRACT_ID, &vec![&f.env, 0u32])
            .error_code,
        Some(Error::EmergencyActive as u32)
    );
    f.set(&DataKey::Paused, &true);
    assert_eq!(
        c.simulate_release_milestone(&CONTRACT_ID, &f.client, &0)
            .error_code,
        Some(Error::ContractPaused as u32)
    );
}

#[test]
fn not_initialized_is_reported() {
    let f = Fixture::new();
    f.seed(ContractStatus::Funded, &[100], 100);
    f.env.as_contract(&f.escrow, || {
        f.env.storage().persistent().remove(&DataKey::Initialized);
    });
    let r = f
        .escrow_client()
        .simulate_refund(&CONTRACT_ID, &vec![&f.env, 0u32]);
    assert_eq!(r.error_code, Some(Error::NotInitialized as u32));
}

// ── Release success path ─────────────────────────────────────────────────────

#[test]
fn release_success_projection() {
    let f = Fixture::new();
    f.seed(ContractStatus::Funded, &[100, 200], 300);
    let r = f
        .escrow_client()
        .simulate_release_milestone(&CONTRACT_ID, &f.client, &1);
    assert!(r.would_succeed);
    assert_eq!(r.error_code, None);
    assert_eq!(r.gross_amount, 200);
    assert_eq!(r.net_amount, 200);
    assert_eq!(r.projected_released_amount, 200);
    assert!(!r.would_complete_contract);
}

// ── Panicking dry runs: typed errors ─────────────────────────────────────────

#[test]
fn create_contract_dry_run_reads_next_id_and_total() {
    let f = Fixture::new();
    f.set(&DataKey::NextContractId, &5u32);
    let out = f.escrow_client().simulate_create_contract(
        &f.client,
        &Address::generate(&f.env),
        &None,
        &vec![&f.env, 100i128, 250],
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(out.contract_id, 5);
    assert_eq!(out.total_amount, 350);
}

#[test]
fn create_contract_dry_run_rejects_malformed_next_id() {
    let f = Fixture::new();
    f.set(&DataKey::NextContractId, &symbol_short!("junk"));
    let res = f.escrow_client().try_simulate_create_contract(
        &f.client,
        &Address::generate(&f.env),
        &None,
        &vec![&f.env, 100i128],
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(
        res,
        Err(Ok(contract_error(Error::StorageInvariantViolated)))
    );
}

/// A malformed governed-parameters entry must not silently lift the cap.
#[test]
fn create_contract_dry_run_rejects_malformed_governed_parameters() {
    let f = Fixture::new();
    f.set(&DataKey::GovernedParameters, &7u32);
    let res = f.escrow_client().try_simulate_create_contract(
        &f.client,
        &Address::generate(&f.env),
        &None,
        &vec![&f.env, 100i128],
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(
        res,
        Err(Ok(contract_error(Error::StorageInvariantViolated)))
    );
}

#[test]
fn create_contract_dry_run_honours_governed_cap() {
    let f = Fixture::new();
    f.set(
        &DataKey::GovernedParameters,
        &GovernedParameters {
            protocol_fee_bps: 0,
            max_escrow_total_stroops: 100,
        },
    );
    let res = f.escrow_client().try_simulate_create_contract(
        &f.client,
        &Address::generate(&f.env),
        &None,
        &vec![&f.env, 60i128, 41],
        &ReleaseAuthorization::ClientOnly,
    );
    assert!(res.is_err());
}

#[test]
fn deposit_dry_run_rejects_malformed_contract_with_typed_error() {
    let f = Fixture::new();
    let token = f
        .env
        .register_stellar_asset_contract_v2(Address::generate(&f.env))
        .address();
    f.set(&DataKey::SettlementToken, &token);
    f.seed(ContractStatus::Created, &[100], 0);
    f.set(&DataKey::Contract(CONTRACT_ID), &symbol_short!("junk"));

    let res = f
        .escrow_client()
        .try_simulate_deposit_funds(&CONTRACT_ID, &f.client, &50);
    assert_eq!(
        res,
        Err(Ok(contract_error(Error::StorageInvariantViolated)))
    );
}

#[test]
fn deposit_dry_run_projection() {
    let f = Fixture::new();
    let token = f
        .env
        .register_stellar_asset_contract_v2(Address::generate(&f.env))
        .address();
    f.set(&DataKey::SettlementToken, &token);
    f.seed(ContractStatus::Created, &[100, 200], 0);
    let c = f.escrow_client();

    let partial = c.simulate_deposit_funds(&CONTRACT_ID, &f.client, &120);
    assert_eq!(partial.new_funded_amount, 120);
    assert_eq!(partial.projected_status, ContractStatus::PartiallyFunded);
    assert_eq!(partial.total_milestone_amount, 300);

    let full = c.simulate_deposit_funds(&CONTRACT_ID, &f.client, &300);
    assert_eq!(full.projected_status, ContractStatus::Funded);

    // One stroop over the schedule is rejected.
    assert!(c
        .try_simulate_deposit_funds(&CONTRACT_ID, &f.client, &301)
        .is_err());
}

// ── Determinism and no side effects ──────────────────────────────────────────

#[test]
fn dry_runs_are_repeatable_and_do_not_write() {
    let f = Fixture::new();
    f.seed(ContractStatus::Funded, &[100, 200], 300);
    f.set(&DataKey::NextContractId, &2u32);
    let c = f.escrow_client();

    let snapshot = || {
        f.env.as_contract(&f.escrow, || {
            let contract: Contract = f
                .env
                .storage()
                .persistent()
                .get(&DataKey::Contract(CONTRACT_ID))
                .unwrap();
            let milestones: Vec<Milestone> = f
                .env
                .storage()
                .persistent()
                .get(&f.milestone_key())
                .unwrap();
            let next: u32 = f
                .env
                .storage()
                .persistent()
                .get(&DataKey::NextContractId)
                .unwrap();
            (contract, milestones, next)
        })
    };
    let before = snapshot();

    let r1 = c.simulate_refund(&CONTRACT_ID, &vec![&f.env, 0u32, 1]);
    let r2 = c.simulate_refund(&CONTRACT_ID, &vec![&f.env, 0u32, 1]);
    assert_eq!(r1, r2);

    let s1 = c.simulate_release_milestone(&CONTRACT_ID, &f.client, &0);
    let s2 = c.simulate_release_milestone(&CONTRACT_ID, &f.client, &0);
    assert_eq!(s1, s2);

    let p1 = c.simulate_create_contract(
        &f.client,
        &Address::generate(&f.env),
        &None,
        &vec![&f.env, 10i128],
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(p1.contract_id, 2);

    assert_eq!(snapshot(), before);
}
