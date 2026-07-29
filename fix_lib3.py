import re
with open('contracts/escrow/src/lib.rs', 'r') as f:
    lib = f.read()
# First remove any rogue mod create_contract
lib = re.sub(r'mod create_contract;\n?', '', lib)
lib = re.sub(r'mod dispute;\n?', '', lib)
lib = re.sub(r'mod governance;\n?', '', lib)
# Add them back after mod utils;
lib = lib.replace('mod utils;\n', 'mod utils;\nmod create_contract;\nmod dispute;\nmod governance;\n')

# replace DisputeMetadata with crate::types::DisputeSummary? 
# Wait, maybe they are different. Let's see if DisputeMetadata is in types.rs
