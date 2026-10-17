mod github;
mod models;
mod prompt;
mod review;
mod supabase;
mod sync;
mod theme;
mod worker;

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

use eframe::egui::{self, Color32, RichText, ScrollArea, Stroke, Vec2};
use models::{Batch, PullRequest, SystemInfo};
use prompt::{generate_ai_prompt, generate_merge_script};
use theme::*;
use worker::{CpuWorker, detect_system_info, initial_system_info};

#[allow(dead_code)]
#[derive(PartialEq, Clone, Copy)]
enum AppTab {
    Minibatches,
    BatchInspector,
    CpuWorker,
    EntireRepo,
}

enum AsyncMessage {
    DataLoaded {
        batches: Vec<Batch>,
        prs: Vec<PullRequest>,
        warning: Option<String>,
    },
    DeltaPrsLoaded {
        deltas: Vec<PullRequest>,
        open_numbers: Vec<u64>,
        batches: Option<Vec<Batch>>,
    },
    HeartbeatTick(Result<supabase::HeartbeatResponse, String>),
    SyncFinished(Result<String, String>),
    Error(String),
    DescriptionLoaded(u64, Option<String>, Result<String, String>),
    ChecksLoaded(u64, Result<github::Checks, String>),
    #[allow(dead_code)]
    ReviewLogin(Result<review::Session, String>),
    ReviewAction(Result<String, String>, Option<u64>),
    LoggedOut(Result<(), String>),
    AccountLoaded(Result<(github::Account, Option<egui::ColorImage>), String>),
    SystemInfoLoaded(SystemInfo, bool),
}

struct BatchMeta {
    summary: String,
    blocker: Option<String>,
    churn: Option<i64>,
    number: u64,
    risk_level: &'static str,
    risk_color: Color32,
    title_lower: String,
    category_lower: String,
}

#[derive(PartialEq, Default)]
struct BatchFilterKey {
    scope: u8,
    sort: u8,
    query: String,
    category: String,
    complexity: String,
    login: Option<String>,
}

#[derive(PartialEq, Default)]
struct PrFilterKey {
    query: String,
    newest_first: bool,
    category: String,
    status: String,
}

struct AnyPs5PureRustApp {
    batches: Vec<Batch>,
    prs: HashMap<u64, PullRequest>,
    selected_batch: Option<Batch>,
    active_tab: AppTab,
    search_query: String,
    batch_query: String,
    category_filter: String,
    complexity_filter: String,
    newest_first: bool,
    batch_sort: u8,
    batch_scope: u8,
    inspector_prompt: bool,
    detail_tab: u8,
    descriptions: HashMap<(u64, Option<String>), Result<String, String>>,
    descriptions_busy: std::collections::HashSet<(u64, Option<String>)>,
    checks: HashMap<u64, github::Checks>,
    checks_busy: std::collections::HashSet<u64>,
    checks_errors: HashMap<u64, String>,
    checks_requested: HashMap<u64, Option<String>>,
    review_cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    review_session: Option<review::Session>,
    review_busy: bool,
    review_message: String,
    drafts: HashMap<u64, review::Draft>,
    selected_pr: Option<u64>,
    raw_prompt: bool,
    repo_category: String,
    repo_status: String,
    status_message: String,
    system_info: SystemInfo,
    runner_configured: bool,
    context: egui::Context,
    is_loading: bool,
    github_login: Option<String>,
    github_avatar: Option<egui::TextureHandle>,
    github_busy: bool,
    github_notice: String,
    runner_dialog: bool,
    runner_token: String,
    app_logo: Option<egui::TextureHandle>,
    use_windows_sandbox: bool,
    copied_notice: Option<(String, std::time::Instant)>,
    batch_meta: Vec<BatchMeta>,
    batch_categories: Vec<String>,
    repo_categories: Vec<String>,
    repo_statuses: Vec<String>,
    open_prs_count: usize,
    open_prs_label: String,
    batches_count_label: String,
    cache_prs_len: usize,
    batch_filter_key: Option<BatchFilterKey>,
    filtered_batch_indices: Vec<usize>,
    pr_filter_key: Option<PrFilterKey>,
    filtered_pr_numbers: Vec<u64>,
    prompt_cache: Option<(String, String)>,
    instance_id: String,
    auto_sync_enabled: bool,
    data_version: i64,
    active_peers: usize,
    slot_index: usize,
    last_heartbeat: std::time::Instant,
    heartbeat_interval: std::time::Duration,
    last_max_updated_at: Option<String>,
    sync_busy: bool,
    sync_status: String,
    tx: Sender<AsyncMessage>,
    rx: Receiver<AsyncMessage>,
}

