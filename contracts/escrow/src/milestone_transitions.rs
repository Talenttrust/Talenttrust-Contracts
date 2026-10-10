use crate::{DataKey, Error, Milestone};
use soroban_sdk::{symbol_short, Address, BytesN, Env, Symbol};

fn milestone_key(contract_id: u32, milestone_index: u32) -> (DataKey, (Symbol, u32)) {
    (
        DataKey::Contract(contract_id),
        (symbol_short!("mile"), milestone_index),
    )
}

/// Represents the logical state of a milestone based on its `released` and `refunded` flags.
///
/// The milestone state machine uses two boolean fields to represent implicit states:
/// - `released`: true when funds have been transferred to the freelancer
/// - `refunded`: true when funds have been returned to the client
///
/// This enum makes those states explicit for validation and documentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MilestoneState {
    /// Neither released nor refunded; awaiting action (released: false, refunded: false)
    Pending,
    /// Funds transferred to freelancer (released: true, refunded: false)
    Released,
    /// Funds returned to client (released: false, refunded: true)
    Refunded,
}

impl MilestoneState {
    /// Construct the current state from a Milestone's flags.
    pub fn from_milestone(milestone: &Milestone) -> Result<Self, Error> {
        match (milestone.released, milestone.refunded) {
            (false, false) => Ok(MilestoneState::Pending),
            (true, false) => Ok(MilestoneState::Released),
            (false, true) => Ok(MilestoneState::Refunded),
            (true, true) => {
                // This should never occur if transitions are properly guarded.
                // Both flags set is an invalid state.
                Err(Error::InvalidState)
            }
        }
    }

    /// Convert back to (released, refunded) tuple for storage.
    pub fn to_flags(self) -> (bool, bool) {
        match self {
            MilestoneState::Pending => (false, false),
            MilestoneState::Released => (true, false),
            MilestoneState::Refunded => (false, true),
        }
    }
}

/// Canonical milestone status-transition matrix.
///
/// This function is the single source of truth for determining which milestone
/// status transitions are legal. Every entrypoint that mutates milestone status
/// must call this function to validate the transition before applying the change.
///
/// **Transition Matrix:**
///
/// ```text
/// From\To    | Pending | Released | Refunded
/// -----------+---------+----------+----------
/// Pending    | ✓*      | ✓        | ✓
/// Released   | ✗       | ✓*       | ✗
/// Refunded   | ✗       | ✗        | ✓*
/// ```
///
/// Legend:
/// - ✓ = Valid transition
/// - ✓* = Transition to same state (idempotent, treated as allowed but should be validated per use-case)
/// - ✗ = Invalid transition (rejected with stable error)
///
/// **Intended State Machine Lifecycle:**
/// 1. Milestone created as **Pending** (default state)
/// 2. Can transition to **Released** via `release_milestone` (client, arbiter, or multi-sig approval)
/// 3. Can transition to **Refunded** via `refund_unreleased_milestones` (client-only, respects deadline)
/// 4. Once **Released** or **Refunded**, no further transitions allowed (terminal states)
/// 5. Contract-level cancellation or dispute resolution may affect availability of operations
///    but do not directly change individual milestone states
///
/// **Disagreement Resolution (from Issue #1340):**
/// Previously, both `refund_unreleased_milestones` and `cancel_contract` could refund
/// during dispute, but with different rule sets (deadline checking in refund vs. none in cancel).
/// This matrix enforces a single rule: once in Pending, can go to Released OR Refunded,
/// but no reversals. Authorization boundaries (e.g., only client can call refund) remain
/// enforced by each entrypoint separately, not by this matrix.
///
/// # Arguments
/// * `current` - The milestone's current state
/// * `requested` - The state being requested
///
/// # Returns
/// * `Ok(())` if the transition is valid
/// * `Err(InvalidStatusTransition)` if the transition is not allowed
///
/// # Example
/// ```ignore
/// let current = MilestoneState::Pending;
/// let requested = MilestoneState::Released;
/// validate_milestone_transition(current, requested)?; // OK
///
/// let current = MilestoneState::Released;
/// let requested = MilestoneState::Refunded;
/// validate_milestone_transition(current, requested)?; // Err: cannot reverse from Released to Refunded
/// ```
pub fn validate_milestone_transition(
    current: MilestoneState,
    requested: MilestoneState,
) -> Result<(), Error> {
    match (current, requested) {
        // From Pending
        (MilestoneState::Pending, MilestoneState::Pending) => Ok(()), // Idempotent
        (MilestoneState::Pending, MilestoneState::Released) => Ok(()), // Normal release flow
        (MilestoneState::Pending, MilestoneState::Refunded) => Ok(()), // Normal refund flow

        // From Released (terminal state)
        (MilestoneState::Released, MilestoneState::Released) => Ok(()), // Idempotent
        (MilestoneState::Released, MilestoneState::Pending) => {
            Err(Error::InvalidStatusTransition) // Cannot reverse from Released to Pending
        }
        (MilestoneState::Released, MilestoneState::Refunded) => {
            Err(Error::InvalidStatusTransition) // Cannot transition from Released to Refunded
        }

        // From Refunded (terminal state)
        (MilestoneState::Refunded, MilestoneState::Refunded) => Ok(()), // Idempotent
        (MilestoneState::Refunded, MilestoneState::Pending) => {
            Err(Error::InvalidStatusTransition) // Cannot reverse from Refunded to Pending
        }
        (MilestoneState::Refunded, MilestoneState::Released) => {
            Err(Error::InvalidStatusTransition) // Cannot transition from Refunded to Released
        }
    }
}

