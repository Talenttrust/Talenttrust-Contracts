//! Test harness for the TalentTrust escrow contract.
//!
//! # Structure
//!
//! Shared helpers and constants are defined here.  Child modules, each in
//! their own file under `test/`, import them via `use super::{...}`.
//!
//! | Module                | Focus                                              |
//! |-----------------------|----------------------------------------------------|
//! | `base`                | Hello / smoke tests                                |
//! | `create_contract_errors` | Validation errors on contract creation          |
//! | `operation_errors`    | Validation errors on deposit / release / rep       |
//! | `flows`               | Happy-path end-to-end flow tests                   |
//! | `lifecycle`           | Lifecycle state and status transitions             |
//! | `governance`          | Protocol-governance admin management               |
//! | `pause_controls`      | Normal pause / unpause logic                       |
//! | `emergency_controls`  | Emergency pause / resolve logic                    |
//! | `security`            | Access control, replay protection, edge cases      |
//! | `performance`         | Resource-budget regression baselines               |
//! | `events`              | Event payload and ordering assertions              |

extern crate std;

use soroban_sdk::{symbol_short, testutils::Address as _, vec, Address, Env, Symbol, Vec};

use crate::{Escrow, EscrowClient};

// ─── Shared constants ────────────────────────────────────────────────────────

/// First milestone amount (2 XLM equivalents in stroops).
pub const MILESTONE_ONE: i128 = 200_0000000;

/// Second milestone amount.
pub const MILESTONE_TWO: i128 = 400_0000000;

/// Third milestone amount.
pub const MILESTONE_THREE: i128 = 600_0000000;

// ─── Shared helpers ──────────────────────────────────────────────────────────

/// Sum of all three default milestone amounts.
pub fn total_milestone_amount() -> i128 {
    MILESTONE_ONE + MILESTONE_TWO + MILESTONE_THREE
}

/// Constructs the default three-milestone `Vec` for the given environment.
pub fn default_milestones(env: &Env) -> Vec<i128> {
    vec![env, MILESTONE_ONE, MILESTONE_TWO, MILESTONE_THREE]
}

/// Generates two fresh random addresses `(client, freelancer)`.
pub fn generated_participants(env: &Env) -> (Address, Address) {
    (Address::generate(env), Address::generate(env))
}

/// Registers the `Escrow` contract and returns its client.
pub fn register_client(env: &Env) -> EscrowClient<'_> {
    EscrowClient::new(env, &env.register(Escrow, ()))
}

/// A generic greeting symbol used in `hello` smoke tests.
pub fn world_symbol() -> Symbol {
    symbol_short!("World")
}

// ─── Child test modules ──────────────────────────────────────────────────────

mod base;
mod create_contract_errors;
mod emergency_controls;
mod events;
mod flows;
mod governance;
mod lifecycle;
mod operation_errors;
mod pause_controls;
mod performance;
mod security;
