import re

filepath = 'contracts/escrow/src/test/reputation.rs'
with open(filepath, 'r') as f:
    content = f.read()

content = content.replace('complete_contract(', 'complete_contract_for(')
content = content.replace('let client = register_client(&env);', 'let client = register_client_with_token(&env, &token);')

with open(filepath, 'w') as f:
    f.write(content)