/// Metadata for version control and audit trails on milestone transitions.
///
/// Since we cannot modify the Milestone struct directly (backward compatibility),
/// these are stored separately under MilestoneVersion and MilestoneLastModifiedBy keys.
pub struct MilestoneTransitionMetadata {
    /// Version number (incremented on each successful transition) for optimistic concurrency control.
    pub version: u32,
    /// Address of the party that performed the last transition.
    pub last_modified_by: Address,
}

// ── Storage Access Helpers ───────────────────────────────────────────────────────

/// Reads the version and actor metadata for a milestone.
///
/// Returns defaults (version=0, actor=Address::from_contract_id(env, 0)) if not yet set,
/// ensuring backward compatibility with milestones created before this feature.
pub fn read_milestone_version_and_actor(
    env: &Env,
    contract_id: u32,
    milestone_index: u32,
) -> MilestoneTransitionMetadata {
    let version_key = DataKey::MilestoneVersion(contract_id, milestone_index);
    let actor_key = DataKey::MilestoneLastModifiedBy(contract_id, milestone_index);

    let version: u32 = env.storage().persistent().get(&version_key).unwrap_or(0);

    let last_modified_by: Address =
        env.storage()
            .persistent()
            .get(&actor_key)
            .unwrap_or_else(|| {
                // Default to current contract address for backward compatibility
                env.current_contract_address()
            });

    MilestoneTransitionMetadata {
        version,
        last_modified_by,
    }
}

/// Atomically increments the version and records the actor for a milestone transition.
///
/// Call this after successfully validating and applying a milestone status change.
/// This ensures the version/actor metadata is persisted in the same atomic storage
/// operation as the status change itself.
///
/// # Arguments
/// * `env` - The contract environment
/// * `contract_id` - The contract ID
/// * `milestone_index` - The milestone index
/// * `actor` - The address performing the transition
///
/// # Returns
/// The new version number after increment
pub fn store_milestone_transition(
    env: &Env,
    contract_id: u32,
    milestone_index: u32,
    actor: Address,
) -> u32 {
    let metadata = read_milestone_version_and_actor(env, contract_id, milestone_index);
    let new_version = metadata
        .version
        .checked_add(1)
        .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));

    let version_key = DataKey::MilestoneVersion(contract_id, milestone_index);
    let actor_key = DataKey::MilestoneLastModifiedBy(contract_id, milestone_index);

    env.storage().persistent().set(&version_key, &new_version);
    env.storage().persistent().set(&actor_key, &actor);

    new_version
}

