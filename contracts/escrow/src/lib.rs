//! # TalentTrust Escrow Contract
//!
//! A Soroban smart contract implementing a milestone-based escrow protocol for
//! the TalentTrust decentralized freelancer platform on the Stellar network.
//!
//! ## Overview
//!
//! The escrow contract holds funds on behalf of a client and releases them to a
//! freelancer as individual milestones are approved. An optional arbiter can be
//! designated for dispute resolution. Four authorization schemes are supported:
//! `ClientOnly`, `ArbiterOnly`, `ClientAndArbiter`, and `MultiSig`.
//!
//! ## Lifecycle
//!
//! ```text
//! create_contract → deposit_funds → approve_milestone_release → release_milestone
//!                                                              ↑ (repeat per milestone)
//! ```
//!
//! When every milestone has been released the contract status transitions to
//! `Completed` automatically.
//!
//! ## Security Assumptions
//!
//! - All callers that mutate state must pass `require_auth()`.
//! - The contract stores a single escrow record keyed by `"contract"`. A
//!   production deployment should key by `contract_id`.
//! - No native token transfer is performed in this implementation; fund custody
//!   and transfer must be wired up via the Stellar asset contract.

#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, Map, Symbol, Vec,
};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// The lifecycle state of an [`EscrowContract`].
///
/// Transitions:
/// - `Created`  → `Funded`    (via [`Escrow::deposit_funds`])
/// - `Funded`   → `Completed` (automatically when all milestones are released)
/// - `Funded`   → `Disputed`  (reserved for future dispute-resolution logic)
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractStatus {
    /// Contract has been created but not yet funded.
    Created = 0,
    /// Client has deposited the full escrow amount.
    Funded = 1,
    /// All milestones have been released to the freelancer.
    Completed = 2,
    /// A dispute has been raised; funds are frozen pending resolution.
    Disputed = 3,
}

/// A single payment milestone within an [`EscrowContract`].
#[contracttype]
#[derive(Clone, Debug)]
pub struct Milestone {
    /// Payment amount in stroops (1 XLM = 10 000 000 stroops).
    pub amount: i128,
    /// Whether the milestone payment has been released to the freelancer.
    pub released: bool,
    /// Address of the party that last approved this milestone, if any.
    pub approved_by: Option<Address>,
    /// Ledger timestamp at which the approval was recorded, if any.
    pub approval_timestamp: Option<u64>,
}

/// Defines who is authorised to approve and release milestone payments.
///
/// | Variant            | Approve                  | Release condition          |
/// |--------------------|--------------------------|----------------------------|
/// | `ClientOnly`       | Client                   | Client approved            |
/// | `ArbiterOnly`      | Arbiter                  | Arbiter approved           |
/// | `ClientAndArbiter` | Client **or** Arbiter    | Either approved            |
/// | `MultiSig`         | Client **or** Arbiter    | Client approved (see note) |
///
/// > **Note:** `MultiSig` is a placeholder for a full multi-signature flow.
/// > In the current implementation it behaves like `ClientOnly` at release
/// > time. A future version will require both client and arbiter approvals.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseAuthorization {
    /// Only the client may approve and trigger releases.
    ClientOnly = 0,
    /// Either the client or the arbiter may approve; either may trigger release.
    ClientAndArbiter = 1,
    /// Only the arbiter may approve and trigger releases.
    ArbiterOnly = 2,
    /// Both client and arbiter must approve before release (partial implementation).
    MultiSig = 3,
}

/// The on-chain record for a single escrow agreement.
#[contracttype]
#[derive(Clone, Debug)]
pub struct EscrowContract {
    /// Address of the client who funds the escrow.
    pub client: Address,
    /// Address of the freelancer who receives milestone payments.
    pub freelancer: Address,
    /// Optional arbiter address used for dispute resolution or multi-sig flows.
    pub arbiter: Option<Address>,
    /// Ordered list of milestones; index is used as `milestone_id`.
    pub milestones: Vec<Milestone>,
    /// Current lifecycle status of the contract.
    pub status: ContractStatus,
    /// Authorization scheme governing who can approve and release milestones.
    pub release_auth: ReleaseAuthorization,
    /// Ledger timestamp at which the contract was created.
    pub created_at: u64,
}

/// Tracks per-milestone multi-party approval state.
///
/// Used internally to support [`ReleaseAuthorization::MultiSig`] flows where
/// multiple parties must independently approve before a release is permitted.
#[contracttype]
#[derive(Clone, Debug)]
pub struct MilestoneApproval {
    /// Index of the milestone this record belongs to.
    pub milestone_id: u32,
    /// Map from approver address to approval boolean.
    pub approvals: Map<Address, bool>,
    /// Number of approvals required before release is permitted.
    pub required_approvals: u32,
    /// Aggregated approval status derived from `approvals`.
    pub approval_status: Approval,
}

