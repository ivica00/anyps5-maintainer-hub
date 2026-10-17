use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PullRequest {
    pub number: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub author: serde_json::Value,
    #[serde(default = "missing_churn")]
    pub additions: i64,
    #[serde(default = "missing_churn")]
    pub deletions: i64,
    #[serde(default)]
    pub files: Vec<serde_json::Value>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub risk_tier: String,
    #[serde(default)]
    pub is_draft: bool,
    #[serde(default)]
    pub github_state: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub checklist: serde_json::Value,
    #[serde(default)]
    pub assigned_user_id: Option<String>,
    #[serde(default)]
    pub assigned_reviewer: Option<String>,
    #[serde(default)]
    pub head_sha: Option<String>,
}

fn missing_churn() -> i64 {
    -1
}

impl PullRequest {
    pub fn diff_summary(&self) -> String {
        if self.additions < 0 || self.deletions < 0 {
            "Unavailable".into()
        } else {
            format!("+{} / -{}", self.additions, self.deletions)
        }
    }

    pub fn author_name(&self) -> String {
        if let Some(s) = self.author.as_str() {
            s.to_string()
        } else if let Some(obj) = self.author.as_object() {
            obj.get("login")
                .or_else(|| obj.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string()
        } else {
            "unknown".to_string()
        }
    }

    pub fn file_paths(&self) -> Vec<String> {
        self.files
            .iter()
            .filter_map(|f| {
                if let Some(s) = f.as_str() {
                    Some(s.to_string())
                } else if let Some(obj) = f.as_object() {
                    obj.get("path")
                        .and_then(|p| p.as_str())
                        .map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn is_open(&self) -> bool {
        self.github_state.as_deref().unwrap_or("open") == "open"
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Batch {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub complexity: String,
    #[serde(default)]
    pub complexity_label: Option<String>,
    #[serde(default)]
    pub pr_numbers: Vec<u64>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub reviewer: Option<String>,
    #[serde(default)]
    pub claimed_by: Option<String>,
    #[serde(default)]
    pub claimed_user_id: Option<String>,
    #[serde(default)]
    pub claimed_at: Option<String>,
    #[serde(default)]
    pub batch_type: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SystemInfo {
    pub os: String,
    pub arch: String,
    pub cpu_cores: usize,
    pub has_git: bool,
    pub has_cmake: bool,
    pub has_ninja: bool,
    pub has_clang: bool,
}

impl Batch {
    pub fn owner(&self) -> Option<&str> {
        self.claimed_by
            .as_deref()
            .or(self.reviewer.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("none"))
    }
    pub fn available(&self) -> bool {
        self.status.as_deref() == Some("available")
            && self.owner().is_none()
            && self.claimed_user_id.is_none()
    }
    pub fn workflow_label(&self) -> &str {
        if self.available() {
            "Available"
        } else {
            match self.status.as_deref() {
                Some("claimed" | "in-review") => "In review",
                Some("available") => "Assigned",
                Some(status) if !status.is_empty() => status,
                _ => "Unknown status",
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absent_diff_stats_are_distinct_from_zero_changes() {
        let missing: PullRequest = serde_json::from_str(r#"{"number":1}"#).unwrap();
        let zero: PullRequest =
            serde_json::from_str(r#"{"number":2,"additions":0,"deletions":0}"#).unwrap();
        assert_eq!(missing.diff_summary(), "Unavailable");
        assert_eq!(zero.diff_summary(), "+0 / -0");
    }
}
