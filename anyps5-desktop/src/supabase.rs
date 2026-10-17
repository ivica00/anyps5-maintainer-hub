use crate::models::{Batch, PullRequest};
use serde::de::DeserializeOwned;
use std::{fs, path::PathBuf, time::Duration};

pub const SUPABASE_URL: &str = "https://nzaxslgwsygpgwqcllpi.supabase.co";
pub const SUPABASE_ANON_KEY: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6Im56YXhzbGd3c3lncGd3cWNsbHBpIiwicm9sZSI6ImFub24iLCJpYXQiOjE3OTIxNTU2NjksImV4cCI6MjEwNzczMTY2OX0.G_sibL-5v-wxWagMeY1eAXZXm2b5Bsh0nziN94a4moY";

// Continue until an empty page, including when the server caps page sizes.
fn collect_pages<T>(
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

fn fetch_table<T: DeserializeOwned>(table: &str, order: &str) -> Result<Vec<T>, String> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .build();
    let agent: ureq::Agent = config.into();
    collect_pages(|offset| {
        let url = format!(
            "{SUPABASE_URL}/rest/v1/{table}?select=*&order={order}&limit=500&offset={offset}"
        );
        let mut response = agent
            .get(&url)
            .header("apikey", SUPABASE_ANON_KEY)
            .header("Authorization", &format!("Bearer {SUPABASE_ANON_KEY}"))
            .call()
            .map_err(|e| format!("Fetching {table}: {e}"))?;
        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|e| e.to_string())?;
        serde_json::from_str(&body).map_err(|e| format!("Invalid {table} data: {e}"))
    })
}

pub fn fetch_batches() -> Result<Vec<Batch>, String> {
    fetch_table("batches", "id.asc")
}
pub fn fetch_prs() -> Result<Vec<PullRequest>, String> {
    fetch_table("prs", "number.desc")
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
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
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
