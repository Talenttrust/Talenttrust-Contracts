use super::{default_milestones, EscrowFixture};

use soroban_sdk::vec;

use crate::MilestoneEntry;

#[test]
fn unknown_contract_returns_empty_page() {
    let fixture = EscrowFixture::builder().build();
    let page = fixture
        .escrow()
        .get_milestones_page(&9999u32, &0u32, &10u32);
    assert_eq!(page.len(), 0);
}

#[test]
fn full_page_of_pending_milestones() {
    let fixture = EscrowFixture::builder().funded().build();
    let page = fixture
        .escrow()
        .get_milestones_page(&fixture.escrow_id, &0u32, &10u32);
    assert_eq!(page.len(), 3);
    for i in 0..3 {
        let entry: MilestoneEntry = page.get(i).unwrap();
        assert_eq!(entry.index, i);
        assert_eq!(entry.status, 0);
    }
    let default = default_milestones(&fixture.env);
    assert_eq!(page.get(0).unwrap().amount, default.get(0).unwrap());
    assert_eq!(page.get(1).unwrap().amount, default.get(1).unwrap());
    assert_eq!(page.get(2).unwrap().amount, default.get(2).unwrap());
}

#[test]
fn start_beyond_end_returns_empty() {
    let fixture = EscrowFixture::builder().funded().build();
    let page = fixture
        .escrow()
        .get_milestones_page(&fixture.escrow_id, &100u32, &10u32);
    assert_eq!(page.len(), 0);
}

#[test]
fn start_at_last_milestone_returns_one() {
    let fixture = EscrowFixture::builder().funded().build();
    let page = fixture
        .escrow()
        .get_milestones_page(&fixture.escrow_id, &2u32, &10u32);
    assert_eq!(page.len(), 1);
    assert_eq!(page.get(0).unwrap().index, 2);
}

#[test]
fn limit_clamped_to_page_ceiling() {
    let fixture = EscrowFixture::builder().funded().build();
    let page = fixture
        .escrow()
        .get_milestones_page(&fixture.escrow_id, &0u32, &1000u32);
    assert_eq!(page.len(), 3);
}

#[test]
fn zero_limit_returns_empty_page() {
    let fixture = EscrowFixture::builder().funded().build();
    let page = fixture
        .escrow()
        .get_milestones_page(&fixture.escrow_id, &0u32, &0u32);
    assert_eq!(page.len(), 0);
}

#[test]
fn continuation_page_fetches_remaining() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    let cid = fixture.escrow_id;

    let page1 = escrow.get_milestones_page(&cid, &0u32, &1u32);
    assert_eq!(page1.len(), 1);
    assert_eq!(page1.get(0).unwrap().index, 0);

    let page2 = escrow.get_milestones_page(&cid, &1u32, &1u32);
    assert_eq!(page2.len(), 1);
    assert_eq!(page2.get(0).unwrap().index, 1);

    let page3 = escrow.get_milestones_page(&cid, &2u32, &1u32);
    assert_eq!(page3.len(), 1);
    assert_eq!(page3.get(0).unwrap().index, 2);

    let page4 = escrow.get_milestones_page(&cid, &3u32, &1u32);
    assert_eq!(page4.len(), 0);
}

#[test]
fn exact_page_boundary() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    let cid = fixture.escrow_id;

    let page = escrow.get_milestones_page(&cid, &0u32, &3u32);
    assert_eq!(page.len(), 3);
    let page_next = escrow.get_milestones_page(&cid, &3u32, &3u32);
    assert_eq!(page_next.len(), 0);
}

#[test]
fn released_milestone_shows_status_1() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    let cid = fixture.escrow_id;

    escrow.approve_milestone_release(&cid, &fixture.client, &0u32);
    escrow.release_milestone(&cid, &fixture.client, &0u32);

    let page = escrow.get_milestones_page(&cid, &0u32, &10u32);
    assert_eq!(page.len(), 3);
    assert_eq!(page.get(0).unwrap().status, 1);
    assert_eq!(page.get(1).unwrap().status, 0);
}

#[test]
fn refunded_milestone_shows_status_2() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    let cid = fixture.escrow_id;

    let indices = vec![&fixture.env, 2u32];
    escrow.refund_unreleased_milestones(&cid, &indices);

    let page = escrow.get_milestones_page(&cid, &0u32, &10u32);
    assert_eq!(page.len(), 3);
    assert_eq!(page.get(2).unwrap().status, 2);
}

#[test]
fn mixed_statuses_across_pages() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    let cid = fixture.escrow_id;

    escrow.approve_milestone_release(&cid, &fixture.client, &0u32);
    escrow.release_milestone(&cid, &fixture.client, &0u32);

    let indices = vec![&fixture.env, 2u32];
    escrow.refund_unreleased_milestones(&cid, &indices);

    let page1 = escrow.get_milestones_page(&cid, &0u32, &1u32);
    assert_eq!(page1.len(), 1);
    assert_eq!(page1.get(0).unwrap().status, 1);

    let page2 = escrow.get_milestones_page(&cid, &1u32, &1u32);
    assert_eq!(page2.len(), 1);
    assert_eq!(page2.get(0).unwrap().status, 0);

    let page3 = escrow.get_milestones_page(&cid, &2u32, &1u32);
    assert_eq!(page3.len(), 1);
    assert_eq!(page3.get(0).unwrap().status, 2);
}

#[test]
fn single_milestone_contract_pagination() {
    let builder = EscrowFixture::builder();
    let milestones = vec![builder.env(), 5_000_000i128];
    let fixture = builder.with_milestones(milestones).funded().build();
    let escrow = fixture.escrow();
    let cid = fixture.escrow_id;

    let page = escrow.get_milestones_page(&cid, &0u32, &10u32);
    assert_eq!(page.len(), 1);
    assert_eq!(page.get(0).unwrap().index, 0);
    assert_eq!(page.get(0).unwrap().amount, 5_000_000);
    assert_eq!(page.get(0).unwrap().status, 0);
}