/// Atomically validates the expected version and applies a milestone transition
/// in a single guarded operation to prevent TOCTOU races between concurrent callers.
///
/// This is the preferred entry point for any code path that mutates milestone
/// status. It performs the following steps in order:
///
/// 1. Reads the current `(state, version)` for the milestone.
/// 2. Verifies `expected_version` matches the stored version (optimistic
///    concurrency check). Mismatch returns `Error::InvalidStatusTransition`.
/// 3. Validates the requested state transition against the canonical matrix.
/// 4. Applies the new flags and bumps the version/actor metadata.
///
/// Because Soroban executes a contract invocation atomically, steps 1–4 are
/// observed as a single unit by any other invocation: a racing caller will
/// either see the pre-transition state (and succeed) or the post-transition
/// state (and fail the version check). Duplicate/idempotent retries with the
/// same `expected_version` are rejected on the second attempt, which is the
/// desired behavior for release/refund operations that must not double-spend.
///
/// # Arguments
/// * `env` - The contract environment
/// * `contract_id` - The contract ID
/// * `milestone_index` - The milestone index
/// * `expected_version` - The version the caller observed before deciding to act
/// * `requested` - The desired target state
/// * `actor` - The address performing the transition
///
/// # Returns
/// * `Ok(new_version)` on success
/// * `Err(InvalidStatusTransition)` on version mismatch or illegal transition
/// * `Err(InvalidState)` if the stored milestone has both flags set
pub fn apply_milestone_transition(
    env: &Env,
    contract_id: u32,
    milestone_index: u32,
    expected_version: u32,
    requested: MilestoneState,
    actor: Address,
) -> Result<u32, Error> {
    // 1. Optimistic concurrency check against the stored version.
    check_version_for_concurrency(env, contract_id, milestone_index, expected_version)?;

    // 2. Load the milestone and derive its current state.
    let key = milestone_key(contract_id, milestone_index);
    let mut milestone: Milestone = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(Error::MilestoneNotFound)?;
    let current = MilestoneState::from_milestone(&milestone)?;

    // 3. Validate the requested transition against the canonical matrix.
    validate_milestone_transition(current, requested)?;

    // 4. Apply the new flags and persist the milestone before bumping metadata
    //    so that a panic in metadata storage cannot leave flags updated without
    //    a corresponding version bump.
    let (released, refunded) = requested.to_flags();
    milestone.released = released;
    milestone.refunded = refunded;
    env.storage().persistent().set(&key, &milestone);

    let new_version = store_milestone_transition(env, contract_id, milestone_index, actor);
    Ok(new_version)
}

/// Validates that the version matches the current stored version (optimistic concurrency check).
///
/// This detects if another transaction has modified the milestone between when the caller
/// read it and now. If the versions don't match, returns an error (InvalidStatusTransition
/// is repurposed here to indicate a concurrent modification conflict).
///
/// # Arguments
/// * `env` - The contract environment
/// * `contract_id` - The contract ID
/// * `milestone_index` - The milestone index
/// * `expected_version` - The version the caller believes the milestone is at
///
/// # Returns
/// * `Ok(())` if versions match (no concurrent modification)
/// * `Err(InvalidStatusTransition)` if versions don't match (concurrent modification detected)
pub fn check_version_for_concurrency(
    env: &Env,
    contract_id: u32,
    milestone_index: u32,
    expected_version: u32,
) -> Result<(), Error> {
    let metadata = read_milestone_version_and_actor(env, contract_id, milestone_index);
    if metadata.version == expected_version {
        Ok(())
    } else {
        Err(Error::InvalidStatusTransition) // Repurposed to indicate concurrent modification
    }
}

/// Check an expected milestone version and fail with a dedicated stale-version
/// error suitable for contract entrypoints.
pub fn require_expected_version(
    env: &Env,
    contract_id: u32,
    milestone_index: u32,
    expected_version: u32,
) {
    check_version_for_concurrency(env, contract_id, milestone_index, expected_version)
        .unwrap_or_else(|_| env.panic_with_error(Error::StaleMilestoneVersion));
}

