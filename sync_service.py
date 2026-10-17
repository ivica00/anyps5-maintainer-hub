#!/usr/bin/env python3
"""
AnyPS5 Supabase Sync Service
Synchronizes open, merged, and closed PRs from GitHub directly into Supabase using the service_role key.
"""

import argparse
import json
import subprocess
from collections import Counter
from concurrent.futures import ThreadPoolExecutor

from backend_api import read_all, request

REPOSITORY = 'boykopovar/AnyPS5'


def is_boilerplate_file(path):
    p = str(path).lower().replace('\\', '/')
    return p.endswith('cmakelists.txt') or 'docs/dev/debt/' in p


def is_test_or_boilerplate_hotspot(path):
    p = str(path).lower().replace('\\', '/')
    return is_boilerplate_file(p) or '/tests/' in p or p.startswith('tests/')


def github_pages(endpoint):
    result = subprocess.run(
        ['gh', 'api', '--paginate', '--slurp', endpoint],
        capture_output=True, text=True, encoding='utf-8', timeout=600)
    if result.returncode:
        raise RuntimeError('GitHub fetch failed; database was not changed')
    pages = json.loads(result.stdout)
    if not isinstance(pages, list) or any(not isinstance(page, list) for page in pages):
        raise ValueError('Invalid GitHub pagination response')
    return [row for page in pages for row in page]


def fetch_github_prs(existing_map=None):
    # All states are explicit: never infer closure from absence in a partial list.
    pulls = github_pages(f'repos/{REPOSITORY}/pulls?state=all&per_page=100&sort=created&direction=asc')
    states = [{'number': p['number'], 'github_state':
               'merged' if p.get('merged_at') else p['state']} for p in pulls]
    open_pulls = [p for p in pulls if p['state'] == 'open']
    by_number = {p['number']: p for p in states}

    to_fetch = []
    raw = []
    for p in open_pulls:
        num = p['number']
        head_sha = (p.get('head') or {}).get('sha')
        cached = (existing_map or {}).get(num)
        if (cached and head_sha and cached.get('head_sha') == head_sha
                and cached.get('github_state') == 'open'
                and cached.get('files') and len(cached.get('files')) > 0):
            paths = [f['path'] if isinstance(f, dict) else str(f) for f in cached['files']]
            raw.append({
                'number': num,
                'title': p.get('title') or cached.get('title', ''),
                'author': {'login': (p.get('user') or {}).get('login') or cached.get('author', 'ghost')},
                'createdAt': p.get('created_at') or cached.get('created_at'),
                'url': p.get('html_url') or cached.get('url'),
                'additions': cached.get('additions', 0),
                'deletions': cached.get('deletions', 0),
                'files': [{'path': path} for path in paths if path],
                'changedFiles': cached.get('file_count', len(paths)),
                'isDraft': bool(p.get('draft', cached.get('is_draft', False))),
                'head_sha': head_sha,
                'mergeable_state': cached.get('mergeable_state') or 'unknown'
            })
        else:
            to_fetch.append(p)

    print(f'Fetching complete metadata for {len(to_fetch)} open PRs ({len(raw)} unchanged cached by SHA)...', flush=True)
    with ThreadPoolExecutor(max_workers=4) as pool:
        for count, (pull, result) in enumerate(zip(to_fetch, pool.map(fetch_open_pr, to_fetch)), 1):
            record, github_state = result
            by_number[pull['number']]['github_state'] = github_state
            if record:
                raw.append(record)
            if count % 50 == 0:
                print(f'Fetched {count}/{len(to_fetch)} PRs', flush=True)
    raw.sort(key=lambda r: r['number'])
    return raw, states


def fetch_open_pr(p):
    result = subprocess.run(['gh', 'api', f"repos/{REPOSITORY}/pulls/{p['number']}"],
                            capture_output=True, text=True, encoding='utf-8', timeout=60)
    if result.returncode:
        raise RuntimeError(f"Failed to inspect PR #{p['number']}; database was not changed")
    detail = json.loads(result.stdout)
    # A PR may have closed since the initial list was fetched.
    state = 'merged' if detail.get('merged_at') else detail['state']
    if state != 'open':
        return None, state
    files = github_pages(f"repos/{REPOSITORY}/pulls/{p['number']}/files?per_page=100")
    if len(files) != detail['changed_files']:
        raise RuntimeError(f"Incomplete file list for PR #{p['number']}; database was not changed")
    verify = subprocess.run(['gh', 'api', f"repos/{REPOSITORY}/pulls/{p['number']}"],
                            capture_output=True, text=True, encoding='utf-8', timeout=60)
    if verify.returncode:
        raise RuntimeError('GitHub verification failed; database was not changed')
    verified = json.loads(verify.stdout)
    if verified['head']['sha'] != detail['head']['sha'] or verified['state'] != 'open':
        raise RuntimeError(f"PR #{p['number']} changed during fetch; retry the sync")
    paths = sorted({path for f in files for path in
                    [f['filename'], f.get('previous_filename')] if path})
    record = {'number': p['number'], 'title': detail['title'],
                'author': {'login': (detail.get('user') or {}).get('login', 'ghost')},
                'createdAt': detail['created_at'], 'url': detail['html_url'],
                'additions': detail['additions'], 'deletions': detail['deletions'],
                'files': [{'path': path} for path in paths], 'changedFiles': detail['changed_files'],
                'isDraft': detail['draft'], 'head_sha': detail['head']['sha'],
                'mergeable_state': detail.get('mergeable_state') or 'unknown'}
    return record, state


