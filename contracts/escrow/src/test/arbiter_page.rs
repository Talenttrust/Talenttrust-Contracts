#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env};

use crate::{
    test::{default_milestones, EscrowFixture},
    ArbiterEntry, Escrow, EscrowClient, ReleaseAuthorization, PAGE_CEILING,
};

fn create_with_optional_arbiter(
    escrow: &EscrowClient<'_>,
    env: &Env,
    client: &Address,
    freelancer: &Address,
    arbiter: Option<Address>,
) -> u32 {
    escrow.create_contract(
        client,
        freelancer,
        &arbiter,
        &default_milestones(env),
        &ReleaseAuthorization::ClientOnly,
    )
}

#[test]
fn empty_when_no_contracts_exist() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let escrow_address = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &escrow_address);
    let admin = Address::generate(&env);
    escrow.initialize(&admin);

    let page = escrow.get_arbiters_page(&0u32, &10u32);
    assert_eq!(page.len(), 0);
}

#[test]
fn empty_when_contracts_have_no_arbiter() {
    let fixture = EscrowFixture::builder().build();
    let page = fixture.escrow().get_arbiters_page(&0u32, &10u32);
    assert_eq!(page.len(), 0);
}

#[test]
fn full_page_of_arbiter_records() {
    let fixture = EscrowFixture::builder().build();
    let env = &fixture.env;
    let escrow = fixture.escrow();
    let arbiter = Address::generate(env);

    let id = create_with_optional_arbiter(
        &escrow,
        env,
        &fixture.client,
        &fixture.freelancer,
        Some(arbiter.clone()),
    );

    let page = escrow.get_arbiters_page(&0u32, &10u32);
    assert_eq!(page.len(), 1);
    let entry: ArbiterEntry = page.get(0).unwrap();
    assert_eq!(entry.contract_id, id);
    assert_eq!(entry.arbiter, arbiter);
}

#[test]
fn skips_contracts_without_arbiter() {
    let fixture = EscrowFixture::builder().build();
    let env = &fixture.env;
    let escrow = fixture.escrow();
    let arbiter_a = Address::generate(env);
    let arbiter_b = Address::generate(env);

    let _no_arbiter =
        create_with_optional_arbiter(&escrow, env, &fixture.client, &fixture.freelancer, None);
    let id_a = create_with_optional_arbiter(
        &escrow,
        env,
        &fixture.client,
        &fixture.freelancer,
        Some(arbiter_a.clone()),
    );
    let _also_none =
        create_with_optional_arbiter(&escrow, env, &fixture.client, &fixture.freelancer, None);
    let id_b = create_with_optional_arbiter(
        &escrow,
        env,
        &fixture.client,
        &fixture.freelancer,
        Some(arbiter_b.clone()),
    );

    let page = escrow.get_arbiters_page(&0u32, &10u32);
    assert_eq!(page.len(), 2);
    assert_eq!(page.get(0).unwrap().contract_id, id_a);
    assert_eq!(page.get(0).unwrap().arbiter, arbiter_a);
    assert_eq!(page.get(1).unwrap().contract_id, id_b);
    assert_eq!(page.get(1).unwrap().arbiter, arbiter_b);
}

#[test]
fn continuation_page_fetches_remaining() {
    let fixture = EscrowFixture::builder().build();
    let env = &fixture.env;
    let escrow = fixture.escrow();

    let mut ids = [0u32; 3];
    for i in 0..3 {
        let arbiter = Address::generate(env);
        ids[i] = create_with_optional_arbiter(
            &escrow,
            env,
            &fixture.client,
            &fixture.freelancer,
            Some(arbiter),
        );
    }

    let page1 = escrow.get_arbiters_page(&0u32, &1u32);
    assert_eq!(page1.len(), 1);
    assert_eq!(page1.get(0).unwrap().contract_id, ids[0]);

    let page2 = escrow.get_arbiters_page(&1u32, &1u32);
    assert_eq!(page2.len(), 1);
    assert_eq!(page2.get(0).unwrap().contract_id, ids[1]);

    let page3 = escrow.get_arbiters_page(&2u32, &1u32);
    assert_eq!(page3.len(), 1);
    assert_eq!(page3.get(0).unwrap().contract_id, ids[2]);

    let page4 = escrow.get_arbiters_page(&3u32, &1u32);
    assert_eq!(page4.len(), 0);
}

#[test]
fn start_beyond_end_returns_empty() {
    let fixture = EscrowFixture::builder().build();
    let env = &fixture.env;
    let escrow = fixture.escrow();
    let arbiter = Address::generate(env);
    create_with_optional_arbiter(
        &escrow,
        env,
        &fixture.client,
        &fixture.freelancer,
        Some(arbiter),
    );

    let page = escrow.get_arbiters_page(&100u32, &10u32);
    assert_eq!(page.len(), 0);
}

#[test]
fn zero_limit_returns_empty_page() {
    let fixture = EscrowFixture::builder().build();
    let env = &fixture.env;
    let escrow = fixture.escrow();
    let arbiter = Address::generate(env);
    create_with_optional_arbiter(
        &escrow,
        env,
        &fixture.client,
        &fixture.freelancer,
        Some(arbiter),
    );

    let page = escrow.get_arbiters_page(&0u32, &0u32);
    assert_eq!(page.len(), 0);
}

#[test]
fn limit_clamped_to_page_ceiling() {
    let fixture = EscrowFixture::builder().build();
    let env = &fixture.env;
    let escrow = fixture.escrow();
    let client = Address::generate(env);
    let freelancer = Address::generate(env);
    let arbiter = Address::generate(env);

    // Create more arbiter records than PAGE_CEILING.
    let total = PAGE_CEILING + 5;
    for _ in 0..total {
        create_with_optional_arbiter(&escrow, env, &client, &freelancer, Some(arbiter.clone()));
    }

    let page = escrow.get_arbiters_page(&0u32, &1000u32);
    assert_eq!(page.len(), PAGE_CEILING);

    let next = escrow.get_arbiters_page(&PAGE_CEILING, &1000u32);
    assert_eq!(next.len(), 5);
}

#[test]
fn exact_page_boundary() {
    let fixture = EscrowFixture::builder().build();
    let env = &fixture.env;
    let escrow = fixture.escrow();

    for _ in 0..3 {
        let arbiter = Address::generate(env);
        create_with_optional_arbiter(
            &escrow,
            env,
            &fixture.client,
            &fixture.freelancer,
            Some(arbiter),
        );
    }

    let page = escrow.get_arbiters_page(&0u32, &3u32);
    assert_eq!(page.len(), 3);
    let page_next = escrow.get_arbiters_page(&3u32, &3u32);
    assert_eq!(page_next.len(), 0);
}
