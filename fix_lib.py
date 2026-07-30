import re
with open('contracts/escrow/src/lib.rs', 'r') as f:
    lib = f.read()
lib = lib.replace('mod create_contract;\nmod dispute;\nmod governance;\n', '')
with open('contracts/escrow/src/lib.rs', 'w') as f:
    f.write(lib)
