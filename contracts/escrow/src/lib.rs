#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype,
    crypto::Hash,
    Address, Bytes, Env, Symbol, Vec,
};

// ── Status ────────────────────────────────────────────────────────────────────

/// Overall lifecycle state of an escrow contract.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractStatus {
    Created   = 0,
    Funded    = 1,
    Completed = 2,
    Disputed  = 3,
}

// ── Milestone ─────────────────────────────────────────────────────────────────

/// A single payment milestone inside an escrow.
#[contracttype]
#[derive(Clone, Debug)]
pub struct Milestone {
    pub amount:   i128,
    pub released: bool,
}

// ── Approval action types ─────────────────────────────────────────────────────

/// Distinguishes what a signature is authorising.
///
/// - `MilestoneAcceptance` – client signs off that a deliverable was received.
/// - `DisputeAction`       – either party signs a dispute-resolution action.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApprovalAction {
    MilestoneAcceptance = 0,
    DisputeAction       = 1,
}

// ── Approval request (the message that gets signed) ───────────────────────────

/// Canonical struct that both parties hash-and-sign.
///
/// Encoding it as a `contracttype` means Soroban serialises it
/// deterministically, so both sides always hash the same bytes.
#[contracttype]
#[derive(Clone, Debug)]
pub struct ApprovalRequest {
    /// Which escrow this approval belongs to.
    pub contract_id:  u32,
    /// Which milestone (or dispute round) is being approved.
    pub milestone_id: u32,
    /// What kind of action is being authorised.
    pub action:       ApprovalAction,
    /// Replay-protection: the ledger sequence number at signing time.
    pub nonce:        u64,
}

// ── Signature record stored on-chain ─────────────────────────────────────────

/// An approval that has been verified and persisted.
#[contracttype]
#[derive(Clone, Debug)]
pub struct SignedApproval {
    pub request:   ApprovalRequest,
    /// The signer's address (used for access-control checks).
    pub signer:    Address,
    /// Raw 64-byte Ed25519 signature over the SHA-256 digest of `request`.
    pub signature: Bytes,
    /// Ledger sequence at which the approval was recorded.
    pub timestamp: u32,
}

// ── Storage keys ──────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug)]
pub enum DataKey {
    /// Approval indexed by (contract_id, milestone_id, action).
    Approval(u32, u32, ApprovalAction),
    /// Simple nonce counter per signer address to prevent replay.
    Nonce(Address),
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct Escrow;

#[contractimpl]
impl Escrow {

    // ── Original entry-points (unchanged behaviour) ───────────────────────────

    /// Create a new escrow contract.
    pub fn create_contract(
        _env: Env,
        _client: Address,
        _freelancer: Address,
        _milestone_amounts: Vec<i128>,
    ) -> u32 {
        1
    }

    /// Deposit funds into escrow.
    pub fn deposit_funds(_env: Env, _contract_id: u32, _amount: i128) -> bool {
        true
    }

    /// Release a milestone payment to the freelancer after verification.
    pub fn release_milestone(_env: Env, _contract_id: u32, _milestone_id: u32) -> bool {
        true
    }

    /// Issue a reputation credential for the freelancer.
    pub fn issue_reputation(_env: Env, _freelancer: Address, _rating: i128) -> bool {
        true
    }

    /// Hello-world style function for testing and CI.
    pub fn hello(_env: Env, to: Symbol) -> Symbol {
        to
    }

    // ── Signature-based approval entry-points ─────────────────────────────────

    /// Submit a signed approval for milestone acceptance or a dispute action.
    ///
    /// # Security properties
    /// * `signer.require_auth()` — Soroban enforces that the transaction was
    ///   authorised by the declared signer; no spoofing is possible.
    /// * The `nonce` inside `request` must equal the signer's on-chain nonce,
    ///   preventing replay of old signatures.
    /// * The `signature` bytes are verified against the SHA-256 digest of the
    ///   XDR-encoded `ApprovalRequest`, binding the sig to exact parameters.
    ///
    /// # Errors
    /// Panics (traps) on:
    /// - nonce mismatch
    /// - signature verification failure
    /// - duplicate approval for the same (contract, milestone, action) triple
    pub fn submit_approval(
        env:       Env,
        request:   ApprovalRequest,
        signer:    Address,
        signature: Bytes,
    ) -> bool {
        // 1. Soroban auth: transaction must be signed by `signer`.
        signer.require_auth();

        // 2. Replay protection — check and increment nonce.
        let nonce_key = DataKey::Nonce(signer.clone());
        let expected_nonce: u64 = env
            .storage()
            .instance()
            .get(&nonce_key)
            .unwrap_or(0u64);
        assert!(
            request.nonce == expected_nonce,
            "invalid nonce: replay or out-of-order submission"
        );
        env.storage()
            .instance()
            .set(&nonce_key, &(expected_nonce + 1));

        // 3. Duplicate-approval guard.
        let approval_key = DataKey::Approval(
            request.contract_id,
            request.milestone_id,
            request.action,
        );
        assert!(
            !env.storage().instance().has(&approval_key),
            "approval already recorded for this milestone/action"
        );

        // 4. Cryptographic verification.
        //    Hash the canonical XDR encoding of the request, then verify the
        //    Ed25519 signature supplied by the caller.
        let digest: Hash<32> = env.crypto().sha256(
            &env.to_xdr(&request),
        );
        env.crypto()
            .ed25519_verify(&signer.to_string().into_val(&env), &digest.into(), &signature);

        // 5. Persist the verified approval.
        let record = SignedApproval {
            request:   request.clone(),
            signer:    signer.clone(),
            signature: signature.clone(),
            timestamp: env.ledger().sequence(),
        };
        env.storage().instance().set(&approval_key, &record);

        // 6. Emit an event so off-chain indexers can track approvals.
        env.events().publish(
            (Symbol::new(&env, "approval"), signer),
            (request.contract_id, request.milestone_id, request.action),
        );

        true
    }

    /// Retrieve a previously recorded approval (read-only).
    ///
    /// Returns `None` if no approval exists for the given triple.
    pub fn get_approval(
        env:          Env,
        contract_id:  u32,
        milestone_id: u32,
        action:       ApprovalAction,
    ) -> Option<SignedApproval> {
        let key = DataKey::Approval(contract_id, milestone_id, action);
        env.storage().instance().get(&key)
    }

    /// Return the current (next expected) nonce for a signer address.
    pub fn get_nonce(env: Env, signer: Address) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::Nonce(signer))
            .unwrap_or(0u64)
    }
}

#[cfg(test)]
mod test;