impl AnyPs5PureRustApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Apply AAA PS5 Dark Theme
        theme::apply_ps5_theme(&cc.egui_ctx);

        let (tx, rx) = channel();
        let system_info = initial_system_info();

        let app_logo = {
            let bytes = include_bytes!("../assets/anyps5_icon_white.png");
            if let Ok(img) = image::load_from_memory(bytes) {
                let rgba = img.thumbnail(64, 64).into_rgba8();
                let (w, h) = rgba.dimensions();
                let color_image = egui::ColorImage::from_rgba_unmultiplied(
                    [w as usize, h as usize],
                    &rgba.into_raw(),
                );
                Some(cc.egui_ctx.load_texture(
                    "anyps5-brand-logo",
                    color_image,
                    egui::TextureOptions::LINEAR,
                ))
            } else {
                None
            }
        };

        let mut app = Self {
            batches: Vec::new(),
            prs: HashMap::new(),
            selected_batch: None,
            active_tab: AppTab::Minibatches,
            search_query: String::new(),
            batch_query: String::new(),
            category_filter: String::new(),
            complexity_filter: String::new(),
            newest_first: true,
            batch_sort: 3,
            batch_scope: 0,
            inspector_prompt: false,
            detail_tab: 0,
            descriptions: HashMap::new(),
            descriptions_busy: std::collections::HashSet::new(),
            checks: HashMap::new(),
            checks_busy: std::collections::HashSet::new(),
            checks_errors: HashMap::new(),
            checks_requested: HashMap::new(),
            review_cancel: None,
            review_session: None,
            review_busy: false,
            review_message: String::new(),
            drafts: HashMap::new(),
            selected_pr: None,
            raw_prompt: false,
            repo_category: String::new(),
            repo_status: String::new(),
            status_message: "Connecting to Supabase...".to_string(),
            system_info,
            runner_configured: false,
            context: cc.egui_ctx.clone(),
            is_loading: false,
            github_login: None,
            github_avatar: None,
            github_busy: false,
            github_notice: String::new(),
            runner_dialog: false,
            runner_token: String::new(),
            app_logo,
            use_windows_sandbox: false,
            copied_notice: None,
            batch_meta: Vec::new(),
            batch_categories: Vec::new(),
            repo_categories: Vec::new(),
            repo_statuses: Vec::new(),
            open_prs_count: 0,
            open_prs_label: "0".into(),
            batches_count_label: "0".into(),
            cache_prs_len: 0,
            batch_filter_key: None,
            filtered_batch_indices: Vec::new(),
            pr_filter_key: None,
            filtered_pr_numbers: Vec::new(),
            prompt_cache: None,
            instance_id: {
                let pid = std::process::id();
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis();
                format!("inst-{pid:x}-{now:x}")
            },
            auto_sync_enabled: true,
            data_version: 0,
            active_peers: 1,
            slot_index: 0,
            last_heartbeat: std::time::Instant::now(),
            heartbeat_interval: std::time::Duration::from_secs(25 + ((std::process::id() % 11) as u64)),
            last_max_updated_at: None,
            sync_busy: false,
            sync_status: String::new(),
            tx: tx.clone(),
            rx,
        };

        // Background system info & runner credentials check (non-blocking startup)
        {
            let tx = tx.clone();
            let context = cc.egui_ctx.clone();
            thread::spawn(move || {
                let info = detect_system_info();
                let configured = CpuWorker::has_runner_credentials();
                let _ = tx.send(AsyncMessage::SystemInfoLoaded(info, configured));
                context.request_repaint();
            });
        }

        // Initial background fetch from Supabase
        app.trigger_fetch();
        app.check_github(false);

        app
    }

    fn ensure_data_cache(&mut self) {
        if self.batch_meta.len() == self.batches.len() && self.cache_prs_len == self.prs.len() {
            return;
        }
        self.rebuild_data_cache();
    }

    fn rebuild_data_cache(&mut self) {
        self.batch_meta = self
            .batches
            .iter()
            .map(|batch| {
                let (mut risk_level, mut risk_color) = risk(&batch.complexity);
                if risk_level == "Unrated"
                    && let Some(label) = batch.complexity_label.as_deref()
                {
                    (risk_level, risk_color) = risk(label);
                }
                BatchMeta {
                    summary: batch_summary(batch, &self.prs),
                    blocker: batch_blocker(batch, &self.prs),
                    churn: batch_churn(batch, &self.prs),
                    number: batch_number(&batch.title),
                    risk_level,
                    risk_color,
                    title_lower: batch.title.to_lowercase(),
                    category_lower: batch.category.to_lowercase(),
                }
            })
            .collect();

        let mut b_cats: Vec<String> = self.batches.iter().map(|b| b.category.clone()).collect();
        b_cats.sort();
        b_cats.dedup();
        self.batch_categories = b_cats;

        let mut r_cats = Vec::new();
        let mut r_stats = Vec::new();
        let mut open_count = 0usize;
        for pr in self.prs.values() {
            if pr.is_open() {
                open_count += 1;
            }
            if !pr.category.is_empty() {
                r_cats.push(pr.category.clone());
            }
            if !pr.status.is_empty() {
                r_stats.push(pr.status.clone());
            }
        }
        r_cats.sort();
        r_cats.dedup();
        r_stats.sort();
        r_stats.dedup();
        self.repo_categories = r_cats;
        self.repo_statuses = r_stats;
        self.open_prs_count = open_count;
        self.open_prs_label = open_count.to_string();
        self.batches_count_label = self.batches.len().to_string();
        self.cache_prs_len = self.prs.len();
        self.batch_filter_key = None;
        self.pr_filter_key = None;
        self.prompt_cache = None;
    }

    fn check_github(&mut self, login: bool) {
        if self.github_busy {
            return;
        }
        self.github_busy = true;
        self.github_notice = if login {
            "Complete GitHub sign-in in the browser / login window."
        } else {
            "Checking GitHub account…"
        }
        .into();
        let tx = self.tx.clone();
        let context = self.context.clone();
        thread::spawn(move || {
            let result = if login {
                github::login().and_then(|()| github::account())
            } else {
                github::account()
            };
            let _ = tx.send(AsyncMessage::AccountLoaded(result));
            context.request_repaint();
        });
    }

    fn trigger_fetch(&mut self) {
        if self.is_loading {
            return;
        }
        self.is_loading = true;
        self.status_message = "Loading Supabase data...".into();
        let tx = self.tx.clone();
        let context = self.context.clone();
        thread::spawn(move || {
            let (batches, prs) = thread::scope(|s| {
                let b = s.spawn(supabase::fetch_batches);
                let p = s.spawn(supabase::fetch_prs);
                (
                    b.join().unwrap_or_else(|_| Err("Batches thread panicked".into())),
                    p.join().unwrap_or_else(|_| Err("PRs thread panicked".into())),
                )
            });
            let message = match (batches, prs) {
                (Ok(batches), Ok(prs)) => AsyncMessage::DataLoaded {
                    batches,
                    prs,
                    warning: None,
                },
                (batches, prs) => {
                    let errors = [batches.as_ref().err(), prs.as_ref().err()]
                        .into_iter()
                        .flatten()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("; ");
                    let local_prs = prs.ok().or_else(supabase::load_local_prs);
                    match local_prs {
                        Some(prs) => AsyncMessage::DataLoaded {
                            batches: batches.unwrap_or_default(),
                            prs,
                            warning: Some(format!("Partial/offline data; {errors}")),
                        },
                        None => AsyncMessage::Error(errors),
                    }
                }
            };
            let _ = tx.send(message);
            context.request_repaint();
        });
    }

    fn check_async_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                AsyncMessage::DataLoaded {
                    batches,
                    prs,
                    warning,
                } => {
                    self.is_loading = false;
                    self.status_message = format!(
                        "Supabase Live ({} batches, {} PRs)",
                        batches.len(),
                        prs.len()
                    );
                    if let Some(warning) = warning {
                        self.status_message = warning;
                    }
                    self.prs.clear();
                    self.prs.reserve(prs.len());
                    for pr in prs {
                        self.prs.insert(pr.number, pr);
                    }
                    let id = self.selected_batch.as_ref().map(|b| b.id.as_str());
                    self.selected_batch = batches
                        .iter()
                        .find(|b| Some(b.id.as_str()) == id)
                        .or_else(|| batches.first())
                        .cloned();
                    self.batches = batches;
                    self.rebuild_data_cache();
                    if self.data_version == 0 {
                        self.trigger_heartbeat();
                    }
                }
                AsyncMessage::SystemInfoLoaded(info, configured) => {
                    self.system_info = info;
                    self.runner_configured = configured;
                }
                AsyncMessage::LoggedOut(result) => {
                    self.github_busy = false;
                    match result {
                        Ok(()) => {
                            self.review_session = None;
                            self.github_login = None;
                            self.github_avatar = None;
                            self.github_notice.clear();
                        }
                        Err(error) => {
                            self.copied_notice = Some((error, std::time::Instant::now()));
                        }
                    }
                }
                AsyncMessage::AccountLoaded(result) => {
                    self.github_busy = false;
                    match result {
                        Ok((account, avatar)) => {
                            self.github_login = Some(account.login);
                            self.github_avatar = avatar.map(|image| {
                                self.context.load_texture(
                                    "github-avatar",
                                    image,
                                    egui::TextureOptions::LINEAR,
                                )
                            });
                            self.github_notice.clear();
                        }
                        Err(error) => {
                            self.github_notice = error;
                        }
                    }
                }
                AsyncMessage::DescriptionLoaded(number, sha, result) => {
                    let key = (number, sha);
                    self.descriptions_busy.remove(&key);
                    self.descriptions.insert(key, result);
                }
                AsyncMessage::ChecksLoaded(number, result) => {
                    self.checks_busy.remove(&number);
                    match result {
                        Ok(checks) => {
                            self.checks_errors.remove(&number);
                            self.checks.insert(number, checks);
                        }
                        Err(error) => {
                            self.checks_errors.insert(number, error);
                        }
                    }
                }
                AsyncMessage::ReviewLogin(result) => {
                    self.review_busy = false;
                    self.review_cancel = None;
                    match result {
                        Ok(session) => {
                            self.review_session = Some(session);
                            self.review_message = "Review session connected.".into();
                        }
                        Err(error) => self.review_message = error,
                    }
                }
                AsyncMessage::ReviewAction(result, saved) => {
                    self.review_busy = false;
                    match result {
                        Ok(msg) => {
                            if let Some(number) = saved {
                                self.drafts.remove(&number);
                            }
                            self.review_message = msg.clone();
                            self.copied_notice = Some((msg, std::time::Instant::now()));
                            self.trigger_fetch();
                        }
                        Err(error) => self.review_message = error,
                    }
                }
                AsyncMessage::DeltaPrsLoaded {
                    deltas,
                    open_numbers,
                    batches,
                } => {
                    self.is_loading = false;
                    for pr in deltas {
                        self.prs.insert(pr.number, pr);
                    }
                    if !open_numbers.is_empty() {
                        let open_set: std::collections::HashSet<u64> = open_numbers.into_iter().collect();
                        self.prs.retain(|num, pr| pr.github_state.as_deref().unwrap_or("open") != "open" || open_set.contains(num));
                    }
                    if let Some(batches) = batches {
                        let id = self.selected_batch.as_ref().map(|b| b.id.as_str());
                        self.selected_batch = batches
                            .iter()
                            .find(|b| Some(b.id.as_str()) == id)
                            .or_else(|| batches.first())
                            .cloned();
                        self.batches = batches;
                    }
                    self.status_message = format!(
                        "Supabase Live ({} batches, {} PRs)",
                        self.batches.len(),
                        self.prs.len()
                    );
                    self.rebuild_data_cache();
                }
                AsyncMessage::HeartbeatTick(result) => {
                    match result {
                        Ok(hb) => {
                            self.active_peers = hb.active_count;
                            self.slot_index = hb.slot_index;
                            if self.data_version == 0 {
                                self.data_version = hb.data_version;
                                self.last_max_updated_at = hb.last_synced_at.clone();
                            } else if hb.data_version > self.data_version {
                                self.data_version = hb.data_version;
                                self.trigger_delta_fetch(hb.open_pr_numbers);
                                self.last_max_updated_at = hb.last_synced_at.clone();
                            }
                            if self.auto_sync_enabled
                                && !self.sync_busy
                                && !self.is_loading
                                && !self.prs.is_empty()
                            {
                                let needs_sync = hb.last_synced_at.as_deref().is_none_or(|ts| {
                                    // If last sync > 3 mins ago
                                    let now_utc = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_secs();
                                    let ts_sec = github::parse_rfc3339_secs(ts).unwrap_or(0).max(0) as u64;
                                    now_utc.saturating_sub(ts_sec) >= 180
                                });
                                let is_leased = hb.lease_until.as_deref().is_some_and(|u| {
                                    let now_utc = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_secs();
                                    let until_sec = github::parse_rfc3339_secs(u).unwrap_or(0).max(0) as u64;
                                    until_sec > now_utc
                                });
                                if needs_sync && !is_leased {
                                    self.trigger_sync(180);
                                }
                            }
                        }
                        Err(e) => {
                            // Non-fatal background heartbeat error
                            let _ = e;
                        }
                    }
                }
                AsyncMessage::SyncFinished(result) => {
                    self.sync_busy = false;
                    match result {
                        Ok(msg) => {
                            self.sync_status = msg.clone();
                            self.copied_notice = Some((format!("🚀 {msg}"), std::time::Instant::now()));
                            self.trigger_fetch();
                        }
                        Err(err) => {
                            self.sync_status = format!("Sync error: {err}");
                        }
                    }
                }
                AsyncMessage::Error(err) => {
                    self.is_loading = false;
                    self.status_message = format!("Supabase Error: {}", err);
                }
            }
        }
    }

    fn trigger_delta_fetch(&mut self, open_numbers: Vec<u64>) {
        let since = self.last_max_updated_at.clone().unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string());
        let tx = self.tx.clone();
        let context = self.context.clone();
        thread::spawn(move || {
            let deltas = supabase::fetch_prs_delta(&since).unwrap_or_default();
            let batches = supabase::fetch_batches().ok();
            let _ = tx.send(AsyncMessage::DeltaPrsLoaded {
                deltas,
                open_numbers,
                batches,
            });
            context.request_repaint();
        });
    }

    fn trigger_heartbeat(&mut self) {
        self.last_heartbeat = std::time::Instant::now();
        let inst_id = self.instance_id.clone();
        let user_label = self.github_login.clone().unwrap_or_else(|| "maintainer".into());
        let tx = self.tx.clone();
        let context = self.context.clone();
        thread::spawn(move || {
            let res = supabase::sync_heartbeat(&inst_id, &user_label);
            let _ = tx.send(AsyncMessage::HeartbeatTick(res));
            context.request_repaint();
        });
    }

    fn trigger_sync(&mut self, min_interval_secs: i64) {
        if self.sync_busy {
            return;
        }
        self.sync_busy = true;
        self.sync_status = "Acquiring sync lease…".into();
        let inst_id = self.instance_id.clone();
        let user_label = self.github_login.clone().unwrap_or_else(|| "maintainer".into());
        let tx = self.tx.clone();
        let context = self.context.clone();
        let prs_map = self.prs.clone();
        let batches = self.batches.clone();
        let slot_index = self.slot_index;
        let active_count = self.active_peers;

        thread::spawn(move || {
            let lease_res = supabase::try_acquire_sync_lease(&inst_id, &user_label, min_interval_secs, 120);
            match lease_res {
                Ok(lease) => {
                    if !lease.acquired {
                        let reason = lease.reason.as_deref().unwrap_or("busy");
                        let msg = if reason == "fresh" {
                            "Sync skipped: recently synced by another instance.".to_string()
                        } else if reason == "leased" {
                            format!(
                                "Sync in progress by @{} (lease active)",
                                lease.lease_user_label.as_deref().unwrap_or("maintainer")
                            )
                        } else {
                            format!("Sync lease busy ({reason})")
                        };
                        let _ = tx.send(AsyncMessage::SyncFinished(Ok(msg)));
                        context.request_repaint();
                        return;
                    }
                }
                Err(e) => {
                    let _ = tx.send(AsyncMessage::SyncFinished(Err(e)));
                    context.request_repaint();
                    return;
                }
            }

            let sync_result = sync::run_full_sync(&prs_map, &batches, slot_index, active_count);
            let _ = supabase::finish_sync_lease(&inst_id);
            let _ = tx.send(AsyncMessage::SyncFinished(sync_result));
            context.request_repaint();
        });
    }

    fn render_window_controls(
        &self,
        ctx: &egui::Context,
        is_maximized: bool,
        viewport_rect: egui::Rect,
    ) {
        let button_size = Vec2::new(44.0, 30.0);
        let exit_button_size = Vec2::new(48.0, 30.0);
        let buttons_total_width = button_size.x * 2.0 + exit_button_size.x;

        egui::Area::new(egui::Id::unique("window_controls_overlay"))
            .fixed_pos(egui::pos2(
                viewport_rect.max.x - buttons_total_width,
                viewport_rect.min.y,
            ))
            .order(egui::Order::Foreground)
            .movable(false)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::ZERO;

                    // 1. Minimize Button
                    let (rect, resp) = ui.allocate_exact_size(button_size, egui::Sense::click());
                    let is_hovered = resp.hovered();
                    let is_down = resp.is_pointer_button_down_on();
                    let bg_color = if is_down {
                        Color32::from_rgba_unmultiplied(255, 255, 255, 35)
                    } else if is_hovered {
                        Color32::from_rgba_unmultiplied(255, 255, 255, 20)
                    } else {
                        Color32::TRANSPARENT
                    };
                    ui.painter().rect_filled(rect, 0, bg_color);
                    let stroke_color = if is_hovered {
                        Color32::WHITE
                    } else {
                        COLOR_TEXT_MUTED
                    };
                    let c = rect.center();
                    let stroke = Stroke::new(1.2, stroke_color);
                    ui.painter()
                        .line_segment([c + Vec2::new(-5.0, 0.0), c + Vec2::new(5.0, 0.0)], stroke);
                    let resp = resp.on_hover_text("Minimize");
                    if resp.clicked() {
                        ui.ctx()
                            .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    }

                    // 2. Maximize / Restore Button
                    let (rect, resp) = ui.allocate_exact_size(button_size, egui::Sense::click());
                    let is_hovered = resp.hovered();
                    let is_down = resp.is_pointer_button_down_on();
                    let bg_color = if is_down {
                        Color32::from_rgba_unmultiplied(255, 255, 255, 35)
                    } else if is_hovered {
                        Color32::from_rgba_unmultiplied(255, 255, 255, 20)
                    } else {
                        Color32::TRANSPARENT
                    };
                    ui.painter().rect_filled(rect, 0, bg_color);
                    let stroke_color = if is_hovered {
                        Color32::WHITE
                    } else {
                        COLOR_TEXT_MUTED
                    };
                    let c = rect.center();
                    let stroke = Stroke::new(1.2, stroke_color);
                    let resp = if is_maximized {
                        let back_rect = egui::Rect::from_center_size(
                            c + Vec2::new(1.5, -1.5),
                            Vec2::new(8.0, 8.0),
                        );
                        ui.painter()
                            .rect_stroke(back_rect, 0, stroke, egui::StrokeKind::Middle);
                        let front_rect = egui::Rect::from_center_size(
                            c + Vec2::new(-1.5, 1.5),
                            Vec2::new(8.0, 8.0),
                        );
                        ui.painter()
                            .rect_filled(front_rect, 0, Color32::from_rgb(14, 25, 48));
                        ui.painter()
                            .rect_stroke(front_rect, 0, stroke, egui::StrokeKind::Middle);
                        resp.on_hover_text("Restore Down")
                    } else {
                        let square_rect = egui::Rect::from_center_size(c, Vec2::new(9.5, 9.5));
                        ui.painter()
                            .rect_stroke(square_rect, 0, stroke, egui::StrokeKind::Middle);
                        resp.on_hover_text("Maximize")
                    };
                    if resp.clicked() {
                        ui.ctx()
                            .send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
                    }

                    // 3. Exit / Close Button (in top-right corner)
                    let (rect, resp) =
                        ui.allocate_exact_size(exit_button_size, egui::Sense::click());
                    let is_hovered = resp.hovered();
                    let is_down = resp.is_pointer_button_down_on();
                    let bg_color = if is_down {
                        Color32::from_rgb(180, 25, 35)
                    } else if is_hovered {
                        Color32::from_rgb(225, 40, 50)
                    } else {
                        Color32::TRANSPARENT
                    };
                    ui.painter().rect_filled(rect, 0, bg_color);
                    let stroke_color = if is_hovered {
                        Color32::WHITE
                    } else {
                        COLOR_TEXT_MUTED
                    };
                    let c = rect.center();
                    let stroke = Stroke::new(1.2, stroke_color);
                    ui.painter()
                        .line_segment([c + Vec2::new(-4.5, -4.5), c + Vec2::new(4.5, 4.5)], stroke);
                    ui.painter()
                        .line_segment([c + Vec2::new(-4.5, 4.5), c + Vec2::new(4.5, -4.5)], stroke);
                    let resp = resp.on_hover_text("Close");
                    if resp.clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });
    }
}

