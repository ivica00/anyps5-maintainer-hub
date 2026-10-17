#!/usr/bin/env python3
"""
Deterministic AI Prompt Generator for AnyPS5 Pull Request Batches.
100% free, zero API calls, zero rate-limits, 0ms latency.
Generates professionally structured prompts optimized for Claude 3.7 Sonnet, OpenAI o3/GPT-4o, Gemini 2.5 Pro, or DeepSeek R1.
"""

import sys
import json
import argparse
import subprocess
import urllib.request
from collections import defaultdict

# Ensure UTF-8 output on Windows consoles (CP1250 / CP437)
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
if hasattr(sys.stderr, "reconfigure"):
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")


SUPABASE_URL = "https://nzaxslgwsygpgwqcllpi.supabase.co"
SUPABASE_ANON_KEY = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6Im56YXhzbGd3c3lncGd3cWNsbHBpIiwicm9sZSI6ImFub24iLCJpYXQiOjE3OTIxNTU2NjksImV4cCI6MjEwNzczMTY2OX0.G_sibL-5v-wxWagMeY1eAXZXm2b5Bsh0nziN94a4moY"

def load_data():
    """Loads PRs and batches from local all_prs.json or directly from Supabase."""
    prs = {}
    batches = []

    # 1. Attempt to load local all_prs.json
    try:
        with open("all_prs.json", "r", encoding="utf-8") as f:
            local_list = json.load(f)
            for p in local_list:
                prs[p["number"]] = p
    except Exception:
        pass

    # 2. Fetch batches from Supabase REST API
    headers = {
        "apikey": SUPABASE_ANON_KEY,
        "Authorization": f"Bearer {SUPABASE_ANON_KEY}"
    }
    try:
        req_b = urllib.request.Request(f"{SUPABASE_URL}/rest/v1/batches?select=*&order=id.asc", headers=headers)
        with urllib.request.urlopen(req_b, timeout=6) as resp:
            batches = json.loads(resp.read().decode())
    except Exception as e:
        print(f"[!] Warning fetching batches from Supabase: {e}", file=sys.stderr)

    # 3. If PRs missing locally, fetch from Supabase
    if not prs:
        try:
            req_p = urllib.request.Request(f"{SUPABASE_URL}/rest/v1/prs?select=*&order=number.desc", headers=headers)
            with urllib.request.urlopen(req_p, timeout=8) as resp:
                supa_prs = json.loads(resp.read().decode())
                for p in supa_prs:
                    prs[p["number"]] = p
        except Exception as e:
            print(f"[!] Error fetching PRs: {e}", file=sys.stderr)

    return prs, batches

