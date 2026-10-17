use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Serialize, Clone, PartialEq, Eq, Default)]
pub struct PrFile {
    pub path: Option<Box<str>>,
    pub additions: Option<i64>,
    pub deletions: Option<i64>,
    #[serde(rename = "changeType")]
    pub change_type: Option<Box<str>>,
}

impl<'de> Deserialize<'de> for PrFile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum RawFile {
            Path(String),
            Obj {
                path: Option<String>,
                additions: Option<i64>,
                deletions: Option<i64>,
                #[serde(rename = "changeType")]
                change_type: Option<String>,
            },
            Other(serde::de::IgnoredAny),
        }
        Ok(match RawFile::deserialize(deserializer)? {
            RawFile::Path(p) => Self {
                path: Some(p.into_boxed_str()),
                additions: None,
                deletions: None,
                change_type: None,
            },
            RawFile::Obj {
                path,
                additions,
                deletions,
                change_type,
            } => Self {
                path: path.map(String::into_boxed_str),
                additions,
                deletions,
                change_type: change_type.map(String::into_boxed_str),
            },
            RawFile::Other(_) => Self::default(),
        })
    }
}

fn deserialize_author<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum RawAuthor {
        Name(String),
        Obj {
            login: Option<String>,
            name: Option<String>,
        },
        Other(serde::de::IgnoredAny),
    }
    Ok(match RawAuthor::deserialize(deserializer)? {
        RawAuthor::Name(s) if !s.is_empty() => s,
        RawAuthor::Obj { login, name } => login
            .or(name)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "unknown".into()),
        _ => "unknown".into(),
    })
}

fn default_author() -> String {
    "unknown".into()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PullRequest {
    pub number: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default = "default_author", deserialize_with = "deserialize_author")]
    pub author: String,
    #[serde(default = "missing_churn")]
    pub additions: i64,
    #[serde(default = "missing_churn")]
    pub deletions: i64,
    #[serde(default)]
    pub files: Vec<PrFile>,
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

    pub fn author_name(&self) -> &str {
        if self.author.is_empty() {
            "unknown"
        } else {
            &self.author
        }
    }

    pub fn file_paths(&self) -> Vec<&str> {
        self.files
            .iter()
            .filter_map(|f| f.path.as_deref())
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
