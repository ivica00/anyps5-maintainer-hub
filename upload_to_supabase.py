import json
import urllib.request
import urllib.error
from collections import Counter

SUPABASE_URL = "https://nzaxslgwsygpgwqcllpi.supabase.co"
ANON_KEY = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6Im56YXhzbGd3c3lncGd3cWNsbHBpIiwicm9sZSI6ImFub24iLCJpYXQiOjE3OTIxNTU2NjksImV4cCI6MjEwNzczMTY2OX0.G_sibL-5v-wxWagMeY1eAXZXm2b5Bsh0nziN94a4moY"

with open("all_prs.json", "r", encoding="utf-8") as f:
    raw_prs = json.load(f)

# Count file frequencies to identify hotspots
file_freq = Counter()
for p in raw_prs:
    for fl in p.get("files", []):
        file_freq[fl["path"]] += 1

HOTSPOT_THRESHOLD = 15
hotspots = [path for path, cnt in file_freq.most_common() if cnt >= HOTSPOT_THRESHOLD]
hotspot_set = set(hotspots)

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

records = []
for p in raw_prs:
    paths = [fl["path"] for fl in p.get("files", [])]
    adds = p.get("additions", 0)
    dels = p.get("deletions", 0)
    tot = adds + dels
    
    pr_hotspots = [path for path in paths if path in hotspot_set]
    cat = classify_pr(paths, p.get("title", ""))
    
    if len(pr_hotspots) > 0 or len(paths) > 10 or tot > 1500:
        risk = "high"
    elif len(paths) >= 4 or tot > 300:
        risk = "medium"
    else:
        risk = "low"

    if tot < 25:
        size = "XS"
    elif tot <= 100:
        size = "S"
    elif tot <= 400:
        size = "M"
    elif tot <= 1200:
        size = "L"
    else:
        size = "XL"

    author_obj = p.get("author") or {}
    author_login = author_obj.get("login", "ghost")
    author_avatar = f"https://github.com/{author_login}.png?size=64"
    num = p["number"]
    url = p.get("url") or f"https://github.com/boykopovar/AnyPS5/pull/{num}"

    records.append({
        "number": num,
        "title": p.get("title", ""),
        "author": author_login,
        "author_avatar": author_avatar,
        "created_at": p.get("createdAt", None),
        "url": url,
        "additions": adds,
        "deletions": dels,
        "total_lines": tot,
        "file_count": len(paths),
        "files": paths,
        "category": cat,
        "hotspots": pr_hotspots,
        "risk_level": risk,
        "size": size,
        "is_draft": p.get("isDraft", False),
        "assigned_reviewer": "none",
        "status": "unassigned",
        "notes": "",
        "checklist": {}
    })

print(f"Prepared {len(records)} PR records for Supabase.")

# Upload in batches of 100
BATCH_SIZE = 100
api_endpoint = f"{SUPABASE_URL}/rest/v1/prs"

for i in range(0, len(records), BATCH_SIZE):
    batch = records[i:i + BATCH_SIZE]
    payload = json.dumps(batch).encode("utf-8")
    
    req = urllib.request.Request(api_endpoint, data=payload, headers={
        "apikey": ANON_KEY,
        "Authorization": f"Bearer {ANON_KEY}",
        "Content-Type": "application/json",
        "Prefer": "resolution=merge-duplicates"
    })
    
    try:
        with urllib.request.urlopen(req) as resp:
            print(f"Uploaded batch {i // BATCH_SIZE + 1} ({len(batch)} PRs) - Status: {resp.status}")
    except urllib.error.HTTPError as e:
        print(f"Error on batch {i}: {e.code} - {e.read().decode()}")
        break

print("All batches processed successfully!")
