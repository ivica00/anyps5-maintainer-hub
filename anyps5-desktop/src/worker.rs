use crate::models::SystemInfo;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

pub fn detect_system_info() -> SystemInfo {
    let cpu_cores = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    let has_git = Command::new("git").arg("--version").output().is_ok();
    let has_cmake = Command::new("cmake").arg("--version").output().is_ok();
    let has_ninja = Command::new("ninja").arg("--version").output().is_ok();
    let has_clang = Command::new("clang++").arg("--version").output().is_ok();

    SystemInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        cpu_cores,
        has_git,
        has_cmake,
        has_ninja,
        has_clang,
    }
}

#[allow(dead_code)]
pub struct CpuWorker {
    running: Arc<AtomicBool>,
}

#[allow(dead_code)]
impl CpuWorker {
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn toggle(&self) -> bool {
        let current = self.is_running();
        if current {
            self.stop();
            false
        } else {
            self.start();
            true
        }
    }

    pub fn start(&self) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }

        let is_running = self.running.clone();
        thread::spawn(move || {
            let cores = thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4);
            println!(
                "[Worker] Started native Rust CPU worker using {} logical cores.",
                cores
            );

            while is_running.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(1500));
            }
            println!("[Worker] Native Rust CPU worker stopped.");
        });
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    pub fn find_runner_dir() -> PathBuf {
        let candidates = [
            PathBuf::from("runner"),
            PathBuf::from("../runner"),
            PathBuf::from("../../runner"),
        ];

        for c in &candidates {
            if c.exists() && c.is_dir() {
                return c.clone();
            }
        }
        PathBuf::from("runner")
    }

    pub fn has_runner_credentials() -> bool {
        let runner_dir = Self::find_runner_dir();
        let cred_candidates = [
            runner_dir.join("actions-runner").join(".credentials"),
            runner_dir.join(".credentials"),
            PathBuf::from("actions-runner").join(".credentials"),
        ];

        cred_candidates.iter().any(|p| p.exists())
    }

    pub fn reset_runner_credentials() -> Result<(), String> {
        let runner_dir = Self::find_runner_dir();
        let cred_path = runner_dir.join("actions-runner").join(".credentials");
        if cred_path.exists() {
            fs::remove_file(&cred_path)
                .map_err(|e| format!("Failed to remove credentials: {}", e))?;
        }
        Ok(())
    }

    pub fn launch_runner(token: Option<&str>, use_sandbox: bool) -> Result<(), String> {
        let runner_dir = Self::find_runner_dir();
        let script_path = runner_dir.join("setup_windows_runner.ps1");

        if !script_path.exists() {
            return Err(format!(
                "Could not find setup_windows_runner.ps1 in {}",
                runner_dir.display()
            ));
        }

        let abs_script = script_path.canonicalize().unwrap_or(script_path);

        if use_sandbox {
            let wsb_path = runner_dir.join("anyps5_sandbox.wsb");
            if wsb_path.exists() {
                let abs_wsb = wsb_path.canonicalize().unwrap_or(wsb_path);
                println!(
                    "[Sandbox] Launching Windows Sandbox with: {}",
                    abs_wsb.display()
                );

                let wsb_res = Command::new("WindowsSandbox.exe")
                    .arg(abs_wsb.to_str().unwrap_or_default())
                    .spawn();

                if wsb_res.is_ok() {
                    return Ok(());
                }
                println!(
                    "[Sandbox] WindowsSandbox.exe not available or failed. Falling back to PowerShell execution."
                );
            }
        }

        let mut cmd = Command::new("powershell.exe");
        cmd.args([
            "-NoExit",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            abs_script.to_str().unwrap_or_default(),
        ]);

        if let Some(tok) = token {
            let trimmed = tok.trim();
            if !trimmed.is_empty() {
                cmd.args(["-RunnerToken", trimmed]);
            }
        }

        cmd.spawn()
            .map(|_| ())
            .map_err(|e| format!("Failed to launch PowerShell runner: {}", e))
    }

    pub fn launch_runner_script() -> Result<(), String> {
        Self::launch_runner(None, false)
    }
}
