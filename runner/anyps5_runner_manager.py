#!/usr/bin/env python3
"""
AnyPS5 Self-Hosted Runner Management Utility.
Allows maintainers and developers to:
1. Check live online status of community runners.
2. Generate fresh registration tokens via GitHub API or `gh` CLI.
3. Trigger CI builds for specific PR numbers on community machines.
"""

import sys
import os
import json
import argparse
import subprocess
import urllib.request
import urllib.error

# Ensure UTF-8 output on Windows consoles
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")

DEFAULT_REPO = "boykopovar/AnyPS5"

def get_auth_token():
    """Retrieve GitHub token from GITHUB_TOKEN env or gh CLI auth token."""
    token = os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN")
    if token:
        return token
    try:
        res = subprocess.run(["gh", "auth", "token"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, check=True)
        return res.stdout.strip()
    except Exception:
        return None

def github_api_request(endpoint, method="GET", payload=None, token=None):
    """Executes an authenticated GitHub REST API request."""
    if not token:
        token = get_auth_token()
    if not token:
        raise ValueError(
            "GitHub token required. Please set GITHUB_TOKEN environment variable "
            "or log in via GitHub CLI ('gh auth login')."
        )

    url = f"https://api.github.com{endpoint}"
    data = json.dumps(payload).encode("utf-8") if payload else None
    req = urllib.request.Request(
        url,
        data=data,
        method=method,
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "User-Agent": "AnyPS5-Runner-Manager",
            "X-GitHub-Api-Version": "2022-11-28"
        }
    )
    with urllib.request.urlopen(req, timeout=15) as resp:
        body = resp.read().decode("utf-8")
        return json.loads(body) if body else {}

def list_runners(repo):
    """List all registered runners for the repository."""
    print(f"\n[*] Fetching self-hosted runners for {repo}...")
    try:
        data = github_api_request(f"/repos/{repo}/actions/runners")
        total = data.get("total_count", 0)
        runners = data.get("runners", [])
        print(f"\n[+] Total Registered Runners: {total}")
        print("=" * 70)
        print(f"{'ID':<10} {'NAME':<25} {'OS':<10} {'STATUS':<10} {'LABELS'}")
        print("-" * 70)
        for r in runners:
            labels = ", ".join(l["name"] for l in r.get("labels", []))
            status_icon = "🟢" if r.get("status") == "online" else "⚪"
            print(f"{r.get('id'):<10} {r.get('name')[:24]:<25} {r.get('os'):<10} {status_icon} {r.get('status'):<8} {labels}")
        print("=" * 70)
    except Exception as e:
        print(f"[-] Error fetching runners: {e}", file=sys.stderr)

def get_registration_token(repo):
    """Generate a runner registration token."""
    print(f"[*] Requesting registration token for {repo}...")
    try:
        data = github_api_request(f"/repos/{repo}/actions/runners/registration-token", method="POST")
        token = data.get("token")
        expires = data.get("expires_at")
        print(f"\n[+] New Runner Registration Token:")
        print(f"    TOKEN:   {token}")
        print(f"    EXPIRES: {expires}")
        print("\nQuick Run Command (PowerShell):")
        print(f"    .\\runner\\setup_windows_runner.ps1 -RunnerToken \"{token}\"\n")
        return token
    except Exception as e:
        print(f"[-] Failed to obtain registration token: {e}", file=sys.stderr)
        return None

def trigger_ci(repo, pr_number, subsystem="all"):
    """Trigger the AnyPS5 self-hosted workflow via workflow_dispatch."""
    print(f"[*] Triggering self-hosted CI for PR #{pr_number} on {repo}...")
    try:
        payload = {
            "ref": "main",
            "inputs": {
                "pr_number": str(pr_number),
                "subsystem": subsystem
            }
        }
        github_api_request(
            f"/repos/{repo}/actions/workflows/anyps5_self_hosted_ci.yml/dispatches",
            method="POST",
            payload=payload
        )
        print(f"[+] Successfully dispatched CI build for PR #{pr_number} to community runners!")
    except Exception as e:
        print(f"[-] Failed to dispatch workflow: {e}", file=sys.stderr)

def main():
    parser = argparse.ArgumentParser(description="AnyPS5 Community Runner Manager")
    parser.add_argument("--repo", default=DEFAULT_REPO, help="GitHub repository (owner/name)")
    parser.add_argument("--status", action="store_true", help="List all registered runners and their online status")
    parser.add_argument("--get-token", action="store_true", help="Generate a fresh runner registration token")
    parser.add_argument("--trigger-pr", type=int, help="Dispatch self-hosted CI for a specific PR number")
    parser.add_argument("--subsystem", default="all", choices=["all", "relinker", "register-context", "write-watch"])

    args = parser.parse_args()

    if args.status:
        list_runners(args.repo)
    elif args.get_token:
        get_registration_token(args.repo)
    elif args.trigger_pr:
        trigger_ci(args.repo, args.trigger_pr, args.subsystem)
    else:
        parser.print_help()

if __name__ == "__main__":
    main()
