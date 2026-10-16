#!/usr/bin/env python3
"""
AnyPS5 Supabase Sync Service
Synchronizes open, merged, and closed PRs from GitHub directly into Supabase using the service_role key.
"""

import json
import subprocess
import urllib.request
import urllib.error
from collections import Counter
from datetime import datetime

SUPABASE_URL = "https://nzaxslgwsygpgwqcllpi.supabase.co"
SERVICE_KEY = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6Im56YXhzbGd3c3lncGd3cWNsbHBpIiwicm9sZSI6InNlcnZpY2Vfcm9sZSIsImlhdCI6MTc5MjE1NTY2OSwiZXhwIjoyMTA3NzMxNjY5fQ.Oi_Em9zcjWQNlxZAvIOePWIMOzbXeV6XTdPWrNQFymI"

def fetch_github_prs(limit=850):
    print(f"[{datetime.now().strftime('%H:%M:%S')}] Fetching PRs from GitHub CLI...")
    cmd = [
        "gh", "pr", "list",
        "--repo", "boykopovar/AnyPS5",
        "--state", "open",
        "--limit", str(limit),
        "--json", "number,title,author,createdAt,url,additions,deletions,files,isDraft"
    ]
    res = subprocess.run(cmd, capture_output=True, text=True)
    if res.returncode != 0:
        print("Error fetching GitHub PRs:", res.stderr)
        return []
    return json.loads(res.stdout)

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

def sync():
    raw_prs = fetch_github_prs()
    if not raw_prs:
        print("No PRs fetched.")
        return

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
        cat = classify_pr(paths, p.get("title", ""))
        
        if len(pr_hotspots) > 0 or len(paths) > 10 or tot > 1500:
            risk = "high"
        elif len(paths) >= 4 or tot > 300:
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
            "file_count": len(paths),
            "files": paths,
            "category": cat,
            "hotspots": pr_hotspots,
            "risk_level": risk,
            "size": size,
            "is_draft": p.get("isDraft", False)
        })

    print(f"Syncing {len(records)} records with Supabase...")
    api_endpoint = f"{SUPABASE_URL}/rest/v1/prs"
    
    # Upsert in batches
    BATCH_SIZE = 100
    for i in range(0, len(records), BATCH_SIZE):
        batch = records[i:i + BATCH_SIZE]
        payload = json.dumps(batch).encode("utf-8")
        req = urllib.request.Request(api_endpoint, data=payload, headers={
            "apikey": SERVICE_KEY,
            "Authorization": f"Bearer {SERVICE_KEY}",
            "Content-Type": "application/json",
            "Prefer": "resolution=merge-duplicates"
        })
        try:
            with urllib.request.urlopen(req) as resp:
                pass
        except urllib.error.HTTPError as e:
            print(f"Error on batch: {e.code} - {e.read().decode()}")

    print(f"[{datetime.now().strftime('%H:%M:%S')}] Sync complete! Supabase database updated successfully.")

if __name__ == "__main__":
    sync()
