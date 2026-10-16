import json
from collections import Counter
from datetime import datetime

with open('all_prs.json', 'r', encoding='utf-8') as f:
    raw_prs = json.load(f)

# File frequency to detect hotspots
file_freq = Counter()
for p in raw_prs:
    for f in p.get('files', []):
        file_freq[f['path']] += 1

HOTSPOT_THRESHOLD = 15
hotspots = [{'path': path, 'count': cnt} for path, cnt in file_freq.most_common() if cnt >= HOTSPOT_THRESHOLD]
hotspot_paths = set(h['path'] for h in hotspots)

def classify_pr(paths, title):
    p_lower = [p.lower() for p in paths]
    t_lower = title.lower()
    
    # Check AGC
    if any('agc' in p or 'vulkandevice' in p for p in p_lower) or 'agc' in t_lower:
        return 'AGC & Graphics'
    # Shader / Relinker
    if any('shader' in p or 'relinker' in p for p in p_lower) or 'shader' in t_lower or 'relinker' in t_lower:
        return 'Shader & Relinker'
    # Kernel
    if any('libkernel' in p or 'kqueue' in p or 'directmemory' in p or 'virtualmemory' in p or 'vm.cpp' in p for p in p_lower) or 'kernel' in t_lower:
        return 'Kernel & Syscalls'
    # LibC
    if any('/libc/' in p or '/libc.' in p or 'libscelibcinternal' in p or '/libm/' in p or 'libunwind' in p for p in p_lower) or 'libc' in t_lower:
        return 'LibC & Base Runtime'
    # High-level PRX
    if any('core/libs/prx/' in p for p in p_lower) or any(t_lower.startswith(x) for x in ['feat(libsce', 'fix(libsce']):
        return 'PRX High-Level Modules'
    # Build & Tooling
    if any('tools/' in p or '.github/' in p or p.endswith('cmakelists.txt') for p in p_lower):
        return 'Build & Tooling'
    # Docs
    if all('docs/' in p for p in p_lower) or t_lower.startswith('docs'):
        return 'Documentation & Debt'
    return 'Other Core Subsystems'

prs = []
for p in raw_prs:
    paths = [f['path'] for f in p.get('files', [])]
    adds = p.get('additions', 0)
    dels = p.get('deletions', 0)
    tot = adds + dels
    
    pr_hotspots = [path for path in paths if path in hotspot_paths]
    cat = classify_pr(paths, p.get('title', ''))
    
    if len(pr_hotspots) > 0 or len(paths) > 10 or tot > 1500:
        risk = 'high'
    elif len(paths) >= 4 or tot > 300:
        risk = 'medium'
    else:
        risk = 'low'

    # Size bucket
    if tot < 25:
        size = 'XS'
    elif tot <= 100:
        size = 'S'
    elif tot <= 400:
        size = 'M'
    elif tot <= 1200:
        size = 'L'
    else:
        size = 'XL'

    author_obj = p.get('author') or {}
    author_login = author_obj.get('login', 'ghost')
    author_avatar = f'https://github.com/{author_login}.png?size=64'
    pr_num = p['number']
    pr_url = p.get('url') or f'https://github.com/boykopovar/AnyPS5/pull/{pr_num}'

    prs.append({
        'number': pr_num,
        'title': p.get('title', ''),
        'author': author_login,
        'authorAvatar': author_avatar,
        'createdAt': p.get('createdAt', ''),
        'url': pr_url,
        'additions': adds,
        'deletions': dels,
        'totalLines': tot,
        'fileCount': len(paths),
        'files': paths,
        'category': cat,
        'hotspots': pr_hotspots,
        'riskLevel': risk,
        'size': size,
        'isDraft': p.get('isDraft', False)
    })

dataset = {
    'repo': 'boykopovar/AnyPS5',
    'fetchedAt': datetime.now().isoformat(),
    'totalPRs': len(prs),
    'hotspots': hotspots,
    'prs': prs
}

with open('prs_data.js', 'w', encoding='utf-8') as f:
    f.write('window.ANYPS5_DATA = ' + json.dumps(dataset, ensure_ascii=False) + ';')

print(f'Successfully wrote prs_data.js with {len(prs)} PRs and {len(hotspots)} hotspots!')
