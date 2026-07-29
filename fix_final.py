import re

# fix events.rs
with open('contracts/escrow/src/events.rs', 'r') as f:
    events_content = f.read()
events_content = events_content.replace('pub use crate::types::MilestoneIndexEvent;\n', '')
with open('contracts/escrow/src/events.rs', 'w') as f:
    f.write(events_content)

# fix lib.rs
with open('contracts/escrow/src/lib.rs', 'r') as f:
    lib_content = f.read()
lib_content = lib_content.replace('pub use types::DISPUTE_STORAGE_VERSION;\n', '')
# rename get_pending_governance_admin_proposed_at
lib_content = lib_content.replace('get_pending_governance_admin_proposed_at', 'pending_gov_admin_proposed_at')
with open('contracts/escrow/src/lib.rs', 'w') as f:
    f.write(lib_content)
    
# fix tests imports
for test_file in ['create_contract_bounds.rs', 'dispute.rs', 'simulate_create_contract.rs', 'simulate_release.rs']:
    filepath = f'contracts/escrow/src/test/{test_file}'
    with open(filepath, 'r') as f:
        content = f.read()
    content = content.replace(' ContractBounds,', ' types::ContractBounds,')
    content = content.replace(' SimulateDisputeOutcome,', ' types::SimulateDisputeOutcome,')
    content = content.replace(' SimulateCreateContractOutcome', ' types::SimulateCreateContractOutcome')
    content = content.replace(' SimulatedRelease', ' types::SimulatedRelease')
    with open(filepath, 'w') as f:
        f.write(content)
