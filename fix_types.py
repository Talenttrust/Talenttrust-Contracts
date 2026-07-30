import re

with open('contracts/escrow/src/types.rs', 'r') as f:
    types = f.read()

dispute_structs = """
pub const DISPUTE_STORAGE_VERSION: u32 = 1;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeMetadataV0 {
    pub contract_id: u32,
    pub arbiter: soroban_sdk::Address,
    pub schema_version: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeMetadata {
    pub contract_id: u32,
    pub arbiter: soroban_sdk::Address,
    pub schema_version: u32,
    pub timestamp: u64,
}
"""

types = types.replace('pub struct DisputeConfig {', dispute_structs + '\npub struct DisputeConfig {')

with open('contracts/escrow/src/types.rs', 'w') as f:
    f.write(types)