impl eframe::App for AnyPs5PureRustApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.check_async_messages();
        self.ensure_data_cache();

        if self.last_heartbeat.elapsed() >= self.heartbeat_interval {
            self.trigger_heartbeat();
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(2));

        if let Some((_, time)) = &self.copied_notice {
            let duration = std::time::Duration::from_secs(4);
            if time.elapsed() >= duration {
                self.copied_notice = None;
            } else {
                ui.ctx()
                    .request_repaint_after(duration.saturating_sub(time.elapsed()));
            }
        }
        // Paint a quiet vertical blue gradient without a texture dependency.
        let rect = ui.max_rect();
        let mut mesh = egui::Mesh::default();
        mesh.colored_vertex(rect.left_top(), Color32::from_rgb(17, 32, 61));
        mesh.colored_vertex(rect.right_top(), Color32::from_rgb(9, 20, 42));
        mesh.colored_vertex(rect.right_bottom(), COLOR_OBSIDIAN);
        mesh.colored_vertex(rect.left_bottom(), COLOR_OBSIDIAN);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        ui.painter().add(egui::Shape::mesh(mesh));

        let viewport_rect = ui.input(|i| i.viewport_rect());
        let is_maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));

        // Allow moving and maximizing window by dragging/double-clicking empty top area
        let drag_rect = egui::Rect::from_min_max(
            viewport_rect.min,
            egui::pos2(viewport_rect.max.x - 140.0, viewport_rect.min.y + 36.0),
        );
        let drag_response = ui.interact(
            drag_rect,
            ui.scope_id().with("frameless_window_drag"),
            egui::Sense::click_and_drag(),
        );
        if drag_response.drag_started_by(egui::PointerButton::Primary) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if drag_response.double_clicked() {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
        }

        egui::Panel::left("navigation")
            .exact_size(236.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(COLOR_SIDEBAR).inner_margin(16))
            .show(ui, |ui| {
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    ui.add_space(6.0);

                    // 1. User Profile row at the absolute bottom
                    self.render_sidebar_profile(ui);

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);

                    // 2. Compact Connected status card
                    self.render_sidebar_connected_card(ui);

                    ui.add_space(14.0);

                    // 3. Top-down section filling the remaining space upwards
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        ScrollArea::vertical()
                            .id_salt("sidebar_scroll")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.add_space(4.0);
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 8.0;
                                    if let Some(logo) = &self.app_logo {
                                        ui.add(
                                            egui::Image::new(logo)
                                                .fit_to_exact_size(Vec2::splat(26.0)),
                                        );
                                    }
                                    ui.label(
                                        RichText::new("ANYPS5")
                                            .size(24.0)
                                            .strong()
                                            .color(COLOR_TEXT_WHITE),
                                    );
                                });
                                ui.label(
                                    RichText::new("MAINTAINER WORKSPACE")
                                        .size(9.5)
                                        .color(COLOR_TEXT_MUTED),
                                );
                                ui.add_space(16.0);

                                for (tab, symbol, title, count) in [
                                    (
                                        AppTab::Minibatches,
                                        "▦",
                                        "Minibatches",
                                        self.batches_count_label.as_str(),
                                    ),
                                    (AppTab::BatchInspector, "≡", "AI Inspector", ""),
                                    (
                                        AppTab::EntireRepo,
                                        "◎",
                                        "Pull Requests",
                                        self.open_prs_label.as_str(),
                                    ),
                                ] {
                                    let active = self.active_tab == tab;
                                    let (rect, response) = ui.allocate_exact_size(
                                        Vec2::new(ui.available_width(), 40.0),
                                        egui::Sense::click(),
                                    );
                                    let amount = ui.ctx().animate_bool(
                                        response.id.with("hover"),
                                        response.hovered(),
                                    );
                                    ui.painter().rect_filled(
                                        rect,
                                        10,
                                        if active {
                                            COLOR_PS5_BLUE
                                        } else {
                                            COLOR_CARD_BG.gamma_multiply(amount * 0.6)
                                        },
                                    );
                                    let font = egui::FontId::proportional(13.0);
                                    ui.painter().text(
                                        rect.left_center() + Vec2::new(10.0, 0.0),
                                        egui::Align2::LEFT_CENTER,
                                        symbol,
                                        font.clone(),
                                        COLOR_TEXT_WHITE,
                                    );
                                    ui.painter().text(
                                        rect.left_center() + Vec2::new(28.0, 0.0),
                                        egui::Align2::LEFT_CENTER,
                                        title,
                                        font.clone(),
                                        COLOR_TEXT_WHITE,
                                    );
                                    ui.painter().text(
                                        rect.right_center() - Vec2::new(8.0, 0.0),
                                        egui::Align2::RIGHT_CENTER,
                                        count,
                                        egui::FontId::proportional(11.0),
                                        COLOR_TEXT_WHITE,
                                    );
                                    focus_ring(ui, &response, 10);
                                    if response.clicked() {
                                        self.active_tab = tab;
                                    }
                                    ui.add_space(6.0);
                                }

                                ui.add_space(14.0);
                                self.render_sidebar_runner_card(ui);
                            });
                    });
                });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().inner_margin(24))
            .show(ui, |ui| {
                if let Some((notice, _)) = &self.copied_notice {
                    card().show(ui, |ui| {
                        ui.label(notice);
                    });
                    ui.add_space(12.0);
                }
                match self.active_tab {
                    AppTab::Minibatches => self.render_minibatches_view(ui),
                    AppTab::BatchInspector => self.render_batch_inspector_view(ui),
                    AppTab::CpuWorker => self.render_cpu_worker_view(ui),
                    AppTab::EntireRepo => self.render_entire_repo_view(ui),
                }
            });
        if self.runner_dialog {
            let mut open = true;
            egui::Window::new("Build Runner")
                .id(egui::Id::unique("runner_setup_dialog"))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(440.0)
                .show(ui.ctx(), |ui| {
                    ScrollArea::vertical().max_height(480.0).show(ui, |ui| {
                        self.runner_setup_panel(ui);
                    });
                    ui.add_space(12.0);
                    ui.hyperlink_to(
                        "View jobs on GitHub ↗",
                        "https://github.com/boykopovar/AnyPS5/actions",
                    );
                });
            self.runner_dialog = open;
        }

        let viewport_rect = ui.input(|i| i.viewport_rect());
        let is_maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));

        // Floating window controls in top-right corner of application
        self.render_window_controls(ui.ctx(), is_maximized, viewport_rect);

        if !is_maximized {
            // Draw a subtle 1px border around the entire window for sleek definition
            ui.painter().rect_stroke(
                viewport_rect,
                0,
                Stroke::new(1.0, Color32::from_rgb(25, 38, 62)),
                egui::StrokeKind::Inside,
            );
            handle_window_resize(ui, viewport_rect);
        }
    }
}

fn handle_window_resize(ui: &mut egui::Ui, screen: egui::Rect) {
    let border_thickness = 5.0;
    let corner_size = 12.0;

    // Resizing corners
    let corners = [
        (
            egui::Rect::from_min_max(screen.min, screen.min + Vec2::splat(corner_size)),
            egui::ResizeDirection::NorthWest,
            egui::CursorIcon::ResizeNorthWest,
        ),
        (
            egui::Rect::from_min_max(
                egui::pos2(screen.min.x, screen.max.y - corner_size),
                egui::pos2(screen.min.x + corner_size, screen.max.y),
            ),
            egui::ResizeDirection::SouthWest,
            egui::CursorIcon::ResizeSouthWest,
        ),
        (
            egui::Rect::from_min_max(screen.max - Vec2::splat(corner_size), screen.max),
            egui::ResizeDirection::SouthEast,
            egui::CursorIcon::ResizeSouthEast,
        ),
    ];

    for (corner_rect, direction, cursor) in corners {
        let resp = ui.interact(
            corner_rect,
            ui.scope_id().with(("resize_corner", direction as u8)),
            egui::Sense::click_and_drag(),
        );
        if resp.hovered() {
            ui.ctx().set_cursor_icon(cursor);
        }
        if resp.drag_started_by(egui::PointerButton::Primary) {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
        }
    }

    // Resizing edges (top and east edges stay clear of top-right control buttons)
    let edges = [
        (
            egui::Rect::from_min_max(
                egui::pos2(screen.min.x + corner_size, screen.min.y),
                egui::pos2(screen.max.x - 145.0, screen.min.y + border_thickness),
            ),
            egui::ResizeDirection::North,
            egui::CursorIcon::ResizeNorth,
        ),
        (
            egui::Rect::from_min_max(
                egui::pos2(screen.min.x + corner_size, screen.max.y - border_thickness),
                egui::pos2(screen.max.x - corner_size, screen.max.y),
            ),
            egui::ResizeDirection::South,
            egui::CursorIcon::ResizeSouth,
        ),
        (
            egui::Rect::from_min_max(
                egui::pos2(screen.min.x, screen.min.y + corner_size),
                egui::pos2(screen.min.x + border_thickness, screen.max.y - corner_size),
            ),
            egui::ResizeDirection::West,
            egui::CursorIcon::ResizeWest,
        ),
        (
            egui::Rect::from_min_max(
                egui::pos2(screen.max.x - border_thickness, screen.min.y + 32.0),
                egui::pos2(screen.max.x, screen.max.y - corner_size),
            ),
            egui::ResizeDirection::East,
            egui::CursorIcon::ResizeEast,
        ),
    ];

    for (edge_rect, direction, cursor) in edges {
        let resp = ui.interact(
            edge_rect,
            ui.scope_id().with(("resize_edge", direction as u8)),
            egui::Sense::click_and_drag(),
        );
        if resp.hovered() {
            ui.ctx().set_cursor_icon(cursor);
        }
        if resp.drag_started_by(egui::PointerButton::Primary) {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
        }
    }
}

fn heading(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.label(RichText::new(title).size(28.0).color(COLOR_TEXT_WHITE));
    ui.label(RichText::new(subtitle).size(14.0).color(COLOR_TEXT_MUTED));
    ui.add_space(20.0);
}

fn primary(ui: &mut egui::Ui, title: &str) -> egui::Response {
    let response = ui.add(
        egui::Button::new(RichText::new(title).color(Color32::WHITE).strong())
            .fill(COLOR_PS5_BLUE)
            .min_size(Vec2::new(130.0, 38.0))
            .corner_radius(20)
            .stroke(egui::Stroke::NONE),
    );
    focus_ring(ui, &response, 20);
    response
}

fn description_view(ui: &mut egui::Ui, body: &str) {
    let mut code = false;
    for line in body.lines() {
        if line.trim_start().starts_with("```") {
            code = !code;
            continue;
        }
        if line.trim().is_empty() {
            ui.add_space(5.0);
            continue;
        }
        if line.trim() == "---" {
            ui.separator();
            continue;
        }
        let heading = line.chars().take_while(|c| *c == '#').count();
        let text = if heading > 0 && !code {
            line.trim_start_matches('#').trim()
        } else {
            line
        };
        let rich = if code {
            RichText::new(text).monospace().size(13.0)
        } else if heading > 0 {
            RichText::new(text)
                .size(if heading == 1 { 20.0 } else { 17.0 })
                .strong()
        } else {
            RichText::new(text).size(14.0)
        };
        ui.add(egui::Label::new(rich).wrap().selectable(true));
    }
    egui::CollapsingHeader::new("Raw Markdown").show(ui, |ui| {
        ui.add(
            egui::Label::new(RichText::new(body).monospace().size(12.0))
                .wrap()
                .selectable(true),
        );
    });
}

fn check_row(ui: &mut egui::Ui, entry: &github::CheckEntry) {
    let color = match entry.result {
        github::CheckResult::Passed => COLOR_SUCCESS,
        github::CheckResult::Failed => COLOR_DANGER,
        github::CheckResult::Running => COLOR_WARNING,
        _ => COLOR_TEXT_MUTED,
    };
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(color, entry.result.label());
        if let Some(url) = &entry.url {
            if url.starts_with("https://github.com/") || url.starts_with("https://") {
                ui.hyperlink_to(&entry.name, url);
            } else {
                ui.label(&entry.name);
            }
        } else {
            ui.label(&entry.name);
        }
        ui.label(
            RichText::new(&entry.duration)
                .size(12.0)
                .color(COLOR_TEXT_MUTED),
        )
        .on_hover_text(entry.source);
    });
    ui.separator();
}

fn batch_number(title: &str) -> u64 {
    title
        .strip_prefix("Batch ")
        .and_then(|n| n.parse().ok())
        .unwrap_or(u64::MAX)
}

fn batch_blocker(batch: &Batch, prs: &HashMap<u64, PullRequest>) -> Option<String> {
    if let Err(error) = prompt::validate_batch(batch, prs) {
        return Some(error);
    }
    for number in &batch.pr_numbers {
        if let Some(pr) = prs.get(number) {
            if pr.is_draft {
                return Some(format!("PR #{number} is a draft."));
            }
            if pr.github_state.as_deref().is_some_and(|state| {
                state.eq_ignore_ascii_case("closed") || state.eq_ignore_ascii_case("merged")
            }) {
                return Some(format!("PR #{number} is no longer open."));
            }
        }
    }
    None
}

fn batch_churn(batch: &Batch, prs: &HashMap<u64, PullRequest>) -> Option<i64> {
    batch.pr_numbers.iter().try_fold(0_i64, |sum, n| {
        let pr = prs.get(n)?;
        (pr.additions >= 0 && pr.deletions >= 0).then(|| {
            sum.saturating_add(pr.additions)
                .saturating_add(pr.deletions)
        })
    })
}

fn colored_batch_summary(summary: &str) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let mut append = |text: &str, color| {
        job.append(
            text,
            0.0,
            egui::TextFormat {
                font_id: egui::FontId::proportional(12.0),
                color,
                ..Default::default()
            },
        )
    };
    if let Some((prefix, churn)) = summary.rsplit_once(" · ")
        && let Some((adds, dels)) = churn.split_once(" / ")
        && adds.starts_with('+')
        && dels.starts_with('−')
    {
        append(&format!("{prefix} · "), COLOR_TEXT_MUTED);
        append(adds, COLOR_SUCCESS);
        append(" / ", COLOR_TEXT_MUTED);
        append(dels, COLOR_DANGER);
    } else {
        append(summary, COLOR_TEXT_MUTED);
    }
    job
}

