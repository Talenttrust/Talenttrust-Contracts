<<<<<<< HEAD
use soroban_sdk::{symbol_short, Address, Env, Symbol};

use crate::{DataKey, Error, Escrow};

/// Constant for basis point calculation (10000 basis points = 100%)
const BASIS_POINTS_DENOMINATOR: i128 = 10000;
const ROUNDING_ADJUSTMENT: i128 = 9999;

impl Escrow {
    /// Initialize contract with admin and protocol fee configuration.
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `admin` - The address of the contract administrator
    /// * `protocol_fee_bps` - Protocol fee in basis points (1 bps = 0.01%)
    /// 
    /// # Requirements
    /// - Can only be called once
    /// - Admin must authorize this call
    /// 
    /// # Events
    /// Emits an `initialized` event with (admin, protocol_fee_bps, timestamp)
    /// 
    /// # Errors
    /// * `AlreadyInitialized` - If contract is already initialized
    pub fn initialize(env: Env, admin: Address, protocol_fee_bps: u32) -> bool {
        // Check if already initialized
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Initialized)
            .unwrap_or(false)
        {
            env.panic_with_error(Error::AlreadyInitialized);
        }

        admin.require_auth();

        // Store admin
        env.storage()
            .persistent()
            .set(&DataKey::Admin, &admin);

        // Store protocol fee bps
        env.storage()
            .persistent()
            .set(&DataKey::ProtocolFeeBps, &protocol_fee_bps);

        // Initialize accumulated protocol fees to 0
        env.storage()
            .persistent()
            .set(&DataKey::AccumulatedProtocolFees, &0i128);

        // Mark as initialized
        env.storage()
            .persistent()
            .set(&DataKey::Initialized, &true);

        // Emit initialization event
        env.events().publish(
            (symbol_short!("init"),),
            (admin, protocol_fee_bps, env.ledger().timestamp()),
=======
use crate::{DataKey, EscrowError};
use soroban_sdk::{symbol_short, Address, Env};

#[allow(dead_code)]
impl super::Escrow {
    /// Withdraws accumulated protocol fees from contract state.
    ///
    /// Requires the stored admin to authorize the call, rejects when no fees are
    /// available, and rejects while the contract is paused or in emergency state.
    ///
    /// The accumulator at `DataKey::AccumulatedProtocolFees` is reset to zero
    /// atomically and a `fee_wd` event is emitted with `(recipient, amount,
    /// timestamp)`.
    pub fn withdraw_protocol_fees(env: Env, admin: Address, recipient: Address) -> bool {
        Self::require_admin(&env, &admin);

        if env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Paused)
            .unwrap_or(false)
        {
            env.panic_with_error(EscrowError::ContractPaused);
        }

        if env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Emergency)
            .unwrap_or(false)
        {
            env.panic_with_error(EscrowError::EmergencyActive);
        }

        let amount: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::AccumulatedProtocolFees)
            .unwrap_or(0_i128);

        if amount <= 0 {
            env.panic_with_error(EscrowError::InsufficientAccumulatedFees);
        }

        env.storage()
            .persistent()
            .set(&DataKey::AccumulatedProtocolFees, &0_i128);

        env.events().publish(
            (symbol_short!("fee_wd"),),
            (recipient.clone(), amount, env.ledger().timestamp()),
>>>>>>> 30df75a (I've completed this successfully.)
        );

        true
    }

<<<<<<< HEAD
    /// Withdraw accumulated protocol fees to a designated recipient.
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// * `admin` - The admin address making the withdrawal request
    /// * `recipient` - The address that will receive the fees
    /// * `amount` - The amount of fees to withdraw
    /// * `token` - The token address for the fee transfer
    /// 
    /// # Requirements
    /// - Contract must be initialized
    /// - Caller must be the authorized admin
    /// - Amount must be less than or equal to accumulated fees
    /// - Accumulated fees must be > 0
    /// 
    /// # Returns
    /// `true` if withdrawal was successful
    /// 
    /// # Events
    /// Emits a `fee_wd` (fee withdrawal) event with (recipient, amount, timestamp)
    /// 
    /// # Errors
    /// * `NotInitialized` - If contract is not initialized
    /// * `UnauthorizedRole` - If caller is not the admin
    /// * `InsufficientAccumulatedFees` - If accumulated fees < amount
    pub fn withdraw_protocol_fees(
        env: Env,
        admin: Address,
        recipient: Address,
        amount: i128,
        token: Address,
    ) -> bool {
        // Verify contract is initialized
        if !env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Initialized)
            .unwrap_or(false)
        {
            env.panic_with_error(Error::NotInitialized);
        }

