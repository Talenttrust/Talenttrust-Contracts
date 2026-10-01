//! Compatibility contract of `rollback_dispute` (#1511).
//!
//! Each test pins one item of the contract documented at the top of
//! `src/rollback.rs`: guard order and error codes, empty and malformed data,
//! the success effects and event shape, and the frozen snapshot layout.
//!
//! State is seeded directly with `env.as_contract`, so the tests exercise the
//! rollback guards in isolation from the dispute-raising flow.
//!
//! ```sh
//! cargo test -p escrow --lib test::rollback
//! ```

use crate::rollback::{
    check_rollback_preconditions, check_rollback_unchanged, clear_dispute_rollback,
    read_dispute_rollback, store_dispute_rollback, DisputeRollbackRecord, RollbackRecordRead,
};
use crate::ReleaseAuthorization;
use crate::{Contract, ContractStatus, DataKey, Error, Escrow, EscrowClient, Milestone};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events as _, Ledger as _},
    vec, Address, Env, IntoVal, Map, Symbol, TryFromVal, Val, Vec,
};

const ID: u32 = 1;

fn contract_with(env: &Env, status: ContractStatus) -> Contract {
    Contract {
        client: Address::generate(env),
        freelancer: Address::generate(env),
        arbiter: None,
        status,
        total_deposited: 300,
        funded_amount: 300,
        released_amount: 0,
        refunded_amount: 0,
        release_authorization: ReleaseAuthorization::ClientOnly,
        reputation_issued: false,
    }
}

fn milestones(env: &Env) -> Vec<Milestone> {
    let m = |amount| Milestone {
        amount,
        funded_amount: amount,
        released: false,
        refunded: false,
        work_evidence: None,
        refunded_amount: 0,
        deadline: None,
    };
    vec![env, m(100), m(200)]
}

fn record(contract: &Contract, milestones: &Vec<Milestone>) -> RollbackRecordRead {
    RollbackRecordRead::Present(DisputeRollbackRecord {
        contract: contract.clone(),
        milestones: milestones.clone(),
    })
}

fn disputed(contract: &Contract) -> Contract {
    let mut c = contract.clone();
    c.status = ContractStatus::Disputed;
    c
}

fn milestone_key(env: &Env) -> (DataKey, Symbol) {
    (DataKey::Contract(ID), Symbol::new(env, "milestones"))
}

fn contract_error(error: Error) -> soroban_sdk::Error {
    soroban_sdk::Error::from_contract_error(error as u32)
}

// ── Pure guards 7–9 and 11 ────────────────────────────────────────────────────

#[test]
fn preconditions_reject_every_non_disputed_status() {
    let env = Env::default();
    let snapshot = contract_with(&env, ContractStatus::Funded);
    let read = record(&snapshot, &milestones(&env));
    for status in [
        ContractStatus::Created,
        ContractStatus::Accepted,
        ContractStatus::Funded,
        ContractStatus::Completed,
        ContractStatus::Cancelled,
        ContractStatus::Refunded,
        ContractStatus::PartiallyFunded,
    ] {
        let current = contract_with(&env, status);
        assert_eq!(
            check_rollback_preconditions(&current, &read),
            Err(Error::RollbackNotAllowed),
            "{status:?}"
        );
    }
}

#[test]
fn preconditions_missing_and_malformed_snapshot() {
    let env = Env::default();
    let current = disputed(&contract_with(&env, ContractStatus::Funded));
    assert_eq!(
        check_rollback_preconditions(&current, &RollbackRecordRead::Missing),
        Err(Error::RollbackNotAllowed)
    );
    assert_eq!(
        check_rollback_preconditions(&current, &RollbackRecordRead::Malformed),
        Err(Error::StorageInvariantViolated)
    );
}

