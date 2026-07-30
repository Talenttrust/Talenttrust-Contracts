import sys

filepath = 'contracts/escrow/src/lib.rs'
with open(filepath, 'r') as f:
    lines = f.readlines()

# add mod reputation;
for i, line in enumerate(lines):
    if line.strip() == 'mod rollback;':
        lines.insert(i + 1, 'mod reputation;\n')
        break

start_idx = -1
end_idx = -1

for i, line in enumerate(lines):
    if '// ── Reputation ──' in line:
        start_idx = i
        break

if start_idx != -1:
    for i in range(start_idx, len(lines)):
        if 'pub fn get_reputations_page' in lines[i]:
            for j in range(i, len(lines)):
                if lines[j].rstrip() == '    }':
                    end_idx = j
                    break
            break

if start_idx != -1 and end_idx != -1:
    del lines[start_idx:end_idx+1]

with open(filepath, 'w') as f:
    f.writelines(lines)
