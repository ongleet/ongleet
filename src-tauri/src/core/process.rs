//! Bounded cancellable native jobs, independent of plugin feature behavior.
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

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
    pub cwd: Option<String>,
    pub timeout_ms: Option<u64>,
    pub output_limit: Option<usize>,
}
#[derive(Debug, serde::Serialize)]
pub struct ProcessResult {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
}
pub fn execute(request: Request, cancel: Arc<AtomicBool>) -> Result<ProcessResult, String> {
    if serde_json::to_vec(&request)
        .map_err(|e| e.to_string())?
        .len()
        > 65_536
    {
        return Err("Process request exceeds 64 KiB".into());
    }
    let timeout = request.timeout_ms.unwrap_or(20_000);
    let limit = request.output_limit.unwrap_or(512 * 1024);
    if timeout == 0 || timeout > 20_000 || limit == 0 || limit > 512 * 1024 {
        return Err("Invalid process limits".into());
    }
    let config = super::pty::TabConfig {
        executable: Some(request.executable.clone()),
        args: request.args.clone(),
        env: request.env.clone(),
        cwd_override: request.cwd.clone(),
        ..Default::default()
    };
    config.validate()?;
    let mut command = Command::new(request.executable);
    command
        .args(request.args)
        .envs(request.env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    if let Some(cwd) = request.cwd {
        command.current_dir(cwd);
    }
    execute_command(command, cancel, Duration::from_millis(timeout), limit)
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
fn drain(pipe: &mut impl Read, bytes: &mut Vec<u8>, limit: usize) -> Result<(), String> {
    let mut buffer = [0; 8192];
    // Bound work per iteration so cancellation/timeout also works during floods.
    for _ in 0..16 {
        match pipe.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                if bytes.len() + n > limit {
                    return Err("Process output exceeds limit".into());
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
fn execute_command(
    mut command: Command,
    cancel: Arc<AtomicBool>,
    timeout: Duration,
    limit: usize,
) -> Result<ProcessResult, String> {
    if cancel.load(Ordering::Acquire) {
        return Err("Process cancelled".into());
    }
    let mut child = ChildGuard(
        command
            .spawn()
            .map_err(|e| format!("Cannot launch process: {e}"))?,
    );
    let mut stdout = child.0.stdout.take().ok_or("Process stdout unavailable")?;
    let mut stderr = child.0.stderr.take().ok_or("Process stderr unavailable")?;
    nonblocking(&stdout)?;
    nonblocking(&stderr)?;
    let mut out = Vec::new();
    let mut err = Vec::new();
    let deadline = Instant::now() + timeout;
    loop {
        if cancel.load(Ordering::Acquire) {
            return Err("Process cancelled".into());
        }
        if Instant::now() >= deadline {
            return Err("Process timed out".into());
        }
        drain(&mut stdout, &mut out, limit)?;
        drain(&mut stderr, &mut err, limit)?;
        if let Some(status) = child.0.try_wait().map_err(|e| e.to_string())? {
            drain(&mut stdout, &mut out, limit)?;
            drain(&mut stderr, &mut err, limit)?;
            return Ok(ProcessResult {
                stdout: String::from_utf8_lossy(&out).into_owned(),
                stderr: String::from_utf8_lossy(&err).into_owned(),
                code: status.code().unwrap_or(-1),
            });
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(script: &str) -> Request {
        Request {
            executable: "/usr/bin/python3".into(),
            args: vec!["-c".into(), script.into()],
            env: Default::default(),
            cwd: None,
            timeout_ms: Some(2_000),
            output_limit: Some(512 * 1024),
        }
    }
    #[test]
    fn literal_arguments_environment_streams_status_and_limits() {
        let mut options = request("import os,sys; print(sys.argv[1]); print(os.environ['SDK_TEST']); print('diagnostic',file=sys.stderr); sys.exit(4)");
        options.args.push("$(touch /never-execute); 日本語".into());
        options
            .env
            .insert("SDK_TEST".into(), "explicit override".into());
        let result = execute(options, Arc::new(AtomicBool::new(false))).unwrap();
        assert_eq!(result.code, 4);
        assert_eq!(
            result.stdout,
            "$(touch /never-execute); 日本語\nexplicit override\n"
        );
        assert_eq!(result.stderr, "diagnostic\n");
        for stream in ["sys.stdout", "sys.stderr"] {
            let options = request(&format!("import sys; {stream}.write('x'*600000)"));
            assert!(execute(options, Arc::new(AtomicBool::new(false)))
                .unwrap_err()
                .contains("exceeds"));
        }
        let mut options = request("import time; time.sleep(60)");
        options.timeout_ms = Some(80);
        let began = Instant::now();
        assert!(execute(options, Arc::new(AtomicBool::new(false)))
            .unwrap_err()
            .contains("timed out"));
        assert!(began.elapsed() < Duration::from_secs(2));
    }
    #[test]
    fn cancellation_joins_child_and_flood_cannot_starve_deadline() {
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = cancel.clone();
        let worker =
            std::thread::spawn(move || execute(request("import time; time.sleep(60)"), cancel));
        std::thread::sleep(Duration::from_millis(80));
        signal.store(true, Ordering::Release);
        assert!(worker.join().unwrap().unwrap_err().contains("cancelled"));
        let mut options =
            request("import sys;\nwhile True: sys.stdout.write('x'*8192); sys.stdout.flush()");
        options.timeout_ms = Some(80);
        options.output_limit = Some(1024);
        assert!(execute(options, Arc::new(AtomicBool::new(false)))
            .unwrap_err()
            .contains("exceeds"));
    }
}
