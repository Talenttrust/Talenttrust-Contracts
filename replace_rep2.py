import os

with open('contracts/escrow/src/lib.rs', 'r') as f:
    content = f.read()

# Add `mod reputation;`
content = content.replace('mod dispute;', 'mod dispute;\nmod reputation;')

funcs_to_replace = {
    'pub(crate) fn grant_pending_reputation_credit(env: &Env, freelancer: &Address)': '        reputation::grant_pending_reputation_credit(env, freelancer);',
    'pub fn get_reputation_config(env: Env) -> ReputationConfig': '        reputation::get_reputation_config(&env)',
    'pub fn set_reputation_config(\n        env: Env,\n        min_rating: u32,\n        max_rating: u32,\n        max_comment_bytes: u32,\n    ) -> bool': '        reputation::set_reputation_config(&env, min_rating, max_rating, max_comment_bytes)',
    'pub fn reset_reputation_config(env: Env) -> bool': '        reputation::reset_reputation_config(&env)',
    'pub fn issue_reputation(\n        env: Env,\n        contract_id: u32,\n        caller: Address,\n        rating: u32,\n        comment: String,\n    ) -> bool': '        reputation::issue_reputation(&env, contract_id, caller, rating, comment)',
    'pub fn get_reputation_comment(env: Env, contract_id: u32) -> Option<String>': '        reputation::get_reputation_comment(&env, contract_id)',
    'pub fn get_reputation(env: Env, address: Address) -> Option<types::Reputation>': '        reputation::get_reputation(&env, address)',
    'pub fn get_average_rating(env: Env, address: Address) -> Option<i128>': '        reputation::get_average_rating(&env, address)',
    'pub fn get_pending_reputation_credits(env: Env, address: Address) -> i128': '        reputation::get_pending_reputation_credits(&env, address)',
    'pub fn get_reputations_page(env: Env, start: u32, limit: u32) -> Vec<types::ReputationEntry>': '        reputation::get_reputations_page(&env, start, limit)'
}

for sig, new_body in funcs_to_replace.items():
    start_idx = content.find(sig)
    if start_idx == -1:
        print(f"Failed to find signature:\n{sig}")
        continue
    
    # find the next '{'
    brace_idx = content.find('{', start_idx)
    
    # parse until matching '}'
    depth = 1
    i = brace_idx + 1
    while depth > 0 and i < len(content):
        if content[i] == '{':
            depth += 1
        elif content[i] == '}':
            depth -= 1
        i += 1
    
    end_idx = i - 1
    
    content = content[:brace_idx + 1] + '\n' + new_body + '\n    ' + content[end_idx:]

with open('contracts/escrow/src/lib.rs', 'w') as f:
    f.write(content)

