"""Generate batches without changing existing claimed memberships."""
import hashlib
from collections import defaultdict
from backend_api import read_all, request


def compute_complexity(p):
    tot = p.get("total_lines", 0)
    fc = p.get("file_count", 1)
    risk = p.get("risk_level", "low")
    
    score = 1
    if tot > 1000: score += 4
    elif tot > 300: score += 2
    elif tot > 100: score += 1
    
    if fc > 10: score += 3
    elif fc > 4: score += 1
    
    if risk == "high": score += 2
    elif risk == "medium": score += 1
    
    if score <= 2: return "low", 10, "🟢 Easy (up to 10 PRs)"
    elif score <= 5: return "medium", 5, "🟡 Medium (up to 5 PRs)"
    else: return "high", 2, "🔴 Complex (1-2 PRs)"

def generate_batches(prs, existing):
    protected = [b for b in existing if b.get('status') != 'available'
                 or b.get('claimed_user_id') or b.get('claimed_by') not in (None, 'none')]
    reserved = {n for b in protected for n in b['pr_numbers']}
    groups = defaultdict(list)
    for pr in prs:
        if (pr['number'] not in reserved and pr.get('github_state') == 'open'
                and not pr.get('is_draft') and pr.get('files')
                and not pr.get('assigned_user_id')
                and pr.get('assigned_reviewer') in (None, 'none')):
            groups[pr.get('category') or 'Other Core Subsystems'].append(pr)
    result = []
    for category, pool in sorted(groups.items()):
        pool.sort(key=lambda p: (compute_complexity(p)[0], p.get('total_lines', 0), p['number']))
        while pool:
            lead = pool.pop(0)
            tier, limit, label = compute_complexity(lead)
            members = [lead]
            paths = set(lead['files'])
            for candidate in list(pool):
                if len(members) >= limit:
                    break
                if compute_complexity(candidate)[0] == tier and not paths.intersection(candidate['files']):
                    members.append(candidate)
                    paths.update(candidate['files'])
                    pool.remove(candidate)
            numbers = sorted(p['number'] for p in members)
            # IDs depend on membership, never on a changing category counter.
            digest = hashlib.sha256(','.join(map(str, numbers)).encode()).hexdigest()[:24]
            result.append({'id': f'batch-{digest}', 'title': f'{category} • {numbers[0]}',
                           'category': category, 'complexity': tier, 'complexity_label': label,
                           'pr_count': len(numbers), 'pr_numbers': numbers,
                           'status': 'available', 'claimed_by': 'none'})
    return result


def generate_and_upload():
    prs = read_all('prs?select=*&order=number.asc')
    existing = read_all('batches?select=*&order=id.asc')
    batches = generate_batches(prs, existing)
    # Server verifies the full snapshots under the same lock used by claims.
    request('rpc/replace_available_batches', {'p_pr_snapshot': prs,
            'p_batch_snapshot': existing, 'p_batches': batches})
    print(f'Published {len(batches)} available batches; existing claims preserved.')


if __name__ == '__main__':
    generate_and_upload()
