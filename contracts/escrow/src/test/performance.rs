use super::{default_milestones, generated_participants, register_client, total_milestone_amount};
use soroban_sdk::Env;

#[derive(Clone, Copy)]
struct ResourceBaseline {
    max_instructions: i64,
    max_mem_bytes: i64,
    max_read_entries: u32,
    max_write_entries: u32,
    max_read_bytes: u32,
    max_write_bytes: u32,
    max_fee_total: i64,
}

#[derive(Clone, Copy)]
struct MeasuredResources {
    instructions: i64,
    mem_bytes: i64,
    read_entries: u32,
    write_entries: u32,
    read_bytes: u32,
    write_bytes: u32,
}

const CREATE_CONTRACT_BASELINE: ResourceBaseline = ResourceBaseline {
    max_instructions: 8_000_000,
    max_mem_bytes: 800_000,
    max_read_entries: 2,
    max_write_entries: 3,
    max_read_bytes: 2_048,
    max_write_bytes: 8_192,
    max_fee_total: 1_650_000,
};

const DEPOSIT_FUNDS_BASELINE: ResourceBaseline = ResourceBaseline {
    max_instructions: 6_500_000,
    max_mem_bytes: 700_000,
    max_read_entries: 2,
    max_write_entries: 2,
    max_read_bytes: 2_048,
    max_write_bytes: 8_192,
    max_fee_total: 1_550_000,
};

const RELEASE_MILESTONE_BASELINE: ResourceBaseline = ResourceBaseline {
    max_instructions: 7_000_000,
    max_mem_bytes: 750_000,
    max_read_entries: 2,
    max_write_entries: 2,
    max_read_bytes: 2_048,
    max_write_bytes: 10_240,
    max_fee_total: 1_550_000,
};

fn measure_last_invocation(env: &Env) -> (MeasuredResources, i64) {
    let resources = env.cost_estimate().resources();
    let fee = env.cost_estimate().fee();

    (
        MeasuredResources {
            instructions: resources.instructions,
            mem_bytes: resources.mem_bytes,
            read_entries: resources.read_entries,
            write_entries: resources.write_entries,
            read_bytes: resources.read_bytes,
            write_bytes: resources.write_bytes,
        },
        fee.total,
    )
}

fn assert_within_baseline(
    label: &str,
    resources: MeasuredResources,
    fee_total: i64,
    baseline: ResourceBaseline,
) {
    assert!(
        resources.instructions <= baseline.max_instructions,
        "{} instruction regression: {} > {}",
        label,
        resources.instructions,
        baseline.max_instructions
    );
    assert!(
        resources.mem_bytes <= baseline.max_mem_bytes,
        "{} memory regression: {} > {}",
        label,
        resources.mem_bytes,
        baseline.max_mem_bytes
    );
    assert!(
        resources.read_entries <= baseline.max_read_entries,
        "{} read-entry regression: {} > {}",
        label,
        resources.read_entries,
        baseline.max_read_entries
    );
    assert!(
        resources.write_entries <= baseline.max_write_entries,
        "{} write-entry regression: {} > {}",
        label,
        resources.write_entries,
        baseline.max_write_entries
    );
    assert!(
        resources.read_bytes <= baseline.max_read_bytes,
        "{} read-byte regression: {} > {}",
        label,
        resources.read_bytes,
        baseline.max_read_bytes
    );
    assert!(
        resources.write_bytes <= baseline.max_write_bytes,
        "{} write-byte regression: {} > {}",
        label,
        resources.write_bytes,
        baseline.max_write_bytes
    );
    assert!(
        fee_total <= baseline.max_fee_total,
        "{} fee regression: {} > {}",
        label,
        fee_total,
        baseline.max_fee_total
    );
}

#[test]
fn test_create_contract_resource_baseline() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);

    let _ = client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));

    let (resources, fee_total) = measure_last_invocation(&env);
    assert_within_baseline(
        "create_contract",
        resources,
        fee_total,
        CREATE_CONTRACT_BASELINE,
    );
}

#[test]
fn test_deposit_funds_resource_baseline() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);

    let contract_id =
        client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));
    let _ = client.deposit_funds(&contract_id, &total_milestone_amount());

    let (resources, fee_total) = measure_last_invocation(&env);
    assert_within_baseline(
        "deposit_funds",
        resources,
        fee_total,
        DEPOSIT_FUNDS_BASELINE,
    );
}

#[test]
fn test_release_milestone_resource_baseline() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);

    let contract_id =
        client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));
    let _ = client.deposit_funds(&contract_id, &total_milestone_amount());
    let _ = client.release_milestone(&contract_id, &0);

    let (resources, fee_total) = measure_last_invocation(&env);
    assert_within_baseline(
        "release_milestone",
        resources,
        fee_total,
        RELEASE_MILESTONE_BASELINE,
    );
}

#[test]
fn test_end_to_end_budget_baseline() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);

    let contract_id =
        client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));
    let (create_resources, _) = measure_last_invocation(&env);

    let _ = client.deposit_funds(&contract_id, &total_milestone_amount());
    let (deposit_resources, _) = measure_last_invocation(&env);

    let _ = client.release_milestone(&contract_id, &0);
    let (release_resources, _) = measure_last_invocation(&env);

    let total_instructions = create_resources.instructions
        + deposit_resources.instructions
        + release_resources.instructions;
    let total_memory =
        create_resources.mem_bytes + deposit_resources.mem_bytes + release_resources.mem_bytes;

    assert!(
        total_instructions <= 18_000_000,
        "end-to-end instruction regression: {} > {}",
        total_instructions,
        18_000_000
    );
    assert!(
        total_memory <= 2_000_000,
        "end-to-end memory regression: {} > {}",
        total_memory,
        2_000_000
    );
}
