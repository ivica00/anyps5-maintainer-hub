use crate::models::{Batch, PullRequest};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{fs, io::BufReader, path::PathBuf, sync::LazyLock, time::Duration};

pub const SUPABASE_URL: &str = "https://nzaxslgwsygpgwqcllpi.supabase.co";
pub const SUPABASE_ANON_KEY: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6Im56YXhzbGd3c3lncGd3cWNsbHBpIiwicm9sZSI6ImFub24iLCJpYXQiOjE3OTIxNTU2NjksImV4cCI6MjEwNzczMTY2OX0.G_sibL-5v-wxWagMeY1eAXZXm2b5Bsh0nziN94a4moY";
pub const SUPABASE_SERVICE_ROLE_KEY: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6Im56YXhzbGd3c3lncGd3cWNsbHBpIiwicm9sZSI6InNlcnZpY2Vfcm9sZSIsImlhdCI6MTc5MjE1NTY2OSwiZXhwIjoyMTA3NzMxNjY5fQ.Oi_Em9zcjWQNlxZAvIOePWIMOzbXeV6XTdPWrNQFymI";

pub static HTTP_AGENT: LazyLock<ureq::Agent> = LazyLock::new(|| {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .build()
        .into()
});

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HeartbeatResponse {
    pub data_version: i64,
    pub last_synced_at: Option<String>,
    pub lease_owner: Option<String>,
    pub lease_user_label: Option<String>,
    pub lease_until: Option<String>,
    pub active_count: usize,
    pub slot_index: usize,
    #[serde(default)]
    pub open_pr_numbers: Vec<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LeaseResponse {
    pub acquired: bool,
    pub reason: Option<String>,
    pub lease_owner: Option<String>,
    pub lease_user_label: Option<String>,
    pub lease_until: Option<String>,
    pub last_synced_at: Option<String>,
}

pub fn service_key() -> &'static str {
    SUPABASE_SERVICE_ROLE_KEY
}

// Continue until an empty page, including when the server caps page sizes.
pub fn collect_pages<T>(
    mut fetch: impl FnMut(usize) -> Result<Vec<T>, String>,
) -> Result<Vec<T>, String> {
    let mut rows = Vec::new();
    loop {
        let page = fetch(rows.len())?;
        if page.is_empty() {
            return Ok(rows);
        }
        rows.extend(page);
    }
}

pub fn fetch_table<T: DeserializeOwned>(table: &str, order: &str) -> Result<Vec<T>, String> {
    let key = service_key();
    let auth = format!("Bearer {key}");
    collect_pages(|offset| {
        let url = format!(
            "{SUPABASE_URL}/rest/v1/{table}?select=*&order={order}&limit=500&offset={offset}"
        );
        let mut response = HTTP_AGENT
            .get(&url)
            .header("apikey", key)
            .header("Authorization", &auth)
            .call()
            .map_err(|e| format!("Fetching {table}: {e}"))?;
        response
            .body_mut()
            .read_json()
            .map_err(|e| format!("Invalid {table} data: {e}"))
    })
}

pub fn fetch_batches() -> Result<Vec<Batch>, String> {
    fetch_table("batches", "id.asc")
}

pub fn fetch_prs() -> Result<Vec<PullRequest>, String> {
    fetch_table("prs", "number.desc")
}

pub fn fetch_prs_delta(since_updated_at: &str) -> Result<Vec<PullRequest>, String> {
    let key = service_key();
    let auth = format!("Bearer {key}");
    let url = format!(
        "{SUPABASE_URL}/rest/v1/prs?select=*&updated_at=gt.{since_updated_at}&order=number.desc&limit=500"
    );
    let mut response = HTTP_AGENT
        .get(&url)
        .header("apikey", key)
        .header("Authorization", &auth)
        .call()
        .map_err(|e| format!("Fetching delta PRs: {e}"))?;
    response
        .body_mut()
        .read_json()
        .map_err(|e| format!("Invalid delta PR data: {e}"))
}

pub fn sync_heartbeat(instance_id: &str, user_label: &str) -> Result<HeartbeatResponse, String> {
    let key = service_key();
    let auth = format!("Bearer {key}");
    let url = format!("{SUPABASE_URL}/rest/v1/rpc/sync_heartbeat");
    let payload = serde_json::json!({
        "p_instance_id": instance_id,
        "p_user_label": user_label
    });
    let mut response = HTTP_AGENT
        .post(&url)
        .header("apikey", key)
        .header("Authorization", &auth)
        .header("Content-Type", "application/json")
        .send_json(&payload)
        .map_err(|e| format!("Heartbeat failed: {e}"))?;
    response
        .body_mut()
        .read_json()
        .map_err(|e| format!("Invalid heartbeat response: {e}"))
}

pub fn try_acquire_sync_lease(
    instance_id: &str,
    user_label: &str,
    min_interval_secs: i64,
    ttl_secs: i64,
) -> Result<LeaseResponse, String> {
    let key = service_key();
    let auth = format!("Bearer {key}");
    let url = format!("{SUPABASE_URL}/rest/v1/rpc/try_acquire_sync_lease");
    let payload = serde_json::json!({
        "p_instance_id": instance_id,
        "p_user_label": user_label,
        "p_min_interval_secs": min_interval_secs,
        "p_ttl_secs": ttl_secs
    });
    let mut response = HTTP_AGENT
        .post(&url)
        .header("apikey", key)
        .header("Authorization", &auth)
        .header("Content-Type", "application/json")
        .send_json(&payload)
        .map_err(|e| format!("Acquire lease failed: {e}"))?;
    response
        .body_mut()
        .read_json()
        .map_err(|e| format!("Invalid lease response: {e}"))
}

pub fn finish_sync_lease(instance_id: &str) -> Result<(), String> {
    let key = service_key();
    let auth = format!("Bearer {key}");
    let url = format!("{SUPABASE_URL}/rest/v1/rpc/finish_sync_lease");
    let payload = serde_json::json!({
        "p_instance_id": instance_id
    });
    let _ = HTTP_AGENT
        .post(&url)
        .header("apikey", key)
        .header("Authorization", &auth)
        .header("Content-Type", "application/json")
        .send_json(&payload)
        .map_err(|e| format!("Finish lease failed: {e}"))?;
    Ok(())
}

pub fn find_resource(relative: &str) -> Option<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        roots.extend(parent.ancestors().take(5).map(PathBuf::from));
    }
    if let Ok(cwd) = std::env::current_dir() {
        roots.extend(cwd.ancestors().take(3).map(PathBuf::from));
    }
    roots
        .into_iter()
        .map(|root| root.join(relative))
        .find(|p| p.is_file())
}

pub fn load_local_prs() -> Option<Vec<PullRequest>> {
    let path = find_resource("all_prs.json")?;
    let file = fs::File::open(path).ok()?;
    serde_json::from_reader(BufReader::new(file)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pagination_continues_past_short_server_pages() {
        let mut offsets = Vec::new();
        let rows = collect_pages(|offset| {
            offsets.push(offset);
            Ok(if offset < 790 {
                (offset..(offset + 200).min(790)).collect()
            } else {
                vec![]
            })
        })
        .unwrap();
        assert_eq!(rows.len(), 790);
        assert_eq!(offsets, [0, 200, 400, 600, 790]);
    }
    #[test]
    fn partial_download_is_not_reported_as_complete() {
        assert!(
            collect_pages(|offset| if offset == 0 {
                Ok(vec![1])
            } else {
                Err("offline".into())
            })
            .is_err()
        );
    }
}
