//! Dispute payout arithmetic and final-status helpers.
//!
//! This module owns dispute-related helpers:
//!
//! - [`resolution_payouts`] computes how the available escrow balance should be
//!   split for a [`DisputeResolution`].
//! - [`final_status_after_resolution`] decides whether dispute settlement leaves
//!   the contract as [`ContractStatus::Completed`] or [`ContractStatus::Refunded`].
//!
//! The root `raise_dispute` / `resolve_dispute` entrypoints live in
//! `contracts/escrow/src/lib.rs`.

use crate::{
    safe_add_amounts, Contract, ContractStatus, DisputeResolution, Error, Escrow,
    MAX_SINGLE_AMOUNT_STROOPS,
};

/// Compute the payout split for a dispute resolution.
///
/// Returns `(client_payout, freelancer_payout)` where both values are non-negative
/// and sum to the available balance.
///
/// # Errors
/// - `AccountingInvariantViolated` if available would be negative
/// - `PotentialOverflow` if intermediate calculations overflow
/// - `InvalidDisputeSplit` for Split variant with invalid amounts
pub fn resolution_payouts(
    contract: &Contract,
    resolution: &DisputeResolution,
) -> Result<(i128, i128), Error> {
    let available = crate::checked_available_balance(
        contract.funded_amount,
        contract.released_amount,
        contract.refunded_amount,
    )?;

    match resolution {
        DisputeResolution::FullRefund => Ok((available, 0)),
        DisputeResolution::PartialRefund => {
            let freelancer_payout = available
                .checked_mul(30)
                .and_then(|value| value.checked_div(100))
                .ok_or(Error::PotentialOverflow)?;
            Ok((available - freelancer_payout, freelancer_payout))
        }
        DisputeResolution::FullPayout => Ok((0, available)),
        DisputeResolution::Split(split) => {
            if split.client_amount < 0 || split.freelancer_amount < 0 {
                return Err(Error::InvalidDisputeSplit);
            }
            if split.client_amount > MAX_SINGLE_AMOUNT_STROOPS
                || split.freelancer_amount > MAX_SINGLE_AMOUNT_STROOPS
            {
                return Err(Error::InvalidDisputeSplit);
            }
            if split.client_amount > available || split.freelancer_amount > available {
                return Err(Error::InvalidDisputeSplit);
            }
            let total = safe_add_amounts(split.client_amount, split.freelancer_amount)
                .ok_or(Error::PotentialOverflow)?;
            if total > available || total != available {
                return Err(Error::InvalidDisputeSplit);
            }
            Ok((split.client_amount, split.freelancer_amount))
        }
    }
}

/// Determine the final contract status after dispute resolution.
///
/// Returns `Refunded` only when the full deposit has been refunded.
/// Otherwise returns `Completed`.
pub fn final_status_after_resolution(contract: &Contract) -> ContractStatus {
    if contract.refunded_amount == contract.funded_amount {
        ContractStatus::Refunded
    } else {
        ContractStatus::Completed
    }
}
