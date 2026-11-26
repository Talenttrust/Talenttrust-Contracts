//! Dispute opening, arbitration voting, bonds and settlement.

use super::*;

#[test]
fn dispute_split_and_cancel() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 400 * U],
    );
    s.escrow.deposit_funds(&id, &(400 * U));
    open_dispute_bonded(&s, &id, &s.freelancer);
    resolve_refunding_bond(&s, &id, &2_500);
    assert_eq!(s.token.balance(&s.freelancer), 100 * U);
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
    assert_eq!(
        s.escrow.get_contract(&id2).status,
        ContractStatus::Cancelled
    );
}

#[test]
fn dispute_evidence_is_party_only_and_emits_event() {
    let s = setup();
    let id = funded_escrow(&s);
    let hash = BytesN::from_array(&s.env, &[7; 32]);
    let outsider = Address::generate(&s.env);

    assert_eq!(
        s.escrow.try_submit_evidence(&id, &s.client, &hash),
        Err(Ok(EscrowError::InvalidStatus))
    );
    open_dispute_bonded(&s, &id, &s.client);
    assert_eq!(
        s.escrow.try_submit_evidence(&id, &outsider, &hash),
        Err(Ok(EscrowError::Unauthorized))
    );

    set_ts(&s.env, 1_234);
    s.escrow.submit_evidence(&id, &s.client, &hash);
    let freelancer_hash = BytesN::from_array(&s.env, &[9; 32]);
    s.escrow
        .submit_evidence(&id, &s.freelancer, &freelancer_hash);
    assert_eq!(
        s.escrow.get_evidence(&id),
        vec![
            &s.env,
            (s.client.clone(), hash.clone(), 1_234),
            (s.freelancer.clone(), freelancer_hash, 1_234),
        ]
    );
    assert_eq!(
        last_escrow_event::<EvidenceEvent>(&s, "evidence", id),
        Some(EvidenceEvent {
            submitter: s.freelancer.clone(),
            evidence_hash: BytesN::from_array(&s.env, &[9; 32]),
            submitted_at: 1_234,
        })
    );
}

#[test]
fn dispute_evidence_enforces_party_cap_pause_and_freeze() {
    let s = setup();
    let id = funded_escrow(&s);
    open_dispute_bonded(&s, &id, &s.freelancer);
    let hash = BytesN::from_array(&s.env, &[1; 32]);

    s.escrow.set_pause_flags(&s.admin, &PAUSE_DISPUTE);
    assert_eq!(
        s.escrow.try_submit_evidence(&id, &s.client, &hash),
        Err(Ok(EscrowError::Paused))
    );
    s.escrow.set_pause_flags(&s.admin, &0);

    s.escrow.freeze_escrow(&id, &symbol_short!("hold"));
    assert_eq!(
        s.escrow.try_submit_evidence(&id, &s.client, &hash),
        Err(Ok(EscrowError::EscrowFrozen))
    );
    s.escrow.unfreeze_escrow(&id);

    for index in 0..MAX_EVIDENCE_PER_PARTY {
        let hash = BytesN::from_array(&s.env, &[index as u8; 32]);
        s.escrow.submit_evidence(&id, &s.client, &hash);
    }
    assert_eq!(
        s.escrow.try_submit_evidence(&id, &s.client, &hash),
        Err(Ok(EscrowError::InvalidAmount))
    );
    s.escrow.submit_evidence(&id, &s.freelancer, &hash);
    assert_eq!(
        s.escrow.get_evidence(&id).len(),
        MAX_EVIDENCE_PER_PARTY + 1
    );
}

#[test]
fn dispute_evidence_ttl_extends_and_settlement_clears_it() {
    let s = setup();
    let id = funded_escrow(&s);
    open_dispute_bonded(&s, &id, &s.client);
    let hash = BytesN::from_array(&s.env, &[3; 32]);
    s.escrow.submit_evidence(&id, &s.client, &hash);

    s.env.ledger().set_sequence_number(QUIET);
    assert!(persistent_ttl(&s, &DataKey::Evidence(id)) < PERSISTENT_TTL_THRESHOLD);
    s.escrow.extend_escrow_ttl(&id);
    assert!(persistent_ttl(&s, &DataKey::Evidence(id)) >= PERSISTENT_TTL_THRESHOLD);

    resolve_refunding_bond(&s, &id, &5_000);
    assert!(s.escrow.get_evidence(&id).is_empty());
    s.env.as_contract(&s.escrow.address, || {
        assert!(!s.env.storage().persistent().has(&DataKey::Evidence(id)));
    });
}

