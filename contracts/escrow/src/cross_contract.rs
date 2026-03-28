//! # Cross-Contract Call Hardening
//!
//! This module provides validated, guarded wrappers around every external
//! (cross-contract) call made by the TalentTrust Escrow contract.
//!
//! ## Threat model
//!
//! | Threat                       | Mitigation                                      |
//! |------------------------------|-------------------------------------------------|
//! | Reentrancy attack            | [`CallLock`] stored in instance storage before  |
//! |                              | any external call; cleared after return         |
//! | Self-referential call        | [`validate_external_address`] rejects `addr ==  |
//! |                              | env.current_contract_address()`                 |
//! | Silent transfer failure      | `try_transfer` result is checked; returns       |
//! |                              | [`EscrowError::ExternalCallFailed`] on error    |
//! | Unregistered token contract  | [`get_required_token_contract`] returns          |
//! |                              | [`EscrowError::TokenContractNotSet`] if absent  |
//! | Zero-amount transfer         | Guard validates `amount > 0` before call        |
//! | Balance drift                | Pre/post balance comparison via                 |
//! |                              | [`verify_balance_received`]                     |
//!
//! ## Usage
//!
//! ```text
//! // Inside release_milestone:
//! let token = get_required_token_contract(&env)?;
//! safe_token_transfer(&env, &token, &from, &to, amount)?;
//! ```

use soroban_sdk::{token, Address, Env};

use crate::{DataKey, EscrowError};

// ── Reentrancy guard ──────────────────────────────────────────────────────────

/// Acquire the single reentrancy lock stored in contract instance storage.
///
/// # Security
/// Sets `DataKey::CallLock = true` before any external call so that a
/// malicious callee cannot re-enter the escrow contract mid-execution and
/// observe or mutate inconsistent state.
///
/// If the lock is already held (i.e., another external call is in progress
/// on this contract), the function returns
/// [`EscrowError::ReentrancyDetected`].
///
/// # Errors
/// - [`EscrowError::ReentrancyDetected`] – lock already acquired.
pub fn acquire_call_lock(env: &Env) -> Result<(), EscrowError> {
    let locked: bool = env
        .storage()
        .instance()
        .get(&DataKey::CallLock)
        .unwrap_or(false);

    if locked {
        return Err(EscrowError::ReentrancyDetected);
    }

    env.storage().instance().set(&DataKey::CallLock, &true);
    Ok(())
}

/// Release the reentrancy lock.
///
/// Must be called after every successful [`acquire_call_lock`], including
/// the error paths so the contract is never left in a permanently locked state.
/// Because Soroban rolls back all storage writes on panic, a forgotten
/// `release_call_lock` after a panicking path is safe—the lock is cleared by
/// the host rollback—but callers should still release explicitly for clarity.
pub fn release_call_lock(env: &Env) {
    env.storage().instance().remove(&DataKey::CallLock);
}

// ── Address validation ────────────────────────────────────────────────────────

/// Validate that `addr` is a safe target for an external call.
///
/// Guards enforced (in order):
/// 1. **Non-self**: `addr` must not equal `env.current_contract_address()`.
///    Self-calls would cause the escrow contract to manipulate its own state
///    unexpectedly mid-execution.
///
/// # Errors
/// - [`EscrowError::SelfReferentialAddress`] – `addr` is this contract.
pub fn validate_external_address(env: &Env, addr: &Address) -> Result<(), EscrowError> {
    if *addr == env.current_contract_address() {
        return Err(EscrowError::SelfReferentialAddress);
    }
    Ok(())
}

// ── Token contract access ─────────────────────────────────────────────────────

/// Retrieve the registered token contract address, returning an error if
/// none has been set via [`crate::Escrow::set_token_contract`].
///
/// # Errors
/// - [`EscrowError::TokenContractNotSet`] – no token contract registered.
pub fn get_required_token_contract(env: &Env) -> Result<Address, EscrowError> {
    env.storage()
        .instance()
        .get(&DataKey::TokenContract)
        .ok_or(EscrowError::TokenContractNotSet)
}

// ── Safe token transfer ───────────────────────────────────────────────────────

