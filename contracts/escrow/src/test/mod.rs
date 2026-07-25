#![cfg(test)]
#![allow(dead_code)]

use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, vec, Address, Env, Vec};

use crate::{
    Contract, ContractStatus, Escrow, EscrowClient, EscrowError, Milestone, ReleaseAuthorization,
};

// --- Submodules ---
mod approval_expiry;
mod cancel_contract;
mod client_migration;
mod contract_events;
mod create_contract_bounds;
mod deposit;
mod dispute;
mod emergency_controls;
mod events_comprehensive;
mod governance_events;
mod input_sanitization_amounts;
mod input_sanitization_identities;
mod mainnet_readiness;
mod overflow_saturation;
mod pause_controls;
mod persistence;
mod refund;
mod release;
mod release_authorization;
mod reputation;
mod reputation_bounds_tests;
mod security;
mod ttl_tests;

// --- Shared constants ---

pub const MILESTONE_ONE: i128 = 200_0000000;
pub const MILESTONE_TWO: i128 = 400_0000000;
pub const MILESTONE_THREE: i128 = 600_0000000;

/// A complete, test-only escrow fixture.
///
/// The fixture owns its Soroban [`Env`] and records the generated addresses and
/// contract ID. Call [`EscrowFixtureBuilder::funded`] when a suite needs a
/// ready-to-use escrow with a bound SAC and a fully funded contract.
pub struct EscrowFixture {
    pub env: Env,
    pub admin: Address,
    pub client: Address,
    pub freelancer: Address,
    pub arbiter: Option<Address>,
    pub escrow_address: Address,
    pub escrow_id: u32,
    pub settlement_token: Option<Address>,
}

impl EscrowFixture {
    /// Start a fixture with generated participants and default milestones.
    pub fn builder() -> EscrowFixtureBuilder {
        EscrowFixtureBuilder::new()
    }

    /// Return a client for invoking the escrow contract in this fixture.
    pub fn escrow(&self) -> EscrowClient<'_> {
        EscrowClient::new(&self.env, &self.escrow_address)
    }

    /// Return the total configured milestone value.
    pub fn total_amount(&self) -> i128 {
        self.escrow()
            .get_milestones(&self.escrow_id)
            .iter()
            .fold(0_i128, |total, milestone| total + milestone.amount)
    }
}

/// Builder for a reusable escrow test fixture.
///
/// Every generated fixture initializes the escrow and mocks authorization. The
/// optional settlement-token setup is intentionally explicit for tests that
/// exercise the unbound-token failure path; [`Self::funded`] enables it because
/// deposits require custody to be configured.
pub struct EscrowFixtureBuilder {
    env: Env,
    admin: Option<Address>,
    participants: Option<(Address, Address, Option<Address>)>,
    milestones: Option<Vec<i128>>,
    settlement_token: bool,
    fund: bool,
}

impl EscrowFixtureBuilder {
    /// Create a builder backed by a fresh mocked Soroban environment.
    pub fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();
        Self {
            env,
            admin: None,
            participants: None,
            milestones: None,
            settlement_token: false,
            fund: false,
        }
    }

    /// Expose the builder environment for generating compatible test values.
    pub fn env(&self) -> &Env {
        &self.env
    }

    /// Use `admin` instead of a generated administrator.
    pub fn with_admin(mut self, admin: Address) -> Self {
        self.admin = Some(admin);
        self
    }

    /// Use explicit client, freelancer, and optional arbiter addresses.
    pub fn with_participants(
        mut self,
        client: Address,
        freelancer: Address,
        arbiter: Option<Address>,
    ) -> Self {
        self.participants = Some((client, freelancer, arbiter));
        self
    }

    /// Use the supplied milestone amounts instead of the default 3-step plan.
    pub fn with_milestones(mut self, milestones: Vec<i128>) -> Self {
        self.milestones = Some(milestones);
        self
    }

    /// Register and bind a Stellar Asset Contract for custody transfers.
    pub fn with_settlement_token(mut self) -> Self {
        self.settlement_token = true;
        self
    }

    /// Create and fully fund the escrow contract during [`Self::build`].
    pub fn funded(mut self) -> Self {
        self.fund = true;
        self.settlement_token = true;
        self
    }

    /// Build the configured fixture and return its ready escrow ID in
    /// [`EscrowFixture::escrow_id`].
    pub fn build(self) -> EscrowFixture {
        let admin = self.admin.unwrap_or_else(|| Address::generate(&self.env));
        let (client, freelancer, arbiter) = self.participants.unwrap_or_else(|| {
            (
                Address::generate(&self.env),
                Address::generate(&self.env),
                None,
            )
        });
        let milestones = self
            .milestones
            .unwrap_or_else(|| default_milestones(&self.env));
        let escrow_address = self.env.register(Escrow, ());
        let escrow = EscrowClient::new(&self.env, &escrow_address);
        escrow.initialize(&admin);

        let settlement_token = self.settlement_token.then(|| {
            let token = self.env.register_stellar_asset_contract(admin.clone());
            escrow.bind_settlement_token(&admin, &token);
            token
        });

        let escrow_id = escrow.create_contract(
            &client,
            &freelancer,
            &arbiter,
            &milestones,
            &ReleaseAuthorization::ClientOnly,
        );

        if self.fund {
            let total = milestones.iter().fold(0_i128, |sum, amount| sum + amount);
            let token = settlement_token
                .as_ref()
                .expect("funded fixtures always configure a settlement token");
            StellarAssetClient::new(&self.env, token).mint(&client, &total);
            escrow.deposit_funds(&escrow_id, &client, &total);
        }

        EscrowFixture {
            env: self.env,
            admin,
            client,
            freelancer,
            arbiter,
            escrow_address,
            escrow_id,
            settlement_token,
        }
    }
}