fn batch_summary(batch: &Batch, prs: &HashMap<u64, PullRequest>) -> String {
    let mut files = std::collections::HashSet::new();
    let mut adds = 0_i64;
    let mut dels = 0_i64;
    let mut complete = true;
    for number in &batch.pr_numbers {
        if let Some(pr) = prs.get(number) {
            let paths = pr.file_paths();
            if paths.is_empty() || paths.len() != pr.files.len() {
                complete = false;
            }
            files.extend(paths);
            adds = adds.saturating_add(pr.additions.max(0));
            dels = dels.saturating_add(pr.deletions.max(0));
        } else {
            complete = false;
        }
    }
    let files = if complete {
        format!("{} files", files.len())
    } else {
        "Files unknown".into()
    };
    let churn = if batch_churn(batch, prs).is_some() {
        format!("+{adds} / −{dels}")
    } else {
        "Changes unknown".into()
    };
    format!(
        "{} · {} PRs · {files} · {churn}",
        batch.category,
        batch.pr_numbers.len()
    )
}

fn risk(complexity: &str) -> (&'static str, Color32) {
    let text = complexity.to_lowercase();
    if text.contains("high") || text.contains("complex") {
        ("High", COLOR_DANGER)
    } else if text.contains("moderate") || text.contains("medium") {
        ("Medium", COLOR_WARNING)
    } else if text.contains("safe") || text.contains("low") || text.contains("simple") {
        ("Low", COLOR_SUCCESS)
    } else {
        ("Unrated", COLOR_TEXT_MUTED)
    }
}

fn table_cell(ui: &mut egui::Ui, width: f32, height: f32, text: RichText) -> egui::Response {
    ui.allocate_ui_with_layout(
        Vec2::new(width, height),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_max_width(width);
            ui.add(egui::Label::new(text).truncate())
        },
    )
    .inner
}

fn pr_card(ui: &mut egui::Ui, pr: &PullRequest) {
    card().show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(
            RichText::new(format!("#{}", pr.number))
                .color(COLOR_PS5_CYAN)
                .strong(),
        );
        ui.add(
            egui::Label::new(RichText::new(&pr.title).size(16.0).color(COLOR_TEXT_WHITE)).wrap(),
        );
        ui.label(
            RichText::new(format!(
                "@{}   ·   {}   ·   {} files",
                pr.author_name(),
                pr.diff_summary(),
                pr.files.len()
            ))
            .size(12.0)
            .color(COLOR_TEXT_MUTED),
        );
        egui::CollapsingHeader::new("Changed files")
            .id_salt(pr.number)
            .show(ui, |ui| {
                for path in pr.file_paths() {
                    ui.add(egui::Label::new(RichText::new(path).monospace().size(12.0)).wrap());
                }
            });
        ui.hyperlink_to(
            "Open on GitHub ↗",
            format!("https://github.com/boykopovar/AnyPS5/pull/{}", pr.number),
        );
    });
    ui.add_space(12.0);
}

