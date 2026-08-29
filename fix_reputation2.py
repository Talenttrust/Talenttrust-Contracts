import re

with open('contracts/escrow/src/test/reputation.rs', 'r') as f:
    content = f.read()

# Replace complete_contract( with complete_contract_for( but only where it's a function call.
content = content.replace('complete_contract(&env', 'complete_contract_for(&env')

with open('contracts/escrow/src/test/reputation.rs', 'w') as f:
    f.write(content)