impl Default for EscrowFixtureBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// --- Compatibility helpers for suites not migrated to EscrowFixtureBuilder ---

pub fn setup() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    (env, client_addr, freelancer_addr)
}

pub fn create_client(env: &Env) -> EscrowClient<'_> {
    let id = env.register(Escrow, ());
    EscrowClient::new(env, &id)
}

/// Create a 3-milestone contract (200 / 400 / 600 = 1 200 total) with ClientOnly auth.
pub fn create_default_contract(
    env: &Env,
    client: &EscrowClient,
    client_addr: &Address,
    freelancer_addr: &Address,
) -> u32 {
    let milestones = vec![env, MILESTONE_ONE, MILESTONE_TWO, MILESTONE_THREE];
    client.create_contract(
        client_addr,
        freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    )
}

/// Assert contract accounting fields match expected values.
pub fn assert_contract_state(
    contract: crate::Contract,
    expected_status: ContractStatus,
    expected_funded: i128,
    expected_released: i128,
    expected_refunded: i128,
) {
    assert_eq!(contract.status, expected_status);
    assert_eq!(contract.funded_amount, expected_funded);
    assert_eq!(contract.released_amount, expected_released);
    assert_eq!(contract.refunded_amount, expected_refunded);
}

pub fn register_client(env: &Env) -> EscrowClient<'_> {
    let id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &id);
    let admin = Address::generate(env);
    env.mock_all_auths();
    client.initialize(&admin);
    client
}

pub fn default_milestones(env: &Env) -> soroban_sdk::Vec<i128> {
    vec![env, MILESTONE_ONE, MILESTONE_TWO, MILESTONE_THREE]
}

pub fn total_milestone_amount() -> i128 {
    MILESTONE_ONE + MILESTONE_TWO + MILESTONE_THREE
}

/// Generate a fresh (client, freelancer) address pair for a test.
pub fn generated_participants(env: &Env) -> (Address, Address) {
    (Address::generate(env), Address::generate(env))
}

/// Create, fund, and fully release a 3-milestone contract, driving it to
/// [`ContractStatus::Completed`]. Returns (client_addr, freelancer_addr, contract_id).
pub fn complete_contract(env: &Env, client: &EscrowClient) -> (Address, Address, u32) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(env),
        &ReleaseAuthorization::ClientOnly,
    );
    let total = total_milestone_amount();
    client.deposit_funds(&contract_id, &client_addr, &total);
    for milestone_index in 0..3u32 {
        client.approve_milestone_release(&contract_id, &client_addr, &milestone_index);
        client.release_milestone(&contract_id, &client_addr, &milestone_index);
    }
    (client_addr, freelancer_addr, contract_id)
}

/// Create a 3-milestone contract with an arbiter configured (not yet funded).
/// Returns (client_addr, freelancer_addr, arbiter_addr, contract_id).
pub fn create_contract_with_arbiter(
    env: &Env,
    client: &EscrowClient,
) -> (Address, Address, Address, u32) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let arbiter_addr = Address::generate(env);
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &default_milestones(env),
        &ReleaseAuthorization::ClientOnly,
    );
    (client_addr, freelancer_addr, arbiter_addr, contract_id)
}

/// Create a contract and return (client_addr, freelancer_addr, contract_id).
pub fn create_contract(env: &Env, client: &EscrowClient) -> (Address, Address, u32) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let milestones = default_milestones(env);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    (client_addr, freelancer_addr, id)
}

/// Assert that a `try_*` call returns the expected contract error.
///
/// Soroban `try_*` methods return:
///   `Result<Result<T, E>, Result<soroban_sdk::Error, InvokeError>>`
/// A contract-level `panic_with_error` surfaces as `Err(Ok(soroban_sdk::Error))`.
/// The `expected` argument can be any type convertible to `soroban_sdk::Error`,
/// including both `EscrowError` and the canonical `Error` from `types.rs`.
pub fn assert_contract_error<
    T: core::fmt::Debug,
    InnerError: core::fmt::Debug,
    E: Into<soroban_sdk::Error> + core::fmt::Debug,
>(
    result: Result<Result<T, InnerError>, Result<soroban_sdk::Error, soroban_sdk::InvokeError>>,
    expected: E,
) {
    match result {
        Err(Ok(e)) => {
            let expected_err: soroban_sdk::Error = expected.into();
            assert_eq!(e, expected_err, "contract error code mismatch");
        }
        _other => panic!(
            "expected contract error {:?}, got unexpected result variant: {:?}",
            expected, _other
        ),
    }
}
