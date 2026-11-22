extern crate std;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _, MockAuth, MockAuthInvoke},
    token::StellarAssetClient,
    token::TokenClient,
    vec, IntoVal, TryFromVal, TryIntoVal,
};

/// One token amount unit in tests: the minimum milestone amount, so every
/// milestone used below is >= `MIN_MILESTONE_AMOUNT`.
const U: i128 = MIN_MILESTONE_AMOUNT;

struct Setup<'a> {
    env: Env,
    admin: Address,
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
    StellarAssetClient::new(&env, &sac.address()).mint(&client, &(1_000 * U));
    let id = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &id);
    escrow.initialize(&admin);
    let token = TokenClient::new(&env, &sac.address());
    Setup {
        env,
        admin,
        escrow,
        token,
        client,
        freelancer,
    }
}

/// Since #1683 opening a dispute collects a refundable bond from the opener.
/// Mint exactly that bond to `caller` first, so pre-existing scenarios keep
/// their balances.
fn open_dispute_bonded(s: &Setup, id: &u64, caller: &Address) {
    let c = s.escrow.get_contract(id);
    let bond = dispute_bond(c.total_amount);
    if bond > 0 {
        StellarAssetClient::new(&s.env, &c.token).mint(caller, &bond);
    }
    s.escrow.open_dispute(id, caller);
}

/// Resolves as the admin returning the bond to the opener, then burns that
/// refunded bond so the opener ends where the pre-bond flow left them.
fn resolve_refunding_bond(s: &Setup, id: &u64, bps: &u32) {
    let bond = s.escrow.get_dispute_bond(id);
    s.escrow.resolve_dispute(&s.admin, id, bps, &true);
    if let Some((opener, amount)) = bond {
        if amount > 0 {
            s.token.burn(&opener, &amount);
        }
    }
}

struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }
    fn next_u32(&mut self) -> u32 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.state >> 32) as u32
    }
    fn next_bounded(&mut self, bound: u32) -> u32 {
        if bound == 0 {
            0
        } else {
            self.next_u32() % bound
        }
    }
}

/// Helper: create + fully fund an escrow, returning its id.
fn funded_escrow(s: &Setup) -> u64 {
    let id = s
        .escrow
        .create_contract(&s.client, &s.freelancer, &s.token.address, &vec![&s.env, 100 * U]);
    s.escrow.deposit_funds(&id, &(100 * U));
    id
}

/// Helper that builds a valid milestone vector for a given amount.
fn milestones(s: &Setup, amount: i128) -> Vec<i128> {
    vec![&s.env, amount]
}

fn three_milestone_escrow(s: &Setup, sequential: bool) -> u64 {
    let amounts = vec![&s.env, 100 * U, 200 * U, 300 * U];
    let id = if sequential {
        s.escrow
            .create_contract_sequential(&s.client, &s.freelancer, &s.token.address, &amounts)
    } else {
        s.escrow
            .create_contract(&s.client, &s.freelancer, &s.token.address, &amounts)
    };
    s.escrow.deposit_funds(&id, &(600 * U));
    id
}

fn completed_escrow(s: &Setup, amount: i128) -> u64 {
    StellarAssetClient::new(&s.env, &s.token.address).mint(&s.client, &amount);
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, amount],
    );
    s.escrow.deposit_funds(&id, &amount);
    s.escrow.release_milestone(&id, &0);
    id
}

fn disputed_high_value_escrow(s: &Setup, amount: i128) -> u64 {
    StellarAssetClient::new(&s.env, &s.token.address).mint(&s.client, &amount);
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, amount],
    );
    s.escrow.deposit_funds(&id, &amount);
    open_dispute_bonded(&s, &id, &s.client);
    id
}

fn set_now(env: &Env, secs: u64) {
    env.ledger().with_mut(|li| {
        li.timestamp = secs;
    });
}

fn assert_invariants(s: &Setup, c: &EscrowContract) {
    assert!(c.released_amount <= c.funded_amount);
    let mut total_paid: i128 = 0;
    for (i, m) in c.milestones.iter().enumerate() {
        let paid: i128 = s.escrow.get_milestone_paid(&c.id, &(i as u32));
        assert!(paid <= m.amount, "milestone {i} paid exceeds amount");
        if m.released {
            assert_eq!(paid, m.amount, "milestone {i} released but underpaid");
        }
        total_paid = total_paid.checked_add(paid).unwrap();
    }
    assert!(total_paid <= c.total_amount);
}

