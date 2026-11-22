//! Governance, pause/freeze controls, roles and administrative behaviour.

use super::*;

#[test]
fn pause_blocks_mutations() {
    let s = setup();
    s.escrow.set_paused(&s.admin, &true);
    assert_eq!(
        s.escrow.try_create_contract(
            &s.client,
            &s.freelancer,
            &s.token.address,
            &vec![&s.env, U]
        ),
        Err(Ok(EscrowError::Paused))
    );
}

#[test]
fn bonus_blocked_when_paused() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.set_paused(&s.admin, &true);
    assert_eq!(
        s.escrow.try_pay_bonus(&id, &(10 * U)),
        Err(Ok(EscrowError::Paused))
    );
}

#[test]
fn test_per_escrow_freeze_isolation_and_unfreeze() {
    let s = setup();

    // Create two separate escrows
    let id_a = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U, 200 * U],
    );
    let id_b = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U, 200 * U],
    );

    // Freeze escrow A
    let reason = symbol_short!("fraud");
    s.escrow.freeze_escrow(&id_a, &reason);
    assert!(s.escrow.is_frozen(&id_a));
    assert!(!s.escrow.is_frozen(&id_b));

    // Record initial token balances
    let client_bal_before = s.token.balance(&s.client);
    let freelancer_bal_before = s.token.balance(&s.freelancer);
    let contract_bal_before = s.token.balance(&s.escrow.address);

    // All listed mutations on A must return EscrowFrozen and leave balances unchanged
    assert_eq!(
        s.escrow.try_deposit_funds(&id_a, &(300 * U)),
        Err(Ok(EscrowError::EscrowFrozen))
    );
    assert_eq!(
        s.escrow.try_release_milestone(&id_a, &0),
        Err(Ok(EscrowError::EscrowFrozen))
    );
    assert_eq!(
        s.escrow.try_open_dispute(&id_a, &s.client),
        Err(Ok(EscrowError::EscrowFrozen))
    );
    assert_eq!(
        s.escrow.try_cancel_contract(&id_a),
        Err(Ok(EscrowError::EscrowFrozen))
    );

    // Verify token balances are unchanged
    assert_eq!(s.token.balance(&s.client), client_bal_before);
    assert_eq!(s.token.balance(&s.freelancer), freelancer_bal_before);
    assert_eq!(s.token.balance(&s.escrow.address), contract_bal_before);

    // Escrow B operations succeed normally
    assert_eq!(s.escrow.try_deposit_funds(&id_b, &(300 * U)), Ok(Ok(())));
    assert_eq!(s.escrow.try_release_milestone(&id_b, &0), Ok(Ok(())));
    assert_eq!(s.token.balance(&s.freelancer), freelancer_bal_before + 100 * U);

    // Unfreeze Escrow A and verify normal operation is restored
    s.escrow.unfreeze_escrow(&id_a);
    assert!(!s.escrow.is_frozen(&id_a));
    assert_eq!(s.escrow.try_deposit_funds(&id_a, &(300 * U)), Ok(Ok(())));
}

#[test]
fn test_resolve_dispute_allowed_on_frozen_escrow() {
    let s = setup();

    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 400 * U],
    );
    s.escrow.deposit_funds(&id, &(400 * U));

    // Open dispute before freezing
    open_dispute_bonded(&s, &id, &s.freelancer);

    // Admin freezes escrow
    let reason = symbol_short!("suspect");
    s.escrow.freeze_escrow(&id, &reason);
    assert!(s.escrow.is_frozen(&id));

    // Admin resolves dispute on frozen escrow (must succeed)
    resolve_refunding_bond(&s, &id, &2_500);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Resolved);
    assert_eq!(s.token.balance(&s.freelancer), 100 * U);
    assert_eq!(s.token.balance(&s.client), 900 * U);
}

#[test]
fn test_freeze_error_conditions() {
    let s = setup();

    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );

    // ContractNotFound for unknown IDs
    assert_eq!(
        s.escrow.try_freeze_escrow(&999, &symbol_short!("none")),
        Err(Ok(EscrowError::ContractNotFound))
    );
    assert_eq!(
        s.escrow.try_unfreeze_escrow(&999),
        Err(Ok(EscrowError::ContractNotFound))
    );

    // Unfreezing non-frozen escrow returns InvalidStatus
    assert_eq!(
        s.escrow.try_unfreeze_escrow(&id),
        Err(Ok(EscrowError::InvalidStatus))
    );

    // Freeze valid escrow
    s.escrow.freeze_escrow(&id, &symbol_short!("reason"));

    // Freezing an already-frozen escrow returns EscrowFrozen
    assert_eq!(
        s.escrow.try_freeze_escrow(&id, &symbol_short!("again")),
        Err(Ok(EscrowError::EscrowFrozen))
    );
}

#[test]
fn test_non_admin_freeze_rejected() {
    let s = setup();
    let contract_id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );

    // No authorization mocked: the admin has not signed, so the freeze must fail.
    s.env.mock_auths(&[]);
    let res = s
        .escrow
        .try_freeze_escrow(&contract_id, &symbol_short!("fraud"));
    assert!(res.is_err());
    s.env.mock_all_auths();
    assert!(!s.escrow.is_frozen(&contract_id));
}