#[test]
fn dispute_outcomes_freelancer_client_and_exact_threshold() {
    let s = setup();

    // 1. Freelancer-favoured resolution (> 5_000 bps): 7_500 bps
    let id1 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 200 * U],
    );
    s.escrow.deposit_funds(&id1, &(200 * U));
    open_dispute_bonded(&s, &id1, &s.client);
    resolve_refunding_bond(&s, &id1, &7_500);

    let client_outcomes1 = s.escrow.get_outcomes(&s.client);
    let freelancer_outcomes1 = s.escrow.get_outcomes(&s.freelancer);
    assert_eq!(client_outcomes1.disputes_opened, 1);
    assert_eq!(client_outcomes1.disputes_won, 0);
    assert_eq!(client_outcomes1.disputes_lost, 1);
    assert_eq!(freelancer_outcomes1.disputes_opened, 0);
    assert_eq!(freelancer_outcomes1.disputes_won, 1);
    assert_eq!(freelancer_outcomes1.disputes_lost, 0);

    // 2. Client-favoured resolution (< 5_000 bps): 2_000 bps
    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 200 * U],
    );
    s.escrow.deposit_funds(&id2, &(200 * U));
    open_dispute_bonded(&s, &id2, &s.freelancer);
    resolve_refunding_bond(&s, &id2, &2_000);

    let client_outcomes2 = s.escrow.get_outcomes(&s.client);
    let freelancer_outcomes2 = s.escrow.get_outcomes(&s.freelancer);
    assert_eq!(client_outcomes2.disputes_opened, 1);
    assert_eq!(client_outcomes2.disputes_won, 1);
    assert_eq!(client_outcomes2.disputes_lost, 1);
    assert_eq!(freelancer_outcomes2.disputes_opened, 1);
    assert_eq!(freelancer_outcomes2.disputes_won, 1);
    assert_eq!(freelancer_outcomes2.disputes_lost, 1);

    // 3. Exactly 5_000 bps threshold: counts as a win for freelancer, loss for client
    let id3 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 200 * U],
    );
    s.escrow.deposit_funds(&id3, &(200 * U));
    open_dispute_bonded(&s, &id3, &s.client);
    resolve_refunding_bond(&s, &id3, &5_000);

    let client_outcomes3 = s.escrow.get_outcomes(&s.client);
    let freelancer_outcomes3 = s.escrow.get_outcomes(&s.freelancer);
    assert_eq!(client_outcomes3.disputes_opened, 2);
    assert_eq!(client_outcomes3.disputes_won, 1);
    assert_eq!(client_outcomes3.disputes_lost, 2);
    assert_eq!(freelancer_outcomes3.disputes_opened, 1);
    assert_eq!(freelancer_outcomes3.disputes_won, 2);
    assert_eq!(freelancer_outcomes3.disputes_lost, 1);
}

