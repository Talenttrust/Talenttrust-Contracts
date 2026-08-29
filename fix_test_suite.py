import re

# 1. Remove mod contracts;
with open('contracts/escrow/src/lib.rs', 'r') as f:
    lib = f.read()
lib = lib.replace('mod contracts;\n', '')
with open('contracts/escrow/src/lib.rs', 'w') as f:
    f.write(lib)

# 2. Fix DisputeInfo in test/dispute.rs
with open('contracts/escrow/src/test/dispute.rs', 'r') as f:
    dispute = f.read()
dispute = dispute.replace('DisputeInfo', 'crate::types::DisputeSummary')
with open('contracts/escrow/src/test/dispute.rs', 'w') as f:
    f.write(dispute)

# 3. Fix DISPUTE_STORAGE_VERSION in test/disputes_page.rs
with open('contracts/escrow/src/test/disputes_page.rs', 'r') as f:
    disputes_page = f.read()
disputes_page = disputes_page.replace('crate::DISPUTE_STORAGE_VERSION', 'crate::types::CONTRACT_SUMMARY_SCHEMA_VERSION')
with open('contracts/escrow/src/test/disputes_page.rs', 'w') as f:
    f.write(disputes_page)

# 4. Fix setup_completed_contract in test/pause_controls.rs
with open('contracts/escrow/src/test/pause_controls.rs', 'r') as f:
    pause_controls = f.read()
# Replace setup_completed_contract with complete_contract
# Wait, if complete_contract doesn't return exactly what setup_completed_contract does... let's check
pause_controls = pause_controls.replace('setup_completed_contract(', 'crate::test::complete_contract(')
pause_controls = pause_controls.replace('EscrowError::ContractPaused', 'crate::EscrowError::ContractPaused')
with open('contracts/escrow/src/test/pause_controls.rs', 'w') as f:
    f.write(pause_controls)

# 5. Fix Env in test/performance.rs
with open('contracts/escrow/src/test/performance.rs', 'r') as f:
    perf = f.read()
if 'soroban_sdk::Env' not in perf:
    perf = perf.replace('soroban_sdk::{vec}', 'soroban_sdk::{vec, Env}')
with open('contracts/escrow/src/test/performance.rs', 'w') as f:
    f.write(perf)

# 6. Fix EscrowError and register_client in test/reputation.rs
with open('contracts/escrow/src/test/reputation.rs', 'r') as f:
    rep = f.read()
if 'crate::EscrowError' not in rep:
    rep = rep.replace('use crate::{', 'use crate::{EscrowError, ')
rep = rep.replace('let client = register_client(&env);', 'let client = crate::test::register_client(&env);')
with open('contracts/escrow/src/test/reputation.rs', 'w') as f:
    f.write(rep)

