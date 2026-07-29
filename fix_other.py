import re

# fix simulate.rs
with open('contracts/escrow/src/simulate.rs', 'r') as f:
    content = f.read()
content = content.replace('Error::AlreadyReleased as u32', 'Error::AlreadyRefunded as u32')
with open('contracts/escrow/src/simulate.rs', 'w') as f:
    f.write(content)

# fix reputation.rs
with open('contracts/escrow/src/test/reputation.rs', 'r') as f:
    content = f.read()
content = content.replace('use super::{complete_contract_funded, register_client_with_token, total_milestones_amount};', 'use super::{complete_contract_funded, register_client_with_token, total_milestones_amount, complete_contract, register_client};')
with open('contracts/escrow/src/test/reputation.rs', 'w') as f:
    f.write(content)