/// Execute a SEP-41 token transfer with all cross-contract call hardening.
///
/// # Hardening steps (in order)
///
/// 1. **Amount guard** – `amount` must be strictly positive.
/// 2. **Address guard** – `token_address` must not be `env.current_contract_address()`.
/// 3. **Reentrancy lock** – acquired before the external call.
/// 4. **Try-transfer** – uses `token::Client::try_transfer` rather than the
///    panicking `transfer`, so the error is captured and mapped to
///    [`EscrowError::ExternalCallFailed`] instead of aborting with an opaque
///    host panic.
/// 5. **Reentrancy unlock** – released unconditionally after the call returns.
///
/// # Arguments
///
/// | Name            | Description                                                    |
/// |-----------------|----------------------------------------------------------------|
/// | `token_address` | The SEP-41 compatible token contract to invoke.               |
/// | `from`          | Source address; must have pre-authorized the transfer.        |
/// | `to`            | Destination address.                                          |
/// | `amount`        | Transfer amount in base units (must be `> 0`).                |
///
/// # Errors
///
/// | Error                              | Condition                              |
/// |------------------------------------|----------------------------------------|
/// | [`EscrowError::AmountMustBePositive`] | `amount <= 0`                       |
/// | [`EscrowError::SelfReferentialAddress`] | `token_address == self`           |
/// | [`EscrowError::ReentrancyDetected`]  | Reentrant call detected              |
/// | [`EscrowError::ExternalCallFailed`]  | Token contract returned an error     |
pub fn safe_token_transfer(
    env: &Env,
    token_address: &Address,
    from: &Address,
    to: &Address,
    amount: i128,
) -> Result<(), EscrowError> {
    // Guard 1: amount must be positive
    if amount <= 0 {
        return Err(EscrowError::AmountMustBePositive);
    }

    // Guard 2: validate token contract address is not self
    validate_external_address(env, token_address)?;

    // Guard 3: acquire reentrancy lock
    acquire_call_lock(env)?;

    // Guard 4: execute transfer using try_transfer for explicit error handling
    let token_client = token::Client::new(env, token_address);
    let result = token_client
        .try_transfer(from, to, &amount)
        .map_err(|_| EscrowError::ExternalCallFailed)
        .and_then(|inner| inner.map_err(|_| EscrowError::ExternalCallFailed));

    // Guard 5: release lock unconditionally before returning
    release_call_lock(env);

    result
}

// ── Balance verification ──────────────────────────────────────────────────────

/// Read the SEP-41 token balance for `addr` from `token_address`.
///
/// This is a read-only call and does not acquire the reentrancy lock.
///
/// # Errors
/// - [`EscrowError::SelfReferentialAddress`] – `token_address == self`.
/// - [`EscrowError::ExternalCallFailed`] – balance query failed.
pub fn query_token_balance(
    env: &Env,
    token_address: &Address,
    addr: &Address,
) -> Result<i128, EscrowError> {
    validate_external_address(env, token_address)?;

    let token_client = token::Client::new(env, token_address);
    token_client
        .try_balance(addr)
        .map_err(|_| EscrowError::ExternalCallFailed)
        .and_then(|inner| inner.map_err(|_| EscrowError::ExternalCallFailed))
}

/// Verify that the balance of `recipient` increased by exactly `expected_delta`
/// compared to `balance_before`.
///
/// Call [`query_token_balance`] before the transfer (capturing `balance_before`),
/// execute the transfer, then call this function to confirm the delta.
///
/// # Errors
/// - All errors from [`query_token_balance`].
/// - [`EscrowError::TransferVerificationFailed`] – balance delta does not match.
pub fn verify_balance_received(
    env: &Env,
    token_address: &Address,
    recipient: &Address,
    balance_before: i128,
    expected_delta: i128,
) -> Result<(), EscrowError> {
    let balance_after = query_token_balance(env, token_address, recipient)?;

    let actual_delta = balance_after
        .checked_sub(balance_before)
        .ok_or(EscrowError::ArithmeticOverflow)?;

    if actual_delta != expected_delta {
        return Err(EscrowError::TransferVerificationFailed);
    }

    Ok(())
}