fn hash32(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

fn two_milestone_escrow(s: &Setup) -> u64 {
    s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U, 200 * U],
    )
}

fn persistent_ttl<K: IntoVal<Env, soroban_sdk::Val>>(s: &Setup, key: &K) -> u32 {
    use soroban_sdk::testutils::storage::Persistent as _;
    s.env
        .as_contract(&s.escrow.address, || s.env.storage().persistent().get_ttl(key))
}

/// Escrow-contract events as (topics, data), in emission order.
fn escrow_events(s: &Setup) -> std::vec::Vec<(Vec<soroban_sdk::Val>, soroban_sdk::Val)> {
    s.env
        .events()
        .all()
        .iter()
        .filter(|e| e.0 == s.escrow.address)
        .map(|e| (e.1, e.2))
        .collect()
}

/// Symbols of the structured (versioned) escrow events, in order.
fn escrow_event_names(s: &Setup) -> std::vec::Vec<Symbol> {
    escrow_events(s)
        .iter()
        .filter(|(topics, _)| {
            topics.len() == 3 && u32::try_from_val(&s.env, &topics.get(0).unwrap()) == Ok(EVENT_SCHEMA_V)
        })
        .map(|(topics, _)| Symbol::try_from_val(&s.env, &topics.get(1).unwrap()).unwrap())
        .collect()
}

/// Last structured event `(1, name, id)` decoded as `T`.
fn last_escrow_event<T: TryFromVal<Env, soroban_sdk::Val>>(s: &Setup, name: &str, id: u64) -> Option<T> {
    let want = Symbol::new(&s.env, name);
    escrow_events(s)
        .into_iter()
        .rev()
        .find(|(topics, _)| {
            topics.len() == 3
                && u32::try_from_val(&s.env, &topics.get(0).unwrap()) == Ok(EVENT_SCHEMA_V)
                && Symbol::try_from_val(&s.env, &topics.get(1).unwrap()) == Ok(want.clone())
                && u64::try_from_val(&s.env, &topics.get(2).unwrap()) == Ok(id)
        })
        .map(|(_, data)| T::try_from_val(&s.env, &data).ok().unwrap())
}

fn count_escrow_events(s: &Setup, name: &str) -> usize {
    let want = Symbol::new(&s.env, name);
    escrow_event_names(s).iter().filter(|n| **n == want).count()
}

/// A minimal, code-free Wasm module that the host accepts as contract code:
/// just the Wasm header plus the `contractenvmetav0` custom section declaring
/// interface version (protocol 21, pre-release 0). Built from readable parts
/// so the fixture is fully transparent; it exports no functions.
fn minimal_contract_wasm() -> std::vec::Vec<u8> {
    let name = b"contractenvmetav0";
    // XDR ScEnvMetaEntry::InterfaceVersion { protocol: 21, pre_release: 0 }
    let mut payload = std::vec::Vec::new();
    payload.extend_from_slice(&0u32.to_be_bytes());
    payload.extend_from_slice(&21u32.to_be_bytes());
    payload.extend_from_slice(&0u32.to_be_bytes());
    let mut section = std::vec::Vec::new();
    section.push(name.len() as u8);
    section.extend_from_slice(name);
    section.extend_from_slice(&payload);
    let mut wasm = std::vec::Vec::new();
    wasm.extend_from_slice(b"\0asm");
    wasm.extend_from_slice(&1u32.to_le_bytes());
    wasm.push(0); // custom section id
    wasm.push(section.len() as u8);
    wasm.extend_from_slice(&section);
    wasm
}

fn set_time(env: &Env, ts: u64) {
    env.ledger().with_mut(|li| li.timestamp = ts);
}

fn last_global_event(s: &Setup) -> (Vec<soroban_sdk::Val>, soroban_sdk::Val) {
    let ev = s
        .env
        .events()
        .all()
        .iter()
        .filter(|e| e.0 == s.escrow.address)
        .last()
        .unwrap();
    (ev.1, ev.2)
}