def classify_pr(paths, title):
    p_lower = [p.lower() for p in paths]
    t_lower = title.lower()
    
    if any("agc" in p or "vulkandevice" in p for p in p_lower) or "agc" in t_lower:
        return "AGC & Graphics"
    if any("shader" in p or "relinker" in p for p in p_lower) or "shader" in t_lower or "relinker" in t_lower:
        return "Shader & Relinker"
    if any("libkernel" in p or "kqueue" in p or "directmemory" in p or "virtualmemory" in p or "vm.cpp" in p for p in p_lower) or "kernel" in t_lower:
        return "Kernel & Syscalls"
    if any("/libc/" in p or "/libc." in p or "libscelibcinternal" in p or "/libm/" in p or "libunwind" in p for p in p_lower) or "libc" in t_lower:
        return "LibC & Base Runtime"
    if any("core/libs/prx/" in p for p in p_lower) or any(t_lower.startswith(x) for x in ["feat(libsce", "fix(libsce"]):
        return "PRX High-Level Modules"
    if any("tools/" in p or ".github/" in p or p.endswith("cmakelists.txt") for p in p_lower):
        return "Build & Tooling"
    if all("docs/" in p for p in p_lower) or t_lower.startswith("docs"):
        return "Documentation & Debt"
    return "Other Core Subsystems"


def build_records(raw_prs):
    # Hotspots
    file_freq = Counter()
    for p in raw_prs:
        for fl in p.get("files", []):
            file_freq[fl["path"]] += 1
    hotspots = set(path for path, cnt in file_freq.items() if cnt >= 15)

    records = []
    for p in raw_prs:
        paths = [fl["path"] for fl in p.get("files", [])]
        adds = p.get("additions", 0)
        dels = p.get("deletions", 0)
        tot = adds + dels
        pr_hotspots = [path for path in paths if path in hotspots]
        core_hotspots = [path for path in pr_hotspots if not is_test_or_boilerplate_hotspot(path)]
        code_paths = [path for path in paths if not is_boilerplate_file(path)]
        eff_file_count = len(code_paths) if code_paths else len(paths)
        cat = classify_pr(paths, p.get("title", ""))
        
        if len(core_hotspots) > 0 or eff_file_count > 10 or tot > 1500:
            risk = "high"
        elif eff_file_count >= 4 or tot > 300 or len(pr_hotspots) > 0:
            risk = "medium"
        else:
            risk = "low"

        size = "XS" if tot < 25 else "S" if tot <= 100 else "M" if tot <= 400 else "L" if tot <= 1200 else "XL"
        author_login = p.get("author", {}).get("login", "ghost") if isinstance(p.get("author"), dict) else "ghost"

        records.append({
            "number": p["number"],
            "title": p.get("title", ""),
            "author": author_login,
            "author_avatar": f"https://github.com/{author_login}.png?size=64",
            "created_at": p.get("createdAt", None),
            "url": p.get("url") or f"https://github.com/boykopovar/AnyPS5/pull/{p['number']}",
            "additions": adds,
            "deletions": dels,
            "total_lines": tot,
            "file_count": p.get("changedFiles", len(paths)),
            "files": paths,
            "category": cat,
            "hotspots": pr_hotspots,
            "risk_level": risk,
            "size": size,
            "is_draft": p.get("isDraft", False),
            "head_sha": p["head_sha"],
            "github_state": "open",
            "mergeable_state": p.get("mergeable_state") or "unknown"
        })

    return records


def cleanup_non_open_prs():
    batches = read_all('batches?select=id,pr_numbers')
    claimed_prs = {n for b in batches for n in b.get('pr_numbers', [])}
    if claimed_prs:
        query = 'prs?github_state=in.(merged,closed)&number=not.in.(' + ','.join(map(str, claimed_prs)) + ')'
    else:
        query = 'prs?github_state=in.(merged,closed)'
    request(query, method='DELETE')


def sync():
    raw_prs, states = fetch_github_prs()
    records = build_records(raw_prs)
    request('rpc/sync_github_snapshot', {'p_records': records, 'p_states': states})
    cleanup_non_open_prs()
    print(f'Synchronized {len(records)} open PRs and {len(states)} explicit GitHub states.')


def sync_incremental():
    existing = read_all('prs?select=*&order=number.asc')
    existing_map = {p['number']: p for p in existing}
    raw_prs, states = fetch_github_prs(existing_map=existing_map)
    records = build_records(raw_prs)
    request('rpc/sync_github_snapshot', {'p_records': records, 'p_states': states})
    cleanup_non_open_prs()
    print(f'Incremental sync completed: {len(records)} open PRs and {len(states)} explicit GitHub states.')


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="AnyPS5 GitHub to Supabase Sync Service")
    parser.add_argument("--incremental", action="store_true", help="Reuse verified SHA-matched open PR file lists")
    args = parser.parse_args()
    if args.incremental:
        sync_incremental()
    else:
        sync()