def build_deterministic_prompt(batch, pr_map):
    """
    Builds a detailed, structured, and deterministic prompt for LLM code review.
    """
    b_id = batch.get("id", "batch-unknown")
    b_title = batch.get("title", "Untitled Batch")
    b_cat = batch.get("category", "Other Core Subsystems")
    b_tier = batch.get("complexity", "medium")
    b_tier_lbl = batch.get("complexity_label", b_tier)
    b_type = batch.get("batch_type", "disjoint")
    pr_numbers = batch.get("pr_numbers", [])

    included_prs = [pr_map[num] for num in pr_numbers if num in pr_map]

    total_additions = sum(p.get("additions", 0) for p in included_prs)
    total_deletions = sum(p.get("deletions", 0) for p in included_prs)
    
    # Collect all modified files
    all_files = set()
    for p in included_prs:
        files = p.get("files", [])
        for f in files:
            if isinstance(f, dict):
                all_files.add(f.get("path", ""))
            elif isinstance(f, str):
                all_files.add(f)
    all_files.discard("")

    if b_type == "cohesive_cluster":
        overlap_policy = "🔵 **COHESIVE MODULE CLUSTER** (PRs intentionally target the same module/file; check for duplicate solutions vs. complementary changes and recommend rebase merge order)"
    elif b_type == "soft_disjoint":
        overlap_policy = "🟢 **CODE-DISJOINT / AUTO-REBASEABLE** (100% disjoint C++/Python source files; overlaps only in boilerplate CMakeLists.txt or docs/dev/debt/*.md)"
    else:
        overlap_policy = "✅ **CONFIRMED 100% DISJOINT** (PRs inside this batch do NOT share modified files, guaranteeing zero direct git conflicts)"

    prompt = f"""# AnyPS5 Pull Request Batch Review: {b_title}

## 🎯 OBJECTIVE & ROLE
You are an elite principal systems software engineer, C++ architect, and PlayStation 5 emulator core contributor.
Your task is to conduct a rigorous, comprehensive architectural and code safety review of a **{b_type.replace('_', ' ')} pull request batch** destined for the `boykopovar/AnyPS5` repository.

## 📦 BATCH SPECIFICATION
- **Batch Identifier**: `{b_id}`
- **Batch Strategy (`batch_type`)**: `{b_type}`
- **Subsystem / Domain**: `{b_cat}`
- **Complexity Tier**: `{b_tier_lbl}`
- **Total Pull Requests**: {len(included_prs)}
- **File Overlap Policy**: {overlap_policy}
- **Aggregate Code Churn**: +{total_additions:,} / -{total_deletions:,} lines across {len(all_files)} files

---

## 🔍 DETAILED INCLUDED PULL REQUESTS

"""

    for i, pr in enumerate(included_prs, 1):
        num = pr.get("number")
        title = pr.get("title", "").strip()
        raw_author = pr.get("author", "unknown")
        if isinstance(raw_author, dict):
            author = raw_author.get("login") or raw_author.get("name") or "unknown"
        else:
            author = str(raw_author)
        url = pr.get("url", f"https://github.com/boykopovar/AnyPS5/pull/{num}")
        adds = pr.get("additions", 0)
        dels = pr.get("deletions", 0)
        risk = pr.get("risk_level") or pr.get("riskLevel", "low")
        risk_reason = pr.get("risk_reason") or pr.get("riskReason", "")

        pr_files = []
        for f in pr.get("files", []):
            if isinstance(f, dict):
                pr_files.append(f.get("path", ""))
            elif isinstance(f, str):
                pr_files.append(f)
        pr_files = [f for f in pr_files if f]

        prompt += f"### {i}. PR #{num}: {title}\n"
        prompt += f"- **Author**: @{author}\n"
        prompt += f"- **Direct Link**: {url}\n"
        prompt += f"- **Assessed Risk Level**: `{risk.upper()}`"
        if risk_reason:
            prompt += f" ({risk_reason})"
        prompt += f"\n- **Diff Churn**: +{adds} / -{dels} lines ({len(pr_files)} files)\n"
        prompt += "- **Modified Files**:\n"
        for pf in pr_files:
            prompt += f"  - `{pf}`\n"
        if not pr_files:
            prompt += "  - *(Core internal module files)*\n"
        prompt += "\n"

    prompt += f"""---

## 🛡️ STRICT EMULATION REVIEW DIRECTIVES & INVARIANTS

Evaluate this batch against critical PS5 emulation architecture principles:

1. **Concurrency & Thread Safety**:
   - Check for data races, mutex deadlocks, lock inversion, or incorrect atomic memory orderings (`std::memory_order_relaxed` vs `acquire/release`).
   - AnyPS5 runs high-throughput multithreaded guest jobs (FreeBSD kernel worker threads, AGC command buffers). Verify all shared state is protected.

2. **FreeBSD & PS5 Kernel ABI Fidelity**:
   - Verify that system calls, ioctls, and POSIX shims strictly match FreeBSD 12/13 ABI semantics and return appropriate negative errno codes where expected.
   - Guard against structure padding, endianness assumptions, and 64-bit pointer truncation.

3. **Graphics (AGC) & Shader Translation Invariants** (if applicable to {b_cat}):
   - Ensure descriptor indexing bounds checks, uniform buffer alignments, and Vulkan/GCN pipeline barriers are sound.
   - Verify shader disk cache serialization does not introduce disk corruption or stale key collisions.

4. **Cross-PR Semantic Side-Effects**:
   - While the files are strictly physically disjoint, verify whether any PR introduces assumptions, global state changes, or build flag dependencies that could impact neighboring modules.

5. **Resource Management**:
   - Verify zero memory leaks (prefer RAII, `std::unique_ptr`, `std::shared_ptr`).
   - Validate bounds checking on guest pointer offsets and packet buffers.

---

## 📋 REQUIRED OUTPUT FORMAT

Please organize your response strictly into the following sections:

### 1. Executive Batch Summary
A concise 2-3 paragraph synthesis of the batch's architectural impact, overall stability, and risk level.

### 2. PR-by-PR Evaluation Table
| PR # | Title | Risk | Verdict (Approve / Request Changes / Needs Benchmarking) | Key Notes |
|:---:|:---|:---:|:---:|:---|
"""
    for pr in included_prs:
        num = pr.get("number")
        title = pr.get("title", "").replace("|", "-")[:55]
        prompt += f"| #{num} | {title} | `LOW` | **[VERDICT]** | [1-sentence assessment] |\n"

    prompt += f"""
### 3. Detailed Findings & Potential Regressions
Highlight any suspicious code patterns, missing edge cases, or suggested defensive assertions for any specific PRs.

### 4. Sequential Merge Recommendation
Provide the optimal sequential merge order or confirmation to squash-merge with `gh pr merge`:
```bash
# Recommended Sequence
"""
    for pr in included_prs:
        prompt += f"gh pr merge {pr.get('number')} --repo boykopovar/AnyPS5 --auto --squash --delete-branch\n"
    prompt += "```\n"

    return prompt