#[test]
fn preconditions_only_restore_funded_states() {
    let env = Env::default();
    let ms = milestones(&env);
    for (status, allowed) in [
        (ContractStatus::Funded, true),
        (ContractStatus::PartiallyFunded, true),
        (ContractStatus::Created, false),
        (ContractStatus::Accepted, false),
        (ContractStatus::Completed, false),
        (ContractStatus::Disputed, false),
        (ContractStatus::Cancelled, false),
        (ContractStatus::Refunded, false),
    ] {
        let snapshot = contract_with(&env, status);
        let read = record(&snapshot, &ms);
        let result = check_rollback_preconditions(&disputed(&snapshot), &read);
        assert_eq!(result.is_ok(), allowed, "{status:?}");
        if !allowed {
            assert_eq!(result, Err(Error::RollbackNotAllowed));
        }
    }
}

#[test]
fn unchanged_check_accepts_only_the_status_flip() {
    let env = Env::default();
    let snapshot = contract_with(&env, ContractStatus::PartiallyFunded);
    let ms = milestones(&env);
    let rec = DisputeRollbackRecord {
        contract: snapshot.clone(),
        milestones: ms.clone(),
    };

    assert_eq!(
        check_rollback_unchanged(&disputed(&snapshot), &ms, &rec),
        Ok(ContractStatus::PartiallyFunded)
    );

    let mut moved = disputed(&snapshot);
    moved.released_amount = 1;
    assert_eq!(
        check_rollback_unchanged(&moved, &ms, &rec),
        Err(Error::RollbackNotAllowed)
    );

    let mut changed_ms = ms.clone();
    let mut m = changed_ms.get(0).unwrap();
    m.released = true;
    changed_ms.set(0, m);
    assert_eq!(
        check_rollback_unchanged(&disputed(&snapshot), &changed_ms, &rec),
        Err(Error::RollbackNotAllowed)
    );

    // Boundary: an empty milestone vector must match exactly too.
    assert_eq!(
        check_rollback_unchanged(&disputed(&snapshot), &Vec::new(&env), &rec),
        Err(Error::RollbackNotAllowed)
    );
}

// ── Snapshot storage ─────────────────────────────────────────────────────────

#[test]
fn snapshot_store_read_clear_round_trip() {
    let env = Env::default();
    let escrow = env.register(Escrow, ());
    env.as_contract(&escrow, || {
        assert_eq!(read_dispute_rollback(&env, ID), RollbackRecordRead::Missing);

        let c = contract_with(&env, ContractStatus::Funded);
        let ms = milestones(&env);
        store_dispute_rollback(&env, ID, &c, &ms);
        assert_eq!(read_dispute_rollback(&env, ID), record(&c, &ms));

        clear_dispute_rollback(&env, ID);
        assert_eq!(read_dispute_rollback(&env, ID), RollbackRecordRead::Missing);
        // Idempotent.
        clear_dispute_rollback(&env, ID);
        assert_eq!(read_dispute_rollback(&env, ID), RollbackRecordRead::Missing);
    });
}

/// The snapshot is persisted, so its layout is part of the compatibility
/// contract: a `#[contracttype]` struct is stored as a map keyed by field
/// name. Renaming or adding a field breaks decoding of existing snapshots and
/// must fail this test.
#[test]
fn snapshot_layout_is_frozen() {
    let env = Env::default();
    let rec = DisputeRollbackRecord {
        contract: contract_with(&env, ContractStatus::Funded),
        milestones: milestones(&env),
    };
    let raw: Val = rec.clone().into_val(&env);
    let map = Map::<Symbol, Val>::try_from_val(&env, &raw).unwrap();
    assert_eq!(
        map.keys(),
        vec![
            &env,
            Symbol::new(&env, "contract"),
            Symbol::new(&env, "milestones")
        ]
    );
    assert_eq!(
        DisputeRollbackRecord::try_from_val(&env, &raw).unwrap(),
        rec
    );
}

// ── Entrypoint ───────────────────────────────────────────────────────────────

struct Fixture {
    env: Env,
    escrow: Address,
    admin: Address,
    snapshot: Contract,
}