#[test]
fn panel_two_of_three_different_votes_do_not_settle_until_matching() {
    let s = setup();
    let m1 = Address::generate(&s.env);
    let m2 = Address::generate(&s.env);
    let m3 = Address::generate(&s.env);
    s.escrow
        .set_panel(&vec![&s.env, m1.clone(), m2.clone(), m3.clone()], &2);
    let id = disputed_high_value_escrow(&s, PANEL_THRESHOLD_AMOUNT);
    // A high-value escrow cannot be settled by a single admin call.
    assert_eq!(
        s.escrow.try_resolve_dispute(&s.admin, &id, &5_000, &true),
        Err(Ok(EscrowError::PanelRequired))
    );
    // Two different splits do not settle.
    s.escrow.vote_resolution(&m1, &id, &4_000);
    s.escrow.vote_resolution(&m2, &id, &6_000);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Disputed);
    assert_eq!(s.token.balance(&s.freelancer), 0);
    assert_eq!(s.token.balance(&s.client), 1_000 * U);
    // The second matching vote (m3 joins m1 at 4_000) settles the escrow.
    s.escrow.vote_resolution(&m3, &id, &4_000);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Resolved);
    let to_freelancer = PANEL_THRESHOLD_AMOUNT * 4_000 / BPS_DENOMINATOR;
    assert_eq!(s.token.balance(&s.freelancer), to_freelancer);
    assert_eq!(
        s.token.balance(&s.client),
        1_000 * U + PANEL_THRESHOLD_AMOUNT - to_freelancer + dispute_bond(PANEL_THRESHOLD_AMOUNT)
    );
    // Votes are cleared once the escrow settles.
    assert_eq!(s.escrow.get_votes(&id).len(), 0);
}

#[test]
fn vote_change_replaces_vote_and_requires_membership() {
    let s = setup();
    let m1 = Address::generate(&s.env);
    let m2 = Address::generate(&s.env);
    let m3 = Address::generate(&s.env);
    s.escrow
        .set_panel(&vec![&s.env, m1.clone(), m2.clone(), m3.clone()], &2);
    let id = disputed_high_value_escrow(&s, PANEL_THRESHOLD_AMOUNT);

    // A non-member cannot vote.
    let outsider = Address::generate(&s.env);
    assert_eq!(
        s.escrow.try_vote_resolution(&outsider, &id, &5_000),
        Err(Ok(EscrowError::Unauthorized))
    );
    // A split above 100% is rejected.
    assert_eq!(
        s.escrow.try_vote_resolution(&m1, &id, &10_001),
        Err(Ok(EscrowError::InvalidSplit))
    );
    // Parties to the escrow cannot vote, even if on the panel.
    s.escrow
        .set_panel(&vec![&s.env, s.client.clone(), m1.clone(), m2.clone()], &2);
    assert_eq!(
        s.escrow.try_vote_resolution(&s.client, &id, &5_000),
        Err(Ok(EscrowError::Unauthorized))
    );
    assert_eq!(
        s.escrow.try_vote_resolution(&s.freelancer, &id, &5_000),
        Err(Ok(EscrowError::Unauthorized))
    );

    // Changing a vote replaces it: the first vote no longer counts.
    s.escrow.vote_resolution(&m1, &id, &5_000);
    s.escrow.vote_resolution(&m1, &id, &7_000);
    let votes = s.escrow.get_votes(&id);
    assert_eq!(votes.len(), 1);
    assert_eq!(votes.get(m1.clone()), Some(7_000));
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Disputed);

    // m2 matching m1's new vote settles at 7_000.
    s.escrow.vote_resolution(&m2, &id, &7_000);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Resolved);
    let to_freelancer = PANEL_THRESHOLD_AMOUNT * 7_000 / BPS_DENOMINATOR;
    // Panel split favours the freelancer, so the client opener's bond goes to the freelancer.
    assert_eq!(
        s.token.balance(&s.freelancer),
        to_freelancer + dispute_bond(PANEL_THRESHOLD_AMOUNT)
    );
    assert_eq!(
        s.token.balance(&s.client),
        1_000 * U + PANEL_THRESHOLD_AMOUNT - to_freelancer
    );
}

#[test]
fn panel_change_mid_dispute_ignores_removed_members_votes() {
    let s = setup();
    let m1 = Address::generate(&s.env);
    let m2 = Address::generate(&s.env);
    let m3 = Address::generate(&s.env);
    let m4 = Address::generate(&s.env);
    s.escrow
        .set_panel(&vec![&s.env, m1.clone(), m2.clone(), m3.clone()], &2);
    let id = disputed_high_value_escrow(&s, PANEL_THRESHOLD_AMOUNT);
    s.escrow.vote_resolution(&m1, &id, &3_000);
    // Rule: when the panel changes, votes from removed members stop counting.
    s.escrow
        .set_panel(&vec![&s.env, m2.clone(), m3.clone(), m4.clone()], &2);
    s.escrow.vote_resolution(&m2, &id, &3_000);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Disputed);
    assert_eq!(s.token.balance(&s.freelancer), 0);
    // A second vote from a current member reaches the threshold.
    s.escrow.vote_resolution(&m3, &id, &3_000);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Resolved);
    let to_freelancer = PANEL_THRESHOLD_AMOUNT * 3_000 / BPS_DENOMINATOR;
    assert_eq!(s.token.balance(&s.freelancer), to_freelancer);
    assert_eq!(
        s.token.balance(&s.client),
        1_000 * U + PANEL_THRESHOLD_AMOUNT - to_freelancer + dispute_bond(PANEL_THRESHOLD_AMOUNT)
    );
}

