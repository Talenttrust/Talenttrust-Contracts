use soroban_sdk::Env;

/// Returns the current ledger timestamp in seconds.
///
/// This is the single source of truth for all time-related operations in the contract.
/// Using this helper ensures:
/// - Consistent time handling across all modules
/// - Deterministic behavior in production
/// - Reliable testing with mocked ledger time
///
/// # Arguments
/// * `env` - The contract environment providing access to the ledger
///
/// # Returns
/// The current ledger timestamp as a `u64` representing seconds since Unix epoch
///
/// # Example
/// ```ignore
/// use crate::utils::now_seconds;
///
/// pub fn check_timeout(env: &Env, deadline: u64) -> bool {
///     now_seconds(env) > deadline
/// }
/// ```
///
/// # Testing
/// In tests, use `env.ledger().set()` to control time:
/// ```ignore
/// use soroban_sdk::testutils::Ledger;
///
/// env.ledger().set(LedgerInfo {
///     timestamp: 1234567890,
///     ..Default::default()
/// });
/// ```
pub fn now_seconds(env: &Env) -> u64 {
    env.ledger().timestamp()
}
