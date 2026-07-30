import re

# fix deposit.rs
with open('contracts/escrow/src/deposit.rs', 'r') as f:
    content = f.read()
content = content.replace('storage_validation::validate_stroop_amount', 'crate::storage_validation::validate_stroop_amount')
content = content.replace('MAX_SINGLE_AMOUNT_STROOPS', 'crate::MAX_SINGLE_AMOUNT_STROOPS')
with open('contracts/escrow/src/deposit.rs', 'w') as f:
    f.write(content)

# fix events.rs
with open('contracts/escrow/src/events.rs', 'r') as f:
    content = f.read()
if 'soroban_sdk::Address' not in content:
    content = content.replace('use soroban_sdk::{Env, Symbol};', 'use soroban_sdk::{Env, Symbol, Address};')
content = content.replace('ContractStatus,', 'crate::types::ContractStatus,')
with open('contracts/escrow/src/events.rs', 'w') as f:
    f.write(content)

# fix finalize.rs
with open('contracts/escrow/src/finalize.rs', 'r') as f:
    content = f.read()
content = content.replace('keys::milestone_key', 'crate::keys::milestone_key')
with open('contracts/escrow/src/finalize.rs', 'w') as f:
    f.write(content)

# fix contracts.rs
with open('contracts/escrow/src/contracts.rs', 'r') as f:
    content = f.read()
content = content.replace('crate::ContractBounds', 'crate::types::ContractBounds')
with open('contracts/escrow/src/contracts.rs', 'w') as f:
    f.write(content)
    
# fix create_contract.rs
with open('contracts/escrow/src/create_contract.rs', 'r') as f:
    content = f.read()
content = content.replace('Symbol::new', 'soroban_sdk::Symbol::new')
with open('contracts/escrow/src/create_contract.rs', 'w') as f:
    f.write(content)

