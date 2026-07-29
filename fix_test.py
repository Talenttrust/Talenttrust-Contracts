import re

filepath = 'contracts/escrow/src/test/reputation_config_setter.rs'
with open(filepath, 'r') as f:
    content = f.read()

# Add imports
content = content.replace('use crate::{Escrow, EscrowClient};', 'use crate::{Escrow, EscrowClient, Error, types::ReputationConfig};')

# Add setup function
setup_fn = """fn setup(env: &Env) -> (EscrowClient<'_>, Address) {
    let escrow_address = env.register(Escrow, ());
    let client = EscrowClient::new(env, &escrow_address);
    let admin = Address::generate(env);
    env.mock_all_auths();
    client.initialize(&admin);
    (client, admin)
}

"""

if 'fn setup(' not in content:
    content = content.replace('#[test]\nfn test_reputation_config_setter', setup_fn + '#[test]\nfn test_reputation_config_setter')

with open(filepath, 'w') as f:
    f.write(content)