impl Fixture {
    /// An initialized system with contract `ID` disputed from `pre_status`
    /// and a matching snapshot.
    fn disputed_from(pre_status: ContractStatus) -> Self {
        let env = Env::default();
        env.mock_all_auths();
        let escrow = env.register(Escrow, ());
        let admin = Address::generate(&env);
        let snapshot = contract_with(&env, pre_status);
        let ms = milestones(&env);
        env.as_contract(&escrow, || {
            let s = env.storage().persistent();
            s.set(&DataKey::Initialized, &true);
            s.set(&DataKey::Admin, &admin);
            s.set(&DataKey::Contract(ID), &disputed(&snapshot));
            s.set(&milestone_key(&env), &ms);
            store_dispute_rollback(&env, ID, &snapshot, &ms);
        });
        Fixture {
            env,
            escrow,
            admin,
            snapshot,
        }
    }

    fn client(&self) -> EscrowClient<'_> {
        EscrowClient::new(&self.env, &self.escrow)
    }

    fn set<K: IntoVal<Env, Val>, V: IntoVal<Env, Val>>(&self, key: &K, value: &V) {
        self.env.as_contract(&self.escrow, || {
            self.env.storage().persistent().set(key, value);
        });
    }

    fn remove<K: IntoVal<Env, Val>>(&self, key: &K) {
        self.env.as_contract(&self.escrow, || {
            self.env.storage().persistent().remove(key);
        });
    }

    fn stored_contract(&self) -> Contract {
        self.env.as_contract(&self.escrow, || {
            self.env
                .storage()
                .persistent()
                .get(&DataKey::Contract(ID))
                .unwrap()
        })
    }

    fn snapshot_read(&self) -> RollbackRecordRead {
        self.env
            .as_contract(&self.escrow, || read_dispute_rollback(&self.env, ID))
    }

    fn expect_err(&self, id: u32, error: Error) {
        assert_eq!(
            self.client().try_rollback_dispute(&id),
            Err(Ok(contract_error(error)))
        );
    }
}

#[test]
fn success_restores_status_clears_snapshot_and_emits_event() {
    let f = Fixture::disputed_from(ContractStatus::Funded);
    f.env.ledger().set_timestamp(1_700_000_000);

    assert!(f.client().rollback_dispute(&ID));
    // Captured first: later `as_contract` reads reset the host event buffer.
    let events = f.env.events().all();

    assert_eq!(f.stored_contract(), f.snapshot);
    assert_eq!(f.snapshot_read(), RollbackRecordRead::Missing);

    assert_eq!(events.len(), 1, "exactly one event");
    let (emitter, topics, data) = events.last().unwrap();
    assert_eq!(emitter, f.escrow);
    assert_eq!(topics, (symbol_short!("rollback"), ID).into_val(&f.env));
    let data: (Address, ContractStatus, ContractStatus, u64) =
        <_>::try_from_val(&f.env, &data).unwrap();
    assert_eq!(
        data,
        (
            f.admin.clone(),
            ContractStatus::Disputed,
            ContractStatus::Funded,
            1_700_000_000
        )
    );
}

#[test]
fn success_restores_partially_funded() {
    let f = Fixture::disputed_from(ContractStatus::PartiallyFunded);
    assert!(f.client().rollback_dispute(&ID));
    assert_eq!(f.stored_contract().status, ContractStatus::PartiallyFunded);
}

#[test]
fn second_rollback_reports_missing_snapshot() {
    let f = Fixture::disputed_from(ContractStatus::Funded);
    assert!(f.client().rollback_dispute(&ID));
    // The contract is no longer disputed, so guard 7 reports first.
    f.expect_err(ID, Error::RollbackNotAllowed);
}

#[test]
fn zero_id_is_rejected_before_anything_else() {
    let f = Fixture::disputed_from(ContractStatus::Funded);
    f.remove(&DataKey::Initialized);
    f.set(&DataKey::Paused, &true);
    f.expect_err(0, Error::InvalidContractId);
}

#[test]
fn not_initialized() {
    let f = Fixture::disputed_from(ContractStatus::Funded);
    f.remove(&DataKey::Initialized);
    f.expect_err(ID, Error::NotInitialized);
}

#[test]
fn paused_and_emergency() {
    let f = Fixture::disputed_from(ContractStatus::Funded);
    f.set(&DataKey::Emergency, &true);
    f.expect_err(ID, Error::EmergencyActive);
    f.set(&DataKey::Paused, &true);
    f.expect_err(ID, Error::ContractPaused);
}