/// Plant a legacy `DataKey::Contract(id)` entry directly (pre-versioning layout).
fn write_legacy(s: &Setup, c: &EscrowContract) {
    s.env.as_contract(&s.escrow.address, || {
        s.env.storage().persistent().set(&DataKey::Contract(c.id), c);
    });
}

fn has_new_key(s: &Setup, id: u64) -> bool {
    s.env.as_contract(&s.escrow.address, || {
        s.env.storage().persistent().has(&DataKey::ContractV(id))
    })
}

fn has_legacy_key(s: &Setup, id: u64) -> bool {
    s.env.as_contract(&s.escrow.address, || {
        s.env.storage().persistent().has(&DataKey::Contract(id))
    })
}

fn legacy_escrow(s: &Setup, id: u64, amounts: &[i128]) -> EscrowContract {
    let mut milestones = Vec::new(&s.env);
    let mut total = 0;
    for a in amounts {
        milestones.push_back(Milestone {
            amount: *a,
            released: false,
        });
        total += *a;
    }
    EscrowContract {
        id,
        client: s.client.clone(),
        freelancer: s.freelancer.clone(),
        token: s.token.address.clone(),
        milestones,
        total_amount: total,
        funded_amount: 0,
        released_amount: 0,
        refunded_amount: 0,
        status: ContractStatus::Created,
        reputation_issued: false,
    }
}

fn funded_two_milestones(s: &Setup) -> u64 {
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U, 200 * U],
    );
    s.escrow.deposit_funds(&id, &(300 * U));
    id
}

mod fee_token {
    use soroban_sdk::{Address, Env};

    /// Fee-on-transfer token: `from` is debited `amount`, `to` gets `amount - 1%`.
    #[soroban_sdk::contract]
    pub struct MockFeeToken;

    #[soroban_sdk::contractimpl]
    impl MockFeeToken {
        pub fn mint(env: Env, to: Address, amount: i128) {
            let b: i128 = env.storage().persistent().get(&to).unwrap_or(0);
            env.storage().persistent().set(&to, &(b + amount));
        }

        pub fn balance(env: Env, id: Address) -> i128 {
            env.storage().persistent().get(&id).unwrap_or(0)
        }

        pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
            let fee = amount / 100;
            let fb: i128 = env.storage().persistent().get(&from).unwrap_or(0);
            env.storage().persistent().set(&from, &(fb - amount));
            let tb: i128 = env.storage().persistent().get(&to).unwrap_or(0);
            env.storage().persistent().set(&to, &(tb + amount - fee));
        }
    }
}

mod flaky_token {
    use soroban_sdk::{symbol_short, Address, Env};

    /// Moves balances normally until `set_noop(true)`, after which `transfer`
    /// silently does nothing while still "succeeding".
    #[soroban_sdk::contract]
    pub struct MockFlakyToken;

    #[soroban_sdk::contractimpl]
    impl MockFlakyToken {
        pub fn mint(env: Env, to: Address, amount: i128) {
            let b: i128 = env.storage().persistent().get(&to).unwrap_or(0);
            env.storage().persistent().set(&to, &(b + amount));
        }

        pub fn set_noop(env: Env, noop: bool) {
            env.storage().instance().set(&symbol_short!("noop"), &noop);
        }

        pub fn balance(env: Env, id: Address) -> i128 {
            env.storage().persistent().get(&id).unwrap_or(0)
        }

        pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
            let noop: bool = env
                .storage()
                .instance()
                .get(&symbol_short!("noop"))
                .unwrap_or(false);
            if noop {
                return;
            }
            let fb: i128 = env.storage().persistent().get(&from).unwrap_or(0);
            env.storage().persistent().set(&from, &(fb - amount));
            let tb: i128 = env.storage().persistent().get(&to).unwrap_or(0);
            env.storage().persistent().set(&to, &(tb + amount));
        }
    }
}

fn escrow_with_token(env: &Env, token: &Address) -> (EscrowClient<'static>, Address, Address) {
    let admin = Address::generate(env);
    let client = Address::generate(env);
    let freelancer = Address::generate(env);
    let id = env.register(Escrow, ());
    let escrow = EscrowClient::new(env, &id);
    escrow.initialize(&admin);
    let _ = token;
    (escrow, client, freelancer)
}