#[test]
fn below_threshold_escrow_still_settles_via_resolve_dispute() {
    let s = setup();
    let m1 = Address::generate(&s.env);
    let m2 = Address::generate(&s.env);
    s.escrow.set_panel(&vec![&s.env, m1, m2], &2);
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 400 * U],
    );
    s.escrow.deposit_funds(&id, &(400 * U));
    open_dispute_bonded(&s, &id, &s.freelancer);
    resolve_refunding_bond(&s, &id, &2_500);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Resolved);
    assert_eq!(s.token.balance(&s.freelancer), 100 * U);
    assert_eq!(s.token.balance(&s.client), 900 * U);
}

#[test]
fn set_panel_validates_threshold_and_members() {
    let s = setup();
    let m1 = Address::generate(&s.env);
    let m2 = Address::generate(&s.env);
    assert_eq!(
        s.escrow
            .try_set_panel(&vec![&s.env, m1.clone(), m2.clone()], &0),
        Err(Ok(EscrowError::InvalidPanel))
    );
    assert_eq!(
        s.escrow.try_set_panel(&vec![&s.env, m1.clone()], &2),
        Err(Ok(EscrowError::InvalidPanel))
    );
    let too_many = vec![
        &s.env,
        Address::generate(&s.env),
        Address::generate(&s.env),
        Address::generate(&s.env),
        Address::generate(&s.env),
        Address::generate(&s.env),
        Address::generate(&s.env),
        Address::generate(&s.env),
        Address::generate(&s.env),
    ];
    assert_eq!(
        s.escrow.try_set_panel(&too_many, &2),
        Err(Ok(EscrowError::InvalidPanel))
    );
    assert_eq!(
        s.escrow
            .try_set_panel(&vec![&s.env, m1.clone(), m2.clone(), m1.clone()], &2),
        Err(Ok(EscrowError::InvalidPanel))
    );
    s.escrow
        .set_panel(&vec![&s.env, m1.clone(), m2.clone()], &2);
    let panel = s.escrow.get_panel().unwrap();
    assert_eq!(panel.threshold, 2);
    assert_eq!(panel.members.len(), 2);
}

#[test]
fn panel_settlement_records_outcomes_and_liability() {
    let s = setup();
    let m1 = Address::generate(&s.env);
    let m2 = Address::generate(&s.env);
    s.escrow.set_panel(&vec![&s.env, m1.clone(), m2.clone()], &2);
    let id = disputed_high_value_escrow(&s, PANEL_THRESHOLD_AMOUNT);
    s.escrow.vote_resolution(&m1, &id, &6_000);
    s.escrow.vote_resolution(&m2, &id, &6_000);
    let c = s.escrow.get_contract(&id);
    assert_eq!(c.status, ContractStatus::Resolved);
    check_invariants(&c);
    assert_eq!(s.escrow.get_liability(&s.token.address), 0);
    assert_eq!(s.escrow.get_outcomes(&s.freelancer).disputes_won, 1);
    assert_eq!(s.escrow.get_outcomes(&s.client).disputes_lost, 1);
}

#[test]
fn open_dispute_without_bond_tokens_keeps_escrow_funded() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 1_000 * U],
    );
    s.escrow.deposit_funds(&id, &(1_000 * U));
    // The client spent everything, so it cannot pay the bond (2% of the total).
    assert_eq!(s.token.balance(&s.client), 0);
    assert!(s.escrow.try_open_dispute(&id, &s.client).is_err());
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Funded);
    assert_eq!(s.escrow.get_dispute_bond(&id), None);
}

