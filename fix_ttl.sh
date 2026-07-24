#!/bin/bash
find contracts/escrow/src/test -name "*.rs" -type f -exec sed -i 's/let env = Env::default();/let env = Env::default();\n    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });/g' {} +
