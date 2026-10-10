use crate::types::ReputationConfig;
use crate::{
    constants, ttl, types, Contract, ContractStatus, DataKey, Error, Escrow, EscrowError,
    PAGE_CEILING,
};
use soroban_sdk::{symbol_short, Address, Env, String, Symbol, Vec};

/// Maximum number of bytes allowed in a reputation comment.
pub(crate) const MAX_COMMENT_BYTES: u32 = 1_000;

/// Minimum acceptable rating value.
pub(crate) const MIN_RATING: u32 = 1;

/// Maximum acceptable rating value.
pub(crate) const MAX_RATING: u32 = 10;

/// Returns true when the rating falls within the configured inclusive bounds.
pub(crate) fn is_valid_rating(rating: u32, config: &ReputationConfig) -> bool {
    rating >= config.min_rating && rating <= config.max_rating
}

/// Returns true when the comment length falls within the configured inclusive bounds.
pub(crate) fn is_valid_comment_length(len: u32, config: &ReputationConfig) -> bool {
    len >= 1 && len <= config.max_comment_bytes
}

/// Returns true when the configuration values are within the protocol bounds.
pub(crate) fn is_valid_config(
    min_rating: u32,
    max_rating: u32,
    max_comment_bytes: u32,
) -> bool {
    min_rating >= MIN_RATING
        && max_rating >= min_rating
        && max_rating <= MAX_RATING
        && max_comment_bytes >= 1
        && max_comment_bytes <= MAX_COMMENT_BYTES
}

pub(crate) fn get_reputation_config(env: &Env) -> ReputationConfig {
    env.storage()
        .persistent()
        .get(&DataKey::ReputationConfigKey)
        .unwrap_or_default()
}

pub(crate) fn set_reputation_config(
    env: &Env,
    min_rating: u32,
    max_rating: u32,
    max_comment_bytes: u32,
) -> bool {
    Escrow::require_initialized(env);
    Escrow::require_not_paused(env);

    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| env.panic_with_error(EscrowError::NotInitialized));
    admin.require_auth();

    if !is_valid_config(min_rating, max_rating, max_comment_bytes) {
        env.panic_with_error(Error::InvalidProtocolParameters);
    }

    let old_config = get_reputation_config(env);
    let new_config = ReputationConfig {
        min_rating,
        max_rating,
        max_comment_bytes,
    };
    env.storage()
        .persistent()
        .set(&DataKey::ReputationConfigKey, &new_config);

    env.events().publish(
        (Symbol::new(env, "rep_cfg"),),
        (old_config, new_config, admin, env.ledger().timestamp()),
    );
    true
}

pub(crate) fn reset_reputation_config(env: &Env) -> bool {
    Escrow::require_initialized(env);

    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| env.panic_with_error(EscrowError::NotInitialized));
    admin.require_auth();

    let old_config = get_reputation_config(env);
    let default_config = ReputationConfig::default();

    if old_config != default_config {
        env.storage()
            .persistent()
            .set(&DataKey::ReputationConfigKey, &default_config);

        env.events().publish(
            (Symbol::new(env, "rep_cfg_reset"),),
            (old_config, default_config, admin, env.ledger().timestamp()),
        );
    }

    true
}

pub(crate) fn issue_reputation(
    env: &Env,
    contract_id: u32,
    caller: Address,
    rating: u32,
    comment: String,
) -> bool {
    Escrow::require_not_paused(env);

    // Boundary: contract identifiers start at 1. Contract 0 is always invalid.
    if contract_id == 0 {
        env.panic_with_error(Error::ContractNotFound);
    }

    let mut contract: Contract = env
        .storage()
        .persistent()
        .get(&DataKey::Contract(contract_id))
        .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));
    ttl::extend_contract_ttl(env, contract_id);

    if caller != contract.client {
        env.panic_with_error(Error::UnauthorizedRole);
    }

    caller.require_auth();

    if contract.reputation_issued {
        return true;
    }

    let reputation_config = get_reputation_config(env);

    if rating < reputation_config.min_rating || rating > reputation_config.max_rating {
        env.panic_with_error(Error::InvalidRating);
    }

    if comment.len() == 0 {
        env.panic_with_error(Error::EmptyComment);
    }

    if comment.len() > reputation_config.max_comment_bytes {
        env.panic_with_error(Error::CommentTooLong);
    }

    // Contract must be completed before reputation can be issued.
    if contract.status != ContractStatus::Completed {
        env.panic_with_error(Error::NotCompleted);
    }

    // Upgrades may retain either issuance marker. Neither may be cleared or
    // ignored to make a previously rated contract eligible for another credit.
    let issued = env
        .storage()
        .persistent()
        .get::<_, bool>(&DataKey::ReputationIssued(contract_id))
        .unwrap_or(false);
    if contract.reputation_issued || issued {
        env.panic_with_error(Error::ReputationAlreadyIssued);
    }
    if contract.client == contract.freelancer {
        env.panic_with_error(Error::UnauthorizedRole);
    }

    caller.require_auth();
    let pending_key = DataKey::PendingReputationCredits(contract.freelancer.clone());
    let pending: i128 = env.storage().persistent().get(&pending_key).unwrap_or(0);
    if pending <= 0 {
        env.panic_with_error(Error::NotCompleted);
    }
    let new_pending = pending
        .checked_sub(1)
        .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));
    let rep_key = DataKey::Reputation(contract.freelancer.clone());
    let mut rep: types::Reputation = env.storage().persistent().get(&rep_key).unwrap_or_default();
    let first_write = rep.completed_contracts == 0;
    if rep.completed_contracts < 0 || rep.total_rating < 0 || rep.last_rating < 0 {
        env.panic_with_error(Error::InvalidState);
    }
    rep.completed_contracts = rep
        .completed_contracts
        .checked_add(1)
        .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));
    rep.total_rating = rep
        .total_rating
        .checked_add(rating as i128)
        .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));
    rep.last_rating = rating as i128;

    // Validate and authorize before writing. Soroban atomically rolls back all
    // these writes on host failure; retries cannot consume another credit.
    contract.reputation_issued = true;
    env.storage()
        .persistent()
        .set(&DataKey::Contract(contract_id), &contract);
    env.storage()
        .persistent()
        .set(&DataKey::ReputationIssued(contract_id), &true);
    env.storage().persistent().extend_ttl(
        &DataKey::ReputationIssued(contract_id),
        ttl::PERSISTENT_BUMP_THRESHOLD,
        ttl::PERSISTENT_TTL_LEDGERS,
    );
    env.storage().persistent().set(&pending_key, &new_pending);
    env.storage().persistent().set(&rep_key, &rep);
    env.storage().persistent().extend_ttl(
        &rep_key,
        ttl::PERSISTENT_BUMP_THRESHOLD,
        ttl::PERSISTENT_TTL_LEDGERS,
    );

    if first_write {
        let mut idx: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::ReputationIndex)
            .unwrap_or_else(|| Vec::new(env));
        idx.push_back(contract.freelancer.clone());
        env.storage()
            .persistent()
            .set(&DataKey::ReputationIndex, &idx);
    }

    let comment_key = DataKey::ReputationComment(contract_id);
    env.storage().persistent().set(&comment_key, &comment);
    env.storage().persistent().extend_ttl(
        &comment_key,
        ttl::PERSISTENT_BUMP_THRESHOLD,
        ttl::PERSISTENT_TTL_LEDGERS,
    );

    // Preserve the deployed event contract for existing indexers. Emission
    // follows successful writes, so rejected/replayed issuance emits nothing.
    env.events().publish(
        (symbol_short!("rep_issd"), contract_id),
        (contract.freelancer, rating, env.ledger().timestamp()),
    );

    true
}