#[test]
fn test_freeze_blocks_bonus_and_payout_change() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.freeze_escrow(&id, &symbol_short!("fraud"));
    assert_eq!(
        s.escrow.try_pay_bonus(&id, &(10 * U)),
        Err(Ok(EscrowError::EscrowFrozen))
    );
    let wallet = Address::generate(&s.env);
    assert_eq!(
        s.escrow.try_set_payout_address(&id, &wallet),
        Err(Ok(EscrowError::EscrowFrozen))
    );
}

#[test]
fn team_split_rejected_on_frozen_escrow() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.freeze_escrow(&id, &symbol_short!("fraud"));
    let valid = vec![&s.env, (s.freelancer.clone(), 10_000u32)];
    assert_eq!(
        s.escrow.try_set_team_split(&id, &valid),
        Err(Ok(EscrowError::EscrowFrozen))
    );
}

#[test]
fn test_rate_client_pause_gate() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.release_milestone(&id, &0);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);

    // Pause contract
    s.escrow.set_paused(&s.admin, &true);
    assert_eq!(
        s.escrow.try_rate_client(&id, &5),
        Err(Ok(EscrowError::Paused))
    );

    // Unpause contract and verify rating succeeds
    s.escrow.set_paused(&s.admin, &false);
    assert_eq!(s.escrow.try_rate_client(&id, &5), Ok(Ok(())));
    let rep = s.escrow.get_client_reputation(&s.client);
    assert_eq!(rep.completed_contracts, 1);
    assert_eq!(rep.total_rating, 5);
}

#[test]
fn pauser_can_pause_but_cannot_resolve_disputes() {
    let s = setup();
    let pauser = Address::generate(&s.env);
    s.escrow.grant_role(&Role::Pauser, &pauser);
    assert!(s.escrow.has_role(&Role::Pauser, &pauser));

    s.escrow
        .mock_auths(&[MockAuth {
            address: &pauser,
            invoke: &MockAuthInvoke {
                contract: &s.escrow.address,
                fn_name: "set_paused",
                args: (&pauser, true).into_val(&s.env),
                sub_invokes: &[],
            },
        }])
        .set_paused(&pauser, &true);
    assert!(s.escrow.is_paused());

    s.escrow
        .mock_auths(&[MockAuth {
            address: &pauser,
            invoke: &MockAuthInvoke {
                contract: &s.escrow.address,
                fn_name: "set_paused",
                args: (&pauser, false).into_val(&s.env),
                sub_invokes: &[],
            },
        }])
        .set_paused(&pauser, &false);
    assert!(!s.escrow.is_paused());

    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    open_dispute_bonded(&s, &id, &s.freelancer);

    let result = s
        .escrow
        .mock_auths(&[MockAuth {
            address: &pauser,
            invoke: &MockAuthInvoke {
                contract: &s.escrow.address,
                fn_name: "resolve_dispute",
                args: (&pauser, id, 5_000u32, true).into_val(&s.env),
                sub_invokes: &[],
            },
        }])
        .try_resolve_dispute(&pauser, &id, &5_000, &true);
    assert_eq!(result, Err(Ok(EscrowError::Unauthorized)));
}

#[test]
fn resolver_can_resolve_but_cannot_pause() {
    let s = setup();
    let resolver = Address::generate(&s.env);
    s.escrow.grant_role(&Role::Resolver, &resolver);
    assert!(s.escrow.has_role(&Role::Resolver, &resolver));

    let pause_result = s
        .escrow
        .mock_auths(&[MockAuth {
            address: &resolver,
            invoke: &MockAuthInvoke {
                contract: &s.escrow.address,
                fn_name: "set_paused",
                args: (&resolver, true).into_val(&s.env),
                sub_invokes: &[],
            },
        }])
        .try_set_paused(&resolver, &true);
    assert_eq!(pause_result, Err(Ok(EscrowError::Unauthorized)));

    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    open_dispute_bonded(&s, &id, &s.freelancer);

    s.escrow
        .mock_auths(&[MockAuth {
            address: &resolver,
            invoke: &MockAuthInvoke {
                contract: &s.escrow.address,
                fn_name: "resolve_dispute",
                args: (&resolver, id, 10_000u32, true).into_val(&s.env),
                sub_invokes: &[],
            },
        }])
        .resolve_dispute(&resolver, &id, &10_000, &true);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Resolved);
    // Full split to the freelancer plus the opener's refunded dispute bond.
    assert_eq!(
        s.token.balance(&s.freelancer),
        100 * U + dispute_bond(100 * U)
    );
}

#[test]
fn revoking_pauser_role_takes_effect_immediately() {
    let s = setup();
    let pauser = Address::generate(&s.env);
    s.escrow.grant_role(&Role::Pauser, &pauser);
    s.escrow.revoke_role(&Role::Pauser, &pauser);
    assert!(!s.escrow.has_role(&Role::Pauser, &pauser));

    let result = s
        .escrow
        .mock_auths(&[MockAuth {
            address: &pauser,
            invoke: &MockAuthInvoke {
                contract: &s.escrow.address,
                fn_name: "set_paused",
                args: (&pauser, true).into_val(&s.env),
                sub_invokes: &[],
            },
        }])
        .try_set_paused(&pauser, &true);
    assert_eq!(result, Err(Ok(EscrowError::Unauthorized)));
}

