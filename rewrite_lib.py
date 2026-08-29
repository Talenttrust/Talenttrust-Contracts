import os

with open('contracts/escrow/src/lib.rs', 'r') as f:
    content = f.read()

content = content.replace('mod dispute;\nmod governance;', 'mod dispute;\nmod reputation;\nmod governance;')

old_grant = """    pub(crate) fn grant_pending_reputation_credit(env: &Env, freelancer: &Address) {
        let pending_key = DataKey::PendingReputationCredits(freelancer.clone());
        let pending: i128 = env.storage().persistent().get(&pending_key).unwrap_or(0);
        env.storage().persistent().set(&pending_key, &(pending + 1));
    }"""

new_grant = """    pub(crate) fn grant_pending_reputation_credit(env: &Env, freelancer: &Address) {
        reputation::grant_pending_reputation_credit(env, freelancer);
    }"""

content = content.replace(old_grant, new_grant)

start_marker = "    pub fn get_reputation_config(env: Env) -> ReputationConfig {"
end_marker = """            res.push_back(types::ReputationEntry {
                account: acct.clone(),
                completed_contracts: rep.completed_contracts,
                total_rating: rep.total_rating,
                last_rating: rep.last_rating,
            });
        }
        res
    }"""

start_idx = content.find(start_marker)
end_idx = content.find(end_marker) + len(end_marker)

if start_idx != -1 and end_idx != -1:
    new_rep_block = """    pub fn get_reputation_config(env: Env) -> ReputationConfig {
        reputation::get_reputation_config(&env)
    }

    /// Admin-only setter for the reputation validation parameters enforced by
    /// [`Escrow::issue_reputation`].
    ///
    /// Requires the contract to be initialized and not paused, and enforces authorization
    /// for the caller acting as `DataKey::Admin`.
    ///
    /// # Validation
    /// - `min_rating` must be `>= 1`
    /// - `max_rating` must be `>= min_rating` and `<= 10`
    /// - `max_comment_bytes` must be `>= 1` and `<= 1_000`
    ///
    /// # Events
    /// * `(Symbol("rep_cfg"),)`
    /// * Data: `(old_config: ReputationConfig, new_config: ReputationConfig, admin: Address, timestamp: u64)`
    pub fn set_reputation_config(
        env: Env,
        min_rating: u32,
        max_rating: u32,
        max_comment_bytes: u32,
    ) -> bool {
        reputation::set_reputation_config(&env, min_rating, max_rating, max_comment_bytes)
    }

    /// Admin-only operation to restore the default reputation parameters.
    ///
    /// If the configuration is already default, no storage writes or events occur.
    ///
    /// # Events
    /// * `(Symbol("rep_cfg_reset"),)`
    /// * Data: `(old_config: ReputationConfig, default_config: ReputationConfig, admin: Address, timestamp: u64)`
    pub fn reset_reputation_config(env: Env) -> bool {
        reputation::reset_reputation_config(&env)
    }

    /// Issues reputation credit for a completed contract.
    ///
    /// Only the client of a `Completed` contract may issue a rating and comment for the
    /// freelancer. This entrypoint consumes exactly one pending reputation credit.
    ///
    /// # Errors
    /// * `UnauthorizedRole` - If called by anyone other than the client
    /// * `NotCompleted` - If the contract has not reached the `Completed` state
    /// * `InvalidRating` - If the rating is outside the configured bounds
    /// * `CommentTooLong` - If the comment length exceeds the configured maximum
    /// * `EmptyComment` - If the comment is empty
    /// * `ReputationAlreadyIssued` - If reputation was already issued
    /// * `SelfRating` - If the client and freelancer are the same address
    /// * `NoPendingReputationCredits` - If the freelancer has no pending reputation credits
    pub fn issue_reputation(
        env: Env,
        contract_id: u32,
        caller: Address,
        rating: u32,
        comment: String,
    ) -> bool {
        reputation::issue_reputation(&env, contract_id, caller, rating, comment)
    }

    /// Returns the written feedback provided by the client when reputation was issued.
    /// Returns `None` if reputation has not been issued for this contract.
    pub fn get_reputation_comment(env: Env, contract_id: u32) -> Option<String> {
        reputation::get_reputation_comment(&env, contract_id)
    }

    pub fn get_reputation(env: Env, address: Address) -> Option<types::Reputation> {
        reputation::get_reputation(&env, address)
    }

    /// Returns the freelancer's average rating scaled to basis points (×10 000),
    /// or `None` if no reputation record exists or no contracts have been completed.
    ///
    /// # Scaling
    /// `result = total_rating * 10_000 / completed_contracts`
    ///
    /// A raw rating of 5 on a single contract returns `50_000` (5.0000 on a
    /// 1–5 scale).  Clients divide by `10_000` to recover the decimal value.
    ///
    /// Checked arithmetic is used throughout; division by zero is impossible
    /// because `None` is returned whenever `completed_contracts == 0`.
    pub fn get_average_rating(env: Env, address: Address) -> Option<i128> {
        reputation::get_average_rating(&env, address)
    }

    /// Returns the number of completed contracts awaiting a reputation rating.
    ///
    /// This value increments once per completed contract and decrements once
    /// per successful `issue_reputation` call. Refunded contracts do not accrue
    /// pending reputation credits.
    pub fn get_pending_reputation_credits(env: Env, address: Address) -> i128 {
        reputation::get_pending_reputation_credits(&env, address)
    }

    /// Returns a bounded, paginated read view over reputation records.
    ///
    /// - `start` is a zero-based index into the reputations index.
    /// - `limit` is the maximum number of entries to return; it is clamped by PAGE_CEILING.
    ///
    /// Empty-safe: returns empty Vec when the index is missing, start is out-of-range,
    /// or limit is 0. Each returned element includes the account address and the
    /// stored reputation snapshot.
    pub fn get_reputations_page(env: Env, start: u32, limit: u32) -> Vec<types::ReputationEntry> {
        reputation::get_reputations_page(&env, start, limit)
    }"""
    
    content = content[:start_idx] + new_rep_block + content[end_idx:]
else:
    print("Could not find reputation block.")

with open('contracts/escrow/src/lib.rs', 'w') as f:
    f.write(content)
