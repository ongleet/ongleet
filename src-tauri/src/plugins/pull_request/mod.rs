//! GitHub PR status through the user's installed/authenticated gh CLI.
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    os::unix::process::CommandExt,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Association {
    pub repository: String,
    pub number: u64,
}
impl Association {
    pub fn parse(target: &str, number: Option<u64>) -> Result<Self, String> {
        let (repository, number) = if let Some(path) = target.strip_prefix("https://github.com/") {
            let parts: Vec<_> = path.trim_end_matches('/').split('/').collect();
            if parts.len() != 4 || parts[2] != "pull" {
                return Err("Expected a GitHub pull request URL".into());
            }
            (
                format!("{}/{}", parts[0], parts[1]),
                parts[3].parse::<u64>().map_err(|_| "Invalid PR number")?,
            )
        } else {
            (target.to_owned(), number.ok_or("PR number is required")?)
        };
        let association = Self { repository, number };
        association.validate()?;
        Ok(association)
    }
    pub fn validate(&self) -> Result<(), String> {
        let parts: Vec<_> = self.repository.split('/').collect();
        if self.repository.len() > 256
            || parts.len() != 2
            || parts.iter().any(|s| {
                s.is_empty()
                    || s.starts_with(['-', '.'])
                    || !s
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            })
            || self.number == 0
        {
            return Err("Expected owner/repository and a positive PR number".into());
        }
        Ok(())
    }
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {}
pub fn validate(config: &serde_json::Value) -> Result<(), String> {
    serde_json::from_value::<Config>(config.clone())
        .map(|_| ())
        .map_err(|e| e.to_string())
}

struct ChildGuard(std::process::Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        // gh may spawn helpers; terminate the private process group and reap gh.
        unsafe {
            libc::kill(-(self.0.id() as i32), libc::SIGKILL);
        }
        let _ = self.0.wait();
    }
}
fn nonblocking<T: std::os::fd::AsRawFd>(pipe: &T) -> Result<(), String> {
    let fd = pipe.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}
fn drain(pipe: &mut impl Read, bytes: &mut Vec<u8>) -> Result<(), String> {
    let mut buffer = [0; 8192];
    // Bound work per iteration so cancellation/timeout also works during floods.
    for _ in 0..16 {
        match pipe.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                if bytes.len() + n > 512 * 1024 {
                    return Err("gh output exceeds 512 KiB".into());
                }
                bytes.extend_from_slice(&buffer[..n]);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}
pub fn fetch(
    association: &Association,
    cancel: Arc<AtomicBool>,
) -> Result<serde_json::Value, String> {
    association.validate()?;
    let mut command = Command::new("gh");
    command
        .args([
            "pr",
            "view",
            &association.number.to_string(),
            "--repo",
            &association.repository,
            "--json",
            "number,title,url,state,reviewDecision,statusCheckRollup,updatedAt",
        ])
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_HOST", "github.com")
        .env("GH_PAGER", "cat")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    fetch_command(command, cancel, Duration::from_secs(20))
}
fn fetch_command(
    mut command: Command,
    cancel: Arc<AtomicBool>,
    timeout: Duration,
) -> Result<serde_json::Value, String> {
    if cancel.load(Ordering::Acquire) {
        return Err("PR refresh cancelled".into());
    }
    let mut child = ChildGuard(command.spawn().map_err(|e| {
        format!("Cannot launch gh; install and authenticate it with gh auth login: {e}")
    })?);
    let mut stdout = child.0.stdout.take().ok_or("gh stdout unavailable")?;
    let mut stderr = child.0.stderr.take().ok_or("gh stderr unavailable")?;
    nonblocking(&stdout)?;
    nonblocking(&stderr)?;
    let mut out = Vec::new();
    let mut err = Vec::new();
    let deadline = Instant::now() + timeout;
    loop {
        if cancel.load(Ordering::Acquire) {
            return Err("PR refresh cancelled".into());
        }
        if Instant::now() >= deadline {
            return Err("gh request timed out".into());
        }
        drain(&mut stdout, &mut out)?;
        drain(&mut stderr, &mut err)?;
        if let Some(status) = child.0.try_wait().map_err(|e| e.to_string())? {
            drain(&mut stdout, &mut out)?;
            drain(&mut stderr, &mut err)?;
            if !status.success() {
                return Err(format!(
                    "gh failed ({status}): {}",
                    String::from_utf8_lossy(&err[..err.len().min(8192)])
                ));
            }
            return serde_json::from_slice(&out).map_err(|e| format!("Invalid gh response: {e}"));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn script(source: &str) -> Command {
        let mut command = Command::new("/usr/bin/python3");
        command
            .args(["-c", source])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        command
    }
    #[test]
    fn gh_boundary_reports_auth_errors_and_bounds_output_and_waiting() {
        let cancel = Arc::new(AtomicBool::new(false));
        let output = fetch_command(
            script(r#"print('{"state":"MERGED","statusCheckRollup":[]}')"#),
            cancel.clone(),
            Duration::from_secs(2),
        )
        .unwrap();
        assert_eq!(output["state"], "MERGED");
        let error = fetch_command(
            script("import sys; print('gh auth login required', file=sys.stderr); sys.exit(4)"),
            cancel.clone(),
            Duration::from_secs(2),
        )
        .unwrap_err();
        assert!(error.contains("gh auth login required"));
        let error = fetch_command(
            script("print('x' * 600000)"),
            cancel.clone(),
            Duration::from_secs(2),
        )
        .unwrap_err();
        assert!(error.contains("exceeds 512 KiB"));
        let began = Instant::now();
        let error = fetch_command(
            script("import time; time.sleep(60)"),
            cancel,
            Duration::from_millis(80),
        )
        .unwrap_err();
        assert!(error.contains("timed out"));
        assert!(began.elapsed() < Duration::from_secs(2));
    }
    #[test]
    fn cancellation_terminates_pending_work_and_associations_are_explicit() {
        assert!(Association::parse("https://github.com/owner/repo/pull/17", None).is_ok());
        assert!(Association::parse("--repo", Some(1)).is_err());
        assert!(Association::parse("owner/repo", Some(0)).is_err());
        assert!(Association::parse("https://elsewhere/owner/repo/pull/17", None).is_err());
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = cancel.clone();
        let worker = std::thread::spawn(move || {
            fetch_command(
                script("import time; time.sleep(60)"),
                cancel,
                Duration::from_secs(5),
            )
        });
        std::thread::sleep(Duration::from_millis(80));
        signal.store(true, Ordering::Release);
        assert!(worker.join().unwrap().unwrap_err().contains("cancelled"));
    }
}