impl AnyPs5PureRustApp {
    fn render_minibatches_view(&mut self, ui: &mut egui::Ui) {
        self.ensure_data_cache();
        ui.horizontal(|ui| {
            ui.label(RichText::new("Minibatches").size(28.0));
            ui.label(
                RichText::new(format!("{} total", self.batches.len()))
                    .size(12.0)
                    .color(COLOR_TEXT_MUTED),
            );
        });
        ui.horizontal_wrapped(|ui| {
            for (scope, label) in [(0, "All"), (1, "Available"), (2, "Mine"), (3, "Blocked")] {
                ui.selectable_value(&mut self.batch_scope, scope, label);
            }
        });
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.batch_query)
                    .hint_text("Search batches or PR number")
                    .desired_width(240.0),
            );
            egui::ComboBox::from_id_salt("batch_category")
                .selected_text(if self.category_filter.is_empty() {
                    "All categories"
                } else {
                    &self.category_filter
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.category_filter, String::new(), "All categories");
                    for category in &self.batch_categories {
                        ui.selectable_value(
                            &mut self.category_filter,
                            category.clone(),
                            category.as_str(),
                        );
                    }
                });
            egui::ComboBox::from_id_salt("batch_risk")
                .selected_text(if self.complexity_filter.is_empty() {
                    "All complexity"
                } else {
                    &self.complexity_filter
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.complexity_filter,
                        String::new(),
                        "All complexity",
                    );
                    for level in ["Low", "Medium", "High", "Unrated"] {
                        ui.selectable_value(&mut self.complexity_filter, level.into(), level);
                    }
                });
            egui::ComboBox::from_id_salt("batch_sort")
                .selected_text(match self.batch_sort {
                    1 => "Most PRs",
                    2 => "Complexity",
                    3 => "Available first",
                    4 => "Smaller scope",
                    5 => "Oldest claimed",
                    _ => "Name",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.batch_sort, 3, "Available first");
                    ui.selectable_value(&mut self.batch_sort, 4, "Smaller scope");
                    ui.selectable_value(&mut self.batch_sort, 5, "Oldest claimed");
                    ui.selectable_value(&mut self.batch_sort, 0, "Name");
                    ui.selectable_value(&mut self.batch_sort, 1, "Most PRs");
                    ui.selectable_value(&mut self.batch_sort, 2, "Complexity");
                });
        });
        ui.add_space(12.0);
        let key_changed = self.batch_filter_key.as_ref().is_none_or(|k| {
            k.scope != self.batch_scope
                || k.sort != self.batch_sort
                || k.query != self.batch_query
                || k.category != self.category_filter
                || k.complexity != self.complexity_filter
                || k.login != self.github_login
        });
        if key_changed {
            let query = self.batch_query.to_lowercase();
            let mut indices: Vec<usize> = (0..self.batches.len())
                .filter(|&i| {
                    let batch = &self.batches[i];
                    let meta = &self.batch_meta[i];
                    (match self.batch_scope {
                        1 => batch.available(),
                        2 => self.github_login.as_deref().zip(batch.owner()).is_some_and(
                            |(login, owner)| {
                                login.eq_ignore_ascii_case(owner.trim_start_matches('@'))
                            },
                        ),
                        3 => meta.blocker.is_some(),
                        _ => true,
                    }) && (self.category_filter.is_empty()
                        || batch.category == self.category_filter)
                        && (self.complexity_filter.is_empty()
                            || meta.risk_level == self.complexity_filter)
                        && (query.is_empty()
                            || meta.title_lower.contains(&query)
                            || meta.category_lower.contains(&query)
                            || batch
                                .pr_numbers
                                .iter()
                                .any(|n| n.to_string().contains(&query)))
                })
                .collect();
            match self.batch_sort {
                3 => indices.sort_by(|&a, &b| {
                    (!self.batches[a].available(), self.batch_meta[a].number, &self.batches[a].id)
                        .cmp(&(
                            !self.batches[b].available(),
                            self.batch_meta[b].number,
                            &self.batches[b].id,
                        ))
                }),
                4 => indices.sort_by_key(|&i| self.batch_meta[i].churn.unwrap_or(i64::MAX)),
                5 => indices.sort_by(|&a, &b| {
                    (
                        self.batches[a].claimed_at.is_none(),
                        self.batches[a].claimed_at.as_deref(),
                    )
                        .cmp(&(
                            self.batches[b].claimed_at.is_none(),
                            self.batches[b].claimed_at.as_deref(),
                        ))
                }),
                1 => indices.sort_by_key(|&i| std::cmp::Reverse(self.batches[i].pr_numbers.len())),
                2 => indices.sort_by_key(|&i| match self.batch_meta[i].risk_level {
                    "Low" => 0,
                    "Medium" => 1,
                    "High" => 2,
                    _ => 3,
                }),
                _ => indices.sort_by(|&a, &b| {
                    (self.batch_meta[a].number, &self.batches[a].title)
                        .cmp(&(self.batch_meta[b].number, &self.batches[b].title))
                }),
            }
            self.filtered_batch_indices = indices;
            self.batch_filter_key = Some(BatchFilterKey {
                scope: self.batch_scope,
                sort: self.batch_sort,
                query: self.batch_query.clone(),
                category: self.category_filter.clone(),
                complexity: self.complexity_filter.clone(),
                login: self.github_login.clone(),
            });
        }
        let filtered_len = self.filtered_batch_indices.len();
        ui.label(RichText::new(format!("{filtered_len} batches")).color(COLOR_TEXT_MUTED));
        ui.add_space(8.0);
        if filtered_len == 0 {
            ui.label(if self.is_loading {
                "Loading batches…"
            } else {
                "No matching batches. Try changing your filters."
            });
        }
        let mut clicked_batch = None;
        ScrollArea::vertical()
            .id_salt("batch_list")
            .auto_shrink([false, false])
            .show_rows(ui, 72.0, filtered_len, |ui, range| {
                for index in range {
                    let batch_idx = self.filtered_batch_indices[index];
                    let batch = &self.batches[batch_idx];
                    let meta = &self.batch_meta[batch_idx];
                    let summary = &meta.summary;
                    let blocker = &meta.blocker;
                    let owner = batch
                        .owner()
                        .map(|owner| format!(" · @{owner}"))
                        .unwrap_or_default();
                    let status = if blocker.is_some() {
                        format!("Blocked{owner}")
                    } else {
                        format!("{}{owner}", batch.workflow_label())
                    };
                    let (level, color) = (meta.risk_level, meta.risk_color);
                    let title = &batch.title;
                    let (rect, response) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), 72.0),
                        egui::Sense::click(),
                    );
                    ui.painter().rect_filled(
                        rect,
                        10,
                        if response.hovered() || response.has_focus() {
                            Color32::from_rgb(28, 47, 75)
                        } else {
                            COLOR_CARD_BG
                        },
                    );
                    let status_width = (rect.width() * 0.32).clamp(110.0, 250.0);
                    let left = egui::Rect::from_min_max(
                        rect.min + Vec2::new(14.0, 0.0),
                        egui::pos2(rect.right() - status_width - 24.0, rect.bottom()),
                    );
                    let right = egui::Rect::from_min_max(
                        egui::pos2(rect.right() - status_width - 12.0, rect.top()),
                        rect.max - Vec2::new(14.0, 0.0),
                    );
                    ui.painter()
                        .with_clip_rect(left.intersect(ui.clip_rect()))
                        .text(
                            left.left_top() + Vec2::new(0.0, 20.0),
                            egui::Align2::LEFT_CENTER,
                            title,
                            egui::FontId::proportional(15.0),
                            COLOR_TEXT_WHITE,
                        );
                    let galley = ui.painter().layout_job(colored_batch_summary(summary));
                    let position = left.left_top() + Vec2::new(0.0, 49.0 - galley.size().y * 0.5);
                    ui.painter()
                        .with_clip_rect(left.intersect(ui.clip_rect()))
                        .galley(position, galley, COLOR_TEXT_MUTED);
                    let painter = ui.painter().with_clip_rect(right.intersect(ui.clip_rect()));
                    painter.text(
                        right.right_top() + Vec2::new(0.0, 20.0),
                        egui::Align2::RIGHT_CENTER,
                        &status,
                        egui::FontId::proportional(12.0),
                        if blocker.is_some() {
                            COLOR_WARNING
                        } else if batch.available() {
                            COLOR_SUCCESS
                        } else {
                            COLOR_TEXT_MUTED
                        },
                    );
                    painter.text(
                        right.right_top() + Vec2::new(0.0, 49.0),
                        egui::Align2::RIGHT_CENTER,
                        format!("{level} complexity   →"),
                        egui::FontId::proportional(12.0),
                        color,
                    );
                    focus_ring(ui, &response, 10);
                    let response = response.on_hover_ui(|ui| {
                        ui.label(title);
                        ui.label(summary);
                        ui.label(&status);
                        ui.label(blocker.as_deref().unwrap_or(
                            "No local data blockers detected. Current GitHub CI and mergeability are not checked.",
                        ));
                    });
                    if response.clicked() {
                        clicked_batch = Some(batch.clone());
                    }
                }
            });
        if let Some(batch) = clicked_batch {
            self.selected_batch = Some(batch);
            self.active_tab = AppTab::BatchInspector;
        }
    }

    fn render_batch_inspector_view(&mut self, ui: &mut egui::Ui) {
        if ui
            .add(egui::Button::new("← Minibatches").frame(false))
            .clicked()
        {
            self.active_tab = AppTab::Minibatches;
        }
        let Some(batch) = self.selected_batch.clone() else {
            ui.label("Select a batch to begin.");
            return;
        };
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(&batch.title).size(26.0));
            let (level, color) = {
                let (l, c) = risk(&batch.complexity);
                if l != "Unrated" {
                    (l, c)
                } else {
                    risk(
                        batch
                            .complexity_label
                            .as_deref()
                            .unwrap_or(&batch.complexity),
                    )
                }
            };
            ui.colored_label(color, format!("{level} complexity"));
            ui.label(
                RichText::new(format!(
                    "{}{}",
                    batch.workflow_label(),
                    batch
                        .owner()
                        .map(|o| format!(" · @{o}"))
                        .unwrap_or_default()
                ))
                .size(12.0),
            );
        });
        let summary = batch_summary(&batch, &self.prs);
        let job = colored_batch_summary(&summary);
        ui.label(job);
        let reviewed = batch
            .pr_numbers
            .iter()
            .filter_map(|n| self.prs.get(n))
            .filter(|pr| {
                matches!(
                    pr.status.as_str(),
                    "approved" | "changes-requested" | "changes" | "ready"
                )
            })
            .count();
        let _current_decisions = batch
            .pr_numbers
            .iter()
            .filter_map(|n| self.prs.get(n))
            .filter(|pr| {
                matches!(
                    pr.status.as_str(),
                    "approved" | "changes" | "changes-requested" | "ready"
                ) && pr
                    .head_sha
                    .as_deref()
                    .is_some_and(|sha| pr.checklist["_reviewed_head_sha"].as_str() == Some(sha))
            })
            .count();
        let fetched = batch
            .pr_numbers
            .iter()
            .filter(|n| self.checks.contains_key(n))
            .count();
        let checks_label = if fetched == 0 {
            "Checks not fetched".to_string()
        } else {
            format!("Checks fetched {fetched}/{}", batch.pr_numbers.len())
        };
        let blocker = batch_blocker(&batch, &self.prs);
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(format!(
                    "{reviewed}/{} reviewed   ·   {checks_label}",
                    batch.pr_numbers.len()
                ))
                .size(13.0)
                .color(COLOR_TEXT_MUTED),
            );
            if let Some(error) = &blocker {
                ui.colored_label(COLOR_WARNING, error);
            }
        });
        ui.add_space(4.0);
        let current_user = self
            .github_login
            .clone()
            .unwrap_or_else(|| "maintainer".to_string());
        ui.horizontal_wrapped(|ui| {
            ui.add_enabled_ui(!self.review_busy && !self.is_loading, |ui| {
                if batch.available() && blocker.is_none() {
                    if primary(ui, "Claim batch")
                        .on_hover_text(format!(
                            "Claim this batch for @{current_user} and mark its PRs as in-review"
                        ))
                        .clicked()
                    {
                        let batch_id = batch.id.clone();
                        let user = current_user.clone();
                        let tx = self.tx.clone();
                        let context = self.context.clone();
                        self.review_busy = true;
                        self.review_message = "Claiming batch…".into();
                        thread::spawn(move || {
                            let _ = tx.send(AsyncMessage::ReviewAction(
                                review::claim_direct(&user, &batch_id),
                                None,
                            ));
                            context.request_repaint();
                        });
                    }
                } else {
                    if primary(ui, "Release / Unblock batch")
                        .on_hover_text(
                            "Unclaim and unblock this batch, freeing its open PRs back to the available pool",
                        )
                        .clicked()
                    {
                        let batch_id = batch.id.clone();
                        let tx = self.tx.clone();
                        let context = self.context.clone();
                        self.review_busy = true;
                        self.review_message = "Releasing & unblocking batch…".into();
                        thread::spawn(move || {
                            let _ = tx.send(AsyncMessage::ReviewAction(
                                review::release_direct(&batch_id),
                                None,
                            ));
                            context.request_repaint();
                        });
                    }
                    if ui
                        .button("Complete batch")
                        .on_hover_text(
                            "Mark merged/closed PRs as finished, release remaining open PRs, and close this batch",
                        )
                        .clicked()
                    {
                        let batch_id = batch.id.clone();
                        let tx = self.tx.clone();
                        let context = self.context.clone();
                        self.review_busy = true;
                        self.review_message = "Completing batch…".into();
                        thread::spawn(move || {
                            let _ = tx.send(AsyncMessage::ReviewAction(
                                review::complete_direct(&batch_id),
                                None,
                            ));
                            context.request_repaint();
                        });
                    }
                }
            });
            if ui.button("Refresh checks").clicked() {
                self.request_checks(batch.pr_numbers.clone());
            }
            egui::ComboBox::from_id_salt("batch_tools")
                .selected_text("More")
                .show_ui(ui, |ui| {
                    if ui.button("Copy review prompt").clicked() {
                        match generate_ai_prompt(&batch, &self.prs) {
                            Ok(text) => ui.ctx().copy_text(text),
                            Err(error) => self.review_message = error,
                        }
                    }
                    if ui.button("View AI prompt").clicked() {
                        self.inspector_prompt = true;
                    }
                    if ui.button("Copy batch ID").clicked() {
                        ui.ctx().copy_text(batch.id.clone());
                    }
                    if ui.button("Copy merge script").clicked() {
                        match generate_merge_script(&batch, &self.prs) {
                            Ok(text) => ui.ctx().copy_text(text),
                            Err(error) => self.review_message = error,
                        }
                    }
                    ui.separator();
                    if ui
                        .button(format!("Unblock all @{current_user} batches"))
                        .clicked()
                    {
                        let user = current_user.clone();
                        let tx = self.tx.clone();
                        let context = self.context.clone();
                        self.review_busy = true;
                        self.review_message = format!("Unblocking all batches for @{user}…");
                        thread::spawn(move || {
                            let _ = tx.send(AsyncMessage::ReviewAction(
                                review::release_all_direct(&user),
                                None,
                            ));
                            context.request_repaint();
                        });
                    }
                    if ui.button("Unblock ALL claimed batches").clicked() {
                        let tx = self.tx.clone();
                        let context = self.context.clone();
                        self.review_busy = true;
                        self.review_message = "Unblocking all claimed batches…".into();
                        thread::spawn(move || {
                            let _ = tx.send(AsyncMessage::ReviewAction(
                                review::release_all_direct("*"),
                                None,
                            ));
                            context.request_repaint();
                        });
                    }
                });
        });
        if let Some(cancel) = &self.review_cancel {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Waiting for browser sign-in…");
                if ui.small_button("Cancel").clicked() {
                    cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            });
        }
        if !self.review_message.is_empty() && self.review_cancel.is_none() {
            ui.add(
                egui::Label::new(
                    RichText::new(&self.review_message)
                        .size(12.0)
                        .color(COLOR_TEXT_MUTED),
                )
                .wrap(),
            );
        }
        ui.add_space(16.0);
        if self.inspector_prompt {
            if ui.button("← Review workspace").clicked() {
                self.inspector_prompt = false;
            }
            if self
                .prompt_cache
                .as_ref()
                .is_none_or(|(id, _)| id != &batch.id)
            {
                let text = generate_ai_prompt(&batch, &self.prs)
                    .unwrap_or_else(|e| format!("Review blocked: {e}"));
                self.prompt_cache = Some((batch.id.clone(), text));
            }
            let cached_text = self
                .prompt_cache
                .as_ref()
                .map(|(_, t)| t.as_str())
                .unwrap_or("");
            Self::prompt_panel(ui, cached_text, &mut self.raw_prompt);
            return;
        }
        if !self
            .selected_pr
            .is_some_and(|n| batch.pr_numbers.contains(&n))
        {
            self.selected_pr = batch.pr_numbers.first().copied();
        }
        if ui.available_width() < 800.0 {
            egui::ComboBox::from_id_salt("inspector_pr_picker")
                .selected_text(
                    self.selected_pr
                        .map(|n| format!("PR #{n}"))
                        .unwrap_or("Select PR".into()),
                )
                .show_ui(ui, |ui| {
                    for number in &batch.pr_numbers {
                        ui.selectable_value(
                            &mut self.selected_pr,
                            Some(*number),
                            format!(
                                "#{number} {}",
                                self.prs
                                    .get(number)
                                    .map(|p| p.title.as_str())
                                    .unwrap_or("Missing data")
                            ),
                        );
                    }
                });
            self.render_pr_details(ui);
        } else {
            let width = ui.available_width();
            let height = ui.available_height();
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new((width - 12.0) * 0.32, height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_max_width((width - 12.0) * 0.32);
                        ScrollArea::vertical()
                            .id_salt("review_pr_list")
                            .show(ui, |ui| {
                                for number in &batch.pr_numbers {
                                    let selected = self.selected_pr == Some(*number);
                                    let response = card()
                                        .fill(if selected {
                                            Color32::from_rgb(25, 50, 82)
                                        } else {
                                            COLOR_CARD_BG
                                        })
                                        .inner_margin(12)
                                        .show(ui, |ui| {
                                            ui.spacing_mut().item_spacing.y = 5.0;
                                            if let Some(pr) = self.prs.get(number) {
                                                ui.horizontal(|ui| {
                                                    ui.label(
                                                        RichText::new(format!("#{number}"))
                                                            .size(13.0)
                                                            .color(COLOR_PS5_CYAN),
                                                    );
                                                    ui.with_layout(
                                                        egui::Layout::right_to_left(
                                                            egui::Align::Center,
                                                        ),
                                                        |ui| {
                                                            ui.label(
                                                                RichText::new(
                                                                    if pr.status.is_empty()
                                                                        || pr.status == "unassigned"
                                                                    {
                                                                        "Not reviewed"
                                                                    } else {
                                                                        &pr.status
                                                                    },
                                                                )
                                                                .size(12.0)
                                                                .color(COLOR_TEXT_MUTED),
                                                            );
                                                        },
                                                    );
                                                });
                                                let mut job = egui::text::LayoutJob::simple(
                                                    pr.title.clone(),
                                                    egui::FontId::proportional(15.0),
                                                    COLOR_TEXT_WHITE,
                                                    ui.available_width(),
                                                );
                                                job.wrap.max_rows = 2;
                                                job.wrap.overflow_character = Some('…');
                                                let galley = ui.painter().layout_job(job);
                                                ui.label(galley).on_hover_text(&pr.title);
                                                ui.label(colored_batch_summary(&format!(
                                                    "@{} · {} files · {}",
                                                    pr.author_name(),
                                                    pr.files.len(),
                                                    pr.diff_summary().replace('-', "−")
                                                )));
                                                let ci = if self.checks_busy.contains(number) {
                                                    "Loading checks…"
                                                } else if self.checks_errors.contains_key(number) {
                                                    "Checks unavailable"
                                                } else {
                                                    self.checks
                                                        .get(number)
                                                        .map(|c| c.label())
                                                        .unwrap_or("Checks not fetched")
                                                };
                                                ui.label(
                                                    RichText::new(ci)
                                                        .size(12.0)
                                                        .color(COLOR_TEXT_MUTED),
                                                );
                                            } else {
                                                ui.colored_label(
                                                    COLOR_WARNING,
                                                    format!("#{number}: missing data"),
                                                );
                                            }
                                        });
                                    if selected {
                                        ui.painter().rect_filled(
                                            egui::Rect::from_min_size(
                                                response.response.rect.left_top()
                                                    + Vec2::new(1.0, 8.0),
                                                Vec2::new(
                                                    3.0,
                                                    (response.response.rect.height() - 16.0)
                                                        .max(0.0),
                                                ),
                                            ),
                                            2,
                                            COLOR_PS5_CYAN,
                                        );
                                    }
                                    let row = ui.interact(
                                        response.response.rect,
                                        ui.scope_id().with(("pr_card", number)),
                                        egui::Sense::click(),
                                    );
                                    if row.clicked() {
                                        self.selected_pr = Some(*number);
                                    }
                                    ui.add_space(8.0);
                                }
                            });
                    },
                );
                ui.allocate_ui_with_layout(
                    Vec2::new((width - 12.0) * 0.68, height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_max_width((width - 12.0) * 0.68);
                        self.render_pr_details(ui);
                    },
                );
            });
        }
    }

    fn request_checks(&mut self, numbers: Vec<u64>) {
        let numbers: Vec<_> = numbers
            .into_iter()
            .filter(|n| !self.checks_busy.contains(n))
            .collect();
        for number in &numbers {
            self.checks_busy.insert(*number);
            self.checks_requested.insert(
                *number,
                self.prs.get(number).and_then(|p| p.head_sha.clone()),
            );
            self.checks_errors.remove(number);
        }
        if numbers.is_empty() {
            return;
        }
        let tx = self.tx.clone();
        let context = self.context.clone();
        thread::spawn(move || {
            let workers = numbers.len().min(4);
            let idx = std::sync::atomic::AtomicUsize::new(0);
            thread::scope(|s| {
                for _ in 0..workers {
                    let tx = tx.clone();
                    let context = context.clone();
                    let numbers = &numbers;
                    let idx = &idx;
                    s.spawn(move || {
                        loop {
                            let i = idx.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            let Some(&number) = numbers.get(i) else {
                                break;
                            };
                            let _ = tx.send(AsyncMessage::ChecksLoaded(number, github::checks(number)));
                            context.request_repaint();
                        }
                    });
                }
            });
        });
    }

    fn render_checks(&mut self, ui: &mut egui::Ui, pr: &PullRequest) {
        let loading = self.checks_busy.contains(&pr.number);
        ui.horizontal_wrapped(|ui| {
            if loading {
                ui.spinner();
                ui.label("Loading checks…");
            }
            if ui
                .add_enabled(
                    !loading,
                    egui::Button::new(if self.checks_errors.contains_key(&pr.number) {
                        "Retry"
                    } else {
                        "Refresh"
                    }),
                )
                .clicked()
            {
                self.request_checks(vec![pr.number]);
            }
            if let Some(checks) = self.checks.get(&pr.number) {
                ui.label(
                    RichText::new(format!(
                        "Updated {}s ago",
                        checks.fetched.elapsed().as_secs()
                    ))
                    .size(11.0)
                    .color(COLOR_TEXT_MUTED),
                );
            }
        });
        if let Some(error) = self.checks_errors.get(&pr.number) {
            ui.colored_label(COLOR_WARNING, error);
        }
        if let Some(checks) = self.checks.get(&pr.number) {
            let stale = self.checks_errors.contains_key(&pr.number)
                || checks.fetched.elapsed().as_secs() > 300
                || checks.head_changed
                || pr.head_sha.as_deref() != Some(checks.head.as_str());
            card().inner_margin(14).show(ui, |ui| {
                ui.label(
                    RichText::new(checks.label()).size(18.0).color(
                        if checks
                            .entries
                            .iter()
                            .any(|e| e.result == github::CheckResult::Failed)
                        {
                            COLOR_DANGER
                        } else if stale {
                            COLOR_WARNING
                        } else {
                            COLOR_TEXT_WHITE
                        },
                    ),
                );
                ui.add(egui::Label::new(&checks.summary).wrap());
                ui.label(&checks.mergeability);
                ui.label(
                    RichText::new("Required checks: unknown · Review decision: separate")
                        .size(12.0)
                        .color(COLOR_TEXT_MUTED),
                );
                ui.label(
                    RichText::new(format!(
                        "Checked commit: {}",
                        checks.head.get(..8).unwrap_or(&checks.head)
                    ))
                    .monospace()
                    .size(12.0),
                )
                .on_hover_text(&checks.head);
            });
            if stale {
                ui.colored_label(
                    COLOR_WARNING,
                    "Results may be stale. Refresh data and checks before deciding.",
                );
            }
            if checks.entries.is_empty() {
                ui.label("No checks reported for this commit.");
            }
            for entry in checks
                .entries
                .iter()
                .filter(|e| e.result != github::CheckResult::Passed)
            {
                check_row(ui, entry);
            }
            let passed: Vec<_> = checks
                .entries
                .iter()
                .filter(|e| e.result == github::CheckResult::Passed)
                .collect();
            if !passed.is_empty() {
                egui::CollapsingHeader::new(format!("Passed ({})", passed.len()))
                    .default_open(passed.len() < 5)
                    .show(ui, |ui| {
                        for entry in passed {
                            check_row(ui, entry);
                        }
                    });
            }
            ui.add_space(8.0);
            let next = if stale {
                "Refresh the snapshot and review any new commits."
            } else if checks
                .entries
                .iter()
                .any(|e| e.result == github::CheckResult::Failed)
            {
                "Open the failed check logs and resolve the failure."
            } else if checks
                .entries
                .iter()
                .any(|e| e.result == github::CheckResult::Running)
            {
                "Checks are still running. Refresh when they finish."
            } else if checks.entries.is_empty() {
                "Verify which checks are expected in the repository."
            } else {
                "Review the code and repository requirements before recording a decision."
            };
            ui.add(egui::Label::new(format!("Next: {next}")).wrap());
        }
        ui.hyperlink_to(
            "View checks on GitHub ↗",
            format!(
                "https://github.com/boykopovar/AnyPS5/pull/{}/checks",
                pr.number
            ),
        );
    }

    fn request_description(&mut self, pr: &PullRequest) {
        let key = (pr.number, pr.head_sha.clone());
        if !self.descriptions_busy.insert(key.clone()) {
            return;
        }
        let tx = self.tx.clone();
        let context = self.context.clone();
        thread::spawn(move || {
            let result = github::description(key.0, key.1.as_deref());
            let _ = tx.send(AsyncMessage::DescriptionLoaded(key.0, key.1, result));
            context.request_repaint();
        });
    }

    fn render_pr_details(&mut self, ui: &mut egui::Ui) {
        let Some(pr) = self.selected_pr.and_then(|n| self.prs.get(&n)).cloned() else {
            ui.label("PR data unavailable.");
            return;
        };
        if !cfg!(test) && self.detail_tab == 0 {
            let key = (pr.number, pr.head_sha.clone());
            if !self.descriptions.contains_key(&key) && !self.descriptions_busy.contains(&key) {
                self.request_description(&pr);
            }
        }
        if self.detail_tab == 3
            && !self.checks_busy.contains(&pr.number)
            && self.checks_requested.get(&pr.number) != Some(&pr.head_sha)
        {
            self.request_checks(vec![pr.number]);
        }
        card().inner_margin(18).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ScrollArea::vertical().id_salt(("review_details", pr.number, self.detail_tab)).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("#{}",pr.number)).size(12.0).color(COLOR_PS5_CYAN));
                    ui.label(RichText::new(pr.github_state.as_deref().unwrap_or("Unknown state")).size(12.0).color(COLOR_TEXT_MUTED));
                    if pr.is_draft { ui.colored_label(COLOR_WARNING,"Draft"); }
                });
                ui.add(egui::Label::new(RichText::new(&pr.title).size(22.0)).wrap());
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("@{}",pr.author_name())).size(14.0).color(COLOR_TEXT_MUTED));
                    ui.hyperlink_to("View on GitHub ↗",format!("https://github.com/boykopovar/AnyPS5/pull/{}",pr.number));
                });
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    for (tab,label) in [(0,"Overview"),(1,"Files"),(2,"Review"),(3,"Checks")] {
                        let label = if tab == 1 {format!("Files {}",pr.files.len())} else {label.into()};
                        let selected = self.detail_tab == tab;
                        let response = ui.add(egui::Button::new(RichText::new(label).size(14.0).color(if selected {COLOR_TEXT_WHITE} else {COLOR_TEXT_MUTED})).frame(false));
                        if selected {ui.painter().line_segment([response.rect.left_bottom(),response.rect.right_bottom()],Stroke::new(2.0,COLOR_PS5_CYAN));}
                        if response.clicked() {self.detail_tab = tab;}
                    }
                });
                ui.add_space(12.0);
            match self.detail_tab {

                0 => {
                    ui.label(colored_batch_summary(&format!("{} changed files · {} lines",pr.files.len(),pr.diff_summary().replace('-',"−"))));
                    ui.add_space(16.0);
                    ui.label(RichText::new("Description").size(17.0));
                    let key = (pr.number,pr.head_sha.clone());
                    if self.descriptions_busy.contains(&key) {ui.horizontal(|ui| {ui.spinner();ui.label("Loading description…");});}
                    if let Some(result) = self.descriptions.get(&key) {
                        match result {
                            Ok(body) if body.trim().is_empty() => {ui.label(RichText::new("No description provided.").color(COLOR_TEXT_MUTED));},
                            Ok(body) => {description_view(ui,body);},
                            Err(error) => {ui.add(egui::Label::new(RichText::new(error).color(COLOR_WARNING)).wrap());},
                        }
                    } else if let Some(body) = pr.body.as_deref().filter(|body| !body.trim().is_empty()) {description_view(ui,body);}
                    if ui.add_enabled(!self.descriptions_busy.contains(&key),egui::Button::new(if self.descriptions.get(&key).is_some_and(Result::is_err) {"Retry description"} else {"Refresh description"})).clicked() {self.request_description(&pr);}
                    ui.add_space(12.0);
                    egui::CollapsingHeader::new("Commit details").show(ui, |ui| {
                        ui.add(egui::Label::new(RichText::new(pr.head_sha.as_deref().unwrap_or("Unknown SHA")).monospace().size(12.0)).wrap().selectable(true));
                        if let Some(sha) = &pr.head_sha && ui.small_button("Copy SHA").clicked() { ui.ctx().copy_text(sha.clone()); }
                    });
                }
                1 => {
                    ui.label(format!("{} changed files", pr.files.len()));
                    for file in &pr.files {
                        let path = file.path.as_deref().unwrap_or("Unknown path");
                        ui.add(egui::Label::new(RichText::new(path).monospace().size(12.0)).wrap().selectable(true));
                        if let (Some(a), Some(d)) = (file.additions, file.deletions) {
                            ui.label(colored_batch_summary(&format!("{} · +{a} / −{d}", file.change_type.as_deref().unwrap_or("Changed"))));
                        } else {
                            ui.label(RichText::new("Per-file diff statistics unavailable").size(11.0).color(COLOR_TEXT_MUTED));
                        }
                        ui.separator();
                    }
                    ui.hyperlink_to("View full diff on GitHub ↗", format!("https://github.com/boykopovar/AnyPS5/pull/{}/files", pr.number));
                }
                2 => {
                    let current_user = self
                        .github_login
                        .clone()
                        .unwrap_or_else(|| "maintainer".to_string());
                    let draft = self.drafts.entry(pr.number).or_insert_with(|| review::Draft::new(&pr));
                    if draft.original.head_sha != pr.head_sha { ui.colored_label(COLOR_WARNING,"Head SHA changed since this draft was opened. Copy the draft, then reload it before saving."); }
                    ui.label(format!("Saved status: {} · Assigned: {}", pr.status, pr.assigned_reviewer.as_deref().unwrap_or("Unknown")));
                    ui.add_enabled_ui(!self.review_busy, |ui| {
                        for (key,label) in [("build","Build checked"),("types","Types / ABI checked"),("locks","Thread safety checked"),("tests","Tests checked")] {
                            let mut checked = draft.checklist[key].as_bool().unwrap_or(false);
                            if ui.checkbox(&mut checked,label).changed() { draft.checklist[key] = serde_json::json!(checked); }
                        }
                        egui::ComboBox::from_id_salt("review_decision").selected_text(&draft.status).show_ui(ui, |ui| {
                            for status in ["in-review","approved","changes"] { ui.selectable_value(&mut draft.status,status.into(),status); }
                        });
                        ui.add(egui::TextEdit::multiline(&mut draft.notes).desired_width(ui.available_width()).desired_rows(8).hint_text("Review findings and testing notes"));
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Copy review notes").clicked() { ui.ctx().copy_text(draft.notes.clone()); }
                        if ui.button("Reload saved review").on_hover_text("Discard this local draft and reload the database snapshot").clicked() { *draft = review::Draft::new(&pr); }
                        if ui.add_enabled(!self.review_busy && draft.original.head_sha == pr.head_sha, egui::Button::new("Save review to Supabase")).clicked() {
                            let draft = draft.clone();
                            let user = current_user.clone();
                            let tx = self.tx.clone();
                            let context = self.context.clone();
                            self.review_busy = true;
                            thread::spawn(move || {
                                let _ = tx.send(AsyncMessage::ReviewAction(
                                    review::save_direct(&user, &draft),
                                    Some(draft.original.number),
                                ));
                                context.request_repaint();
                            });
                        }
                    });
                }
                _ => {
                    self.render_checks(ui, &pr);

                }
            }
        });
        });
    }

    fn prompt_panel(
        ui: &mut egui::Ui,
        text: &str,
        raw: &mut bool,
    ) {
        ui.heading("Review prompt");
        ui.horizontal(|ui| {
            ui.selectable_value(raw, false, "Preview");
            ui.selectable_value(raw, true, "Markdown");
        });
        ui.add_space(8.0);
        card().show(ui, |ui| {
            ScrollArea::vertical()
                .id_salt("prompt_text")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if *raw {
                        ui.add(
                            egui::Label::new(RichText::new(text).monospace().size(13.0))
                                .wrap()
                                .selectable(true),
                        );
                    } else {
                        for line in text.lines() {
                            if line.trim().is_empty() {
                                ui.add_space(6.0);
                                continue;
                            }
                            if line == "---" {
                                ui.separator();
                                continue;
                            }
                            let level = line.chars().take_while(|c| *c == '#').count();
                            let clean = line
                                .trim_start_matches('#')
                                .trim()
                                .replace("**", "")
                                .replace('`', "");
                            let clean = clean.replace(
                                ['🎯', '📦', '📋', '🛡', '️', '📝', '✅', '🟡', '🔴', '🟢'],
                                "",
                            );
                            let size = match level {
                                1 => 21.0,
                                2 => 17.0,
                                3 => 15.0,
                                _ => 14.0,
                            };
                            let rich = RichText::new(clean).size(size);
                            ui.add(
                                egui::Label::new(if level > 0 { rich.strong() } else { rich })
                                    .wrap()
                                    .selectable(true),
                            );
                        }
                    }
                });
        });
    }

    fn render_sidebar_profile(&mut self, ui: &mut egui::Ui) {
        if let Some(login) = self.github_login.clone() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if let Some(avatar) = &self.github_avatar {
                    ui.add(
                        egui::Image::new(avatar)
                            .fit_to_exact_size(Vec2::splat(32.0))
                            .corner_radius(16),
                    );
                } else {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(32.0), egui::Sense::hover());
                    ui.painter()
                        .circle_filled(rect.center(), 16.0, COLOR_CARD_BG);
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        login
                            .chars()
                            .next()
                            .unwrap_or('?')
                            .to_uppercase()
                            .to_string(),
                        egui::FontId::proportional(15.0),
                        COLOR_TEXT_WHITE,
                    );
                }
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.add(
                        egui::Label::new(
                            RichText::new(format!("@{login}"))
                                .size(12.5)
                                .strong()
                                .color(COLOR_TEXT_WHITE),
                        )
                        .truncate(),
                    )
                    .on_hover_text(&login);

                    if ui
                        .add_enabled(
                            !self.github_busy,
                            egui::Button::new(
                                RichText::new("Log out").size(11.0).color(COLOR_TEXT_MUTED),
                            )
                            .frame(false),
                        )
                        .on_hover_text("Sign out of GitHub")
                        .clicked()
                    {
                        self.github_busy = true;
                        let tx = self.tx.clone();
                        let context = self.context.clone();
                        thread::spawn(move || {
                            let _ = tx.send(AsyncMessage::LoggedOut(github::logout(&login)));
                            context.request_repaint();
                        });
                    }
                });
            });
        } else if ui
            .add_enabled(
                !self.github_busy,
                egui::Button::new(if self.github_busy {
                    "Signing in…"
                } else {
                    "Sign in with GitHub"
                })
                .min_size(Vec2::new(ui.available_width(), 32.0)),
            )
            .on_hover_text(if self.github_notice.is_empty() {
                "Sign in through your browser"
            } else {
                &self.github_notice
            })
            .clicked()
        {
            self.check_github(true);
        }
    }

    fn render_sidebar_connected_card(&mut self, ui: &mut egui::Ui) {
        if !self.sync_status.is_empty() {
            ui.horizontal(|ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(&self.sync_status)
                            .size(10.0)
                            .color(COLOR_PS5_CYAN),
                    )
                    .truncate(),
                );
            });
            ui.add_space(2.0);
        }

        ui.horizontal(|ui| {
            ui.checkbox(
                &mut self.auto_sync_enabled,
                RichText::new("Auto-Sync").size(11.0),
            )
            .on_hover_text(
                "Automatically coordinates GitHub -> Supabase sync and 3-lane batch generation using distributed lease lock.",
            );

            let can_sync = !self.sync_busy && !self.is_loading;
            if ui
                .add_enabled(
                    can_sync,
                    egui::Button::new(RichText::new("⚡ Sync All").size(10.0)),
                )
                .on_hover_text(
                    "Manually trigger GitHub sync and smart batch regeneration right now",
                )
                .clicked()
            {
                self.trigger_sync(0);
            }
        });
        ui.add_space(4.0);

        if self.active_peers > 1 {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{} instances online · slot #{}",
                        self.active_peers,
                        self.slot_index + 1
                    ))
                    .size(10.0)
                    .color(COLOR_TEXT_MUTED),
                );
            });
            ui.add_space(2.0);
        }

        ui.horizontal(|ui| {
            let live = self.status_message.starts_with("Supabase Live");
            ui.colored_label(
                if live && !self.is_loading {
                    COLOR_SUCCESS
                } else {
                    COLOR_WARNING
                },
                "●",
            );
            let label_text = if self.sync_busy {
                "Syncing GitHub…"
            } else if self.is_loading {
                "Refreshing…"
            } else if live {
                if self.active_peers > 1 {
                    "Connected (Cluster)"
                } else {
                    "Connected"
                }
            } else {
                "Offline"
            };
            ui.label(RichText::new(label_text).size(12.0))
                .on_hover_text(&self.status_message);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        !self.is_loading && !self.sync_busy,
                        egui::Button::new("⟳").frame(false),
                    )
                    .on_hover_text("Refresh data now")
                    .clicked()
                {
                    self.trigger_fetch();
                }
            });
        });
    }

    fn render_sidebar_runner_card(&mut self, ui: &mut egui::Ui) {
        card().inner_margin(12).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let configured = self.runner_configured;
            ui.horizontal(|ui| {
                ui.label(RichText::new("Build Runner").size(13.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(if configured {
                            "Configured"
                        } else {
                            "Not set up"
                        })
                        .size(10.0)
                        .color(COLOR_TEXT_MUTED),
                    );
                });
            });
            if ui
                .add_sized(
                    [ui.available_width(), 32.0],
                    egui::Button::new(if configured {
                        "Open runner"
                    } else {
                        "Set up runner"
                    }),
                )
                .clicked()
            {
                self.runner_configured = CpuWorker::has_runner_credentials();
                self.runner_dialog = true;
            }
            let missing: Vec<_> = [
                ("Git", self.system_info.has_git),
                ("CMake", self.system_info.has_cmake),
                ("Ninja", self.system_info.has_ninja),
                ("Clang++", self.system_info.has_clang),
            ]
            .into_iter()
            .filter_map(|(name, installed)| (!installed).then_some(name))
            .collect();
            if !missing.is_empty() {
                ui.label(
                    RichText::new(format!("Missing: {}", missing.join(", ")))
                        .size(11.0)
                        .color(COLOR_WARNING),
                );
            }
        });
    }

    fn runner_setup_panel(&mut self, ui: &mut egui::Ui) {
        card().inner_margin(18).show(ui, |ui| {
            ui.set_min_width(ui.available_width());

            let has_credentials = self.runner_configured;

            if has_credentials {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("●").size(16.0).color(COLOR_SUCCESS));
                    ui.label(RichText::new("Runner Configured").size(20.0).color(COLOR_TEXT_WHITE).strong());
                    ui.colored_label(COLOR_SUCCESS, "(.credentials detected)");
                });
                ui.add_space(4.0);
                ui.add(egui::Label::new("This machine has valid runner credentials and is ready to build AnyPS5 jobs.").wrap());
                ui.add_space(10.0);

                ui.checkbox(&mut self.use_windows_sandbox, "🛡️ Run inside Windows Sandbox (100% Hyper-V Isolation)");
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    if primary(ui, "🚀 1-Click Launch Runner →").clicked() {
                        let res = CpuWorker::launch_runner(None, self.use_windows_sandbox);
                        let message = match res {
                            Ok(()) => if self.use_windows_sandbox {
                                "🚀 Windows Sandbox opened with AnyPS5 runner!".into()
                            } else {
                                "🚀 Runner launched in PowerShell!".into()
                            },
                            Err(error) => error,
                        };
                        self.copied_notice = Some((message, std::time::Instant::now()));
                    }

                    if ui.button(RichText::new("Reset Credentials").size(12.0).color(COLOR_TEXT_MUTED)).on_hover_text("Delete .credentials to re-register with a new token").clicked() {
                        let _ = CpuWorker::reset_runner_credentials();
                        self.runner_configured = CpuWorker::has_runner_credentials();
                        self.copied_notice = Some(("Runner credentials reset. Please re-register.".into(), std::time::Instant::now()));
                    }
                });
            } else {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("●").size(16.0).color(COLOR_WARNING));
                    ui.label(RichText::new("First-Time Setup: Runner Token Required").size(18.0).color(COLOR_TEXT_WHITE).strong());
                });
                ui.add_space(4.0);
                ui.add(egui::Label::new("No .credentials found yet. Enter your registration token from GitHub repository settings:").wrap());
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    ui.label("Token:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.runner_token)
                            .hint_text("Paste GitHub Runner Token...").password(true)
                            .desired_width(260.0)
                    );
                });

                ui.add_space(8.0);
                ui.checkbox(&mut self.use_windows_sandbox, "🛡️ Run inside Windows Sandbox (100% Hyper-V Isolation)");
                ui.add_space(10.0);

                let can_launch = !self.runner_token.trim().is_empty();
                let btn = ui.add_enabled(
                    can_launch,
                    egui::Button::new(RichText::new("⚡ Register & Launch Runner →").color(Color32::WHITE).strong())
                        .fill(COLOR_PS5_BLUE)
                        .min_size(Vec2::new(160.0, 38.0))
                        .corner_radius(20)
                );
                focus_ring(ui, &btn, 20);

                if btn.clicked() {
                    let res = CpuWorker::launch_runner(Some(&self.runner_token), self.use_windows_sandbox);
                    self.runner_configured = CpuWorker::has_runner_credentials();
                    let message = match res {
                        Ok(()) => if self.use_windows_sandbox {
                            "⚡ Runner launched in Windows Sandbox!".into()
                        } else {
                            "⚡ Runner registration launched in PowerShell!".into()
                        },
                        Err(error) => error,
                    };
                    self.copied_notice = Some((message, std::time::Instant::now()));
                }
            }

            ui.add_space(12.0);
            ui.label(RichText::new("Activity: Runner processes jobs directly from GitHub Actions queue.").size(12.0).color(COLOR_TEXT_MUTED));
        });
    }

    fn runner_hardware_panel(&self, ui: &mut egui::Ui) {
        card().inner_margin(18).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.heading("This computer");
            ui.label(format!(
                "{} / {} · {} logical cores",
                self.system_info.os, self.system_info.arch, self.system_info.cpu_cores
            ));
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                for (name, installed) in [
                    ("Git", self.system_info.has_git),
                    ("CMake", self.system_info.has_cmake),
                    ("Ninja", self.system_info.has_ninja),
                    ("Clang++", self.system_info.has_clang),
                ] {
                    ui.colored_label(
                        if installed {
                            COLOR_SUCCESS
                        } else {
                            COLOR_WARNING
                        },
                        format!("{} {name}", if installed { "✓" } else { "–" }),
                    );
                }
            });
        });
    }

    fn render_cpu_worker_view(&mut self, ui: &mut egui::Ui) {
        heading(
            ui,
            "Build Runner",
            "Prepare this computer for GitHub Actions build jobs.",
        );
        ScrollArea::vertical().id_salt("runner_page").show(ui, |ui| {
            if ui.available_width() >= 850.0 {
                ui.columns(2, |columns| {
                    self.runner_setup_panel(&mut columns[0]);
                    self.runner_hardware_panel(&mut columns[1]);
                });
            } else {
                self.runner_setup_panel(ui);
                ui.add_space(12.0);
                self.runner_hardware_panel(ui);
            }
            ui.add_space(20.0);
            ui.heading("Getting started");
            ui.add_space(8.0);
            for (number, title, body) in [
                ("01", "Check your tools", "Confirm your build tools are available. Builds may also require the Vulkan SDK and repository dependencies."),
                ("02", "Register the runner", "Finish setup in the PowerShell window using a repository runner registration token."),
                ("03", "Follow jobs on GitHub", "Keep the runner session open. Repository workflows select the jobs that run here."),
            ] {
                card().inner_margin(14).show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal_top(|ui| {
                        ui.label(RichText::new(number).size(20.0).color(COLOR_PS5_CYAN));
                        ui.vertical(|ui| {
                            ui.label(RichText::new(title).size(16.0));
                            ui.add(egui::Label::new(RichText::new(body).size(13.0).color(COLOR_TEXT_MUTED)).wrap());
                        });
                    });
                });
                ui.add_space(8.0);
            }
            ui.hyperlink_to("Open repository Actions ↗", "https://github.com/boykopovar/AnyPS5/actions");
            ui.add_space(12.0);
            ui.label(RichText::new("Native build worker is not implemented. This app opens runner setup; it does not verify approval gates or monitor jobs.").size(12.0).color(COLOR_TEXT_MUTED));
        });
    }

    fn render_entire_repo_view(&mut self, ui: &mut egui::Ui) {
        self.ensure_data_cache();
        heading(
            ui,
            "Pull Requests",
            "Explore the repository. Select a pull request for its files and details.",
        );
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.search_query)
                    .hint_text("Search title, author or PR number")
                    .desired_width(300.0),
            );
            egui::ComboBox::from_id_salt("repo_sort")
                .selected_text(if self.newest_first {
                    "Newest first"
                } else {
                    "Oldest first"
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.newest_first, true, "Newest first");
                    ui.selectable_value(&mut self.newest_first, false, "Oldest first");
                });
        });
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("repo_category")
                .selected_text(if self.repo_category.is_empty() {
                    "All categories"
                } else {
                    self.repo_category.as_str()
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.repo_category, String::new(), "All categories");
                    for value in &self.repo_categories {
                        ui.selectable_value(
                            &mut self.repo_category,
                            value.clone(),
                            value.as_str(),
                        );
                    }
                });
            egui::ComboBox::from_id_salt("repo_status")
                .selected_text(if self.repo_status.is_empty() {
                    "All statuses"
                } else {
                    self.repo_status.as_str()
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.repo_status, String::new(), "All statuses");
                    for value in &self.repo_statuses {
                        ui.selectable_value(
                            &mut self.repo_status,
                            value.clone(),
                            value.as_str(),
                        );
                    }
                });
        });
        ui.add_space(12.0);
        let pr_key_changed = self.pr_filter_key.as_ref().is_none_or(|k| {
            k.query != self.search_query
                || k.newest_first != self.newest_first
                || k.category != self.repo_category
                || k.status != self.repo_status
        });
        if pr_key_changed {
            let filter = self.search_query.to_lowercase();
            let mut numbers: Vec<u64> = self
                .prs
                .values()
                .filter(|pr| {
                    pr.is_open()
                        && (self.repo_category.is_empty() || pr.category == self.repo_category)
                        && (self.repo_status.is_empty() || pr.status == self.repo_status)
                        && (filter.is_empty()
                            || pr.title.to_lowercase().contains(&filter)
                            || pr.author_name().to_lowercase().contains(&filter)
                            || pr.number.to_string().contains(&filter))
                })
                .map(|pr| pr.number)
                .collect();
            numbers.sort_unstable();
            if self.newest_first {
                numbers.reverse();
            }
            self.filtered_pr_numbers = numbers;
            self.pr_filter_key = Some(PrFilterKey {
                query: self.search_query.clone(),
                newest_first: self.newest_first,
                category: self.repo_category.clone(),
                status: self.repo_status.clone(),
            });
        }
        let filtered_len = self.filtered_pr_numbers.len();
        ui.label(
            RichText::new(format!(
                "{} of {} open pull requests",
                filtered_len, self.open_prs_count
            ))
            .size(12.0)
            .color(COLOR_TEXT_MUTED),
        );
        ui.add_space(12.0);
        // A selected PR opens in a separate bounded window, leaving the virtual list intact.
        if let Some(number) = self.selected_pr {
            let mut open = true;
            egui::Window::new(format!("Pull Request #{number}"))
                .id(egui::Id::unique("pr_details"))
                .open(&mut open)
                .default_width(540.0)
                .max_width(700.0)
                .max_height(650.0)
                .show(ui.ctx(), |ui| {
                    ScrollArea::vertical().show(ui, |ui| {
                        if let Some(pr) = self.prs.get(&number) {
                            pr_card(ui, pr);
                        }
                    });
                });
            if !open {
                self.selected_pr = None;
            }
        }
        let author_width = if ui.available_width() < 600.0 {
            85.0
        } else {
            125.0
        };
        let changes_width = 80.0;
        let number_width = 60.0;
        let title_width =
            (ui.available_width() - author_width - changes_width - number_width - 48.0).max(100.0);
        ui.horizontal(|ui| {
            for (label, width) in [
                ("PR", number_width),
                ("Title", title_width),
                ("Author", author_width),
                ("Changes", changes_width),
            ] {
                table_cell(
                    ui,
                    width,
                    26.0,
                    RichText::new(label).size(12.0).color(COLOR_TEXT_MUTED),
                );
            }
        });
        ui.separator();
        if filtered_len == 0 {
            ui.label("No matching pull requests.");
        }
        let list_width = ui.available_width();
        ScrollArea::vertical()
            .id_salt("repo_rows")
            .auto_shrink([false, false])
            .show_rows(ui, 42.0, filtered_len, |ui, range| {
                for index in range {
                    let pr_number = self.filtered_pr_numbers[index];
                    let Some(pr) = self.prs.get(&pr_number) else {
                        continue;
                    };
                    ui.push_id(pr.number, |ui| {
                        let (rect, click) = ui.allocate_exact_size(
                            Vec2::new(list_width.min(ui.available_width()), 42.0),
                            egui::Sense::click(),
                        );
                        let selected = self.selected_pr == Some(pr.number);
                        let fill = if selected {
                            Color32::from_rgb(23, 55, 101)
                        } else if click.hovered() {
                            Color32::from_rgb(28, 43, 65)
                        } else if index % 2 == 0 {
                            Color32::from_rgba_unmultiplied(28, 42, 64, 130)
                        } else {
                            Color32::TRANSPARENT
                        };
                        ui.painter().rect_filled(rect, 6, fill);
                        let num_str = format!("#{}", pr.number);
                        let diff_str = pr.diff_summary();
                        let mut x = rect.left();
                        for (width, text, color, size) in [
                            (number_width, num_str.as_str(), COLOR_PS5_CYAN, 14.0),
                            (title_width, pr.title.as_str(), COLOR_TEXT_WHITE, 14.0),
                            (author_width, pr.author_name(), COLOR_TEXT_MUTED, 13.0),
                            (changes_width, diff_str.as_str(), COLOR_TEXT_MUTED, 12.0),
                        ] {
                            let cell = egui::Rect::from_min_size(
                                egui::pos2(x, rect.top()),
                                Vec2::new(width, rect.height()),
                            );
                            let painter =
                                ui.painter().with_clip_rect(cell.intersect(ui.clip_rect()));
                            painter.text(
                                cell.left_center() + Vec2::new(0.0, 0.0),
                                egui::Align2::LEFT_CENTER,
                                text,
                                egui::FontId::proportional(size),
                                color,
                            );
                            x += width + ui.spacing().item_spacing.x;
                        }
                        if selected {
                            ui.painter().rect_filled(
                                egui::Rect::from_min_size(
                                    rect.left_top(),
                                    Vec2::new(3.0, rect.height()),
                                ),
                                2,
                                COLOR_PS5_CYAN,
                            );
                        }
                        focus_ring(ui, &click, 6);
                        let click = click.on_hover_text(&pr.title);
                        if click.clicked() {
                            self.selected_pr = Some(pr.number);
                        }
                    });
                }
            });
    }
}

