use crate::types::ReputationConfig;
use crate::{
    ttl, types, Contract, ContractStatus, DataKey, Error, Escrow, EscrowError, PAGE_CEILING,
};
use soroban_sdk::{Address, Env, String, Symbol, Vec};

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

    if min_rating < 1
        || max_rating < min_rating
        || max_rating > 10
        || max_comment_bytes < 1
        || max_comment_bytes > 1_000
    {
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
    let mut contract: Contract = env
        .storage()
        .persistent()
        .get(&DataKey::Contract(contract_id))
        .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));
    ttl::extend_contract_ttl(env, contract_id);

    if caller != contract.client {
        env.panic_with_error(Error::UnauthorizedRole);
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

    if contract.status != ContractStatus::Completed {
        env.panic_with_error(Error::NotCompleted);
    }

    if contract.reputation_issued {
        env.panic_with_error(Error::ReputationAlreadyIssued);
    }
    if contract.client == contract.freelancer {
        env.panic_with_error(Error::UnauthorizedRole);
    }

    caller.require_auth();
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

    let pending_key = DataKey::PendingReputationCredits(contract.freelancer.clone());
    let pending: i128 = env.storage().persistent().get(&pending_key).unwrap_or(0);
    if pending <= 0 {
        env.panic_with_error(Error::NotCompleted);
    }
    let new_pending = pending
        .checked_sub(1)
        .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));
    env.storage().persistent().set(&pending_key, &new_pending);

    let rep_key = DataKey::Reputation(contract.freelancer.clone());
    let mut rep: types::Reputation = env.storage().persistent().get(&rep_key).unwrap_or_default();
    let first_write = rep.completed_contracts == 0;
    rep.completed_contracts += 1;
    rep.total_rating += rating as i128;
    rep.last_rating = rating as i128;
    env.storage().persistent().set(&rep_key, &rep);

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

    true
}

pub(crate) fn get_reputation_comment(env: &Env, contract_id: u32) -> Option<String> {
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

    if rep.completed_contracts == 0 {
        return None;
    }

    rep.total_rating
        .checked_mul(SCALE)
        .and_then(|scaled| scaled.checked_div(rep.completed_contracts))
}

pub(crate) fn get_pending_reputation_credits(env: &Env, address: Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::PendingReputationCredits(address))
        .unwrap_or(0)
}

pub(crate) fn get_reputations_page(
    env: &Env,
    start: u32,
    limit: u32,
) -> Vec<types::ReputationEntry> {
    let limit = limit.min(PAGE_CEILING);
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
        let acct = idx.get(i as u32).unwrap();
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
    let pending_key = DataKey::PendingReputationCredits(freelancer.clone());
    let pending: i128 = env.storage().persistent().get(&pending_key).unwrap_or(0);
    env.storage().persistent().set(&pending_key, &(pending + 1));
}
