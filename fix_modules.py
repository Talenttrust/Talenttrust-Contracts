import re

with open('contracts/escrow/src/lib.rs', 'r') as f:
    content = f.read()

# I will add the module declarations at the top where other modules are
mods = """mod contracts;
mod create_contract;
mod dispute;
mod governance;
"""
content = content.replace('pub mod milestones_consts;\n', 'pub mod milestones_consts;\n' + mods)

with open('contracts/escrow/src/lib.rs', 'w') as f:
    f.write(content)
