"""Generate batches without changing existing claimed memberships."""
import hashlib
from collections import defaultdict
from backend_api import read_all, request

TIER_ORDER = {'low': 0, 'medium': 1, 'high': 2}


def is_boilerplate_file(path):
    p = str(path).lower().replace('\\', '/')
    return p.endswith('cmakelists.txt') or 'docs/dev/debt/' in p


def normalize_files(pr):
    res = []
    for f in pr.get('files') or []:
        path = f.get('path', '') if isinstance(f, dict) else str(f)
        if path:
            res.append(path)
    return sorted(set(res))


def effective_code_files(pr):
    all_f = normalize_files(pr)
    code_f = [f for f in all_f if not is_boilerplate_file(f)]
    return code_f if code_f else all_f


def compute_complexity(p):
    tot = p.get("total_lines", 0)
    all_f = normalize_files(p)
    code_f = [f for f in all_f if not is_boilerplate_file(f)]
    fc = len(code_f) if code_f else p.get("file_count", len(all_f) or 1)
    risk = p.get("risk_level", "low")
    hotspots = p.get("hotspots")
    if isinstance(hotspots, list):
        core_hs = [
            h for h in hotspots
            if not is_boilerplate_file(h) and '/tests/' not in str(h).lower()
        ]
        if not core_hs and risk == "high" and fc <= 10 and tot <= 1500:
            risk = "medium" if (fc >= 4 or tot > 300 or hotspots) else "low"

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


def _assign_batch_metadata(members, category, tier, label, batch_type, known_names, next_number):
    numbers = sorted(p['number'] for p in members)
    digest = hashlib.sha256(','.join(map(str, numbers)).encode()).hexdigest()[:24]
    batch_id = f'batch-{digest}'
    title = known_names.get(batch_id)
    if title is None:
        title = f'Batch {next_number}'
        next_number += 1
    batch = {
        'id': batch_id,
        'title': title,
        'category': category,
        'complexity': tier,
        'complexity_label': label,
        'pr_count': len(numbers),
        'pr_numbers': numbers,
        'status': 'available',
        'claimed_by': 'none',
        'batch_type': batch_type
    }
    return batch, next_number


