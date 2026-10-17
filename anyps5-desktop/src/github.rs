use serde::Deserialize;
use std::{
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[derive(Deserialize)]
pub struct Account {
    pub login: String,
    pub avatar_url: String,
}

pub fn account() -> Result<(Account, Option<eframe::egui::ColorImage>), String> {
    let mut command = Command::new("gh");
    command
        .args([
            "api",
            "--hostname",
            "github.com",
            "user",
            "--jq",
            "{login: .login, avatar_url: .avatar_url}",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .spawn()
        .map_err(|_| "Install GitHub CLI to sign in.".to_string())?;
    let started = Instant::now();
    loop {
        if child
            .try_wait()
            .map_err(|_| "Could not read GitHub account.")?
            .is_some()
        {
            break;
        }
        if started.elapsed() > Duration::from_secs(15) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("GitHub account check timed out. Retry when online.".into());
        }
        thread::sleep(Duration::from_millis(50));
    }
    let output = child
        .wait_with_output()
        .map_err(|_| "Could not read GitHub account.")?;
    if !output.status.success() {
        return Err("Sign in to GitHub or retry the account check.".into());
    }
    let account: Account =
        serde_json::from_slice(&output.stdout).map_err(|_| "Invalid GitHub account response.")?;
    if account.login.is_empty() {
        return Err("GitHub username is unavailable.".into());
    }
    let avatar = avatar(&account.avatar_url);
    Ok((account, avatar))
}

fn avatar(url: &str) -> Option<eframe::egui::ColorImage> {
    if !url.starts_with("https://avatars.githubusercontent.com/") {
        return None;
    }
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(5)))
        .build();
    let agent: ureq::Agent = config.into();
    let mut response = agent.get(url).call().ok()?;
    let bytes = response
        .body_mut()
        .with_config()
        .limit(2 * 1024 * 1024)
        .read_to_vec()
        .ok()?;
    let decoded = image::load_from_memory(&bytes)
        .ok()?
        .thumbnail(80, 80)
        .into_rgba8();
    Some(eframe::egui::ColorImage::from_rgba_unmultiplied(
        [decoded.width() as usize, decoded.height() as usize],
        decoded.as_raw(),
    ))
}

pub fn login() -> Result<(), String> {
    // GitHub CLI owns its OAuth flow and credential storage. Never read its token.
    #[cfg(windows)]
    let mut command = {
        use std::os::windows::process::CommandExt;
        let mut command = Command::new("cmd.exe");
        command.args([
            "/c",
            "gh",
            "auth",
            "login",
            "--hostname",
            "github.com",
            "--web",
            "--clipboard",
            "--git-protocol",
            "https",
        ]);
        command.creation_flags(0x00000010);
        command
    };
    #[cfg(not(windows))]
    let mut command = {
        let mut command = Command::new("gh");
        command.args([
            "auth",
            "login",
            "--hostname",
            "github.com",
            "--web",
            "--clipboard",
            "--git-protocol",
            "https",
        ]);
        command
    };
    let status = command
        .status()
        .map_err(|_| "Could not open GitHub login. Install GitHub CLI first.")?;
    if status.success() {
        Ok(())
    } else {
        Err("GitHub login was cancelled or failed. You can try again.".into())
    }
}