fn setup_admin<'a>() -> (Env, EscrowClient<'a>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let id = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &id);
    escrow.initialize(&admin);
    (env, escrow, admin)
}

/// Exact 256-bit reference for floor(held * bps / 10_000).
fn mul_bps_reference(env: &Env, held: i128, bps: u32) -> i128 {
    soroban_sdk::I256::from_i128(env, held)
        .mul(&soroban_sdk::I256::from_i128(env, bps as i128))
        .div(&soroban_sdk::I256::from_i128(env, BPS_DENOMINATOR))
        .to_i128()
        .unwrap()
}

fn set_ts(env: &Env, ts: u64) {
    env.ledger().with_mut(|li| li.timestamp = ts);
}

/// Prepared escrows for one representative call per pause category.
struct PauseFixture {
    created: u64,
    funded: u64,
    disputable: u64,
    cancellable: u64,
    completed: u64,
}

fn pause_fixture(s: &Setup) -> PauseFixture {
    let mk = |amount: i128| {
        s.escrow.create_contract(
            &s.client,
            &s.freelancer,
            &s.token.address,
            &vec![&s.env, amount],
        )
    };
    let created = mk(10 * U);
    let funded = mk(10 * U);
    s.escrow.deposit_funds(&funded, &(10 * U));
    let disputable = mk(10 * U);
    s.escrow.deposit_funds(&disputable, &(10 * U));
    let cancellable = mk(10 * U);
    s.escrow.deposit_funds(&cancellable, &(10 * U));
    let completed = mk(10 * U);
    s.escrow.deposit_funds(&completed, &(10 * U));
    s.escrow.release_milestone(&completed, &0);
    PauseFixture {
        created,
        funded,
        disputable,
        cancellable,
        completed,
    }
}

fn instance_ttl(s: &Setup) -> u32 {
    use soroban_sdk::testutils::storage::Instance as _;
    s.env
        .as_contract(&s.escrow.address, || s.env.storage().instance().get_ttl())
}

fn escrow_ttl(s: &Setup, id: u64) -> u32 {
    persistent_ttl(s, &DataKey::ContractV(id))
}

/// A sequence where every entry written at sequence 0 is still live but below
/// the extension threshold.
const QUIET: u32 = PERSISTENT_TTL_EXTEND - PERSISTENT_TTL_THRESHOLD / 2;

fn create_indexed_escrow(s: &Setup, freelancer: &Address) -> u64 {
    s.escrow.create_contract(
        &s.client,
        freelancer,
        &s.token.address,
        &vec![&s.env, U],
    )
}

fn assert_ids(actual: &Vec<u64>, expected: &[u64]) {
    assert_eq!(
        actual.len(),
        expected.len() as u32,
        "unexpected number of ids"
    );
    for (index, expected_id) in expected.iter().enumerate() {
        assert_eq!(
            actual.get(index as u32),
            Some(*expected_id),
            "unexpected id at position {index}"
        );
    }
}

fn client_page(s: &Setup, client: &Address, page_index: u32) -> Vec<u64> {
    s.env.as_contract(&s.escrow.address, || {
        s.env
            .storage()
            .persistent()
            .get(&DataKey::ClientPage(client.clone(), page_index))
            .unwrap()
    })
}

fn freelancer_page(s: &Setup, freelancer: &Address, page_index: u32) -> Vec<u64> {
    s.env.as_contract(&s.escrow.address, || {
        s.env
            .storage()
            .persistent()
            .get(&DataKey::FreelancerPage(freelancer.clone(), page_index))
            .unwrap()
    })
}

fn at_time(env: &Env, ts: u64) {
    env.ledger().with_mut(|li| li.timestamp = ts);
}

fn assert_amendment_invariants(s: &Setup, id: u64) {
    let c = s.escrow.get_contract(&id);
    assert!(c.funded_amount <= c.total_amount);
    assert_eq!(
        s.token.balance(&s.escrow.address),
        c.funded_amount - c.released_amount
    );
    check_invariants(&c);
}

mod lifecycle;
mod disputes;
mod reputation;
mod funding;
mod streams_amendments;
mod governance;
mod events;
mod storage;

