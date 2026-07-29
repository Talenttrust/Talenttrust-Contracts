import re

with open('contracts/escrow/src/lib.rs', 'r') as f:
    content = f.read()

funcs_to_delete = [
    'pub fn create_contract(',
    'pub fn set_max_milestones(',
    'pub fn get_max_milestones(',
    'pub fn propose_governance_admin(',
    'pub fn accept_governance_admin('
]

for func in funcs_to_delete:
    while True:
        start_idx = content.find(func)
        if start_idx == -1:
            break
        
        # We need to find the start of the documentation for this function
        # Since 'pub fn' is preceded by whitespace and maybe doc comments,
        # let's just search backwards for '    ///' or just find the closing brace.
        
        brace_start = content.find('{', start_idx)
        depth = 1
        i = brace_start + 1
        while depth > 0 and i < len(content):
            if content[i] == '{':
                depth += 1
            elif content[i] == '}':
                depth -= 1
            i += 1
        end_idx = i
        
        # delete the function
        content = content[:start_idx] + content[end_idx:]

with open('contracts/escrow/src/lib.rs', 'w') as f:
    f.write(content)
