//! Linux cgroup-v2 containment. This is lifecycle ownership, not a security sandbox.
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::{
            fs::{DirBuilderExt, MetadataExt},
            net::{UnixListener, UnixStream},
            process::CommandExt,
        },
    },
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

pub const HELPER_FLAG: &str = "--internal-terminal-helper";
#[path = "containment/watchdog.rs"]
mod watchdog;

fn nonce() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .map_err(|e| e.to_string())?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

pub struct Containment {
    directory: PathBuf,
    kill: File,
}

impl Containment {
    pub fn create() -> Result<Self, String> {
        let watchdog = watchdog::shared()?;
        let uid = unsafe { libc::geteuid() };
        let root = PathBuf::from(format!(
            "/sys/fs/cgroup/user.slice/user-{uid}.slice/user@{uid}.service"
        ));
        let result = (|| {
            if fs::metadata(&root).map_err(|e| e.to_string())?.uid() != uid {
                return Err("User cgroup delegation is not owned by the current user".into());
            }
            let directory = root.join(format!("ongleet-{}-{}", std::process::id(), nonce()?));
            fs::create_dir(&directory).map_err(|e| e.to_string())?;
            match OpenOptions::new()
                .write(true)
                .open(directory.join("cgroup.kill"))
            {
                Ok(kill) => {
                    let group = Self { directory, kill };
                    watchdog.track(&group)?;
                    Ok(group)
                }
                Err(error) => {
                    let _ = fs::remove_dir(&directory);
                    Err(error.to_string())
                }
            }
        })();
        result.map_err(|error: String| format!("Terminal containment unavailable: {error}. A writable systemd user cgroup v2 delegation with cgroup.kill is required; no command was launched."))
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn stop(&self) -> Result<(), String> {
        // A stable open cgroup handle targets this group, never a recycled PID.
        let result = unsafe { libc::write(self.kill.as_raw_fd(), b"1".as_ptr().cast(), 1) };
        if result != 1 {
            let error = std::io::Error::last_os_error();
            if !self.directory.exists() {
                return Ok(());
            }
            // The reaper/watchdog may concurrently remove an already-empty
            // group. ENODEV means it is offline; verify removal below.
            if error.raw_os_error() != Some(libc::ENODEV) {
                return Err(format!("Cannot terminate terminal cgroup: {error}"));
            }
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match fs::read_to_string(self.directory.join("cgroup.events")) {
                Ok(events) if events.lines().any(|line| line == "populated 0") => {
                    let _ = fs::remove_dir(&self.directory);
                    return Ok(());
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(error) if error.raw_os_error() == Some(libc::ENODEV) => {}
                Err(error) => return Err(error.to_string()),
                _ => {}
            }
            if Instant::now() >= deadline {
                return Err("Terminal cgroup did not become empty after SIGKILL".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Containment {
    fn drop(&mut self) {
        let _ = self.stop();
        let _ = fs::remove_dir(&self.directory);
        watchdog::untrack(&self.directory);
    }
}

pub struct Handshake {
    directory: PathBuf,
    listener: UnixListener,
}
impl Handshake {
    pub fn new() -> Result<Self, String> {
        let directory = std::env::temp_dir().join(format!("terminal-launch-{}", nonce()?));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .map_err(|e| e.to_string())?;
        let listener =
            UnixListener::bind(directory.join("ready.sock")).map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        Ok(Self {
            directory,
            listener,
        })
    }
    pub fn socket(&self) -> PathBuf {
        self.directory.join("ready.sock")
    }
    pub fn wait(&self, containment: &Containment) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut stream = loop {
            match self.listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(error) => return Err(format!("Terminal launch handshake failed: {error}")),
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .map_err(|e| e.to_string())?;
        stream
            .set_write_timeout(Some(Duration::from_secs(3)))
            .map_err(|e| e.to_string())?;
        // The bootstrap sends its own pidfd while blocked outside the cgroup.
        // Passing that descriptor avoids a PID-reuse race during registration.
        let mut descriptors = watchdog::receive_fds(&stream, 1)?;
        watchdog::shared()?.register(containment, &descriptors.remove(0))?;
        stream.write_all(b"GO").map_err(|e| e.to_string())?;
        let mut message = String::new();
        Read::by_ref(&mut stream)
            .take(4096)
            .read_to_string(&mut message)
            .map_err(|e| format!("Terminal launch handshake failed: {e}"))?;
        if message == "READY\n" {
            Ok(())
        } else {
            Err(format!("Terminal launch failed: {}", message.trim()))
        }
    }
}
impl Drop for Handshake {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.socket());
        let _ = fs::remove_dir(&self.directory);
    }
}

pub fn executable() -> Result<PathBuf, String> {
    // Cargo's test harness replaces main; the tiny test helper uses the exact
    // same bootstrap implementation. Installed applications reuse their own binary.
    if cfg!(test) {
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        Ok(executable
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("terminal-pty-helper"))
    } else {
        // Re-exec this live image, even after a rebuild/update unlinks it.
        // current_exe() returns a stale path ending in " (deleted)" then;
        // opening the replacement pathname could also mix helper versions.
        Ok(PathBuf::from(format!("/proc/{}/exe", std::process::id())))
    }
}

pub fn run_helper_if_requested() -> bool {
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_some_and(|value| value == watchdog::FLAG) {
        let result = watchdog::run(&args);
        if let Err(error) = &result {
            eprintln!("Terminal watchdog failed: {error}");
        }
        std::process::exit(if result.is_ok() { 0 } else { 1 });
    }
    if args.get(1).is_none_or(|value| value != HELPER_FLAG) {
        return false;
    }
    let result = helper(&args);
    if let Err(error) = result {
        eprintln!("Terminal launch failed: {error}");
    }
    std::process::exit(127);
}

fn helper(args: &[std::ffi::OsString]) -> Result<(), String> {
    if args.len() < 5 {
        return Err("Invalid internal terminal launch request".into());
    }
    let directory = PathBuf::from(&args[2]);
    let mut stream = UnixStream::connect(&args[3]).map_err(|e| e.to_string())?;
    let result = (|| {
        let mut credentials: libc::ucred = unsafe { std::mem::zeroed() };
        let mut size = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        if unsafe {
            libc::getsockopt(
                stream.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                (&mut credentials as *mut libc::ucred).cast(),
                &mut size,
            )
        } != 0
            || credentials.uid != unsafe { libc::geteuid() }
        {
            return Err("Invalid launch parent".into());
        }
        let leader = watchdog::pidfd(unsafe { libc::getpid() })?;
        watchdog::send_fds(&stream, &[leader.as_raw_fd()])?;
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .map_err(|e| e.to_string())?;
        let mut permission = [0; 2];
        stream
            .read_exact(&mut permission)
            .map_err(|e| e.to_string())?;
        if &permission != b"GO" {
            return Err("Terminal watchdog registration was not acknowledged".into());
        }
        // Join only after the app's shared watchdog owns the stable pidfd and
        // cgroup kill handle. The bootstrap becomes the user process via exec.
        fs::write(directory.join("cgroup.procs"), b"0")
            .map_err(|e| format!("Cannot join terminal cgroup: {e}"))?;
        stream.write_all(b"READY\n").map_err(|e| e.to_string())?;
        // UnixStream is CLOEXEC: EOF proves successful exec, or an explicit
        // error below proves failure. The parent never reports a fake success.
        Err(Command::new(&args[4]).args(&args[5..]).exec().to_string())
    })();
    if let Err(ref error) = result {
        let _ = writeln!(stream, "ERROR: {error}");
    }
    result
}

#[cfg(test)]
pub(crate) fn watchdog_pid() -> u32 {
    watchdog::shared().unwrap().pid
}