#[test]
fn resolver_who_is_a_party_cannot_resolve_that_escrow() {
    let s = setup();
    s.escrow.grant_role(&Role::Resolver, &s.client);

    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    open_dispute_bonded(&s, &id, &s.freelancer);

    let result = s
        .escrow
        .mock_auths(&[MockAuth {
            address: &s.client,
            invoke: &MockAuthInvoke {
                contract: &s.escrow.address,
                fn_name: "resolve_dispute",
                args: (&s.client, id, 5_000u32, true).into_val(&s.env),
                sub_invokes: &[],
            },
        }])
        .try_resolve_dispute(&s.client, &id, &5_000, &true);
    assert_eq!(result, Err(Ok(EscrowError::Unauthorized)));
}

#[test]
fn grant_and_revoke_role_unchanged_and_admin_only() {
    let s = setup();
    let account = Address::generate(&s.env);
    assert!(s.escrow.has_role(&Role::Pauser, &s.admin));
    assert!(s.escrow.has_role(&Role::Resolver, &s.admin));
    assert!(!s.escrow.has_role(&Role::Pauser, &account));
    assert_eq!(
        s.escrow.try_revoke_role(&Role::Pauser, &account),
        Err(Ok(EscrowError::RoleUnchanged))
    );
    s.escrow.grant_role(&Role::Pauser, &account);
    assert_eq!(
        s.escrow.try_grant_role(&Role::Pauser, &account),
        Err(Ok(EscrowError::RoleUnchanged))
    );
    // Roles are independent
    assert!(!s.escrow.has_role(&Role::Resolver, &account));
    // A role holder cannot grant roles: without the admin's signature it fails
    s.env.mock_auths(&[]);
    assert!(s
        .escrow
        .try_grant_role(&Role::Resolver, &account)
        .is_err());
    s.env.mock_all_auths();
    assert!(!s.escrow.has_role(&Role::Resolver, &account));
}

#[test]
fn deposit_from_respects_freeze_and_status() {
    let s = setup();
    let sponsor = Address::generate(&s.env);
    StellarAssetClient::new(&s.env, &s.token.address).mint(&sponsor, &(100 * U));
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50 * U],
    );
    s.escrow.freeze_escrow(&id, &symbol_short!("fraud"));
    assert_eq!(
        s.escrow.try_deposit_from(&id, &sponsor, &(10 * U)),
        Err(Ok(EscrowError::EscrowFrozen))
    );
    s.escrow.unfreeze_escrow(&id);
    s.escrow.deposit_from(&id, &sponsor, &(50 * U));
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Funded);
    assert_eq!(
        s.escrow.try_deposit_from(&id, &sponsor, &U),
        Err(Ok(EscrowError::InvalidStatus))
    );
    assert_eq!(s.escrow.get_liability(&s.token.address), 50 * U);
}

#[test]
fn test_expire_unfunded_pause_gate_and_status_checks() {
    let s = setup();
    set_now(&s.env, 1_000_000);

    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    let fund_by = s.escrow.get_fund_by(&id);
    set_now(&s.env, fund_by + 1);

    // Pause blocks expire_unfunded
    s.escrow.set_paused(&s.admin, &true);
    assert_eq!(
        s.escrow.try_expire_unfunded(&id),
        Err(Ok(EscrowError::Paused))
    );

    // Unpause allows expire_unfunded
    s.escrow.set_paused(&s.admin, &false);
    assert_eq!(s.escrow.try_expire_unfunded(&id), Ok(Ok(())));

    // Re-expiring an already cancelled escrow fails with InvalidStatus
    assert_eq!(
        s.escrow.try_expire_unfunded(&id),
        Err(Ok(EscrowError::InvalidStatus))
    );

    // Depositing into cancelled escrow fails with InvalidStatus
    assert_eq!(
        s.escrow.try_deposit_funds(&id, &(50 * U)),
        Err(Ok(EscrowError::InvalidStatus))
    );

    // Missing storage for get_fund_by returns 0 default
    assert_eq!(s.escrow.get_fund_by(&999), 0);
}

#[test]
fn init_and_paused_events_are_versioned() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let id = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &id);
    escrow.initialize(&admin);
    let ev = env.events().all().last().unwrap();
    assert_eq!(ev.0, id);
    assert_eq!(ev.1.len(), 2);
    assert_eq!(u32::try_from_val(&env, &ev.1.get(0).unwrap()).unwrap(), EVENT_SCHEMA_V);
    assert_eq!(
        Symbol::try_from_val(&env, &ev.1.get(1).unwrap()).unwrap(),
        symbol_short!("init")
    );
    assert_eq!(Address::try_from_val(&env, &ev.2).unwrap(), admin);

    escrow.set_paused(&admin, &true);
    let ev = env.events().all().last().unwrap();
    assert_eq!(u32::try_from_val(&env, &ev.1.get(0).unwrap()).unwrap(), EVENT_SCHEMA_V);
    assert_eq!(
        Symbol::try_from_val(&env, &ev.1.get(1).unwrap()).unwrap(),
        symbol_short!("paused")
    );
    assert!(bool::try_from_val(&env, &ev.2).unwrap());
}

