# AnyPS5 Maintainer Workspace

Native Rust desktop application (`anyps5-desktop`) and automated Supabase synchronization tools for managing, reviewing, and batch-merging pull requests on [`boykopovar/AnyPS5`](https://github.com/boykopovar/AnyPS5/pulls).

---

## 🚀 Quick Start (Click-to-Run Scripts)

| Script | Purpose | Recommended Frequency |
| :--- | :--- | :--- |
| **[`run_desktop.bat`](run_desktop.bat)** | Launches the **100% Pure Rust Desktop App** (`egui`/`wgpu` + Supabase Live). | Anytime |
| **[`1_sync_prs.bat`](1_sync_prs.bat)** | Incrementally syncs open PRs from GitHub (`head_sha`-verified) and deletes merged/closed PRs from Supabase. | Anytime (takes ~5–15s) |
| **[`2_generate_batches.bat`](2_generate_batches.bat)** | Runs PR sync first, then generates **3-Lane Smart Batches** (`cohesive_cluster`, `disjoint`, `soft_disjoint`) in Supabase while preserving claimed batches. | ~2x daily (when new PRs accumulate) |

---

## 📁 Repository Structure

```text
AnyPS5 PR/
├── anyps5-desktop/             # Native Rust desktop application (Cargo project)
├── supabase/                   # PostgreSQL migrations & Edge Functions (github-webhook)
├── runner/                     # Self-hosted GitHub Actions build runner scripts & Windows Sandbox
├── tests/                      # Python & PostgreSQL verification test suites
├── Archive/                    # Archived legacy web UI, static JSON dumps, and one-off scripts
│   ├── web-ui/                 # Legacy HTML/JS/Tailwind frontend
│   ├── legacy-data/            # Historical static JSON/JS snapshots & backups
│   ├── legacy-scripts/         # One-off migration and dataset scripts
│   └── branding-duplicates/    # Root icon duplicates
├── 1_sync_prs.bat              # Click-to-run: Incremental GitHub -> Supabase PR sync
├── 2_generate_batches.bat      # Click-to-run: Sync PRs + Generate 3-lane smart batches
├── run_desktop.bat             # Click-to-run: Launch Rust desktop workspace
├── backend_api.py              # Zero-dependency Supabase REST helper (auto-loads .env)
├── sync_service.py             # Incremental GitHub PR synchronization & cleanup engine
└── generate_batches_backend.py # 3-lane smart batching algorithm
```
