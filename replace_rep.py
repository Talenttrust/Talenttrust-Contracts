import re

with open('contracts/escrow/src/lib.rs', 'r') as f:
    content = f.read()

# Add `mod reputation;`
content = content.replace('mod dispute;', 'mod dispute;\nmod reputation;')

replacements = [
    (
        r'pub\(crate\) fn grant_pending_reputation_credit\(env: &Env, freelancer: &Address\) \{[\s\S]*?\}',
        'pub(crate) fn grant_pending_reputation_credit(env: &Env, freelancer: &Address) {\n        reputation::grant_pending_reputation_credit(env, freelancer);\n    }'
    ),
    (
        r'pub fn get_reputation_config\(env: Env\) -> ReputationConfig \{[\s\S]*?\}',
        'pub fn get_reputation_config(env: Env) -> ReputationConfig {\n        reputation::get_reputation_config(&env)\n    }'
    ),
    (
        r'pub fn set_reputation_config\([\s\S]*?max_comment_bytes: u32,[\s\S]*?\) -> bool \{[\s\S]*?\}',
        'pub fn set_reputation_config(\n        env: Env,\n        min_rating: u32,\n        max_rating: u32,\n        max_comment_bytes: u32,\n    ) -> bool {\n        reputation::set_reputation_config(&env, min_rating, max_rating, max_comment_bytes)\n    }'
    ),
    (
        r'pub fn reset_reputation_config\(env: Env\) -> bool \{[\s\S]*?\}',
        'pub fn reset_reputation_config(env: Env) -> bool {\n        reputation::reset_reputation_config(&env)\n    }'
    ),
    (
        r'pub fn issue_reputation\([\s\S]*?comment: String,[\s\S]*?\) -> bool \{[\s\S]*?\}',
        'pub fn issue_reputation(\n        env: Env,\n        contract_id: u32,\n        caller: Address,\n        rating: u32,\n        comment: String,\n    ) -> bool {\n        reputation::issue_reputation(&env, contract_id, caller, rating, comment)\n    }'
    ),
    (
        r'pub fn get_reputation_comment\(env: Env, contract_id: u32\) -> Option<String> \{[\s\S]*?\}',
        'pub fn get_reputation_comment(env: Env, contract_id: u32) -> Option<String> {\n        reputation::get_reputation_comment(&env, contract_id)\n    }'
    ),
    (
        r'pub fn get_reputation\(env: Env, address: Address\) -> Option<types::Reputation> \{[\s\S]*?\}',
        'pub fn get_reputation(env: Env, address: Address) -> Option<types::Reputation> {\n        reputation::get_reputation(&env, address)\n    }'
    ),
    (
        r'pub fn get_average_rating\(env: Env, address: Address\) -> Option<i128> \{[\s\S]*?\}',
        'pub fn get_average_rating(env: Env, address: Address) -> Option<i128> {\n        reputation::get_average_rating(&env, address)\n    }'
    ),
    (
        r'pub fn get_pending_reputation_credits\(env: Env, address: Address\) -> i128 \{[\s\S]*?\}',
        'pub fn get_pending_reputation_credits(env: Env, address: Address) -> i128 {\n        reputation::get_pending_reputation_credits(&env, address)\n    }'
    ),
    (
        r'pub fn get_reputations_page\(env: Env, start: u32, limit: u32\) -> Vec<types::ReputationEntry> \{[\s\S]*?\}',
        'pub fn get_reputations_page(env: Env, start: u32, limit: u32) -> Vec<types::ReputationEntry> {\n        reputation::get_reputations_page(&env, start, limit)\n    }'
    )
]

for regex, replacement in replacements:
    content, count = re.subn(regex, replacement, content)
    if count == 0:
        print(f"Failed to match: {regex[:30]}...")

with open('contracts/escrow/src/lib.rs', 'w') as f:
    f.write(content)