#[test]
fn dispute_bond_returns_to_opener() {
    let s = setup();
    StellarAssetClient::new(&s.env, &s.token.address).mint(&s.freelancer, &(1_000 * U));
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 400 * U],
    );
    s.escrow.deposit_funds(&id, &(400 * U));
    s.escrow.open_dispute(&id, &s.freelancer);
    assert_eq!(
        s.escrow.get_dispute_bond(&id),
        Some((s.freelancer.clone(), 8 * U))
    );
    assert_eq!(s.escrow.get_liability(&s.token.address), 408 * U);

    s.escrow.resolve_dispute(&s.admin, &id, &2_500, &true);

    // Split is 100/300, unaffected by the bond; the 8 bond returns to the opener.
    assert_eq!(s.token.balance(&s.freelancer), 1_000 * U - 8 * U + 100 * U + 8 * U);
    assert_eq!(s.token.balance(&s.client), 1_000 * U - 400 * U + 300 * U);
    assert_eq!(s.token.balance(&s.escrow.address), 0);
    assert_eq!(s.escrow.get_liability(&s.token.address), 0);
    assert_eq!(s.escrow.get_dispute_bond(&id), None);
}

#[test]
fn dispute_bond_forfeited_to_counterparty() {
    let s = setup();
    StellarAssetClient::new(&s.env, &s.token.address).mint(&s.freelancer, &(1_000 * U));
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 400 * U],
    );
    s.escrow.deposit_funds(&id, &(400 * U));
    s.escrow.open_dispute(&id, &s.freelancer);

    s.escrow.resolve_dispute(&s.admin, &id, &2_500, &false);
    let r: ResolvedEvent = last_escrow_event(&s, "resolved", id).unwrap();
    assert_eq!(r.bond_recipient, Some(s.client.clone()));
    assert_eq!(r.bond, 8 * U);

    // Same split, but the 8 bond is forfeited to the client (the counterparty).
    assert_eq!(s.token.balance(&s.freelancer), 1_000 * U - 8 * U + 100 * U);
    assert_eq!(s.token.balance(&s.client), 1_000 * U - 400 * U + 300 * U + 8 * U);
    assert_eq!(s.token.balance(&s.escrow.address), 0);
}

#[test]
fn dispute_bond_rounds_to_zero_for_tiny_total() {
    // Tiny totals round the 2% bond down to 0 (49 * 200 / 10_000 == 0)
    assert_eq!(dispute_bond(49), 0);
    assert_eq!(dispute_bond(50), 1);
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, MIN_MILESTONE_AMOUNT],
    );
    s.escrow.deposit_funds(&id, &MIN_MILESTONE_AMOUNT);
    s.escrow.open_dispute(&id, &s.client);
    assert_eq!(
        s.escrow.get_dispute_bond(&id),
        Some((s.client.clone(), MIN_MILESTONE_AMOUNT * 200 / 10_000))
    );
    s.escrow.resolve_dispute(&s.admin, &id, &5_000, &true);
    assert_eq!(s.token.balance(&s.escrow.address), 0);
    assert_eq!(s.escrow.get_liability(&s.token.address), 0);
}

#[test]
fn sweep_cannot_take_dispute_bonds() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.open_dispute(&id, &s.client);
    assert_eq!(
        s.escrow.try_sweep_surplus(&s.token.address, &s.admin),
        Err(Ok(EscrowError::NoSurplus))
    );
}

#[test]
fn mul_bps_matches_256_bit_reference() {
    let env = Env::default();
    let helds: [i128; 14] = [
        0,
        1,
        9_999,
        10_000,
        10_001,
        123_456_789,
        MIN_MILESTONE_AMOUNT,
        MAX_ESCROW_TOTAL,
        i128::MAX / 10_000,
        i128::MAX / 10_000 + 1,
        i128::MAX / 2,
        i128::MAX - 1,
        i128::MAX,
        170_141_183_460_469_231_731_687_303_715_884_105_000,
    ];
    let bpss: [u32; 9] = [0, 1, 2, 2_500, 4_999, 5_000, 7_777, 9_999, 10_000];
    let mut checked = 0;
    for held in helds.iter() {
        for bps in bpss.iter() {
            assert_eq!(
                mul_bps(*held, *bps),
                mul_bps_reference(&env, *held, *bps),
                "held={} bps={}",
                held,
                bps
            );
            checked += 1;
        }
    }
    assert!(checked >= 100);
}