        // Verify caller is the admin
=======
    fn require_admin(env: &Env, admin: &Address) {
        let initialized = env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Initialized)
            .unwrap_or(false);
        if !initialized {
            env.panic_with_error(EscrowError::NotInitialized);
        }

>>>>>>> 30df75a (I've completed this successfully.)
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
<<<<<<< HEAD
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));

        if admin != stored_admin {
            env.panic_with_error(Error::UnauthorizedRole);
        }

        admin.require_auth();

        // Get accumulated protocol fees
        let accumulated_fees: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::AccumulatedProtocolFees)
            .unwrap_or(0);

        // Verify sufficient accumulated fees
        if accumulated_fees < amount || accumulated_fees == 0 {
            env.panic_with_error(Error::InsufficientAccumulatedFees);
        }

        // Update accumulated protocol fees (subtract withdrawn amount)
        let remaining_fees = accumulated_fees - amount;
        env.storage()
            .persistent()
            .set(&DataKey::AccumulatedProtocolFees, &remaining_fees);

        // Transfer fees using token interface
        let token_client = soroban_sdk::token::Client::new(&env, &token);
        token_client.transfer(&env.current_contract_address(), &recipient, &amount);

        // Emit fee withdrawal event with (recipient, amount, timestamp)
        env.events().publish(
            (Symbol::new(&env, "fee_wd"),),
            (recipient, amount, env.ledger().timestamp()),
        );

        true
    }

    /// Calculate protocol fee for a given amount.
    /// 
    /// Uses ceiling division to round up: `(amount * bps + 9999) / 10000`
    /// 
    /// # Arguments
    /// * `amount` - The base amount to calculate fee on
    /// * `bps` - The fee rate in basis points
    /// 
    /// # Returns
    /// The calculated fee amount, rounded up
    pub(crate) fn calculate_protocol_fee(amount: i128, bps: u32) -> i128 {
        let bps_i128 = bps as i128;
        (amount.saturating_mul(bps_i128).saturating_add(ROUNDING_ADJUSTMENT))
            / BASIS_POINTS_DENOMINATOR
    }

    /// Get the current accumulated protocol fees.
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// 
    /// # Returns
    /// The current accumulated protocol fees
    pub fn get_accumulated_protocol_fees(env: Env) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::AccumulatedProtocolFees)
            .unwrap_or(0)
    }

    /// Get the current protocol fee rate.
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// 
    /// # Returns
    /// The current protocol fee in basis points, or 0 if not initialized
    pub fn get_protocol_fee_bps(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::ProtocolFeeBps)
            .unwrap_or(0u32)
    }

    /// Get the current admin address.
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// 
    /// # Returns
    /// The current admin address, if initialized
    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().persistent().get(&DataKey::Admin)
    }

    /// Check if contract is initialized.
    /// 
    /// # Arguments
    /// * `env` - The contract environment
    /// 
    /// # Returns
    /// `true` if initialized, `false` otherwise
    pub fn is_initialized(env: Env) -> bool {
        env.storage()
            .persistent()
            .get::<_, bool>(&DataKey::Initialized)
            .unwrap_or(false)
=======
            .unwrap_or_else(|| env.panic_with_error(EscrowError::NotInitialized));

        if &stored_admin != admin {
            env.panic_with_error(EscrowError::UnauthorizedRole);
        }

        admin.require_auth();
>>>>>>> 30df75a (I've completed this successfully.)
    }
}