#[test]
fn upgrade_schedule_boundary_and_execute() {
    let s = setup();
    assert_eq!(s.escrow.version(), 1);
    // Existing escrow data must survive the upgrade.
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    let before = s.escrow.get_contract(&id);

    let hash = s
        .env
        .deployer()
        .upload_contract_wasm(soroban_sdk::Bytes::from_slice(&s.env, &minimal_contract_wasm()));
    set_time(&s.env, 1_000);
    s.escrow.schedule_upgrade(&hash);
    let (topics, data) = last_global_event(&s);
    assert_eq!(
        Symbol::try_from_val(&s.env, &topics.get(0).unwrap()).unwrap(),
        symbol_short!("upg_sched")
    );
    let eta = 1_000 + UPGRADE_DELAY_SECS;
    assert_eq!(
        <(BytesN<32>, u64)>::try_from_val(&s.env, &data).unwrap(),
        (hash.clone(), eta)
    );
    assert_eq!(
        s.escrow.get_pending_upgrade(),
        Some(PendingUpgrade {
            wasm_hash: hash.clone(),
            eta,
        })
    );

    set_time(&s.env, eta - 1);
    assert_eq!(
        s.escrow.try_execute_upgrade(),
        Err(Ok(EscrowError::UpgradeNotReady))
    );

    set_time(&s.env, eta);
    s.escrow.execute_upgrade();
    let (topics, data) = last_global_event(&s);
    assert_eq!(
        Symbol::try_from_val(&s.env, &topics.get(0).unwrap()).unwrap(),
        symbol_short!("upgraded")
    );
    assert_eq!(
        <(BytesN<32>, u32)>::try_from_val(&s.env, &data).unwrap(),
        (hash, 2)
    );

    // The instance now runs the new code; storage is untouched and readable.
    let (stored, version, pending): (Option<EscrowContract>, Option<u32>, bool) =
        s.env.as_contract(&s.escrow.address, || {
            (
                s.env
                    .storage()
                    .persistent()
                    .get::<_, StoredEscrow>(&DataKey::ContractV(id))
                    .map(upgrade_to_latest),
                s.env.storage().instance().get(&DataKey::Version),
                s.env.storage().instance().has(&DataKey::PendingUpgrade),
            )
        });
    assert_eq!(stored, Some(before));
    assert_eq!(version, Some(2));
    assert!(!pending);
}

#[test]
fn cancel_upgrade_then_execute_fails() {
    let s = setup();
    assert_eq!(
        s.escrow.try_cancel_upgrade(),
        Err(Ok(EscrowError::NoPendingUpgrade))
    );
    assert_eq!(
        s.escrow.try_execute_upgrade(),
        Err(Ok(EscrowError::NoPendingUpgrade))
    );
    let hash = BytesN::from_array(&s.env, &[7u8; 32]);
    s.escrow.schedule_upgrade(&hash);
    s.escrow.cancel_upgrade();
    let (topics, data) = last_global_event(&s);
    assert_eq!(
        Symbol::try_from_val(&s.env, &topics.get(0).unwrap()).unwrap(),
        symbol_short!("upg_cncl")
    );
    assert_eq!(BytesN::<32>::try_from_val(&s.env, &data).unwrap(), hash);
    assert_eq!(s.escrow.get_pending_upgrade(), None);
    set_time(&s.env, UPGRADE_DELAY_SECS * 2);
    assert_eq!(
        s.escrow.try_execute_upgrade(),
        Err(Ok(EscrowError::NoPendingUpgrade))
    );
    assert_eq!(s.escrow.version(), 1);
}

#[test]
fn non_admin_cannot_schedule_cancel_or_execute_upgrade() {
    let s = setup();
    let hash = BytesN::from_array(&s.env, &[9u8; 32]);
    s.env.mock_auths(&[]);
    assert!(s.escrow.try_schedule_upgrade(&hash).is_err());
    s.env.mock_all_auths();
    s.escrow.schedule_upgrade(&hash);
    set_time(&s.env, UPGRADE_DELAY_SECS * 2);
    s.env.mock_auths(&[]);
    assert!(s.escrow.try_cancel_upgrade().is_err());
    assert!(s.escrow.try_execute_upgrade().is_err());
    s.env.mock_all_auths();
    assert!(s.escrow.get_pending_upgrade().is_some());
    assert_eq!(s.escrow.version(), 1);
}

