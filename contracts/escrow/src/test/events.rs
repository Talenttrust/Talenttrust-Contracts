//! Structured event behaviour.

use super::*;

#[test]
fn created_and_deposit_events_carry_state() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U, 200 * U],
    );
    let created: CreatedEvent = last_escrow_event(&s, "created", id).unwrap();
    assert_eq!(
        created,
        CreatedEvent {
            client: s.client.clone(),
            freelancer: s.freelancer.clone(),
            token: s.token.address.clone(),
            total: 300 * U,
            milestone_count: 2,
        }
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    let dep: DepositEvent = last_escrow_event(&s, "deposit", id).unwrap();
    assert_eq!(
        dep,
        DepositEvent {
            from: s.client.clone(),
            amount: 100 * U,
            funded_amount: 100 * U,
            status: ContractStatus::Created,
        }
    );
    s.escrow.deposit_funds(&id, &(200 * U));
    let dep: DepositEvent = last_escrow_event(&s, "deposit", id).unwrap();
    assert_eq!(dep.funded_amount, 300 * U);
    assert_eq!(dep.status, ContractStatus::Funded);
}

#[test]
fn release_events_and_complete_emitted_exactly_once() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U, 200 * U],
    );
    s.escrow.deposit_funds(&id, &(300 * U));
    s.escrow.release_milestone(&id, &0);
    let rel: ReleaseEvent = last_escrow_event(&s, "release", id).unwrap();
    assert_eq!(
        rel,
        ReleaseEvent {
            index: 0,
            amount: 100 * U,
            to: s.freelancer.clone(),
            released_amount: 100 * U,
            status: ContractStatus::Funded,
            fee: 0,
        }
    );
    assert_eq!(count_escrow_events(&s, "complete"), 0);

    s.escrow.release_milestone(&id, &1);
    let rel: ReleaseEvent = last_escrow_event(&s, "release", id).unwrap();
    assert_eq!(rel.released_amount, 300 * U);
    assert_eq!(rel.status, ContractStatus::Completed);
    assert_eq!(count_escrow_events(&s, "complete"), 1);
    let done: CompletedEvent = last_escrow_event(&s, "complete", id).unwrap();
    assert_eq!(done.released_amount, 300 * U);
    let names = escrow_event_names(&s);
    assert_eq!(names[names.len() - 1], symbol_short!("complete"));
}

#[test]
fn complete_event_for_batch_and_partial_releases() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U, 200 * U],
    );
    s.escrow.deposit_funds(&id, &(300 * U));
    s.escrow.release_partial(&id, &0, &(40 * U));
    assert_eq!(count_escrow_events(&s, "complete"), 0);
    s.escrow.release_milestones(&id, &vec![&s.env, 1u32]);
    assert_eq!(count_escrow_events(&s, "complete"), 0);
    s.escrow.release_partial(&id, &0, &(60 * U));
    assert_eq!(count_escrow_events(&s, "complete"), 1);

    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U, 200 * U],
    );
    s.escrow.deposit_funds(&id2, &(300 * U));
    s.escrow.release_milestones(&id2, &vec![&s.env, 0u32, 1u32]);
    let done: CompletedEvent = last_escrow_event(&s, "complete", id2).unwrap();
    assert_eq!(done.released_amount, 300 * U);
    let rel: ReleaseEvent = last_escrow_event(&s, "release", id2).unwrap();
    assert_eq!((rel.index, rel.amount, rel.released_amount), (1, 200 * U, 300 * U));
}

#[test]
fn dispute_resolved_cancel_and_rep_events() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 400 * U],
    );
    s.escrow.deposit_funds(&id, &(400 * U));
    open_dispute_bonded(&s, &id, &s.freelancer);
    let d: DisputeEvent = last_escrow_event(&s, "dispute", id).unwrap();
    assert_eq!(
        d,
        DisputeEvent {
            by: s.freelancer.clone(),
            status: ContractStatus::Disputed,
            bond: dispute_bond(400 * U),
        }
    );
    // Events are per invocation: read them right after the call.
    s.escrow.resolve_dispute(&s.admin, &id, &2_500, &true);
    let r: ResolvedEvent = last_escrow_event(&s, "resolved", id).unwrap();
    assert_eq!(
        r,
        ResolvedEvent {
            to_freelancer: 100 * U,
            to_client: 300 * U,
            status: ContractStatus::Resolved,
            bond_recipient: Some(s.freelancer.clone()),
            bond: dispute_bond(400 * U),
        }
    );
    s.escrow.issue_reputation(&id, &4);
    let rep: ReputationEvent = last_escrow_event(&s, "rep", id).unwrap();
    assert_eq!(
        rep,
        ReputationEvent {
            freelancer: s.freelancer.clone(),
            rating: 4,
            completed_contracts: 1,
            total_rating: 4,
            weight: 100 * U,
        }
    );

    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50 * U],
    );
    s.escrow.deposit_funds(&id2, &(50 * U));
    s.escrow.cancel_contract(&id2);
    let c: CancelEvent = last_escrow_event(&s, "cancel", id2).unwrap();
    assert_eq!(
        c,
        CancelEvent {
            refund: 50 * U,
            to: s.client.clone(),
            status: ContractStatus::Cancelled,
        }
    );
}
