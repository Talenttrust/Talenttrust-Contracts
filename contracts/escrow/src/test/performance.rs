use super::{create_contract, register_client, total_milestone_amount};
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
    max_instructions: 10_000_000,
    max_mem_bytes: 1_000_000,
    max_read_entries: 4,
    max_write_entries: 3,
    max_read_bytes: 4_096,
    max_write_bytes: 12_288,
    max_fee_total: 2_000_000,
};

const DEPOSIT_FUNDS_BASELINE: ResourceBaseline = ResourceBaseline {
    max_instructions: 8_500_000,
    max_mem_bytes: 900_000,
    max_read_entries: 3,
    max_write_entries: 2,
    max_read_bytes: 4_096,
    max_write_bytes: 8_192,
    max_fee_total: 1_900_000,
};

const RELEASE_MILESTONE_BASELINE: ResourceBaseline = ResourceBaseline {
    max_instructions: 10_000_000,
    max_mem_bytes: 1_000_000,
    max_read_entries: 4,
    max_write_entries: 3,
    max_read_bytes: 4_096,
    max_write_bytes: 14_336,
    max_fee_total: 2_100_000,
};

const REFUND_BASELINE: ResourceBaseline = ResourceBaseline {
    max_instructions: 10_000_000,
    max_mem_bytes: 1_000_000,
    max_read_entries: 4,
    max_write_entries: 3,
    max_read_bytes: 4_096,
    max_write_bytes: 12_288,
    max_fee_total: 2_000_000,
};

const CANCEL_BASELINE: ResourceBaseline = ResourceBaseline {
    max_instructions: 9_000_000,
    max_mem_bytes: 900_000,
    max_read_entries: 3,
    max_write_entries: 2,
    max_read_bytes: 4_096,
    max_write_bytes: 8_192,
    max_fee_total: 1_900_000,
};

const DISPUTE_BASELINE: ResourceBaseline = ResourceBaseline {
    max_instructions: 9_000_000,
    max_mem_bytes: 900_000,
    max_read_entries: 3,
    max_write_entries: 2,
    max_read_bytes: 4_096,
    max_write_bytes: 8_192,
    max_fee_total: 1_900_000,
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
fn create_contract_resource_baseline() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let _ = create_contract(&env, &client);

    let (resources, fee_total) = measure_last_invocation(&env);
    assert_within_baseline(
        "create_contract",
        resources,
        fee_total,
        CREATE_CONTRACT_BASELINE,
    );
}

#[test]
fn deposit_funds_resource_baseline() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (_, _, contract_id) = create_contract(&env, &client);
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
fn release_milestone_resource_baseline() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (_, _, contract_id) = create_contract(&env, &client);
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
fn refund_resource_baseline() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (_, _, contract_id) = create_contract(&env, &client);
    let _ = client.deposit_funds(&contract_id, &total_milestone_amount());
    let _ = client.refund(&contract_id, &0);

    let (resources, fee_total) = measure_last_invocation(&env);
    assert_within_baseline("refund", resources, fee_total, REFUND_BASELINE);
}

#[test]
fn cancel_resource_baseline() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (_, _, contract_id) = create_contract(&env, &client);
    let _ = client.cancel(&contract_id);

    let (resources, fee_total) = measure_last_invocation(&env);
    assert_within_baseline("cancel", resources, fee_total, CANCEL_BASELINE);
}

#[test]
fn dispute_resource_baseline() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (_, _, contract_id) = create_contract(&env, &client);
    let _ = client.deposit_funds(&contract_id, &total_milestone_amount());
    let _ = client.dispute(&contract_id);

    let (resources, fee_total) = measure_last_invocation(&env);
    assert_within_baseline("dispute", resources, fee_total, DISPUTE_BASELINE);
}