def generate_batches(prs, existing, names=None, *, smart_mode=False):
    protected = [b for b in existing if b.get('status') != 'available'
                 or b.get('claimed_user_id') or b.get('claimed_by') not in (None, 'none')]
    reserved = {n for b in protected for n in b['pr_numbers']}
    pr_by_num = {p['number']: p for p in prs}

    # Collect active code files locked by claimed batches so available batches never fail claim_batch.
    reserved_code_files = set()
    for b in protected:
        for n in b['pr_numbers']:
            p = pr_by_num.get(n)
            if p and p.get('github_state') not in ('closed', 'merged'):
                reserved_code_files.update(effective_code_files(p))

    known_names = {b['id']: b['title'] for b in existing if str(b.get('title', '')).startswith('Batch ')}
    known_names.update({row['batch_id']: f"Batch {row['batch_number']}" for row in (names or [])})
    next_number = max((int(title[6:]) for title in known_names.values() if title[6:].isdigit()), default=0) + 1

    eligible = []
    for pr in prs:
        if (pr['number'] not in reserved and pr.get('github_state') == 'open'
                and not pr.get('is_draft') and normalize_files(pr)
                and not pr.get('assigned_user_id')
                and pr.get('assigned_reviewer') in (None, 'none')):
            if reserved_code_files.intersection(effective_code_files(pr)):
                continue
            eligible.append(pr)

    if not smart_mode:
        groups = defaultdict(list)
        for pr in eligible:
            groups[pr.get('category') or 'Other Core Subsystems'].append(pr)
        result = []
        for category, pool in sorted(groups.items()):
            pool.sort(key=lambda p: (TIER_ORDER[compute_complexity(p)[0]], p.get('total_lines', 0), p['number']))
            while pool:
                lead = pool.pop(0)
                tier, limit, label = compute_complexity(lead)
                members = [lead]
                paths = set(normalize_files(lead))
                for candidate in list(pool):
                    if len(members) >= limit:
                        break
                    if compute_complexity(candidate)[0] == tier and not paths.intersection(normalize_files(candidate)):
                        members.append(candidate)
                        paths.update(normalize_files(candidate))
                        pool.remove(candidate)
                batch, next_number = _assign_batch_metadata(
                    members, category, tier, label, 'disjoint', known_names, next_number)
                result.append(batch)
        return result

    result = []

    # Lane 1: Cohesive Module Clusters (2+ PRs targeting the exact same 1-4 core code files)
    by_cat_and_code = defaultdict(list)
    for pr in eligible:
        cat = pr.get('category') or 'Other Core Subsystems'
        cf = tuple(effective_code_files(pr))
        by_cat_and_code[(cat, cf)].append(pr)

    remaining_by_cat = defaultdict(list)
    for (category, cf), group in sorted(by_cat_and_code.items()):
        if len(group) >= 2 and 1 <= len(cf) <= 4:
            group.sort(key=lambda p: (p.get('total_lines', 0), p['number']))
            for idx in range(0, len(group), 10):
                chunk = group[idx:idx + 10]
                if len(chunk) >= 2:
                    target_name = cf[0].split('/')[-1]
                    if len(cf) > 1:
                        target_name += f' +{len(cf) - 1}'
                    label = f'🔵 Module Cluster ({target_name})'
                    batch, next_number = _assign_batch_metadata(
                        chunk, category, 'medium', label, 'cohesive_cluster', known_names, next_number)
                    result.append(batch)
                else:
                    remaining_by_cat[category].extend(chunk)
        else:
            remaining_by_cat[category].extend(group)

    # Lane 2: Fast-Track Strict 100% File-Disjoint Trains (with low+medium pairing)
    leftover_by_cat = defaultdict(list)
    for category, pool in sorted(remaining_by_cat.items()):
        pool.sort(key=lambda p: (TIER_ORDER[compute_complexity(p)[0]], p.get('total_lines', 0), p['number']))
        while pool:
            lead = pool.pop(0)
            tier, limit, label = compute_complexity(lead)
            if tier == 'high':
                limit = max(limit, 3)
            elif tier == 'medium':
                limit = max(limit, 6)
            members = [lead]
            paths = set(normalize_files(lead))
            for candidate in list(pool):
                if len(members) >= limit:
                    break
                cand_tier = compute_complexity(candidate)[0]
                tier_compatible = (cand_tier == tier) or ({tier, cand_tier} <= {'low', 'medium'})
                if tier_compatible and not paths.intersection(normalize_files(candidate)):
                    members.append(candidate)
                    paths.update(normalize_files(candidate))
                    pool.remove(candidate)
            if len(members) >= 2:
                batch, next_number = _assign_batch_metadata(
                    members, category, tier, label, 'disjoint', known_names, next_number)
                result.append(batch)
            else:
                leftover_by_cat[category].append(lead)

    # Lane 3: Boilerplate-Tolerant Code-Disjoint Trains (only CMakeLists.txt / docs/dev/debt/*.md overlap)
    for category, pool in sorted(leftover_by_cat.items()):
        pool.sort(key=lambda p: (TIER_ORDER[compute_complexity(p)[0]], p.get('total_lines', 0), p['number']))
        while pool:
            lead = pool.pop(0)
            tier, limit, _ = compute_complexity(lead)
            limit = min(max(limit, 4), 8)
            members = [lead]
            c_paths = set(effective_code_files(lead))
            for candidate in list(pool):
                if len(members) >= limit:
                    break
                if not c_paths.intersection(effective_code_files(candidate)):
                    members.append(candidate)
                    c_paths.update(effective_code_files(candidate))
                    pool.remove(candidate)
            if len(members) >= 2:
                label = '🟢 Auto-Rebaseable (CMake/Debt Only)'
                batch, next_number = _assign_batch_metadata(
                    members, category, tier, label, 'soft_disjoint', known_names, next_number)
                result.append(batch)

    return result


def generate_and_upload():
    prs = read_all('prs?select=*&order=number.asc')
    existing = read_all('batches?select=*&order=id.asc')
    names = read_all('batch_names?select=*&order=batch_number.asc')
    batches = generate_batches(prs, existing, names, smart_mode=True)
    # Server verifies the full snapshots under the same lock used by claims.
    request('rpc/replace_available_batches', {'p_pr_snapshot': prs,
            'p_batch_snapshot': existing, 'p_batches': batches})
    print(f'Published {len(batches)} available batches; existing claims preserved.')


if __name__ == '__main__':
    generate_and_upload()