fn load_app_icon() -> Option<egui::IconData> {
    let bytes = include_bytes!("../assets/anyps5_icon.png");
    let img = image::load_from_memory(bytes)
        .ok()?
        .thumbnail(64, 64)
        .into_rgba8();
    let (width, height) = img.dimensions();
    Some(egui::IconData {
        rgba: img.into_raw(),
        width,
        height,
    })
}

fn main() -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size(Vec2::new(1440.0, 920.0))
        .with_min_inner_size(Vec2::new(1000.0, 640.0))
        .with_title("AnyPS5 | Maintainer Workspace")
        .with_decorations(false);

    if let Some(icon) = load_app_icon() {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "AnyPS5",
        options,
        Box::new(|cc| Ok(Box::new(AnyPs5PureRustApp::new(cc)))),
    )
}

#[cfg(test)]
mod layout_tests {
    use super::*;
    pub(super) fn fixture(context: &egui::Context) -> AnyPs5PureRustApp {
        let pr: PullRequest = serde_json::from_value(serde_json::json!({
            "number": 1234, "title": "A long pull request title with enough words to require wrapping in a narrow inspector panel ".repeat(3),
            "author": {"login": "maintainer-with-a-long-name"}, "additions": 123, "deletions": 4,
            "files": [{"path": "core/graphics/very_long_component_name/another_long_directory/guest_memory_synchronization.cpp"}]
        })).unwrap();
        let batch: Batch = serde_json::from_value(serde_json::json!({
            "id": "batch-a-very-long-identifier", "title": "Graphics and memory synchronization review",
            "category": "AGC & Graphics", "complexity": "medium", "pr_numbers": [1234]
        })).unwrap();
        let (tx, rx) = channel();
        let mut app = AnyPs5PureRustApp {
            batches: vec![batch.clone()],
            prs: [(1234, pr)].into_iter().collect(),
            selected_batch: Some(batch),
            active_tab: AppTab::Minibatches,
            search_query: String::new(),
            batch_query: String::new(),
            category_filter: String::new(),
            complexity_filter: String::new(),
            newest_first: true,
            batch_sort: 3,
            batch_scope: 0,
            inspector_prompt: false,
            detail_tab: 0,
            descriptions: HashMap::new(),
            descriptions_busy: std::collections::HashSet::new(),
            checks: HashMap::new(),
            checks_busy: std::collections::HashSet::new(),
            checks_errors: HashMap::new(),
            checks_requested: HashMap::new(),
            review_cancel: None,
            review_session: None,
            review_busy: false,
            review_message: String::new(),
            drafts: HashMap::new(),
            selected_pr: None,
            raw_prompt: false,
            repo_category: String::new(),
            repo_status: String::new(),
            status_message: "Offline snapshot".into(),
            system_info: SystemInfo {
                os: "windows".into(),
                arch: "x86_64".into(),
                cpu_cores: 12,
                has_git: true,
                has_cmake: true,
                has_ninja: false,
                has_clang: true,
            },
            runner_configured: false,
            context: context.clone(),
            is_loading: false,
            github_login: None,
            github_avatar: None,
            github_busy: false,
            github_notice: String::new(),
            runner_dialog: false,
            runner_token: String::new(),
            app_logo: None,
            use_windows_sandbox: false,
            copied_notice: None,
            batch_meta: Vec::new(),
            batch_categories: Vec::new(),
            repo_categories: Vec::new(),
            repo_statuses: Vec::new(),
            open_prs_count: 0,
            open_prs_label: "0".into(),
            batches_count_label: "0".into(),
            cache_prs_len: 0,
            batch_filter_key: None,
            filtered_batch_indices: Vec::new(),
            pr_filter_key: None,
            filtered_pr_numbers: Vec::new(),
            prompt_cache: None,
            instance_id: "test-instance".into(),
            auto_sync_enabled: false,
            data_version: 1,
            active_peers: 1,
            slot_index: 0,
            last_heartbeat: std::time::Instant::now(),
            heartbeat_interval: std::time::Duration::from_secs(60),
            last_max_updated_at: None,
            sync_busy: false,
            sync_status: String::new(),
            tx,
            rx,
        };
        app.rebuild_data_cache();
        app
    }
    #[test]
    fn tab_content_stays_inside_available_width() {
        for (width, height) in [(1000.0, 640.0), (1440.0, 920.0)] {
            for scale in [1.0, 1.25, 1.5] {
                for tab in [
                    AppTab::Minibatches,
                    AppTab::BatchInspector,
                    AppTab::CpuWorker,
                    AppTab::EntireRepo,
                ] {
                    for show_prompt in [false, true] {
                        let context = egui::Context::default();
                        theme::apply_ps5_theme(&context);
                        let mut app = fixture(&context);
                        app.inspector_prompt = show_prompt;
                        let size = Vec2::new(width / scale - 270.0, height / scale - 48.0);
                        let mut measured = 0.0_f32;
                        let mut output = context.run_ui(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    size,
                                )),
                                ..Default::default()
                            },
                            |ui| {
                                match tab {
                                    AppTab::Minibatches => app.render_minibatches_view(ui),
                                    AppTab::BatchInspector => app.render_batch_inspector_view(ui),
                                    AppTab::CpuWorker => app.render_cpu_worker_view(ui),
                                    AppTab::EntireRepo => app.render_entire_repo_view(ui),
                                }
                                measured = ui.min_rect().width();
                            },
                        );
                        output.textures_delta.clear();
                        assert!(
                            measured <= size.x + 1.0,
                            "tab {} at {width}x{height} scale {scale}: width {measured} > {}",
                            tab as u8,
                            size.x
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn sidebar_footer_preserves_navigation_scroll_height() {
        let context = egui::Context::default();
        theme::apply_ps5_theme(&context);
        let mut app = fixture(&context);
        app.active_peers = 3;
        app.sync_status = "Synced 16 PRs, 27 batches in 4.2s".into();

        let size = Vec2::new(260.0, 900.0);
        let mut remaining_scroll_height = 0.0_f32;
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            },
            |ui| {
                ui.add_space(40.0);
                ui.separator();
                ui.add_space(12.0);
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    ui.add_space(12.0);
                    app.render_sidebar_connected_card(ui);
                    ui.add_space(10.0);
                    app.render_sidebar_runner_card(ui);
                    ui.add_space(8.0);
                    app.render_sidebar_profile(ui);
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(8.0);

                    remaining_scroll_height = ui.available_height();
                });
            },
        );
        output.textures_delta.clear();
        assert!(
            remaining_scroll_height >= 300.0,
            "sidebar navigation area was crushed: remaining_scroll_height = {remaining_scroll_height}"
        );
        assert_eq!(app.batch_meta[0].risk_level, "Medium");
    }
}

