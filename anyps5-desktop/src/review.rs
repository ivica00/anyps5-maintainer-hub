use crate::{
    models::{Batch, PullRequest},
    supabase::{HTTP_AGENT, SUPABASE_ANON_KEY, SUPABASE_SERVICE_ROLE_KEY, SUPABASE_URL},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ring::{
    digest,
    rand::{SecureRandom, SystemRandom},
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::TcpListener,
    time::{Duration, Instant},
};

#[allow(dead_code)]
fn url_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 8);
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => {
                out.push('%');
                out.push(
                    char::from_digit((b >> 4) as u32, 16)
                        .unwrap_or('0')
                        .to_ascii_uppercase(),
                );
                out.push(
                    char::from_digit((b & 0x0f) as u32, 16)
                        .unwrap_or('0')
                        .to_ascii_uppercase(),
                );
            }
        }
    }
    out
}

#[allow(dead_code)]
fn url_decode(input: &str) -> String {
    let mut bytes = Vec::with_capacity(input.len());
    let raw = input.as_bytes();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'+' {
            bytes.push(b' ');
            i += 1;
        } else if raw[i] == b'%' && i + 2 < raw.len() {
            if let (Some(h), Some(l)) = (
                (raw[i + 1] as char).to_digit(16),
                (raw[i + 2] as char).to_digit(16),
            ) {
                bytes.push(((h << 4) | l) as u8);
                i += 3;
                continue;
            }
            bytes.push(raw[i]);
            i += 1;
        } else {
            bytes.push(raw[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

#[allow(dead_code)]
fn parse_query(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            (url_decode(k), url_decode(v))
        })
        .collect()
}

#[allow(dead_code)]
pub struct Session {
    pub token: String,
    pub user_id: String,
    pub expires: Instant,
}
#[allow(dead_code)]
pub struct Login {
    listener: TcpListener,
    verifier: String,
    state: String,
    pub url: String,
    pub cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
#[allow(dead_code)]
fn random() -> Result<String, String> {
    let mut bytes = [0; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| "Could not prepare login")?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}
fn agent() -> &'static ureq::Agent {
    &HTTP_AGENT
}
#[allow(dead_code)]
pub fn prepare_login() -> Result<Login, String> {
    let listener =
        TcpListener::bind("127.0.0.1:39187").map_err(|_| "Login port 39187 is unavailable.")?;
    listener
        .set_nonblocking(true)
        .map_err(|_| "Could not start login listener")?;
    let verifier = random()?;
    let state = random()?;
    let challenge = URL_SAFE_NO_PAD.encode(digest::digest(&digest::SHA256, verifier.as_bytes()));
    let redirect = format!("http://127.0.0.1:39187/callback?state={state}");
    let url = format!(
        "{SUPABASE_URL}/auth/v1/authorize?provider=github&redirect_to={}&code_challenge={}&code_challenge_method=s256",
        url_encode(&redirect),
        url_encode(&challenge)
    );
    Ok(Login {
        listener,
        verifier,
        state,
        url,
        cancelled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    })
}
#[allow(dead_code)]
impl Login {
    pub fn finish(self) -> Result<Session, String> {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(180) {
            if self.cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                return Err("Login cancelled.".into());
            }
            match self.listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
                    let mut buf = [0; 8192];
                    let n = stream
                        .read(&mut buf)
                        .map_err(|_| "Could not read login callback")?;
                    let request = String::from_utf8_lossy(&buf[..n]);
                    let target = request
                        .lines()
                        .next()
                        .and_then(|l| l.split_whitespace().nth(1))
                        .unwrap_or("");
                    let (path, query) = target.split_once('?').unwrap_or((target, ""));
                    let params = parse_query(query);
                    if path != "/callback" || params.get("state") != Some(&self.state) {
                        let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                        continue;
                    }
                    let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nCache-Control: no-store\r\nConnection: close\r\n\r\nReturn to AnyPS5 to check the sign-in result.");
                    let code = params
                        .get("code")
                        .ok_or("GitHub authorization was cancelled or failed.")?;
                    let mut response = agent()
                        .post(format!("{SUPABASE_URL}/auth/v1/token?grant_type=pkce"))
                        .header("apikey", SUPABASE_ANON_KEY)
                        .send_json(json!({"auth_code":code,"code_verifier":self.verifier}))
                        .map_err(|_| "Could not exchange login code.")?;
                    let value: Value = response
                        .body_mut()
                        .read_json()
                        .map_err(|_| "Invalid login response")?;
                    if self.cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                        return Err("Login cancelled.".into());
                    }
                    return Ok(Session {
                        token: value["access_token"]
                            .as_str()
                            .ok_or("No access token")?
                            .into(),
                        user_id: value["user"]["id"].as_str().ok_or("No account ID")?.into(),
                        expires: Instant::now()
                            + Duration::from_secs(
                                value["expires_in"]
                                    .as_u64()
                                    .unwrap_or(3600)
                                    .saturating_sub(30),
                            ),
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(100))
                }
                Err(_) => return Err("Login listener failed".into()),
            }
        }
        Err("Login timed out. Ensure Supabase allows http://127.0.0.1:39187/callback and try again.".into())
    }
}

