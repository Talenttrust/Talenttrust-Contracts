import re

# Fix lib.rs line 108 MAX_SINGLE_AMOUNT_STROOPS
with open('contracts/escrow/src/lib.rs', 'r') as f:
    lib_content = f.read()
lib_content = lib_content.replace('pub const MAX_SINGLE_AMOUNT_STROOPS: i128 = crate::amount_validation::MAX_SINGLE_AMOUNT_STROOPS;\n', '')
with open('contracts/escrow/src/lib.rs', 'w') as f:
    f.write(lib_content)

# Fix types.rs line 96 MaxMilestones
with open('contracts/escrow/src/types.rs', 'r') as f:
    types_content = f.read()
# Replace the second occurrence of "MaxMilestones,"
types_content = types_content.replace('    MaxMilestones,\n', '', 1)
# Wait, let's just delete the exact line if we can. Actually replacing the first one is fine if they are identical!
with open('contracts/escrow/src/types.rs', 'w') as f:
    f.write(types_content)

