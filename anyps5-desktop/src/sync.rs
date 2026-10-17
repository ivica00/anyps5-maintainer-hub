use crate::models::{Batch, PullRequest};
use crate::supabase::{self, HTTP_AGENT, SUPABASE_SERVICE_ROLE_KEY, SUPABASE_URL};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

const REPOSITORY: &str = "boykopovar/AnyPS5";

fn is_boilerplate_file(path: &str) -> bool {
    let p = path.to_ascii_lowercase().replace('\\', "/");
    p.ends_with("cmakelists.txt") || p.contains("docs/dev/debt/")
}

fn is_test_or_boilerplate_hotspot(path: &str) -> bool {
    let p = path.to_ascii_lowercase().replace('\\', "/");
    is_boilerplate_file(&p) || p.contains("/tests/") || p.starts_with("tests/")
}

fn effective_code_files<'a>(files: &'a [&'a str]) -> Vec<&'a str> {
    let non_bp: Vec<&'a str> = files
        .iter()
        .copied()
        .filter(|&f| !is_boilerplate_file(f))
        .collect();
    if non_bp.is_empty() {
        files.to_vec()
    } else {
        non_bp
    }
}

fn classify_pr(paths: &[&str], title: &str) -> &'static str {
    let p_lower: Vec<String> = paths
        .iter()
        .map(|p| p.to_ascii_lowercase().replace('\\', "/"))
        .collect();
    let t_lower = title.to_ascii_lowercase();

    if p_lower
        .iter()
        .any(|p| p.contains("agc") || p.contains("vulkandevice"))
        || t_lower.contains("agc")
    {
        return "AGC & Graphics";
    }
    if p_lower
        .iter()
        .any(|p| p.contains("shader") || p.contains("relinker"))
        || t_lower.contains("shader")
        || t_lower.contains("relinker")
    {
        return "Shader & Relinker";
    }
    if p_lower.iter().any(|p| {
        p.contains("libkernel")
            || p.contains("kqueue")
            || p.contains("directmemory")
            || p.contains("virtualmemory")
            || p.contains("vm.cpp")
    }) || t_lower.contains("kernel")
    {
        return "Kernel & Syscalls";
    }
    if p_lower.iter().any(|p| {
        p.contains("/libc/")
            || p.contains("/libc.")
            || p.contains("libscelibcinternal")
            || p.contains("/libm/")
            || p.contains("libunwind")
    }) || t_lower.contains("libc")
    {
        return "LibC & Base Runtime";
    }
    if p_lower.iter().any(|p| p.contains("core/libs/prx/"))
        || t_lower.starts_with("feat(libsce")
        || t_lower.starts_with("fix(libsce")
    {
        return "PRX High-Level Modules";
    }
    if p_lower.iter().any(|p| {
        p.contains("tools/") || p.contains(".github/") || p.ends_with("cmakelists.txt")
    }) {
        return "Build & Tooling";
    }
    if !p_lower.is_empty() && p_lower.iter().all(|p| p.contains("docs/"))
        || t_lower.starts_with("docs")
    {
        return "Documentation & Debt";
    }
    "Other Core Subsystems"
}