#[allow(dead_code)]
pub fn claim(token: &str, batch: &Batch, release: bool) -> Result<(), String> {
    let rpc = if release {
        "release_batch"
    } else {
        "claim_batch"
    };
    agent()
        .post(format!("{SUPABASE_URL}/rest/v1/rpc/{rpc}"))
        .header("apikey", SUPABASE_ANON_KEY)
        .header("Authorization", &format!("Bearer {token}"))
        .send_json(json!({"p_batch_id":batch.id}))
        .map_err(
            |_| "Action rejected. Check ownership, maintainer membership and current batch data.",
        )?;
    Ok(())
}

pub fn claim_direct(username: &str, batch_id: &str) -> Result<String, String> {
    let key = SUPABASE_SERVICE_ROLE_KEY;
    agent()
        .post(format!("{SUPABASE_URL}/rest/v1/rpc/maintainer_claim_batch"))
        .header("apikey", key)
        .header("Authorization", &format!("Bearer {key}"))
        .header("Content-Type", "application/json")
        .send_json(json!({
            "p_batch_id": batch_id,
            "p_username": username
        }))
        .map_err(|e| format!("Could not claim batch: {e}"))?;
    Ok(format!("Batch claimed for @{username}. Refreshing…"))
}

pub fn release_direct(batch_id: &str) -> Result<String, String> {
    let key = SUPABASE_SERVICE_ROLE_KEY;
    let mut resp = agent()
        .post(format!("{SUPABASE_URL}/rest/v1/rpc/maintainer_release_batch"))
        .header("apikey", key)
        .header("Authorization", &format!("Bearer {key}"))
        .header("Content-Type", "application/json")
        .send_json(json!({
            "p_batch_id": batch_id
        }))
        .map_err(|e| format!("Could not release batch: {e}"))?;
    let val: Value = resp.body_mut().read_json().unwrap_or(json!({}));
    if val["action"].as_str() == Some("dissolved_blocked") {
        Ok("Blocked batch dissolved & open PRs unlocked. Refreshing…".into())
    } else {
        Ok("Batch released back to available pool. Refreshing…".into())
    }
}

pub fn release_all_direct(username: &str) -> Result<String, String> {
    let key = SUPABASE_SERVICE_ROLE_KEY;
    let mut resp = agent()
        .post(format!("{SUPABASE_URL}/rest/v1/rpc/maintainer_release_all_batches"))
        .header("apikey", key)
        .header("Authorization", &format!("Bearer {key}"))
        .header("Content-Type", "application/json")
        .send_json(json!({
            "p_username": username
        }))
        .map_err(|e| format!("Could not unblock batches: {e}"))?;
    let val: Value = resp.body_mut().read_json().unwrap_or(json!({}));
    let count = val["released_batches"].as_u64().unwrap_or(0);
    Ok(format!("Unblocked {count} batch(es). Refreshing…"))
}

pub fn complete_direct(batch_id: &str) -> Result<String, String> {
    let key = SUPABASE_SERVICE_ROLE_KEY;
    agent()
        .post(format!("{SUPABASE_URL}/rest/v1/rpc/maintainer_complete_batch"))
        .header("apikey", key)
        .header("Authorization", &format!("Bearer {key}"))
        .header("Content-Type", "application/json")
        .send_json(json!({
            "p_batch_id": batch_id
        }))
        .map_err(|e| format!("Could not complete batch: {e}"))?;
    Ok("Batch completed & closed. Refreshing…".into())
}