pub(crate) fn get_reputation_comment(env: &Env, contract_id: u32) -> Option<String> {
    // Boundary: contract identifiers start at 1. Contract 0 is always invalid.
    if contract_id == 0 {
        env.panic_with_error(Error::ContractNotFound);
    }

    // Reject identifiers that have never been allocated.
    let next_id: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::NextContractId)
        .unwrap_or(1);
    if contract_id >= next_id {
        env.panic_with_error(Error::ContractNotFound);
    }

    let comment_key = DataKey::ReputationComment(contract_id);
    let comment: Option<String> = env.storage().persistent().get(&comment_key);
    if comment.is_some() {
        env.storage().persistent().extend_ttl(
            &comment_key,
            ttl::PERSISTENT_BUMP_THRESHOLD,
            ttl::PERSISTENT_TTL_LEDGERS,
        );
    }
    comment
}

pub(crate) fn get_reputation(env: &Env, address: Address) -> Option<types::Reputation> {
    env.storage()
        .persistent()
        .get(&DataKey::Reputation(address))
}

pub(crate) fn get_average_rating(env: &Env, address: Address) -> Option<i128> {
    const SCALE: i128 = 10_000;

    let rep: types::Reputation = env
        .storage()
        .persistent()
        .get(&DataKey::Reputation(address))?;

    if rep.completed_contracts <= 0 {
        return None;
    }

    rep.total_rating
        .checked_mul(SCALE)
        .and_then(|scaled| scaled.checked_div(rep.completed_contracts))
}

pub(crate) fn get_pending_reputation_credits(env: &Env, address: Address) -> i128 {
    let pending: i128 = env
        .storage()
        .persistent()
        .get(&DataKey::PendingReputationCredits(address.clone()))
        .unwrap_or(0);
    if pending > 0 {
        ttl::extend_pending_reputation_credits_ttl(env, &address);
    }
    pending
}

pub(crate) fn get_reputations_page(
    env: &Env,
    start: u32,
    limit: u32,
) -> Vec<types::ReputationEntry> {
    let limit = crate::constants::normalize_page_limit(limit);
    if limit == 0 {
        return Vec::new(env);
    }

    let idx: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::ReputationIndex)
        .unwrap_or_else(|| Vec::new(env));

    let total = idx.len();
    let start_usize = start as usize;
    if start_usize >= total as usize {
        return Vec::new(env);
    }
    let end = (start_usize + limit as usize).min(total as usize);

    let mut res: Vec<types::ReputationEntry> = Vec::new(env);
    for i in start_usize..end {
        let acct = match idx.get(i as u32) {
            Some(a) => a,
            None => continue,
        };
        let rep: types::Reputation = env
            .storage()
            .persistent()
            .get(&DataKey::Reputation(acct.clone()))
            .unwrap_or_default();
        res.push_back(types::ReputationEntry {
            account: acct.clone(),
            completed_contracts: rep.completed_contracts,
            total_rating: rep.total_rating,
            last_rating: rep.last_rating,
        });
    }
    res
}

pub(crate) fn grant_pending_reputation_credit(env: &Env, freelancer: &Address) {
    // Delegate to the root implementation so the crate has exactly one accrual
    // policy. A second, private copy of `pending + 1` is precisely how the two
    // implementations of this ledger drifted apart, which is the divergence
    // issue #1404 removes.
    Escrow::grant_pending_reputation_credit(env, freelancer);
}
