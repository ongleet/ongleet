//! Linux cgroup-v2 containment. This is lifecycle ownership, not a security sandbox.
use std::{
    ffi::CString,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
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
        let uid = unsafe { libc::geteuid() };
        let root = PathBuf::from(format!(
            "/sys/fs/cgroup/user.slice/user-{uid}.slice/user@{uid}.service"
        ));
        let result = (|| {
            if fs::metadata(&root).map_err(|e| e.to_string())?.uid() != uid {
                return Err("User cgroup delegation is not owned by the current user".into());
            }
            let directory = root.join(format!(
                "terminal-workspace-{}-{}",
                std::process::id(),
                nonce()?
            ));
            fs::create_dir(&directory).map_err(|e| e.to_string())?;
            match OpenOptions::new()
                .write(true)
                .open(directory.join("cgroup.kill"))
            {
                Ok(kill) => Ok(Self { directory, kill }),
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
            return Err(format!("Cannot terminate terminal cgroup: {error}"));
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match fs::read_to_string(self.directory.join("cgroup.events")) {
                Ok(events) if events.lines().any(|line| line == "populated 0") => {
                    let _ = fs::remove_dir(&self.directory);
                    return Ok(());
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
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
    pub fn wait(&self) -> Result<(), String> {
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
        let parent = unsafe { libc::syscall(libc::SYS_pidfd_open, credentials.pid, 0) } as i32;
        let leader = unsafe { libc::syscall(libc::SYS_pidfd_open, libc::getpid(), 0) } as i32;
        if parent < 0 || leader < 0 {
            return Err("pidfd_open is required for terminal lifecycle ownership".into());
        }
        let parent = unsafe { File::from_raw_fd(parent) };
        let leader = unsafe { File::from_raw_fd(leader) };
        let kill = OpenOptions::new()
            .write(true)
            .open(directory.join("cgroup.kill"))
            .map_err(|e| e.to_string())?;
        let path =
            CString::new(directory.as_os_str().as_encoded_bytes()).map_err(|e| e.to_string())?;
        let child = unsafe { libc::fork() };
        if child < 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        if child == 0 {
            // This short-lived watchdog is outside the terminal cgroup. It can
            // kill the group after either exec'd leader exit or app death, then
            // remove the empty group. No shell/configuration is involved.
            unsafe {
                libc::setsid();
                libc::close(stream.as_raw_fd());
                for fd in 0..3 {
                    libc::close(fd);
                }
                let mut fds = [
                    libc::pollfd {
                        fd: parent.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    },
                    libc::pollfd {
                        fd: leader.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    },
                ];
                while libc::poll(fds.as_mut_ptr(), 2, -1) < 0 {
                    if *libc::__errno_location() != libc::EINTR {
                        libc::_exit(1);
                    }
                }
                libc::write(kill.as_raw_fd(), b"1".as_ptr().cast(), 1);
                for _ in 0..300 {
                    if libc::rmdir(path.as_ptr()) == 0 || *libc::__errno_location() == libc::ENOENT
                    {
                        libc::_exit(0);
                    }
                    libc::usleep(10_000);
                }
                libc::_exit(1);
            }
        }
        // Join before any user executable can run or fork. The watchdog does
        // not inherit this membership and cannot keep PTY descriptors open.
        fs::write(directory.join("cgroup.procs"), b"0").map_err(|e| e.to_string())?;
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
