import re

with open('contracts/escrow/src/lib.rs', 'r') as f:
    content = f.read()

# Find all 'pub fn' definitions in impl Escrow
# We'll use a regex to capture them.

def extract_funcs(content):
    funcs = []
    # match pub fn name(
    pattern = re.compile(r'(pub fn ([a-zA-Z0-9_]+)\s*\()')
    for m in pattern.finditer(content):
        start = m.start(1)
        name = m.group(2)
        # find matching brace
        brace_start = content.find('{', start)
        if brace_start == -1:
            continue
        depth = 1
        i = brace_start + 1
        while depth > 0 and i < len(content):
            if content[i] == '{':
                depth += 1
            elif content[i] == '}':
                depth -= 1
            i += 1
        end = i
        funcs.append((name, start, end))
    return funcs

funcs = extract_funcs(content)
seen = set()
to_delete = []

for name, start, end in funcs:
    if name in seen:
        print(f"Duplicate found: {name} at {start}")
        to_delete.append((start, end))
    else:
        seen.add(name)

# Delete from back to front
for start, end in reversed(to_delete):
    content = content[:start] + content[end:]

with open('contracts/escrow/src/lib.rs', 'w') as f:
    f.write(content)