// Compact pure Rust SHA-256 for deterministic batch IDs ("batch-{sha256[:24]}")
pub fn sha256_hex(data: &[u8]) -> String {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let k: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut h_val = h[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h_val
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(k[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h_val = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(h_val);
    }

    let mut out = String::with_capacity(64);
    for val in h {
        out.push_str(&format!("{val:08x}"));
    }
    out
}

#[derive(Debug, Clone, Deserialize)]
struct GhPullItem {
    number: u64,
    title: Option<String>,
    state: String,
    created_at: Option<String>,
    merged_at: Option<String>,
    draft: Option<bool>,
    user: Option<GhUser>,
    head: Option<GhHead>,
    additions: Option<i64>,
    deletions: Option<i64>,
    mergeable_state: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct GhUser {
    login: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct GhHead {
    sha: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct GhFileItem {
    filename: String,
}

#[derive(Debug, Clone, Deserialize)]
struct BatchNameRow {
    batch_id: String,
    batch_number: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawPrRecord {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub author_avatar: String,
    pub created_at: Option<String>,
    pub url: String,
    pub additions: i64,
    pub deletions: i64,
    pub total_lines: i64,
    pub file_count: usize,
    pub files: Vec<String>,
    pub category: String,
    pub hotspots: Vec<String>,
    pub risk_level: String,
    pub size: String,
    pub is_draft: bool,
    pub head_sha: String,
    pub github_state: String,
    pub mergeable_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateRow {
    pub number: u64,
    pub github_state: String,
}

fn gh_pages<T: serde::de::DeserializeOwned>(endpoint: &str) -> Result<Vec<T>, String> {
    let mut cmd = Command::new("gh");
    cmd.args(["api", "--paginate", "--slurp", endpoint]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let output = cmd
        .output()
        .map_err(|e| format!("gh api failed to start: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "gh api failed with code {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let pages: Vec<Vec<T>> = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Invalid JSON from gh api {endpoint}: {e}"))?;
    Ok(pages.into_iter().flatten().collect())
}

fn gh_get_json<T: serde::de::DeserializeOwned>(endpoint: &str) -> Result<T, String> {
    let mut cmd = Command::new("gh");
    cmd.args(["api", endpoint]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let output = cmd
        .output()
        .map_err(|e| format!("gh api failed to start: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "gh api failed for {endpoint}: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Invalid JSON from gh api {endpoint}: {e}"))
}

pub fn fetch_github_prs(
    existing_map: &HashMap<u64, PullRequest>,
    slot_index: usize,
    active_count: usize,
) -> Result<(Vec<RawPrRecord>, Vec<StateRow>), String> {
    let pulls: Vec<GhPullItem> = gh_pages(&format!(
        "repos/{REPOSITORY}/pulls?state=all&per_page=100&sort=created&direction=asc"
    ))?;

    let mut states: Vec<StateRow> = Vec::with_capacity(pulls.len());
    let mut open_pulls: Vec<GhPullItem> = Vec::new();

    for p in pulls {
        let state = if p.merged_at.is_some() {
            "merged"
        } else {
            p.state.as_str()
        };
        states.push(StateRow {
            number: p.number,
            github_state: state.to_string(),
        });
        if p.state == "open" {
            open_pulls.push(p);
        }
    }

    let mut to_fetch: Vec<GhPullItem> = Vec::new();
    let mut cached_records: Vec<RawPrRecord> = Vec::new();

    for p in open_pulls {
        let num = p.number;
        let head_sha = p.head.as_ref().and_then(|h| h.sha.clone());
        let cached = existing_map.get(&num);
        if let (Some(cached), Some(head_sha)) = (cached, head_sha.as_ref())
            && cached.head_sha.as_deref() == Some(head_sha)
            && cached.is_open()
            && !cached.files.is_empty()
        {
            let paths: Vec<String> = cached
                .files
                .iter()
                .filter_map(|f| f.path.as_deref().map(String::from))
                .collect();
            let adds = cached.additions.max(0);
            let dels = cached.deletions.max(0);
            let tot = adds + dels;
            let str_paths: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();
            let cat = classify_pr(&str_paths, &cached.title);
            let size = if tot < 25 {
                "XS"
            } else if tot <= 100 {
                "S"
            } else if tot <= 400 {
                "M"
            } else if tot <= 1200 {
                "L"
            } else {
                "XL"
            };

            cached_records.push(RawPrRecord {
                number: num,
                title: cached.title.clone(),
                author: cached.author_name().to_string(),
                author_avatar: format!("https://github.com/{}.png?size=64", cached.author_name()),
                created_at: None,
                url: format!("https://github.com/{REPOSITORY}/pull/{num}"),
                additions: adds,
                deletions: dels,
                total_lines: tot,
                file_count: paths.len(),
                files: paths,
                category: cat.to_string(),
                hotspots: Vec::new(),
                risk_level: "low".into(),
                size: size.into(),
                is_draft: cached.is_draft,
                head_sha: head_sha.clone(),
                github_state: "open".into(),
                mergeable_state: "unknown".into(),
            });
        } else {
            // Optional sharding across online workers if multiple instances are active
            if active_count <= 1 || (num as usize) % active_count == slot_index {
                to_fetch.push(p);
            }
        }
    }

    // Parallel fetch for new or changed PRs
    let fetched_records = std::sync::Mutex::new(Vec::new());
    let fetch_err = std::sync::Mutex::new(None);

    std::thread::scope(|s| {
        let chunks: Vec<_> = to_fetch.chunks(4.max(to_fetch.len() / 4 + 1)).collect();
        for chunk in chunks {
            let fetched_records = &fetched_records;
            let fetch_err = &fetch_err;
            s.spawn(move || {
                for item in chunk {
                    let num = item.number;
                    let detail: Result<GhPullItem, String> =
                        gh_get_json(&format!("repos/{REPOSITORY}/pulls/{num}"));
                    let Ok(detail) = detail else {
                        *fetch_err.lock().unwrap() =
                            Some(format!("Failed to get details for PR #{num}"));
                        return;
                    };
                    if detail.state != "open" {
                        continue;
                    }

                    let files_res: Result<Vec<GhFileItem>, String> = gh_pages(&format!(
                        "repos/{REPOSITORY}/pulls/{num}/files?per_page=100"
                    ));
                    let Ok(files) = files_res else {
                        *fetch_err.lock().unwrap() =
                            Some(format!("Failed to get files for PR #{num}"));
                        return;
                    };

                    let paths: Vec<String> = files.into_iter().map(|f| f.filename).collect();
                    let adds = detail.additions.unwrap_or(0).max(0);
                    let dels = detail.deletions.unwrap_or(0).max(0);
                    let tot = adds + dels;
                    let str_paths: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();
                    let cat = classify_pr(&str_paths, detail.title.as_deref().unwrap_or(""));
                    let author = detail
                        .user
                        .and_then(|u| u.login)
                        .unwrap_or_else(|| "ghost".into());
                    let head_sha = detail
                        .head
                        .and_then(|h| h.sha)
                        .unwrap_or_else(|| "unknown".into());
                    let size = if tot < 25 {
                        "XS"
                    } else if tot <= 100 {
                        "S"
                    } else if tot <= 400 {
                        "M"
                    } else if tot <= 1200 {
                        "L"
                    } else {
                        "XL"
                    };

                    fetched_records.lock().unwrap().push(RawPrRecord {
                        number: num,
                        title: detail.title.unwrap_or_default(),
                        author: author.clone(),
                        author_avatar: format!("https://github.com/{author}.png?size=64"),
                        created_at: detail.created_at,
                        url: format!("https://github.com/{REPOSITORY}/pull/{num}"),
                        additions: adds,
                        deletions: dels,
                        total_lines: tot,
                        file_count: paths.len(),
                        files: paths,
                        category: cat.to_string(),
                        hotspots: Vec::new(),
                        risk_level: "low".into(),
                        size: size.into(),
                        is_draft: detail.draft.unwrap_or(false),
                        head_sha,
                        github_state: "open".into(),
                        mergeable_state: detail.mergeable_state.unwrap_or_else(|| "unknown".into()),
                    });
                }
            });
        }
    });

    if let Some(err) = fetch_err.into_inner().unwrap() {
        return Err(err);
    }

    let mut records = cached_records;
    records.extend(fetched_records.into_inner().unwrap());

    // Calculate file hotspots across all open PRs
    let mut file_freq: HashMap<String, usize> = HashMap::new();
    for r in &records {
        for f in &r.files {
            *file_freq.entry(f.clone()).or_insert(0) += 1;
        }
    }
    let hotspots: HashSet<String> = file_freq
        .into_iter()
        .filter_map(|(k, v)| if v >= 15 { Some(k) } else { None })
        .collect();

    for r in &mut records {
        let pr_hotspots: Vec<String> = r
            .files
            .iter()
            .filter(|f| hotspots.contains(*f))
            .cloned()
            .collect();
        let core_hotspots_count = pr_hotspots
            .iter()
            .filter(|f| !is_test_or_boilerplate_hotspot(f))
            .count();
        let code_paths_count = r
            .files
            .iter()
            .filter(|f| !is_boilerplate_file(f))
            .count();
        let eff_count = if code_paths_count > 0 {
            code_paths_count
        } else {
            r.files.len()
        };

        let risk = if core_hotspots_count > 0 || eff_count > 10 || r.total_lines > 1500 {
            "high"
        } else if eff_count >= 4 || r.total_lines > 300 || !pr_hotspots.is_empty() {
            "medium"
        } else {
            "low"
        };
        r.risk_level = risk.into();
        r.hotspots = pr_hotspots;
    }

    records.sort_by_key(|r| r.number);
    Ok((records, states))
}

fn compute_complexity(p: &PullRequest) -> (&'static str, usize, &'static str) {
    let tot = p.additions.max(0) + p.deletions.max(0);
    let paths = p.file_paths();
    let code_f = effective_code_files(&paths);
    let fc = if code_f.is_empty() {
        paths.len().max(1)
    } else {
        code_f.len()
    };
    let risk = p.risk_tier.to_ascii_lowercase();

    let mut score = 1;
    if tot > 1000 {
        score += 4;
    } else if tot > 300 {
        score += 2;
    } else if tot > 100 {
        score += 1;
    }

    if fc > 10 {
        score += 3;
    } else if fc > 4 {
        score += 1;
    }

    if risk == "high" {
        score += 2;
    } else if risk == "medium" {
        score += 1;
    }

    if score <= 2 {
        ("low", 10, "🟢 Easy (up to 10 PRs)")
    } else if score <= 5 {
        ("medium", 5, "🟡 Medium (up to 5 PRs)")
    } else {
        ("high", 2, "🔴 Complex (1-2 PRs)")
    }
}

fn assign_batch_metadata(
    members: &[&PullRequest],
    category: &str,
    tier: &str,
    label: &str,
    batch_type: &str,
    known_names: &HashMap<String, String>,
    next_number: &AtomicUsize,
) -> serde_json::Value {
    let mut numbers: Vec<u64> = members.iter().map(|p| p.number).collect();
    numbers.sort_unstable();
    let num_str = numbers
        .iter()
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let digest = &sha256_hex(num_str.as_bytes())[..24];
    let batch_id = format!("batch-{digest}");
    let title = known_names.get(&batch_id).cloned().unwrap_or_else(|| {
        let n = next_number.fetch_add(1, Ordering::SeqCst);
        format!("Batch {n}")
    });

    serde_json::json!({
        "id": batch_id,
        "title": title,
        "category": category,
        "complexity": tier,
        "complexity_label": label,
        "pr_count": numbers.len(),
        "pr_numbers": numbers,
        "status": "available",
        "claimed_by": "none",
        "batch_type": batch_type
    })
}

pub fn generate_3lane_batches(
    prs: &[PullRequest],
    existing: &[Batch],
    known_batch_names: &HashMap<String, String>,
) -> Vec<serde_json::Value> {
    let protected: Vec<&Batch> = existing
        .iter()
        .filter(|b| {
            b.status.as_deref().unwrap_or("available") != "available"
                || b.claimed_user_id.is_some()
                || b.claimed_by.as_deref().unwrap_or("none") != "none"
        })
        .collect();

    let reserved: HashSet<u64> = protected
        .iter()
        .flat_map(|b| b.pr_numbers.iter().copied())
        .collect();
    let pr_by_num: HashMap<u64, &PullRequest> = prs.iter().map(|p| (p.number, p)).collect();

    let mut reserved_code_files: HashSet<String> = HashSet::new();
    for b in &protected {
        for n in &b.pr_numbers {
            if let Some(p) = pr_by_num.get(n)
                && p.is_open()
            {
                for f in effective_code_files(&p.file_paths()) {
                    reserved_code_files.insert(f.to_string());
                }
            }
        }
    }

    let mut next_num = 1usize;
    for title in known_batch_names.values() {
        if let Some(stripped) = title.strip_prefix("Batch ")
            && let Ok(n) = stripped.parse::<usize>()
        {
            next_num = next_num.max(n + 1);
        }
    }
    let next_number = AtomicUsize::new(next_num);

    let mut eligible: Vec<&PullRequest> = Vec::new();
    for pr in prs {
        if !reserved.contains(&pr.number)
            && pr.is_open()
            && !pr.is_draft
            && !pr.files.is_empty()
            && pr.assigned_user_id.is_none()
            && pr.assigned_reviewer.as_deref().unwrap_or("none") == "none"
        {
            let pr_paths = pr.file_paths();
            let code_files = effective_code_files(&pr_paths);
            let overlaps = code_files
                .iter()
                .any(|f| reserved_code_files.contains(*f));
            if !overlaps {
                eligible.push(pr);
            }
        }
    }

    let mut result: Vec<serde_json::Value> = Vec::new();

    // Lane 1: Cohesive Module Clusters (2+ PRs targeting exact same 1-4 code files)
    let mut by_cat_and_code: HashMap<(String, Vec<String>), Vec<&PullRequest>> = HashMap::new();
    for pr in eligible {
        let cat = if pr.category.is_empty() {
            "Other Core Subsystems".to_string()
        } else {
            pr.category.clone()
        };
        let mut cf: Vec<String> = effective_code_files(&pr.file_paths())
            .into_iter()
            .map(String::from)
            .collect();
        cf.sort();
        cf.dedup();
        by_cat_and_code.entry((cat, cf)).or_default().push(pr);
    }

    let mut remaining_by_cat: HashMap<String, Vec<&PullRequest>> = HashMap::new();
    let mut sorted_keys: Vec<_> = by_cat_and_code.keys().cloned().collect();
    sorted_keys.sort();

    for key in sorted_keys {
        let (category, cf) = key;
        let mut group = by_cat_and_code.remove(&(category.clone(), cf.clone())).unwrap();
        if group.len() >= 2 && !cf.is_empty() && cf.len() <= 4 {
            group.sort_by_key(|p| (p.additions.max(0) + p.deletions.max(0), p.number));
            for chunk in group.chunks(10) {
                if chunk.len() >= 2 {
                    let first_file = cf[0].split('/').last().unwrap_or(&cf[0]);
                    let label = if cf.len() > 1 {
                        format!("🔵 Module Cluster ({} +{})", first_file, cf.len() - 1)
                    } else {
                        format!("🔵 Module Cluster ({})", first_file)
                    };
                    result.push(assign_batch_metadata(
                        chunk,
                        &category,
                        "medium",
                        &label,
                        "cohesive_cluster",
                        known_batch_names,
                        &next_number,
                    ));
                } else {
                    remaining_by_cat
                        .entry(category.clone())
                        .or_default()
                        .extend(chunk);
                }
            }
        } else {
            remaining_by_cat
                .entry(category)
                .or_default()
                .extend(group);
        }
    }

    // Lane 2: Fast-Track Strict 100% File-Disjoint Trains
    let mut leftover_by_cat: HashMap<String, Vec<&PullRequest>> = HashMap::new();
    let mut categories: Vec<String> = remaining_by_cat.keys().cloned().collect();
    categories.sort();

    for category in categories {
        let mut pool = remaining_by_cat.remove(&category).unwrap();
        pool.sort_by_key(|p| {
            let (tier, _, _) = compute_complexity(p);
            let tier_num = match tier {
                "low" => 0,
                "medium" => 1,
                _ => 2,
            };
            (tier_num, p.additions.max(0) + p.deletions.max(0), p.number)
        });

        while !pool.is_empty() {
            let lead = pool.remove(0);
            let (tier, mut limit, label) = compute_complexity(lead);
            if tier == "high" {
                limit = limit.max(3);
            } else if tier == "medium" {
                limit = limit.max(6);
            }
            let mut members = vec![lead];
            let mut paths: BTreeSet<&str> = lead.file_paths().into_iter().collect();

            let mut i = 0;
            while i < pool.len() && members.len() < limit {
                let cand = pool[i];
                let (cand_tier, _, _) = compute_complexity(cand);
                let compatible = (cand_tier == tier)
                    || (tier == "low" && cand_tier == "medium")
                    || (tier == "medium" && cand_tier == "low");
                let cand_paths = cand.file_paths();
                let overlaps = cand_paths.iter().any(|f| paths.contains(f));
                if compatible && !overlaps {
                    members.push(cand);
                    paths.extend(cand_paths);
                    pool.remove(i);
                } else {
                    i += 1;
                }
            }

            if members.len() >= 2 {
                result.push(assign_batch_metadata(
                    &members,
                    &category,
                    tier,
                    label,
                    "disjoint",
                    known_batch_names,
                    &next_number,
                ));
            } else {
                leftover_by_cat
                    .entry(category.clone())
                    .or_default()
                    .push(lead);
            }
        }
    }

    // Lane 3: Boilerplate-Tolerant Code-Disjoint Trains (only CMakeLists.txt / docs/dev/debt/ overlap)
    let mut leftover_categories: Vec<String> = leftover_by_cat.keys().cloned().collect();
    leftover_categories.sort();

    for category in leftover_categories {
        let mut pool = leftover_by_cat.remove(&category).unwrap();
        pool.sort_by_key(|p| {
            let (tier, _, _) = compute_complexity(p);
            let tier_num = match tier {
                "low" => 0,
                "medium" => 1,
                _ => 2,
            };
            (tier_num, p.additions.max(0) + p.deletions.max(0), p.number)
        });

        while !pool.is_empty() {
            let lead = pool.remove(0);
            let (tier, limit, _) = compute_complexity(lead);
            let lead_paths = lead.file_paths();
            let mut members = vec![lead];
            let mut c_paths: BTreeSet<String> =
                effective_code_files(&lead_paths).into_iter().map(String::from).collect();

            let mut i = 0;
            while i < pool.len() && members.len() < limit {
                let cand = pool[i];
                let cand_paths = cand.file_paths();
                let cand_cpaths = effective_code_files(&cand_paths);
                let overlaps = cand_cpaths.iter().any(|f| c_paths.contains(*f));
                if !overlaps {
                    members.push(cand);
                    for f in cand_cpaths {
                        c_paths.insert(f.to_string());
                    }
                    pool.remove(i);
                } else {
                    i += 1;
                }
            }

            if members.len() >= 2 {
                result.push(assign_batch_metadata(
                    &members,
                    &category,
                    tier,
                    "🟢 Auto-Rebaseable (CMake/Debt Only)",
                    "soft_disjoint",
                    known_batch_names,
                    &next_number,
                ));
            }
        }
    }

    result
}

pub fn run_full_sync(
    existing_prs_map: &HashMap<u64, PullRequest>,
    existing_batches: &[Batch],
    slot_index: usize,
    active_count: usize,
) -> Result<String, String> {
    let (raw_records, states) =
        fetch_github_prs(existing_prs_map, slot_index, active_count)?;

    let key = SUPABASE_SERVICE_ROLE_KEY;
    let auth = format!("Bearer {key}");

    // 1. Sync snapshot into public.prs
    let sync_url = format!("{SUPABASE_URL}/rest/v1/rpc/sync_github_snapshot");
    let sync_payload = serde_json::json!({
        "p_records": raw_records,
        "p_states": states
    });
    HTTP_AGENT
        .post(&sync_url)
        .header("apikey", key)
        .header("Authorization", &auth)
        .header("Content-Type", "application/json")
        .send_json(&sync_payload)
        .map_err(|e| format!("sync_github_snapshot failed: {e}"))?;

    // 2. Clean up non-open PRs not locked in batches
    let claimed_prs: Vec<u64> = existing_batches
        .iter()
        .flat_map(|b| b.pr_numbers.iter().copied())
        .collect();
    let delete_query = if claimed_prs.is_empty() {
        "prs?github_state=in.(merged,closed)".to_string()
    } else {
        let pr_list = claimed_prs
            .iter()
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(",");
        format!("prs?github_state=in.(merged,closed)&number=not.in.({pr_list})")
    };
    let del_url = format!("{SUPABASE_URL}/rest/v1/{delete_query}");
    let _ = HTTP_AGENT
        .delete(&del_url)
        .header("apikey", key)
        .header("Authorization", &auth)
        .call();

    // 3. Load fresh PR snapshot for batch generator
    let prs = supabase::fetch_prs()?;
    let batches = supabase::fetch_batches()?;

    // 4. Load known batch names
    let names: Vec<BatchNameRow> = supabase::fetch_table("batch_names", "batch_number.asc")
        .unwrap_or_default();
    let mut known_names: HashMap<String, String> = HashMap::new();
    for b in &batches {
        if b.title.starts_with("Batch ") {
            known_names.insert(b.id.clone(), b.title.clone());
        }
    }
    for row in names {
        known_names.insert(row.batch_id, format!("Batch {}", row.batch_number));
    }

    // 5. Generate 3-Lane Smart Batches in Rust (< 3ms)
    let new_batches = generate_3lane_batches(&prs, &batches, &known_names);

    // 6. Replace available batches atomically under PostgreSQL advisory lock
    let replace_url = format!("{SUPABASE_URL}/rest/v1/rpc/replace_available_batches");
    let replace_payload = serde_json::json!({
        "p_batches": new_batches
    });
    HTTP_AGENT
        .post(&replace_url)
        .header("apikey", key)
        .header("Authorization", &auth)
        .header("Content-Type", "application/json")
        .send_json(&replace_payload)
        .map_err(|e| format!("replace_available_batches failed: {e}"))?;

    Ok(format!(
        "Synced {} open PRs & generated {} available batches",
        raw_records.len(),
        new_batches.len()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_pr(number: u64, files: Vec<&str>, cat: &str) -> PullRequest {
        PullRequest {
            number,
            title: format!("Test PR #{number}"),
            author: "tester".into(),
            additions: 10,
            deletions: 5,
            files: files
                .into_iter()
                .map(|p| crate::models::PrFile {
                    path: Some(p.into()),
                    additions: None,
                    deletions: None,
                    change_type: None,
                })
                .collect(),
            status: "unassigned".into(),
            category: cat.into(),
            risk_tier: "low".into(),
            is_draft: false,
            github_state: Some("open".into()),
            body: None,
            notes: None,
            checklist: serde_json::json!({}),
            assigned_user_id: None,
            assigned_reviewer: None,
            head_sha: Some("sha".into()),
        }
    }

    #[test]
    fn test_sha256_output() {
        assert_eq!(
            sha256_hex(b"1234"),
            "03ac674216f3e15c761ee1a5e255f067953623c8b388b4459e13f978d7c846f4"
        );
    }

    #[test]
    fn test_smart_mode_cohesive_cluster() {
        let p1 = test_pr(1, vec!["libc/src/Math.cpp"], "LibC & Base Runtime");
        let p2 = test_pr(2, vec!["libc/src/Math.cpp"], "LibC & Base Runtime");
        let p3 = test_pr(3, vec!["libc/src/Math.cpp", "CMakeLists.txt"], "LibC & Base Runtime");
        let p4 = test_pr(4, vec!["isolated_singleton.cpp"], "LibC & Base Runtime");

        let prs = vec![p1, p2, p3, p4];
        let batches = generate_3lane_batches(&prs, &[], &HashMap::new());
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0]["batch_type"], "cohesive_cluster");
        let pr_nums: Vec<u64> = serde_json::from_value(batches[0]["pr_numbers"].clone()).unwrap();
        assert_eq!(pr_nums, vec![1, 2, 3]);
    }

    #[test]
    fn test_smart_mode_soft_disjoint_allows_shared_boilerplate() {
        let p1 = test_pr(10, vec!["src/core/A.cpp", "CMakeLists.txt", "docs/dev/debt/DEBT.md"], "Kernel & Syscalls");
        let p2 = test_pr(11, vec!["src/core/B.cpp", "CMakeLists.txt", "docs/dev/debt/DEBT.md"], "Kernel & Syscalls");

        let prs = vec![p1, p2];
        let batches = generate_3lane_batches(&prs, &[], &HashMap::new());
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0]["batch_type"], "soft_disjoint");
        let pr_nums: Vec<u64> = serde_json::from_value(batches[0]["pr_numbers"].clone()).unwrap();
        assert_eq!(pr_nums, vec![10, 11]);
    }
}
