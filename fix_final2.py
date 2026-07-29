import re

# fix performance.rs
with open('contracts/escrow/src/test/performance.rs', 'r') as f:
    perf = f.read()
if 'use soroban_sdk::{vec, Env}' not in perf and 'use soroban_sdk::Env' not in perf:
    perf = perf.replace('use soroban_sdk::{vec};', 'use soroban_sdk::{vec, Env};')
    perf = perf.replace('use soroban_sdk::vec;', 'use soroban_sdk::{vec, Env};')
with open('contracts/escrow/src/test/performance.rs', 'w') as f:
    f.write(perf)

# fix reputation.rs
with open('contracts/escrow/src/test/reputation.rs', 'r') as f:
    rep = f.read()
rep = rep.replace('create_contract(&env', 'crate::test::create_contract(&env')
with open('contracts/escrow/src/test/reputation.rs', 'w') as f:
    f.write(rep)

# fix access_control.rs
with open('contracts/escrow/src/test/access_control.rs', 'r') as f:
    ac = f.read()
ac = ac.replace('super::super::assert_contract_error', 'crate::test::assert_contract_error')
with open('contracts/escrow/src/test/access_control.rs', 'w') as f:
    f.write(ac)