#[derive(Clone)]
pub struct Draft {
    pub notes: String,
    pub checklist: Value,
    pub status: String,
    pub original: PullRequest,
}
impl Draft {
    pub fn new(pr: &PullRequest) -> Self {
        Self {
            notes: pr.notes.clone().unwrap_or_default(),
            checklist: if pr.checklist.is_object() {
                pr.checklist.clone()
            } else {
                json!({})
            },
            status: pr.status.clone(),
            original: pr.clone(),
        }
    }
}
fn review_payload(draft: &Draft) -> Result<Value, String> {
    if !matches!(draft.status.as_str(), "in-review" | "approved" | "changes") {
        return Err("Select a supported review decision before saving.".into());
    }
    let mut checklist = draft.checklist.clone();
    if let Some(sha) = &draft.original.head_sha {
        checklist["_reviewed_head_sha"] = json!(sha);
    } else if draft.status != "in-review" {
        return Err(
            "Cannot record a decision without a known head SHA. Refresh the PR first.".into(),
        );
    }
    Ok(json!({"notes":draft.notes,"checklist":checklist,"status":draft.status}))
}

#[allow(dead_code)]
pub fn save(token: &str, user: &str, draft: &Draft) -> Result<(), String> {
    let payload = review_payload(draft)?;
    let pr = &draft.original;
    let pairs = [
        ("number", format!("eq.{}", pr.number)),
        ("assigned_user_id", format!("eq.{user}")),
        (
            "head_sha",
            pr.head_sha
                .as_ref()
                .map(|s| format!("eq.{s}"))
                .unwrap_or_else(|| "is.null".into()),
        ),
        (
            "notes",
            pr.notes
                .as_ref()
                .map(|s| format!("eq.{s}"))
                .unwrap_or_else(|| "is.null".into()),
        ),
        ("status", format!("eq.{}", pr.status)),
        (
            "checklist",
            if pr.checklist.is_null() {
                "is.null".into()
            } else {
                format!("eq.{}", pr.checklist)
            },
        ),
    ];
    let query = pairs
        .iter()
        .map(|(k, v)| format!("{k}={}", url_encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    let url = format!("{SUPABASE_URL}/rest/v1/prs?{query}");
    let mut response = agent()
        .patch(&url)
        .header("apikey", SUPABASE_ANON_KEY)
        .header("Authorization", &format!("Bearer {token}"))
        .header("Prefer", "return=representation")
        .send_json(payload)
        .map_err(|_| "Save rejected. Check ownership and sign-in.")?;
    let rows: Vec<Value> = response
        .body_mut()
        .read_json()
        .map_err(|_| "Invalid save response")?;
    if rows.len() != 1 {
        return Err(
            "Data or ownership changed. Refresh before saving again; your draft is retained."
                .into(),
        );
    }
    Ok(())
}

pub fn save_direct(username: &str, draft: &Draft) -> Result<String, String> {
    let payload = review_payload(draft)?;
    let key = SUPABASE_SERVICE_ROLE_KEY;
    agent()
        .post(format!("{SUPABASE_URL}/rest/v1/rpc/maintainer_save_review"))
        .header("apikey", key)
        .header("Authorization", &format!("Bearer {key}"))
        .header("Content-Type", "application/json")
        .send_json(json!({
            "p_pr_number": draft.original.number,
            "p_status": payload["status"],
            "p_notes": payload["notes"],
            "p_checklist": payload["checklist"],
            "p_reviewer": username
        }))
        .map_err(|e| format!("Could not save review: {e}"))?;
    Ok("Review saved to Supabase. Refreshing…".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn draft_preserves_unknown_checklist_fields_and_attaches_review_sha() {
        let pr: PullRequest = serde_json::from_value(json!({"number":1,"status":"in-review","head_sha":"abc","notes":"saved","checklist":{"custom":true}})).unwrap();
        let mut draft = Draft::new(&pr);
        draft.status = "approved".into();
        let payload = review_payload(&draft).unwrap();
        assert_eq!(payload["checklist"]["custom"], true);
        assert_eq!(payload["checklist"]["_reviewed_head_sha"], "abc");
        assert_eq!(pr.checklist, json!({"custom":true}));
    }
    #[test]
    fn decisions_require_a_known_head_sha() {
        let pr: PullRequest =
            serde_json::from_value(json!({"number":1,"status":"approved"})).unwrap();
        assert!(review_payload(&Draft::new(&pr)).is_err());
    }
}