#[test]
fn max_value_escrow_dispute_split_is_exact() {
    let s = setup();
    StellarAssetClient::new(&s.env, &s.token.address).mint(&s.client, &MAX_ESCROW_TOTAL);
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, MAX_ESCROW_TOTAL],
    );
    s.escrow.deposit_funds(&id, &MAX_ESCROW_TOTAL);
    let m1 = Address::generate(&s.env);
    let m2 = Address::generate(&s.env);
    // Totals above the panel threshold settle through the arbiter panel.
    s.escrow.set_panel(&vec![&s.env, m1.clone(), m2.clone()], &2);
    open_dispute_bonded(&s, &id, &s.client);
    let client_before = s.token.balance(&s.client);
    s.escrow.vote_resolution(&m1, &id, &2_500);
    s.escrow.vote_resolution(&m2, &id, &2_500);
    let to_freelancer = MAX_ESCROW_TOTAL / 4;
    assert_eq!(s.token.balance(&s.freelancer), to_freelancer);
    // Client gets the 75% share plus its own bond back (the split favours it).
    assert_eq!(
        s.token.balance(&s.client),
        client_before + (MAX_ESCROW_TOTAL - to_freelancer) + dispute_bond(MAX_ESCROW_TOTAL)
    );
    assert_eq!(s.token.balance(&s.escrow.address), 0);
}

#[test]
fn issue_reputation_overflow_returns_typed_error() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.release_milestone(&id, &0);
    s.env.as_contract(&s.escrow.address, || {
        s.env.storage().persistent().set(
            &DataKey::Reputation(s.freelancer.clone()),
            &Reputation {
                completed_contracts: 1,
                total_rating: u32::MAX - 1,
            },
        );
    });
    assert_eq!(
        s.escrow.try_issue_reputation(&id, &5),
        Err(Ok(EscrowError::ArithmeticOverflow))
    );
    assert!(!s.escrow.get_contract(&id).reputation_issued);
}

#[test]
fn client_proposes_freelancer_accepts_settlement() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 200 * U],
    );
    s.escrow.deposit_funds(&id, &(200 * U));

    s.escrow.propose_settlement(&id, &s.client, &(100 * U));
    let proposal = s.escrow.get_settlement(&id).unwrap();
    assert_eq!(proposal.proposer, s.client);
    assert_eq!(proposal.freelancer_amount, 100 * U);

    s.escrow.accept_settlement(&id, &s.freelancer, &(100 * U));
    assert_eq!(s.token.balance(&s.freelancer), 100 * U);
    assert_eq!(s.token.balance(&s.client), 900 * U);
    assert_eq!(s.token.balance(&s.escrow.address), 0);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Resolved);
    assert!(s.escrow.get_settlement(&id).is_none());
}

#[test]
fn freelancer_proposes_client_accepts_settlement() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 200 * U],
    );
    s.escrow.deposit_funds(&id, &(200 * U));

    s.escrow.propose_settlement(&id, &s.freelancer, &(150 * U));
    s.escrow.accept_settlement(&id, &s.client, &(150 * U));
    assert_eq!(s.token.balance(&s.freelancer), 150 * U);
    assert_eq!(s.token.balance(&s.client), 850 * U);
    assert_eq!(s.token.balance(&s.escrow.address), 0);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Resolved);
}

#[test]
fn proposer_cannot_self_accept_settlement() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 200 * U],
    );
    s.escrow.deposit_funds(&id, &(200 * U));
    s.escrow.propose_settlement(&id, &s.client, &(100 * U));

    assert_eq!(
        s.escrow.try_accept_settlement(&id, &s.client, &(100 * U)),
        Err(Ok(EscrowError::SettlementMismatch))
    );
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Funded);
}