pub fn logout(login: &str) -> Result<(), String> {
    let mut command = Command::new("gh");
    command
        .args([
            "auth",
            "logout",
            "--hostname",
            "github.com",
            "--user",
            login,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let status = command
        .status()
        .map_err(|_| "Could not sign out. Try again.".to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("GitHub sign-out failed. Your profile is still signed in.".into())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckResult {
    Failed,
    Running,
    Unknown,
    Skipped,
    Passed,
}
impl CheckResult {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Failed => "Failed",
            Self::Running => "Running",
            Self::Unknown => "Unknown",
            Self::Skipped => "Skipped",
            Self::Passed => "Passed",
        }
    }
    fn priority(&self) -> u8 {
        match self {
            Self::Failed => 0,
            Self::Running => 1,
            Self::Unknown => 2,
            Self::Skipped => 3,
            Self::Passed => 4,
        }
    }
}
pub struct CheckEntry {
    pub name: String,
    pub result: CheckResult,
    pub url: Option<String>,
    pub duration: String,
    pub source: &'static str,
}
pub struct Checks {
    pub head: String,
    pub summary: String,
    pub mergeability: String,
    pub entries: Vec<CheckEntry>,
    pub fetched: Instant,
    pub head_changed: bool,
}
impl Checks {
    pub fn label(&self) -> &'static str {
        if self.head_changed {
            "Head changed"
        } else if self.entries.iter().any(|e| e.result == CheckResult::Failed) {
            "CI failed"
        } else if self
            .entries
            .iter()
            .any(|e| e.result == CheckResult::Running)
        {
            "CI running"
        } else if self.entries.is_empty() {
            "No checks reported"
        } else if self
            .entries
            .iter()
            .any(|e| e.result == CheckResult::Unknown)
        {
            "CI unknown"
        } else if self
            .entries
            .iter()
            .all(|e| e.result == CheckResult::Skipped)
        {
            "Checks skipped"
        } else {
            "Reported checks passed"
        }
    }
}
fn result(status: &str, conclusion: Option<&str>) -> CheckResult {
    if status != "completed" {
        return CheckResult::Running;
    }
    match conclusion {
        Some("success") => CheckResult::Passed,
        Some("neutral" | "skipped") => CheckResult::Skipped,
        Some(
            "failure" | "cancelled" | "timed_out" | "action_required" | "startup_failure" | "stale",
        ) => CheckResult::Failed,
        _ => CheckResult::Unknown,
    }
}
fn duration(start: Option<&str>, end: Option<&str>) -> String {
    let parse =
        |s| time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok();
    if let Some((start, end)) = start.and_then(parse).zip(end.and_then(parse)) {
        let seconds = (end - start).whole_seconds().max(0);
        format!("{}m {:02}s", seconds / 60, seconds % 60)
    } else {
        "—".into()
    }
}

fn api_summary(path: &str, query: &str) -> Result<serde_json::Value, String> {
    let mut command = Command::new("gh");
    command
        .args(["api", "--hostname", "github.com", path, "--jq", query])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(|_| "GitHub CLI is unavailable")?;
    let stdout = child.stdout.take().ok_or("GitHub output unavailable")?;
    let reader = thread::spawn(move || {
        use std::io::Read;
        let mut bytes = Vec::new();
        stdout
            .take(8 * 1024 * 1024)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let start = Instant::now();
    loop {
        if child
            .try_wait()
            .map_err(|_| "GitHub request failed")?
            .is_some()
        {
            break;
        }
        if start.elapsed() > Duration::from_secs(25) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("GitHub request timed out".into());
        }
        thread::sleep(Duration::from_millis(50));
    }
    let output = child
        .wait_with_output()
        .map_err(|_| "GitHub response unavailable")?;
    if !output.status.success() {
        return Err("GitHub request failed. Check CLI sign-in and connection.".into());
    }
    let bytes = reader
        .join()
        .map_err(|_| "GitHub output reader failed")?
        .map_err(|_| "GitHub output read failed")?;
    serde_json::from_slice(&bytes).map_err(|_| "Invalid GitHub checks response".into())
}
fn pages(
    mut fetch: impl FnMut(usize) -> Result<Vec<serde_json::Value>, String>,
) -> Result<Vec<serde_json::Value>, String> {
    let mut rows = Vec::new();
    for page in 1..=1000 {
        let values = fetch(page)?;
        if values.is_empty() {
            return Ok(rows);
        }
        rows.extend(values);
    }
    Err("Too many check pages; no complete result available".into())
}
pub fn description(number: u64, expected_sha: Option<&str>) -> Result<String, String> {
    let value = api_summary(
        &format!("repos/boykopovar/AnyPS5/pulls/{number}"),
        "{body,head:.head.sha}",
    )?;
    if expected_sha.is_some_and(|sha| value["head"].as_str() != Some(sha)) {
        return Err(
            "The PR head has changed. Refresh the batch before loading its description.".into(),
        );
    }
    match &value["body"] {
        serde_json::Value::Null => Ok(String::new()),
        serde_json::Value::String(body) => Ok(body.clone()),
        _ => Err("Invalid PR description response".into()),
    }
}

pub fn checks(number: u64) -> Result<Checks, String> {
    let base = "repos/boykopovar/AnyPS5";
    let query = "{head:.head.sha,mergeable:.mergeable,state:.state,draft:.draft}";
    let pr = api_summary(&format!("{base}/pulls/{number}"), query)?;
    let head = pr["head"].as_str().ok_or("No GitHub head SHA")?.to_string();
    let runs = pages(|page| {
        let value = api_summary(
            &format!("{base}/commits/{head}/check-runs?filter=latest&per_page=100&page={page}"),
            ".check_runs|map({name,status,conclusion,html_url,started_at,completed_at})",
        )?;
        value
            .as_array()
            .cloned()
            .ok_or("Invalid check-run page".into())
    })?;
    let statuses = pages(|page| {
        let value = api_summary(
            &format!("{base}/commits/{head}/statuses?per_page=100&page={page}"),
            "map({context,state,target_url})",
        )?;
        value
            .as_array()
            .cloned()
            .ok_or("Invalid status page".into())
    })?;
    let mut entries: Vec<_> = runs
        .into_iter()
        .map(|run| CheckEntry {
            name: run["name"].as_str().unwrap_or("Unnamed check").into(),
            result: result(
                run["status"].as_str().unwrap_or("unknown"),
                run["conclusion"].as_str(),
            ),
            url: run["html_url"].as_str().map(str::to_string),
            duration: duration(run["started_at"].as_str(), run["completed_at"].as_str()),
            source: "Check run",
        })
        .collect();
    let mut contexts = std::collections::HashSet::new();
    for status in statuses {
        let name = status["context"].as_str().unwrap_or("Unnamed status");
        if !contexts.insert(name.to_string()) {
            continue;
        } // newest per context
        entries.push(CheckEntry {
            name: name.into(),
            result: match status["state"].as_str() {
                Some("success") => CheckResult::Passed,
                Some("failure" | "error") => CheckResult::Failed,
                Some("pending") => CheckResult::Running,
                _ => CheckResult::Unknown,
            },
            url: status["target_url"].as_str().map(str::to_string),
            duration: "—".into(),
            source: "Commit status",
        });
    }
    entries.sort_by_key(|e| (e.result.priority(), e.name.clone()));
    let summary = format!(
        "{} passed · {} failed · {} running · {} skipped · {} unknown",
        entries
            .iter()
            .filter(|e| e.result == CheckResult::Passed)
            .count(),
        entries
            .iter()
            .filter(|e| e.result == CheckResult::Failed)
            .count(),
        entries
            .iter()
            .filter(|e| e.result == CheckResult::Running)
            .count(),
        entries
            .iter()
            .filter(|e| e.result == CheckResult::Skipped)
            .count(),
        entries
            .iter()
            .filter(|e| e.result == CheckResult::Unknown)
            .count()
    );
    let latest = api_summary(&format!("{base}/pulls/{number}"), query)?;
    let head_changed = latest["head"].as_str() != Some(head.as_str());
    let mergeability = if latest["state"] != "open" {
        "PR is closed"
    } else if latest["draft"] == true {
        "Draft PR"
    } else {
        match latest["mergeable"].as_bool() {
            Some(true) => "No merge conflicts reported",
            Some(false) => "Merge conflicts reported",
            None => "Mergeability pending on GitHub",
        }
    }
    .into();
    Ok(Checks {
        head,
        summary,
        mergeability,
        entries,
        fetched: Instant::now(),
        head_changed,
    })
}

#[cfg(test)]
mod check_tests {
    use super::*;
    #[test]
    fn skipped_and_unknown_are_not_successes() {
        assert_eq!(result("completed", Some("skipped")), CheckResult::Skipped);
        assert_eq!(result("completed", None), CheckResult::Unknown);
        assert_eq!(result("completed", Some("cancelled")), CheckResult::Failed);
        assert_eq!(result("queued", None), CheckResult::Running);
    }
    #[test]
    fn pagination_continues_after_short_pages_and_fails_atomically() {
        let rows = pages(|page| {
            Ok(if page < 3 {
                vec![serde_json::json!(page)]
            } else {
                vec![]
            })
        })
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert!(
            pages(|page| if page == 1 {
                Ok(vec![serde_json::json!(1)])
            } else {
                Err("offline".into())
            })
            .is_err()
        );
    }
    #[test]
    fn run_duration_is_calculated() {
        assert_eq!(
            duration(Some("2026-10-17T10:00:00Z"), Some("2026-10-17T10:04:12Z")),
            "4m 12s"
        );
    }
}