#[test]
fn missing_contract() {
    let f = Fixture::disputed_from(ContractStatus::Funded);
    f.expect_err(9, Error::ContractNotFound);
}

#[test]
fn finalized_is_reported_before_status_checks() {
    let f = Fixture::disputed_from(ContractStatus::Funded);
    f.set(&DataKey::Contract(ID), &f.snapshot); // not disputed any more
    f.set(&DataKey::Finalization(ID), &true);
    f.expect_err(ID, Error::AlreadyFinalized);
}

#[test]
fn not_disputed_and_missing_snapshot() {
    let f = Fixture::disputed_from(ContractStatus::Funded);
    f.remove(&DataKey::DisputeRollback(ID));
    f.expect_err(ID, Error::RollbackNotAllowed);

    f.set(&DataKey::Contract(ID), &f.snapshot);
    f.expect_err(ID, Error::RollbackNotAllowed);
}

#[test]
fn state_changed_since_dispute_is_rejected_without_mutation() {
    let f = Fixture::disputed_from(ContractStatus::Funded);
    let mut moved = disputed(&f.snapshot);
    moved.refunded_amount = 50;
    f.set(&DataKey::Contract(ID), &moved);

    f.expect_err(ID, Error::RollbackNotAllowed);
    assert_eq!(f.stored_contract(), moved);
    assert!(matches!(f.snapshot_read(), RollbackRecordRead::Present(_)));
}

/// Guard 10 runs after guards 7–9: a missing milestone vector only reports
/// `ContractNotFound` once the dispute itself is eligible.
#[test]
fn missing_milestones_order() {
    let f = Fixture::disputed_from(ContractStatus::Funded);
    f.remove(&milestone_key(&f.env));
    f.expect_err(ID, Error::ContractNotFound);

    f.set(&DataKey::Contract(ID), &f.snapshot);
    f.expect_err(ID, Error::RollbackNotAllowed);
}

// ── Malformed data ───────────────────────────────────────────────────────────

#[test]
fn malformed_snapshot_is_typed_and_preserved() {
    let f = Fixture::disputed_from(ContractStatus::Funded);
    f.set(&DataKey::DisputeRollback(ID), &symbol_short!("old"));

    f.expect_err(ID, Error::StorageInvariantViolated);
    assert_eq!(f.snapshot_read(), RollbackRecordRead::Malformed);
    assert_eq!(f.stored_contract().status, ContractStatus::Disputed);
}

/// A snapshot written under a different (e.g. pre-upgrade) layout.
#[test]
fn snapshot_with_foreign_layout_is_typed() {
    let f = Fixture::disputed_from(ContractStatus::Funded);
    let mut foreign = Map::<Symbol, Val>::new(&f.env);
    foreign.set(Symbol::new(&f.env, "contract"), 1u32.into_val(&f.env));
    foreign.set(Symbol::new(&f.env, "milestones"), 2u32.into_val(&f.env));
    f.set(&DataKey::DisputeRollback(ID), &foreign);

    f.expect_err(ID, Error::StorageInvariantViolated);
}

#[test]
fn malformed_contract_milestones_and_admin_are_typed() {
    let f = Fixture::disputed_from(ContractStatus::Funded);
    f.set(&milestone_key(&f.env), &true);
    f.expect_err(ID, Error::StorageInvariantViolated);

    let f = Fixture::disputed_from(ContractStatus::Funded);
    f.set(&DataKey::Contract(ID), &7u32);
    f.expect_err(ID, Error::StorageInvariantViolated);

    let f = Fixture::disputed_from(ContractStatus::Funded);
    f.set(&DataKey::Admin, &7u32);
    f.expect_err(ID, Error::StorageInvariantViolated);
}

#[test]
fn missing_admin_reports_not_initialized() {
    let f = Fixture::disputed_from(ContractStatus::Funded);
    f.remove(&DataKey::Admin);
    f.expect_err(ID, Error::NotInitialized);
}