def copy_to_clipboard(text):
    """Copies text to the clipboard via PowerShell."""
    try:
        cmd = ["powershell", "-NoProfile", "-Command", "Set-Clipboard -Value ([Console]::In.ReadToEnd())"]
        p = subprocess.Popen(cmd, stdin=subprocess.PIPE, text=True, encoding="utf-8")
        p.communicate(input=text)
        return True
    except Exception:
        return False

def main():
    parser = argparse.ArgumentParser(description="Deterministic AI prompt generator for AnyPS5 PR batches")
    parser.add_argument("--batch", "-b", help="Batch ID (e.g. batch-agc-graphics-2)")
    parser.add_argument("--category", "-c", help="Filter by category (e.g. 'AGC & Graphics')")
    parser.add_argument("--top", "-t", type=int, default=0, help="Show top N batches")
    parser.add_argument("--list", "-l", action="store_true", help="List available batches")
    parser.add_argument("--output", "-o", help="Save prompt to specified markdown file (.md)")
    parser.add_argument("--clipboard", action="store_true", default=True, help="Automatically copy prompt to clipboard")
    parser.add_argument("--no-clipboard", action="store_false", dest="clipboard", help="Do not copy to clipboard")
    args = parser.parse_args()

    prs, batches = load_data()
    if not batches:
        print("[!] No batches loaded. Check connection or Supabase database.", file=sys.stderr)
        return 1

    if args.list:
        print(f"\n{'ID':<30} {'Category':<24} {'Complexity':<14} {'PRs':<6} {'Status':<10}")
        print("-" * 86)
        for b in batches[:args.top or 50]:
            print(f"{b['id']:<30} {b.get('category',''):<24} {b.get('complexity',''):<14} {b.get('pr_count', 0):<6} {b.get('status',''):<10}")
        print(f"\nTotal: {len(batches)} batches. To generate a prompt, run:\n  python generate_batch_prompt.py --batch <ID>\n")
        return 0

    target_batch = None
    if args.batch:
        target_batch = next((b for b in batches if b["id"] == args.batch or args.batch in b["id"]), None)
        if not target_batch:
            print(f"[!] Batch with ID '{args.batch}' not found.", file=sys.stderr)
            return 1
    elif args.category:
        target_batch = next((b for b in batches if args.category.lower() in b.get("category", "").lower()), None)
        if not target_batch:
            print(f"[!] No batch found in category '{args.category}'.", file=sys.stderr)
            return 1
    else:
        # Default: pick first available batch
        target_batch = next((b for b in batches if b.get("status") == "available"), batches[0])

    prompt = build_deterministic_prompt(target_batch, prs)

    if args.output:
        with open(args.output, "w", encoding="utf-8") as f:
            f.write(prompt)
        print(f"[✓] Prompt saved to file: {args.output}")

    if args.clipboard:
        if copy_to_clipboard(prompt):
            print(f"[✓] Prompt for batch '{target_batch.get('title')}' COPIED TO CLIPBOARD!")
            print("    You can paste (Ctrl+V) directly into Claude 3.7 Sonnet, ChatGPT, o3, or Gemini.")
        else:
            print("[!] Automatic clipboard copy failed.", file=sys.stderr)

    print("\n" + "=" * 60)
    print(f"PROMPT PREVIEW ({len(prompt.splitlines())} lines, ~{len(prompt.split())} words):")
    print("=" * 60)
    lines = prompt.splitlines()
    print("\n".join(lines[:45]))
    if len(lines) > 45:
        print(f"\n... [{len(lines) - 45} lines remaining] ...")
    print("=" * 60)

    return 0

if __name__ == "__main__":
    sys.exit(main())