#[cfg(test)]
mod account_message_tests {
    use super::*;
    #[test]
    fn account_check_does_not_finish_data_refresh() {
        let context = egui::Context::default();
        let mut app = super::layout_tests::fixture(&context);
        app.is_loading = true;
        app.github_busy = true;
        app.tx
            .send(AsyncMessage::AccountLoaded(Err("Offline".into())))
            .unwrap();
        app.check_async_messages();
        assert!(app.is_loading);
        assert!(!app.github_busy);
        assert_eq!(app.github_notice, "Offline");
    }
}

#[cfg(test)]
mod custom_title_bar_tests {
    use super::*;

    #[test]
    fn custom_window_controls_render_without_panicking() {
        let context = egui::Context::default();
        theme::apply_ps5_theme(&context);
        let app = super::layout_tests::fixture(&context);

        let size = Vec2::new(1440.0, 920.0);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |_ui| {
                app.render_window_controls(&context, false, screen);
            },
        );
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
    }

    #[test]
    fn window_resizing_handles_render_without_panicking() {
        let context = egui::Context::default();
        let size = Vec2::new(1440.0, 920.0);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ui| {
                handle_window_resize(ui, screen);
            },
        );
        output.textures_delta.clear();
    }
}

#[cfg(test)]
mod minibatch_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn summary_deduplicates_files_and_marks_missing_stats() {
        let batch: Batch = serde_json::from_value(json!({"pr_numbers":[1,2]})).unwrap();
        let prs = [1, 2]
            .map(|n| {
                (
                    n,
                    serde_json::from_value(
                        json!({"number":n,"files":["a.cpp"],"additions":4,"deletions":2}),
                    )
                    .unwrap(),
                )
            })
            .into_iter()
            .collect();
        assert!(batch_summary(&batch, &prs).contains("1 files · +8 / −4"));
        assert!(batch_blocker(&batch, &prs).is_some());
        let mut missing = prs;
        missing.remove(&2);
        assert!(batch_summary(&batch, &missing).contains("Files unknown · Changes unknown"));
        assert_eq!(batch_churn(&batch, &missing), None);
    }
    #[test]
    fn claimed_batch_is_not_available_and_has_actual_owner() {
        let batch: Batch = serde_json::from_value(json!({"status":"claimed","claimed_by":"ivica00","claimed_user_id":"user-id","claimed_at":"2026-10-17T12:00:00Z"})).unwrap();
        assert_eq!(batch.owner(), Some("ivica00"));
        assert_eq!(batch.workflow_label(), "In review");
        assert!(!batch.available());
        let unknown: Batch = serde_json::from_value(json!({})).unwrap();
        assert!(!unknown.available());
        assert_eq!(unknown.workflow_label(), "Unknown status");
    }
    #[test]
    fn drafts_are_blocked_without_inventing_ci_results() {
        let batch: Batch = serde_json::from_value(json!({"pr_numbers":[1]})).unwrap();
        let pr: PullRequest =
            serde_json::from_value(json!({"number":1,"files":["a.cpp"],"is_draft":true})).unwrap();
        assert!(
            batch_blocker(&batch, &[(1, pr)].into_iter().collect())
                .unwrap()
                .contains("draft")
        );
    }
}

#[cfg(test)]
mod batch_name_tests {
    use super::*;
    #[test]
    fn names_sort_numerically() {
        let mut names = ["Batch 10", "Batch 2", "Batch 1"];
        names.sort_by_key(|name| batch_number(name));
        assert_eq!(names, ["Batch 1", "Batch 2", "Batch 10"]);
    }
}

#[cfg(test)]
mod description_tests {
    use super::*;
    #[test]
    fn description_is_cached_per_head_without_completing_data_refresh() {
        let context = egui::Context::default();
        let mut app = super::layout_tests::fixture(&context);
        app.is_loading = true;
        app.tx
            .send(AsyncMessage::DescriptionLoaded(
                1234,
                Some("old".into()),
                Ok("Older description".into()),
            ))
            .unwrap();
        app.check_async_messages();
        assert!(app.is_loading);
        assert!(app.descriptions.contains_key(&(1234, Some("old".into()))));
        assert!(!app.descriptions.contains_key(&(1234, Some("new".into()))));
    }
}