#[test]
fn propose_then_accept_transfers_admin() {
    let (env, escrow, old_admin) = setup_admin();
    let new_admin = Address::generate(&env);

    // No pending admin initially.
    assert_eq!(escrow.get_pending_admin(), None);

    escrow.propose_admin(&new_admin);
    assert_eq!(escrow.get_pending_admin(), Some(new_admin.clone()));
    // Control has NOT moved yet.
    assert_eq!(escrow.get_admin(), old_admin);

    escrow.accept_admin();
    assert_eq!(escrow.get_admin(), new_admin.clone());
    assert_eq!(escrow.get_pending_admin(), None);

    // New admin can pause; the old admin can no longer.
    escrow.set_paused(&new_admin, &true);
    assert!(escrow.is_paused());

    // Explicitly authorize the OLD admin for set_paused: must now fail.
    env.set_auths(&[]);
    escrow.mock_auths(&[MockAuth {
        address: &old_admin,
        invoke: &MockAuthInvoke {
            contract: &escrow.address,
            fn_name: "set_paused",
            args: (old_admin.clone(), false).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(escrow.try_set_paused(&old_admin, &false).is_err());
}

#[test]
fn propose_rejects_self_and_non_admin() {
    let (env, escrow, admin) = setup_admin();
    // Proposing the current admin is rejected.
    assert_eq!(
        escrow.try_propose_admin(&admin),
        Err(Ok(EscrowError::InvalidAdmin))
    );

    // A non-admin proposer fails auth. Only `stranger` is authorized here.
    let stranger = Address::generate(&env);
    let nominee = Address::generate(&env);
    env.set_auths(&[]);
    escrow.mock_auths(&[MockAuth {
        address: &stranger,
        invoke: &MockAuthInvoke {
            contract: &escrow.address,
            fn_name: "propose_admin",
            args: (nominee.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(escrow.try_propose_admin(&nominee).is_err());
    assert_eq!(escrow.get_pending_admin(), None);
}

#[test]
fn accept_without_pending_fails() {
    let (_env, escrow, _admin) = setup_admin();
    assert_eq!(
        escrow.try_accept_admin(),
        Err(Ok(EscrowError::NoPendingAdmin))
    );
}

#[test]
fn accept_by_non_nominee_fails() {
    let (env, escrow, _admin) = setup_admin();
    let nominee = Address::generate(&env);
    let imposter = Address::generate(&env);
    escrow.propose_admin(&nominee);

    // Only the imposter is authorized: accept must fail auth and leave the
    // nominee pending and the admin unchanged.
    env.set_auths(&[]);
    escrow.mock_auths(&[MockAuth {
        address: &imposter,
        invoke: &MockAuthInvoke {
            contract: &escrow.address,
            fn_name: "accept_admin",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(escrow.try_accept_admin().is_err());
    assert_eq!(escrow.get_pending_admin(), Some(nominee));
}

#[test]
fn cancel_without_pending_fails_and_cancel_clears_pending() {
    let (env, escrow, _admin) = setup_admin();
    assert_eq!(
        escrow.try_cancel_admin_proposal(),
        Err(Ok(EscrowError::NoPendingAdmin))
    );

    let nominee = Address::generate(&env);
    escrow.propose_admin(&nominee);
    escrow.cancel_admin_proposal();
    assert_eq!(escrow.get_pending_admin(), None);
    // Nominee can no longer accept.
    assert_eq!(
        escrow.try_accept_admin(),
        Err(Ok(EscrowError::NoPendingAdmin))
    );
}

#[test]
fn second_proposal_replaces_first_nominee() {
    let (env, escrow, _admin) = setup_admin();
    let first = Address::generate(&env);
    let second = Address::generate(&env);

    escrow.propose_admin(&first);
    escrow.propose_admin(&second);
    assert_eq!(escrow.get_pending_admin(), Some(second.clone()));

    escrow.accept_admin();
    assert_eq!(escrow.get_admin(), second);
}

#[test]
fn admin_transfer_works_while_paused() {
    let (env, escrow, admin) = setup_admin();
    escrow.set_paused(&admin, &true);
    assert!(escrow.is_paused());

    let new_admin = Address::generate(&env);
    escrow.propose_admin(&new_admin);
    escrow.accept_admin();
    assert_eq!(escrow.get_admin(), new_admin);
    assert_eq!(escrow.get_pending_admin(), None);
    // Still paused: admin transfer is a recovery action, not a state change.
    assert!(escrow.is_paused());
}

#[test]
fn admin_transfer_events_have_topic_and_payload() {
    use soroban_sdk::FromVal;
    let (env, escrow, old_admin) = setup_admin();
    let new_admin = Address::generate(&env);

    escrow.propose_admin(&new_admin);
    let prop = env.events().all().last().unwrap();
    assert_eq!(prop.0, escrow.address);
    let prop_topics: Vec<soroban_sdk::Val> = (symbol_short!("adm_prop"),).into_val(&env);
    assert_eq!(prop.1, prop_topics);
    let prop_data: (Address, Address) = FromVal::from_val(&env, &prop.2);
    assert_eq!(prop_data, (old_admin.clone(), new_admin.clone()));

    escrow.accept_admin();
    let acc = env.events().all().last().unwrap();
    let acc_topics: Vec<soroban_sdk::Val> = (symbol_short!("adm_acc"),).into_val(&env);
    assert_eq!(acc.1, acc_topics);
    let acc_data: (Address, Address) = FromVal::from_val(&env, &acc.2);
    assert_eq!(acc_data, (old_admin.clone(), new_admin.clone()));

    escrow.propose_admin(&old_admin);
    escrow.cancel_admin_proposal();
    let cncl = env.events().all().last().unwrap();
    let cncl_topics: Vec<soroban_sdk::Val> = (symbol_short!("adm_cncl"),).into_val(&env);
    assert_eq!(cncl.1, cncl_topics);
    let cncl_data: Address = FromVal::from_val(&env, &cncl.2);
    assert_eq!(cncl_data, old_admin);
}

#[test]
fn emergency_withdraw_repause_resets_clock() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    set_ts(&s.env, 100);
    s.escrow.set_paused(&s.admin, &true);
    s.escrow.set_paused(&s.admin, &false);
    assert_eq!(s.escrow.get_paused_at(), None);
    let second_pause = 100 + EMERGENCY_EXIT_DELAY_SECS;
    set_ts(&s.env, second_pause);
    s.escrow.set_paused(&s.admin, &true);
    // Re-asserting the pause does not move the clock.
    s.escrow.set_paused(&s.admin, &true);
    assert_eq!(s.escrow.get_paused_at(), Some(second_pause));
    set_ts(&s.env, second_pause + EMERGENCY_EXIT_DELAY_SECS - 1);
    assert_eq!(
        s.escrow.try_emergency_withdraw(&id, &s.client),
        Err(Ok(EscrowError::EmergencyExitNotAvailable))
    );
    set_ts(&s.env, second_pause + EMERGENCY_EXIT_DELAY_SECS);
    s.escrow.emergency_withdraw(&id, &s.client);
    assert_eq!(s.token.balance(&s.client), 1_000 * U);
}

#[test]
fn emergency_withdraw_stranger_and_unpaused_rejected() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    // Unpaused: even long after, exit is not available.
    set_ts(&s.env, EMERGENCY_EXIT_DELAY_SECS * 2);
    assert_eq!(
        s.escrow.try_emergency_withdraw(&id, &s.client),
        Err(Ok(EscrowError::EmergencyExitNotAvailable))
    );
    s.escrow.set_paused(&s.admin, &true);
    set_ts(&s.env, EMERGENCY_EXIT_DELAY_SECS * 3);
    let stranger = Address::generate(&s.env);
    assert_eq!(
        s.escrow.try_emergency_withdraw(&id, &stranger),
        Err(Ok(EscrowError::Unauthorized))
    );
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Funded);
}

#[test]
fn paused_contract_blocks_issue_reputation() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.release_milestone(&id, &0);
    s.escrow.set_paused(&s.admin, &true);
    assert_eq!(s.escrow.get_pause_flags(), PAUSE_ALL);
    assert_eq!(
        s.escrow.try_issue_reputation(&id, &5),
        Err(Ok(EscrowError::Paused))
    );
    s.escrow.set_paused(&s.admin, &false);
    assert_eq!(s.escrow.get_pause_flags(), 0);
    s.escrow.issue_reputation(&id, &5);
}

#[test]
fn each_pause_flag_gates_exactly_its_entry_points() {
    let flags = [
        PAUSE_CREATE,
        PAUSE_DEPOSIT,
        PAUSE_RELEASE,
        PAUSE_DISPUTE,
        PAUSE_CANCEL,
        PAUSE_REPUTATION,
    ];
    for flag in flags.iter() {
        let s = setup();
        let f = pause_fixture(&s);
        s.escrow.set_pause_flags(&s.admin, flag);
        assert!(s.escrow.is_paused());
        let paused = |r: bool, op: u32| {
            if *flag == op {
                assert!(r, "flag {} must block its category", flag);
            } else {
                assert!(!r, "flag {} must not block category {}", flag, op);
            }
        };
        let r = s.escrow.try_create_contract(
            &s.client,
            &s.freelancer,
            &s.token.address,
            &vec![&s.env, 10 * U],
        );
        paused(r == Err(Ok(EscrowError::Paused)), PAUSE_CREATE);
        let r = s.escrow.try_deposit_funds(&f.created, &(10 * U));
        paused(r == Err(Ok(EscrowError::Paused)), PAUSE_DEPOSIT);
        let r = s.escrow.try_release_milestone(&f.funded, &0);
        paused(r == Err(Ok(EscrowError::Paused)), PAUSE_RELEASE);
        let r = s.escrow.try_open_dispute(&f.disputable, &s.client);
        paused(r == Err(Ok(EscrowError::Paused)), PAUSE_DISPUTE);
        let r = s.escrow.try_cancel_contract(&f.cancellable);
        paused(r == Err(Ok(EscrowError::Paused)), PAUSE_CANCEL);
        let r = s.escrow.try_issue_reputation(&f.completed, &4);
        paused(r == Err(Ok(EscrowError::Paused)), PAUSE_REPUTATION);
    }
}

#[test]
fn inflow_pause_still_lets_clients_cancel_and_refund() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(60 * U));
    s.escrow
        .set_pause_flags(&s.admin, &(PAUSE_CREATE | PAUSE_DEPOSIT));
    assert_eq!(
        s.escrow.try_deposit_funds(&id, &(40 * U)),
        Err(Ok(EscrowError::Paused))
    );
    assert_eq!(
        s.escrow.try_create_and_fund(
            &s.client,
            &s.freelancer,
            &s.token.address,
            &vec![&s.env, 10 * U]
        ),
        Err(Ok(EscrowError::Paused))
    );
    s.escrow.cancel_contract(&id);
    assert_eq!(s.token.balance(&s.client), 1_000 * U);
    assert_eq!(s.token.balance(&s.escrow.address), 0);
}

#[test]
fn invalid_pause_bits_rejected_and_only_pausers_set_flags() {
    let s = setup();
    assert_eq!(
        s.escrow.try_set_pause_flags(&s.admin, &(PAUSE_ALL + 1)),
        Err(Ok(EscrowError::InvalidAmount))
    );
    let outsider = Address::generate(&s.env);
    assert_eq!(
        s.escrow.try_set_pause_flags(&outsider, &PAUSE_CREATE),
        Err(Ok(EscrowError::Unauthorized))
    );
    let pauser = Address::generate(&s.env);
    s.escrow.grant_role(&Role::Pauser, &pauser);
    s.escrow.set_pause_flags(&pauser, &PAUSE_RELEASE);
    // Events are per invocation: inspect them before the next call.
    let ev = s
        .env
        .events()
        .all()
        .iter()
        .filter(|e| e.0 == s.escrow.address)
        .last()
        .unwrap();
    assert_eq!(
        Symbol::try_from_val(&s.env, &ev.1.get(0).unwrap()).unwrap(),
        symbol_short!("pause")
    );
    assert_eq!(u32::try_from_val(&s.env, &ev.2).unwrap(), PAUSE_RELEASE);
    assert_eq!(s.escrow.get_pause_flags(), PAUSE_RELEASE);
}

#[test]
fn legacy_paused_flag_means_pause_all() {
    let s = setup();
    s.env.as_contract(&s.escrow.address, || {
        s.env.storage().instance().remove(&DataKey::PauseFlags);
        s.env.storage().instance().set(&DataKey::Paused, &true);
    });
    assert!(s.escrow.is_paused());
    assert_eq!(s.escrow.get_pause_flags(), PAUSE_ALL);
    assert_eq!(
        s.escrow.try_create_contract(
            &s.client,
            &s.freelancer,
            &s.token.address,
            &vec![&s.env, 10 * U]
        ),
        Err(Ok(EscrowError::Paused))
    );
}

#[test]
fn fee_snapshot_survives_admin_fee_increase() {
    let s = setup();
    let treasury = Address::generate(&s.env);
    s.escrow.set_fee(&100, &treasury);
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    assert_eq!(s.escrow.get_escrow_fee(&id), 100);
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.set_fee(&300, &treasury);
    assert_eq!(s.escrow.get_fee_config(), (300, Some(treasury.clone())));
    s.escrow.release_milestone(&id, &0);
    let rel: ReleaseEvent = last_escrow_event(&s, "release", id).unwrap();
    assert_eq!(rel.fee, U);
    // 1% of 100 units, not 3%.
    assert_eq!(s.token.balance(&treasury), U);
    assert_eq!(s.token.balance(&s.freelancer), 99 * U);
    assert_eq!(s.token.balance(&s.escrow.address), 0);
    // Conservation: freelancer + treasury + client deltas = escrow total.
    let client_delta = 1_000 * U - s.token.balance(&s.client);
    assert_eq!(
        s.token.balance(&s.freelancer) + s.token.balance(&treasury),
        client_delta
    );
    // New escrows use the new rate.
    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    assert_eq!(s.escrow.get_escrow_fee(&id2), 300);
}

#[test]
fn fee_rounds_down_on_tiny_amounts() {
    let s = setup();
    let treasury = Address::generate(&s.env);
    s.escrow.set_fee(&100, &treasury);
    // A milestone of 1_000_099 base units: 1% = 10_000.99 -> 10_000 (rounded down).
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 1_000_099i128],
    );
    s.escrow.deposit_funds(&id, &1_000_099);
    s.escrow.release_partial(&id, &0, &99);
    // 99 * 1% < 1 unit: no fee, freelancer receives everything.
    assert_eq!(s.token.balance(&treasury), 0);
    assert_eq!(s.token.balance(&s.freelancer), 99);
    s.escrow.release_milestone(&id, &0);
    assert_eq!(s.token.balance(&treasury), 10_000);
    assert_eq!(s.token.balance(&s.freelancer), 1_000_099 - 10_000);
}

#[test]
fn dispute_fee_only_on_freelancer_share_and_cancel_free() {
    let s = setup();
    let treasury = Address::generate(&s.env);
    s.escrow.set_fee(&500, &treasury);
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 400 * U],
    );
    s.escrow.deposit_funds(&id, &(400 * U));
    open_dispute_bonded(&s, &id, &s.freelancer);
    resolve_refunding_bond(&s, &id, &2_500);
    // Freelancer share 100 units minus 5% fee; the client's 300 refund is fee-free.
    assert_eq!(s.token.balance(&treasury), 5 * U);
    assert_eq!(s.token.balance(&s.freelancer), 95 * U);
    assert_eq!(s.token.balance(&s.client), 900 * U);

    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50 * U],
    );
    s.escrow.deposit_funds(&id2, &(50 * U));
    s.escrow.cancel_contract(&id2);
    assert_eq!(s.token.balance(&s.client), 900 * U);
    assert_eq!(s.token.balance(&treasury), 5 * U);
}

#[test]
fn set_fee_cap_and_admin_only() {
    let s = setup();
    let treasury = Address::generate(&s.env);
    assert_eq!(s.escrow.get_fee_config(), (0, None));
    assert_eq!(
        s.escrow.try_set_fee(&(MAX_FEE_BPS + 1), &treasury),
        Err(Ok(EscrowError::FeeTooHigh))
    );
    s.env.mock_auths(&[]);
    assert!(s.escrow.try_set_fee(&100, &treasury).is_err());
    s.env.mock_all_auths();
    assert_eq!(s.escrow.get_fee_config(), (0, None));
    s.escrow.set_fee(&MAX_FEE_BPS, &treasury);
    let ev = s
        .env
        .events()
        .all()
        .iter()
        .filter(|e| e.0 == s.escrow.address)
        .last()
        .unwrap();
    assert_eq!(
        Symbol::try_from_val(&s.env, &ev.1.get(0).unwrap()).unwrap(),
        symbol_short!("fee_set")
    );
    assert_eq!(
        <(u32, Address)>::try_from_val(&s.env, &ev.2).unwrap(),
        (MAX_FEE_BPS, treasury)
    );
}

#[test]
fn batch_release_charges_exact_per_milestone_fees() {
    let s = setup();
    let treasury = Address::generate(&s.env);
    s.escrow.set_fee(&250, &treasury);
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U, 200 * U],
    );
    s.escrow.deposit_funds(&id, &(300 * U));
    s.escrow
        .release_milestones(&id, &vec![&s.env, 0u32, 1u32]);
    let fee = mul_bps(100 * U, 250) + mul_bps(200 * U, 250);
    assert_eq!(s.token.balance(&treasury), fee);
    assert_eq!(s.token.balance(&s.freelancer), 300 * U - fee);
    assert_eq!(s.token.balance(&s.escrow.address), 0);
}