/// Aggregated approval state for a milestone under a multi-party scheme.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Approval {
    /// No approvals recorded yet.
    None = 0,
    /// Only the client has approved.
    Client = 1,
    /// Only the arbiter has approved.
    Arbiter = 2,
    /// Both client and arbiter have approved.
    Both = 3,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

/// The TalentTrust escrow contract entry point.
#[contract]
pub struct Escrow;

#[contractimpl]
impl Escrow {
    /// Create a new escrow contract with milestone-based release authorization.
    ///
    /// Stores the contract record in persistent storage and returns a numeric
    /// identifier derived from the current ledger sequence number.
    ///
    /// # Arguments
    ///
    /// | Name                | Type                    | Description                                      |
    /// |---------------------|-------------------------|--------------------------------------------------|
    /// | `env`               | `Env`                   | Soroban host environment.                        |
    /// | `client`            | `Address`               | Client who will fund the escrow.                 |
    /// | `freelancer`        | `Address`               | Freelancer who will receive milestone payments.  |
    /// | `arbiter`           | `Option<Address>`       | Optional arbiter for disputes / multi-sig.       |
    /// | `milestone_amounts` | `Vec<i128>`             | Ordered list of milestone amounts in stroops.    |
    /// | `release_auth`      | `ReleaseAuthorization`  | Authorization scheme for milestone releases.     |
    ///
    /// # Returns
    ///
    /// A `u32` contract identifier (current ledger sequence number).
    ///
    /// # Panics
    ///
    /// | Condition                                      | Message                                          |
    /// |------------------------------------------------|--------------------------------------------------|
    /// | `milestone_amounts` is empty                   | `"At least one milestone required"`              |
    /// | `client == freelancer`                         | `"Client and freelancer cannot be the same address"` |
    /// | Any milestone amount is `<= 0`                 | `"Milestone amounts must be positive"`           |
    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        arbiter: Option<Address>,
        milestone_amounts: Vec<i128>,
        release_auth: ReleaseAuthorization,
    ) -> u32 {
        if milestone_amounts.is_empty() {
            panic!("At least one milestone required");
        }

        if client == freelancer {
            panic!("Client and freelancer cannot be the same address");
        }

        for i in 0..milestone_amounts.len() {
            if milestone_amounts.get(i).unwrap() <= 0 {
                panic!("Milestone amounts must be positive");
            }
        }

        let mut milestones = Vec::new(&env);
        for i in 0..milestone_amounts.len() {
            milestones.push_back(Milestone {
                amount: milestone_amounts.get(i).unwrap(),
                released: false,
                approved_by: None,
                approval_timestamp: None,
            });
        }

        let contract_data = EscrowContract {
            client: client.clone(),
            freelancer: freelancer.clone(),
            arbiter,
            milestones,
            status: ContractStatus::Created,
            release_auth,
            created_at: env.ledger().timestamp(),
        };

        let contract_id = env.ledger().sequence();

        env.storage()
            .persistent()
            .set(&symbol_short!("contract"), &contract_data);

        contract_id
    }

    /// Deposit the full escrow amount into the contract.
    ///
    /// Only the client may call this function. The deposited amount must equal
    /// the sum of all milestone amounts. On success the contract status
    /// transitions from `Created` to `Funded`.
    ///
    /// # Arguments
    ///
    /// | Name          | Type      | Description                                         |
    /// |---------------|-----------|-----------------------------------------------------|
    /// | `env`         | `Env`     | Soroban host environment.                           |
    /// | `_contract_id`| `u32`     | Identifier of the escrow contract (reserved).       |
    /// | `caller`      | `Address` | Must be the client address; auth is required.       |
    /// | `amount`      | `i128`    | Amount in stroops; must equal total milestone sum.  |
    ///
    /// # Returns
    ///
    /// `true` on success.
    ///
    /// # Panics
    ///
    /// | Condition                                      | Message                                                    |
    /// |------------------------------------------------|------------------------------------------------------------|
    /// | Contract record not found in storage           | `"Contract not found"`                                     |
    /// | `caller` is not the client                     | `"Only client can deposit funds"`                          |
    /// | Contract status is not `Created`               | `"Contract must be in Created status to deposit funds"`    |
    /// | `amount` ≠ sum of all milestone amounts        | `"Deposit amount must equal total milestone amounts"`      |
    pub fn deposit_funds(env: Env, _contract_id: u32, caller: Address, amount: i128) -> bool {
        caller.require_auth();

        let contract: EscrowContract = env
            .storage()
            .persistent()
            .get(&symbol_short!("contract"))
            .unwrap_or_else(|| panic!("Contract not found"));

        if caller != contract.client {
            panic!("Only client can deposit funds");
        }

        if contract.status != ContractStatus::Created {
            panic!("Contract must be in Created status to deposit funds");
        }

        let mut total_required = 0i128;
        for i in 0..contract.milestones.len() {
            total_required += contract.milestones.get(i).unwrap().amount;
        }

        if amount != total_required {
            panic!("Deposit amount must equal total milestone amounts");
        }

        let mut updated_contract = contract;
        updated_contract.status = ContractStatus::Funded;
        env.storage()
            .persistent()
            .set(&symbol_short!("contract"), &updated_contract);

        true
    }

    /// Record an approval for a specific milestone from an authorised party.
    ///
    /// The caller must be permitted under the contract's [`ReleaseAuthorization`]
    /// scheme. Each address may only approve a given milestone once. Approval
    /// does **not** release funds; call [`Escrow::release_milestone`] after
    /// sufficient approvals have been recorded.
    ///
    /// # Arguments
    ///
    /// | Name           | Type      | Description                                              |
    /// |----------------|-----------|----------------------------------------------------------|
    /// | `env`          | `Env`     | Soroban host environment.                                |
    /// | `_contract_id` | `u32`     | Identifier of the escrow contract (reserved).            |
    /// | `caller`       | `Address` | Approving party; must be authorised and auth is required.|
    /// | `milestone_id` | `u32`     | Zero-based index of the milestone to approve.            |
    ///
    /// # Returns
    ///
    /// `true` on success.
    ///
    /// # Panics
    ///
    /// | Condition                                          | Message                                                          |
    /// |----------------------------------------------------|------------------------------------------------------------------|
    /// | Contract record not found in storage               | `"Contract not found"`                                           |
    /// | Contract status is not `Funded`                    | `"Contract must be in Funded status to approve milestones"`      |
    /// | `milestone_id` ≥ number of milestones              | `"Invalid milestone ID"`                                         |
    /// | Milestone has already been released                | `"Milestone already released"`                                   |
    /// | `caller` is not authorised under `release_auth`    | `"Caller not authorized to approve milestone release"`           |
    /// | `caller` has already approved this milestone       | `"Milestone already approved by this address"`                   |
    pub fn approve_milestone_release(
        env: Env,
        _contract_id: u32,
        caller: Address,
        milestone_id: u32,
    ) -> bool {
        caller.require_auth();

        let mut contract: EscrowContract = env
            .storage()
            .persistent()
            .get(&symbol_short!("contract"))
            .unwrap_or_else(|| panic!("Contract not found"));

        if contract.status != ContractStatus::Funded {
            panic!("Contract must be in Funded status to approve milestones");
        }

        if milestone_id >= contract.milestones.len() {
            panic!("Invalid milestone ID");
        }

        let milestone = contract.milestones.get(milestone_id).unwrap();

        if milestone.released {
            panic!("Milestone already released");
        }

        let is_authorized = match contract.release_auth {
            ReleaseAuthorization::ClientOnly => caller == contract.client,
            ReleaseAuthorization::ArbiterOnly => {
                contract.arbiter.clone().map_or(false, |a| caller == a)
            }
            ReleaseAuthorization::ClientAndArbiter => {
                caller == contract.client || contract.arbiter.clone().map_or(false, |a| caller == a)
            }
            ReleaseAuthorization::MultiSig => {
                caller == contract.client || contract.arbiter.clone().map_or(false, |a| caller == a)
            }
        };

        if !is_authorized {
            panic!("Caller not authorized to approve milestone release");
        }

        if milestone
            .approved_by
            .clone()
            .map_or(false, |addr| addr == caller)
        {
            panic!("Milestone already approved by this address");
        }

        let mut updated_milestone = milestone;
        updated_milestone.approved_by = Some(caller);
        updated_milestone.approval_timestamp = Some(env.ledger().timestamp());

        contract.milestones.set(milestone_id, updated_milestone);
        env.storage()
            .persistent()
            .set(&symbol_short!("contract"), &contract);

        true
    }

    /// Release a milestone payment to the freelancer after sufficient approvals.
    ///
    /// Verifies that the required approvals are in place according to the
    /// contract's [`ReleaseAuthorization`] scheme, marks the milestone as
    /// released, and transitions the contract to `Completed` if all milestones
    /// have been released.
    ///
    /// > **Note:** Actual token transfer to the freelancer is not implemented
    /// > in this version and must be wired up via the Stellar asset contract.
    ///
    /// # Arguments
    ///
    /// | Name           | Type      | Description                                              |
    /// |----------------|-----------|----------------------------------------------------------|
    /// | `env`          | `Env`     | Soroban host environment.                                |
    /// | `_contract_id` | `u32`     | Identifier of the escrow contract (reserved).            |
    /// | `caller`       | `Address` | Caller triggering the release; auth is required.         |
    /// | `milestone_id` | `u32`     | Zero-based index of the milestone to release.            |
    ///
    /// # Returns
    ///
    /// `true` on success.
    ///
    /// # Panics
    ///
    /// | Condition                                          | Message                                                          |
    /// |----------------------------------------------------|------------------------------------------------------------------|
    /// | Contract record not found in storage               | `"Contract not found"`                                           |
    /// | Contract status is not `Funded`                    | `"Contract must be in Funded status to release milestones"`      |
    /// | `milestone_id` ≥ number of milestones              | `"Invalid milestone ID"`                                         |
    /// | Milestone has already been released                | `"Milestone already released"`                                   |
    /// | Required approvals are not present                 | `"Insufficient approvals for milestone release"`                 |
    pub fn release_milestone(
        env: Env,
        _contract_id: u32,
        caller: Address,
        milestone_id: u32,
    ) -> bool {
        caller.require_auth();

        let mut contract: EscrowContract = env
            .storage()
            .persistent()
            .get(&symbol_short!("contract"))
            .unwrap_or_else(|| panic!("Contract not found"));

        if contract.status != ContractStatus::Funded {
            panic!("Contract must be in Funded status to release milestones");
        }

        if milestone_id >= contract.milestones.len() {
            panic!("Invalid milestone ID");
        }

        let milestone = contract.milestones.get(milestone_id).unwrap();

        if milestone.released {
            panic!("Milestone already released");
        }

        let has_sufficient_approval = match contract.release_auth {
            ReleaseAuthorization::ClientOnly => milestone
                .approved_by
                .clone()
                .map_or(false, |addr| addr == contract.client),
            ReleaseAuthorization::ArbiterOnly => {
                contract.arbiter.clone().map_or(false, |arbiter| {
                    milestone
                        .approved_by
                        .clone()
                        .map_or(false, |addr| addr == arbiter)
                })
            }
            ReleaseAuthorization::ClientAndArbiter => {
                milestone.approved_by.clone().map_or(false, |addr| {
                    addr == contract.client
                        || contract
                            .arbiter
                            .clone()
                            .map_or(false, |arbiter| addr == arbiter)
                })
            }
            ReleaseAuthorization::MultiSig => milestone
                .approved_by
                .clone()
                .map_or(false, |addr| addr == contract.client),
        };

        if !has_sufficient_approval {
            panic!("Insufficient approvals for milestone release");
        }

        let mut updated_milestone = milestone;
        updated_milestone.released = true;

        contract.milestones.set(milestone_id, updated_milestone);

        let all_released = contract.milestones.iter().all(|m| m.released);
        if all_released {
            contract.status = ContractStatus::Completed;
        }

        env.storage()
            .persistent()
            .set(&symbol_short!("contract"), &contract);

        true
    }

    /// Issue a reputation credential for a freelancer after contract completion.
    ///
    /// This is a stub for the on-chain reputation system. In a full
    /// implementation it would mint a verifiable credential or update a
    /// reputation ledger entry for `freelancer`.
    ///
    /// # Arguments
    ///
    /// | Name         | Type      | Description                                    |
    /// |--------------|-----------|------------------------------------------------|
    /// | `_env`       | `Env`     | Soroban host environment (unused).             |
    /// | `_freelancer`| `Address` | Freelancer receiving the credential (unused).  |
    /// | `_rating`    | `i128`    | Numeric rating value, e.g. 1–5 (unused).       |
    ///
    /// # Returns
    ///
    /// `true` (always, stub implementation).
    pub fn issue_reputation(_env: Env, _freelancer: Address, _rating: i128) -> bool {
        true
    }

    /// Echo function used for smoke-testing and CI health checks.
    ///
    /// # Arguments
    ///
    /// | Name   | Type     | Description                    |
    /// |--------|----------|--------------------------------|
    /// | `_env` | `Env`    | Soroban host environment.      |
    /// | `to`   | `Symbol` | Symbol value to echo back.     |
    ///
    /// # Returns
    ///
    /// The same `Symbol` that was passed in.
    pub fn hello(_env: Env, to: Symbol) -> Symbol {
        to
    }
}

#[cfg(test)]
mod test;
