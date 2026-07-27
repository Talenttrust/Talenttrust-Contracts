import re, glob

files = glob.glob(r'contracts\escrow\src\test\*.rs')
pattern = re.compile(
    r'(try_issue_reputation|issue_reputation)\(&(\w+),\s*&(\w+),\s*&\w+,\s*&\w+\)'
)
replacement = r'\1(&\2, &\3, &5_u32, &soroban_sdk::String::from_str(&env, "Great"))'

for path in files:
    with open(path, encoding='utf-8') as f:
        txt = f.read()
    changed = pattern.sub(replacement, txt)
    if changed != txt:
        with open(path, 'w', encoding='utf-8') as f:
            f.write(changed)
        print('fixed', path)