// ── Re-exports for convenient use ─────────────────────────────────────────────────

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    // Helper to create a test milestone in Pending state
    fn milestone_pending() -> Milestone {
        Milestone {
            amount: 1000,
            funded_amount: 1000,
            released: false,
            refunded: false,
            work_evidence: None,
            refunded_amount: 0,
            deadline: None,
        }
    }

    // Helper to create a test milestone in Released state
    fn milestone_released() -> Milestone {
        Milestone {
            amount: 1000,
            funded_amount: 1000,
            released: true,
            refunded: false,
            work_evidence: None,
            refunded_amount: 0,
            deadline: None,
        }
    }

    // Helper to create a test milestone in Refunded state
    fn milestone_refunded() -> Milestone {
        Milestone {
            amount: 1000,
            funded_amount: 1000,
            released: false,
            refunded: true,
            work_evidence: None,
            refunded_amount: 1000,
            deadline: None,
        }
    }

    #[test]
    fn test_milestone_state_from_pending() {
        let milestone = milestone_pending();
        let state = MilestoneState::from_milestone(&milestone).unwrap();
        assert_eq!(state, MilestoneState::Pending);
    }

    #[test]
    fn test_milestone_state_from_released() {
        let milestone = milestone_released();
        let state = MilestoneState::from_milestone(&milestone).unwrap();
        assert_eq!(state, MilestoneState::Released);
    }

    #[test]
    fn test_milestone_state_from_refunded() {
        let milestone = milestone_refunded();
        let state = MilestoneState::from_milestone(&milestone).unwrap();
        assert_eq!(state, MilestoneState::Refunded);
    }

    #[test]
    fn test_milestone_state_invalid_both_flags_set() {
        let mut milestone = milestone_pending();
        milestone.released = true;
        milestone.refunded = true;
        let result = MilestoneState::from_milestone(&milestone);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), Error::InvalidState);
    }

    #[test]
    fn test_milestone_state_to_flags_pending() {
        let flags = MilestoneState::Pending.to_flags();
        assert_eq!(flags, (false, false));
    }

    #[test]
    fn test_milestone_state_to_flags_released() {
        let flags = MilestoneState::Released.to_flags();
        assert_eq!(flags, (true, false));
    }

    #[test]
    fn test_milestone_state_to_flags_refunded() {
        let flags = MilestoneState::Refunded.to_flags();
        assert_eq!(flags, (false, true));
    }

    // ── Transition Matrix Tests ──────────────────────────────────────────────

    #[test]
    fn test_transition_pending_to_released_valid() {
        let result =
            validate_milestone_transition(MilestoneState::Pending, MilestoneState::Released);
        assert!(result.is_ok());
    }

    #[test]
    fn test_transition_pending_to_refunded_valid() {
        let result =
            validate_milestone_transition(MilestoneState::Pending, MilestoneState::Refunded);
        assert!(result.is_ok());
    }

    #[test]
    fn test_transition_pending_to_pending_idempotent() {
        let result =
            validate_milestone_transition(MilestoneState::Pending, MilestoneState::Pending);
        assert!(result.is_ok());
    }

    #[test]
    fn test_transition_released_to_released_idempotent() {
        let result =
            validate_milestone_transition(MilestoneState::Released, MilestoneState::Released);
        assert!(result.is_ok());
    }

    #[test]
    fn test_transition_refunded_to_refunded_idempotent() {
        let result =
            validate_milestone_transition(MilestoneState::Refunded, MilestoneState::Refunded);
        assert!(result.is_ok());
    }

    #[test]
    fn test_transition_released_to_refunded_invalid() {
        let result =
            validate_milestone_transition(MilestoneState::Released, MilestoneState::Refunded);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), Error::InvalidStatusTransition);
    }

    #[test]
    fn test_transition_released_to_pending_invalid() {
        let result =
            validate_milestone_transition(MilestoneState::Released, MilestoneState::Pending);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), Error::InvalidStatusTransition);
    }

    #[test]
    fn test_transition_refunded_to_released_invalid() {
        let result =
            validate_milestone_transition(MilestoneState::Refunded, MilestoneState::Released);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), Error::InvalidStatusTransition);
    }

    #[test]
    fn test_transition_refunded_to_pending_invalid() {
        let result =
            validate_milestone_transition(MilestoneState::Refunded, MilestoneState::Pending);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), Error::InvalidStatusTransition);
    }

    // ── Version/Actor Metadata Tests ─────────────────────────────────────────

    #[test]
    fn test_read_milestone_version_and_actor_defaults() {
        let env = Env::default();
        let contract_id = 1u32;
        let milestone_index = 0u32;

        let metadata = read_milestone_version_and_actor(&env, contract_id, milestone_index);
        assert_eq!(metadata.version, 0); // Default version
    }

    #[test]
    fn test_store_and_read_milestone_transition() {
        let env = Env::default();
        let contract_id = 1u32;
        let milestone_index = 0u32;
        let actor = Address::generate(&env);

        let new_version =
            store_milestone_transition(&env, contract_id, milestone_index, actor.clone());
        assert_eq!(new_version, 1);

        let metadata = read_milestone_version_and_actor(&env, contract_id, milestone_index);
        assert_eq!(metadata.version, 1);
        assert_eq!(metadata.last_modified_by, actor);
    }

    #[test]
    fn test_store_milestone_transition_increments_version() {
        let env = Env::default();
        let contract_id = 1u32;
        let milestone_index = 0u32;
        let actor1 = Address::generate(&env);
        let actor2 = Address::generate(&env);

        let v1 = store_milestone_transition(&env, contract_id, milestone_index, actor1);
        assert_eq!(v1, 1);

        let v2 = store_milestone_transition(&env, contract_id, milestone_index, actor2.clone());
        assert_eq!(v2, 2);

        let metadata = read_milestone_version_and_actor(&env, contract_id, milestone_index);
        assert_eq!(metadata.version, 2);
        assert_eq!(metadata.last_modified_by, actor2);
    }

    #[test]
    fn test_check_version_for_concurrency_match() {
        let env = Env::default();
        let contract_id = 1u32;
        let milestone_index = 0u32;
        let actor = Address::generate(&env);

        store_milestone_transition(&env, contract_id, milestone_index, actor);

        let result = check_version_for_concurrency(&env, contract_id, milestone_index, 1);
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_version_for_concurrency_mismatch() {
        let env = Env::default();
        let contract_id = 1u32;
        let milestone_index = 0u32;
        let actor = Address::generate(&env);

        store_milestone_transition(&env, contract_id, milestone_index, actor);

        let result = check_version_for_concurrency(&env, contract_id, milestone_index, 0);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), Error::InvalidStatusTransition);
    }

    #[test]
    fn test_check_version_for_concurrency_uninitialized() {
        let env = Env::default();
        let contract_id = 1u32;
        let milestone_index = 0u32;

        let result = check_version_for_concurrency(&env, contract_id, milestone_index, 0);
        assert!(result.is_ok()); // Defaults to version 0
    }

    // ── apply_milestone_transition: concurrency & idempotency tests ──────────

    fn store_milestone(env: &Env, contract_id: u32, index: u32, milestone: &Milestone) {
        env.storage()
            .persistent()
            .set(&milestone_key(contract_id, index), milestone);
    }

    #[test]
    fn test_apply_transition_pending_to_released_succeeds() {
        let env = Env::default();
        let contract_id = 1u32;
        let index = 0u32;
        let actor = Address::generate(&env);
        store_milestone(&env, contract_id, index, &milestone_pending());

        let new_version = apply_milestone_transition(
            &env,
            contract_id,
            index,
            0,
            MilestoneState::Released,
            actor,
        )
        .unwrap();
        assert_eq!(new_version, 1);

        let stored: Milestone = env
            .storage()
            .persistent()
            .get(&milestone_key(contract_id, index))
            .unwrap();
        assert!(stored.released);
        assert!(!stored.refunded);
    }

    #[test]
    fn test_apply_transition_pending_to_refunded_succeeds() {
        let env = Env::default();
        let contract_id = 1u32;
        let index = 0u32;
        let actor = Address::generate(&env);
        store_milestone(&env, contract_id, index, &milestone_pending());

        let new_version = apply_milestone_transition(
            &env,
            contract_id,
            index,
            0,
            MilestoneState::Refunded,
            actor,
        )
        .unwrap();
        assert_eq!(new_version, 1);

        let stored: Milestone = env
            .storage()
            .persistent()
            .get(&milestone_key(contract_id, index))
            .unwrap();
        assert!(!stored.released);
        assert!(stored.refunded);
    }

    #[test]
    fn test_apply_transition_rejects_stale_version() {
        let env = Env::default();
        let contract_id = 1u32;
        let index = 0u32;
        let actor = Address::generate(&env);
        store_milestone(&env, contract_id, index, &milestone_pending());

        // First transition bumps version to 1.
        apply_milestone_transition(
            &env,
            contract_id,
            index,
            0,
            MilestoneState::Released,
            actor.clone(),
        )
        .unwrap();

        // A racing caller that still believes version 0 must be rejected.
        let err = apply_milestone_transition(
            &env,
            contract_id,
            index,
            0,
            MilestoneState::Refunded,
            actor,
        )
        .unwrap_err();
        assert_eq!(err, Error::InvalidStatusTransition);

        // State must remain Released (no partial mutation).
        let stored: Milestone = env
            .storage()
            .persistent()
            .get(&milestone_key(contract_id, index))
            .unwrap();
        assert!(stored.released);
        assert!(!stored.refunded);
    }

    #[test]
    fn test_apply_transition_rejects_illegal_state_change() {
        let env = Env::default();
        let contract_id = 1u32;
        let index = 0u32;
        let actor = Address::generate(&env);
        store_milestone(&env, contract_id, index, &milestone_released());

        // Released -> Refunded is illegal even with a matching version.
        let err = apply_milestone_transition(
            &env,
            contract_id,
            index,
            0,
            MilestoneState::Refunded,
            actor,
        )
        .unwrap_err();
        assert_eq!(err, Error::InvalidStatusTransition);
    }

    #[test]
    fn test_apply_transition_rejects_missing_milestone() {
        let env = Env::default();
        let contract_id = 1u32;
        let index = 7u32;
        let actor = Address::generate(&env);

        let err = apply_milestone_transition(
            &env,
            contract_id,
            index,
            0,
            MilestoneState::Released,
            actor,
        )
        .unwrap_err();
        assert_eq!(err, Error::MilestoneNotFound);
    }

    #[test]
    fn test_apply_transition_rejects_invalid_stored_state() {
        let env = Env::default();
        let contract_id = 1u32;
        let index = 0u32;
        let actor = Address::generate(&env);
        let mut bad = milestone_pending();
        bad.released = true;
        bad.refunded = true;
        store_milestone(&env, contract_id, index, &bad);

        let err = apply_milestone_transition(
            &env,
            contract_id,
            index,
            0,
            MilestoneState::Released,
            actor,
        )
        .unwrap_err();
        assert_eq!(err, Error::InvalidState);
    }

    #[test]
    fn test_apply_transition_idempotent_retry_is_rejected() {
        let env = Env::default();
        let contract_id = 1u32;
        let index = 0u32;
        let actor = Address::generate(&env);
        store_milestone(&env, contract_id, index, &milestone_pending());

        apply_milestone_transition(
            &env,
            contract_id,
            index,
            0,
            MilestoneState::Released,
            actor.clone(),
        )
        .unwrap();

        // Retrying the same logical operation with the same observed version
        // must fail (no double release) rather than silently no-op.
        let err = apply_milestone_transition(
            &env,
            contract_id,
            index,
            0,
            MilestoneState::Released,
            actor,
        )
        .unwrap_err();
        assert_eq!(err, Error::InvalidStatusTransition);
    }

    #[test]
    fn test_apply_transition_sequential_calls_advance_version() {
        let env = Env::default();
        let contract_id = 1u32;
        let index = 0u32;
        let actor = Address::generate(&env);
        store_milestone(&env, contract_id, index, &milestone_pending());

        // First call: Pending -> Released at version 0.
        let v1 = apply_milestone_transition(
            &env,
            contract_id,
            index,
            0,
            MilestoneState::Released,
            actor.clone(),
        )
        .unwrap();
        assert_eq!(v1, 1);

        // Second call: idempotent Released -> Released at version 1.
        let v2 = apply_milestone_transition(
            &env,
            contract_id,
            index,
            1,
            MilestoneState::Released,
            actor,
        )
        .unwrap();
        assert_eq!(v2, 2);
    }
}