#[test]
fn settlement_mismatched_amount_rejected() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 200 * U],
    );
    s.escrow.deposit_funds(&id, &(200 * U));
    s.escrow.propose_settlement(&id, &s.client, &(100 * U));

    assert_eq!(
        s.escrow.try_accept_settlement(&id, &s.freelancer, &(50 * U)),
        Err(Ok(EscrowError::SettlementMismatch))
    );
    assert_eq!(s.token.balance(&s.freelancer), 0);
    assert_eq!(s.token.balance(&s.client), 800 * U);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Funded);
}

#[test]
fn settlement_invalidated_by_release() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(200 * U));
    s.escrow.propose_settlement(&id, &s.client, &(100 * U));
    s.escrow.release_milestone(&id, &0);
    assert!(s.escrow.get_settlement(&id).is_none());

    assert_eq!(
        s.escrow.try_accept_settlement(&id, &s.freelancer, &(100 * U)),
        Err(Ok(EscrowError::SettlementMismatch))
    );
}

#[test]
fn settlement_from_disputed_blocks_admin_resolution() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 200 * U],
    );
    s.escrow.deposit_funds(&id, &(200 * U));
    open_dispute_bonded(&s, &id, &s.client);

    s.escrow.propose_settlement(&id, &s.freelancer, &(120 * U));
    s.escrow.accept_settlement(&id, &s.client, &(120 * U));
    assert_eq!(s.token.balance(&s.freelancer), 120 * U);
    // The client opener also gets its dispute bond back.
    assert_eq!(s.token.balance(&s.client), 880 * U + dispute_bond(200 * U));
    assert_eq!(s.token.balance(&s.escrow.address), 0);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Resolved);

    assert_eq!(
        s.escrow.try_resolve_dispute(&s.admin, &id, &5_000, &true),
        Err(Ok(EscrowError::InvalidStatus))
    );
    assert_eq!(s.escrow.get_dispute_bond(&id), None);
}

#[test]
fn withdraw_settlement_removes_proposal() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 200 * U],
    );
    s.escrow.deposit_funds(&id, &(200 * U));
    s.escrow.propose_settlement(&id, &s.client, &(100 * U));
    s.escrow.withdraw_settlement(&id, &s.client);
    assert!(s.escrow.get_settlement(&id).is_none());
    assert_eq!(
        s.escrow.try_accept_settlement(&id, &s.freelancer, &(100 * U)),
        Err(Ok(EscrowError::SettlementMismatch))
    );
}

#[test]
fn settlement_amount_out_of_bounds_rejected() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 200 * U],
    );
    s.escrow.deposit_funds(&id, &(200 * U));
    assert_eq!(
        s.escrow.try_propose_settlement(&id, &s.client, &(200 * U + 1)),
        Err(Ok(EscrowError::InvalidAmount))
    );
    assert_eq!(
        s.escrow.try_propose_settlement(&id, &s.client, &-1),
        Err(Ok(EscrowError::InvalidAmount))
    );
}

#[test]
fn settlement_invalidated_by_deposit_and_events_emitted() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 200 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.propose_settlement(&id, &s.freelancer, &(60 * U));
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
        symbol_short!("settle_p")
    );
    s.escrow.deposit_funds(&id, &(50 * U));
    assert!(s.escrow.get_settlement(&id).is_none());
    s.escrow.propose_settlement(&id, &s.client, &(50 * U));
    s.escrow.accept_settlement(&id, &s.freelancer, &(50 * U));
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
        symbol_short!("settled")
    );
    assert_eq!(
        <(i128, i128)>::try_from_val(&s.env, &ev.2).unwrap(),
        (50 * U, 100 * U)
    );
    assert_eq!(s.escrow.get_liability(&s.token.address), 0);
    // A stranger cannot propose or accept
    let stranger = Address::generate(&s.env);
    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 200 * U],
    );
    assert_eq!(
        s.escrow.try_propose_settlement(&id2, &stranger, &0),
        Err(Ok(EscrowError::Unauthorized))
    );
}