#[test]
fn mutating_entry_points_keep_instance_alive() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.env.ledger().set_sequence_number(QUIET);
    assert!(instance_ttl(&s) < PERSISTENT_TTL_THRESHOLD);
    // A non-creating mutation (no new escrow) must bump the instance.
    s.escrow.set_terms(&id, &BytesN::from_array(&s.env, &[1u8; 32]), &Vec::new(&s.env));
    assert!(instance_ttl(&s) >= PERSISTENT_TTL_THRESHOLD);

    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.env.ledger().set_sequence_number(QUIET);
    s.escrow.freeze_escrow(&id, &symbol_short!("check"));
    assert!(instance_ttl(&s) >= PERSISTENT_TTL_THRESHOLD);

    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.env.ledger().set_sequence_number(QUIET);
    s.escrow.propose_settlement(&id, &s.client, &0);
    assert!(instance_ttl(&s) >= PERSISTENT_TTL_THRESHOLD);
}

#[test]
fn extend_escrow_ttl_keeps_quiet_escrow_and_side_keys_alive() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(40 * U));
    s.env.ledger().set_sequence_number(QUIET);
    let before = escrow_ttl(&s, id);
    assert!(before < PERSISTENT_TTL_THRESHOLD);
    // Reads stay read-only.
    let _ = s.escrow.get_contract(&id);
    assert_eq!(escrow_ttl(&s, id), before);

    s.escrow.extend_escrow_ttl(&id);
    assert!(escrow_ttl(&s, id) >= PERSISTENT_TTL_THRESHOLD);
    assert!(persistent_ttl(&s, &DataKey::FundBy(id)) >= PERSISTENT_TTL_THRESHOLD);
    assert!(persistent_ttl(&s, &DataKey::Funders(id)) >= PERSISTENT_TTL_THRESHOLD);
    assert!(instance_ttl(&s) >= PERSISTENT_TTL_THRESHOLD);

    // Past the original expiry the escrow is still reachable.
    s.env
        .ledger()
        .set_sequence_number(PERSISTENT_TTL_EXTEND + PERSISTENT_TTL_THRESHOLD);
    assert_eq!(s.escrow.get_contract(&id).funded_amount, 40 * U);
    assert_eq!(
        s.escrow.try_extend_escrow_ttl(&999),
        Err(Ok(EscrowError::ContractNotFound))
    );
}

#[test]
fn extend_reputation_ttl_keeps_reputation_alive() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.release_milestone(&id, &0);
    s.escrow.issue_reputation(&id, &5);
    s.env.ledger().set_sequence_number(QUIET);
    let key = DataKey::Reputation(s.freelancer.clone());
    let before = persistent_ttl(&s, &key);
    assert!(before < PERSISTENT_TTL_THRESHOLD);
    let _ = s.escrow.get_reputation(&s.freelancer);
    assert_eq!(persistent_ttl(&s, &key), before);
    s.escrow.extend_reputation_ttl(&s.freelancer);
    assert!(persistent_ttl(&s, &key) >= PERSISTENT_TTL_THRESHOLD);
    assert!(
        persistent_ttl(&s, &DataKey::WeightedRep(s.freelancer.clone()))
            >= PERSISTENT_TTL_THRESHOLD
    );
}
