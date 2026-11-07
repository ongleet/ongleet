//! One private, app-owned watchdog process. It never owns a PTY descriptor.
use super::{executable, Containment};
use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
        unix::{net::UnixStream, process::CommandExt},
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::Duration,
};

pub(super) const FLAG: &str = "--internal-terminal-watchdog";
const TIMEOUT: Duration = Duration::from_secs(3);
const MAX_PATH: usize = 4096;
static SHARED: OnceLock<Result<Watchdog, String>> = OnceLock::new();
type Groups = Arc<Mutex<HashMap<PathBuf, File>>>;

struct ChildGuard(std::process::Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub(super) struct Watchdog {
    connection: Mutex<UnixStream>,
    groups: Groups,
    failed: Arc<AtomicBool>,
    #[cfg(test)]
    pub(super) pid: u32,
}

pub(super) fn shared() -> Result<&'static Watchdog, String> {
    SHARED
        .get_or_init(Watchdog::start)
        .as_ref()
        .map_err(Clone::clone)
}

pub(super) fn untrack(directory: &Path) {
    if let Some(Ok(watchdog)) = SHARED.get() {
        watchdog.groups.lock().unwrap().remove(directory);
    }
}

impl Watchdog {
    fn start() -> Result<Self, String> {
        let (mut connection, child_connection) = UnixStream::pair().map_err(|e| e.to_string())?;
        connection
            .set_read_timeout(Some(TIMEOUT))
            .map_err(|e| e.to_string())?;
        connection
            .set_write_timeout(Some(TIMEOUT))
            .map_err(|e| e.to_string())?;
        let mut command = Command::new(executable()?);
        command
            .arg(FLAG)
            .arg(std::process::id().to_string())
            .stdin(Stdio::from(OwnedFd::from(child_connection)))
            .stdout(Stdio::null())
            .stderr(Stdio::inherit());
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = ChildGuard(
            command
                .spawn()
                .map_err(|e| format!("Cannot start terminal watchdog: {e}"))?,
        );
        let mut ready = [0];
        if let Err(error) = connection.read_exact(&mut ready).and_then(|_| {
            if ready == [b'R'] {
                Ok(())
            } else {
                Err(std::io::Error::other(
                    "Invalid watchdog startup acknowledgement",
                ))
            }
        }) {
            return Err(format!("Terminal watchdog startup failed: {error}"));
        }
        let groups: Groups = Arc::default();
        let failed = Arc::new(AtomicBool::new(false));
        let watched_groups = groups.clone();
        let watched_failure = failed.clone();
        #[cfg(test)]
        let pid = child.0.id();
        std::thread::Builder::new().name("terminal-watchdog-reaper".into()).spawn(move || {
                let status = child.0.wait();
            // If the watchdog dies first, the live app remains responsible for
            // killing every group. Refuse all subsequent launches; no fallback.
            let groups = {
                let mut groups = watched_groups.lock().unwrap();
                watched_failure.store(true, Ordering::Release);
                std::mem::take(&mut *groups)
            };
            let groups: Vec<_> = groups.into_iter().map(|(directory, kill)| Containment { directory, kill }).collect();
            kill_all(&groups);
            drop(groups);
            eprintln!("Terminal watchdog exited ({status:?}); terminal groups stopped. Restart Ongleet before launching more terminals.");
        }).map_err(|e| e.to_string())?;
        Ok(Self {
            connection: Mutex::new(connection),
            groups,
            failed,
            #[cfg(test)]
            pid,
        })
    }

    pub(super) fn track(&self, group: &Containment) -> Result<(), String> {
        let mut groups = self.groups.lock().unwrap();
        self.check()?;
        groups.insert(
            group.directory.clone(),
            group.kill.try_clone().map_err(|e| e.to_string())?,
        );
        Ok(())
    }

    fn check(&self) -> Result<(), String> {
        if self.failed.load(Ordering::Acquire) {
            return Err(
                "Terminal watchdog is unavailable; restart Ongleet before launching more terminals"
                    .into(),
            );
        }
        Ok(())
    }

    pub(super) fn register(&self, group: &Containment, leader: &File) -> Result<(), String> {
        let mut connection = self.connection.lock().unwrap();
        self.check()?;
        let result = (|| {
            send_fds(&connection, &[leader.as_raw_fd(), group.kill.as_raw_fd()])?;
            let path = group.directory.as_os_str().as_encoded_bytes();
            if path.len() > MAX_PATH {
                return Err("Terminal cgroup path is too long".into());
            }
            connection
                .write_all(&(path.len() as u32).to_be_bytes())
                .map_err(|e| e.to_string())?;
            connection.write_all(path).map_err(|e| e.to_string())?;
            let mut ready = [0];
            connection
                .read_exact(&mut ready)
                .map_err(|e| e.to_string())?;
            if ready != [b'R'] {
                return Err("Invalid watchdog registration acknowledgement".into());
            }
            self.check()
        })();
        if result.is_err() {
            // Fail the whole channel: a partial registration cannot be reused.
            self.failed.store(true, Ordering::Release);
            let _ = connection.shutdown(std::net::Shutdown::Both);
        }
        result.map_err(|e: String| format!("Terminal watchdog registration failed: {e}"))
    }
}

pub(super) fn pidfd(pid: libc::pid_t) -> Result<File, String> {
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) } as RawFd;
    if fd < 0 {
        return Err(format!(
            "pidfd_open is required for terminal lifecycle ownership: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn pollfd(fd: RawFd) -> libc::pollfd {
    libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    }
}

fn kill_all(groups: &[Containment]) {
    // Signal all groups before waiting for any cleanup, including on app death.
    for group in groups {
        unsafe {
            libc::write(group.kill.as_raw_fd(), b"1".as_ptr().cast(), 1);
        }
    }
}

struct Terminal {
    leader: File,
    group: Containment,
}

pub(super) fn run(args: &[std::ffi::OsString]) -> Result<(), String> {
    let parent: libc::pid_t = args
        .get(2)
        .and_then(|a| a.to_str())
        .and_then(|a| a.parse().ok())
        .filter(|pid| *pid == unsafe { libc::getppid() })
        .ok_or("Invalid watchdog parent")?;
    let parent = pidfd(parent)?;
    // stdin is the private socket inherited from the application; every other
    // inherited application descriptor is CLOEXEC and no PTY is opened here.
    let mut connection = unsafe { UnixStream::from_raw_fd(0) };
    connection
        .set_read_timeout(Some(TIMEOUT))
        .map_err(|e| e.to_string())?;
    connection
        .set_write_timeout(Some(TIMEOUT))
        .map_err(|e| e.to_string())?;
    connection.write_all(b"R").map_err(|e| e.to_string())?;
    let mut terminals: Vec<Terminal> = Vec::new();
    let result = (|| {
        loop {
            let mut fds = vec![pollfd(parent.as_raw_fd()), pollfd(connection.as_raw_fd())];
            fds.extend(terminals.iter().map(|t| pollfd(t.leader.as_raw_fd())));
            if unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, -1) } < 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error.to_string());
            }
            if fds[0].revents != 0
                || fds[1].revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0
            {
                return Ok(());
            }
            // Remove exited leaders before accepting the next registration.
            for index in (0..terminals.len()).rev() {
                if fds[index + 2].revents != 0 {
                    terminals.swap_remove(index);
                }
            }
            if fds[1].revents & libc::POLLIN != 0 {
                let mut files = receive_fds(&connection, 2)?;
                let mut size = [0; 4];
                connection
                    .read_exact(&mut size)
                    .map_err(|e| e.to_string())?;
                let size = u32::from_be_bytes(size) as usize;
                if size == 0 || size > MAX_PATH {
                    return Err("Invalid watchdog registration size".into());
                }
                let mut path = vec![0; size];
                connection
                    .read_exact(&mut path)
                    .map_err(|e| e.to_string())?;
                let directory = PathBuf::from(String::from_utf8(path).map_err(|e| e.to_string())?);
                let leader = files.remove(0);
                let kill = files.remove(0);
                terminals.push(Terminal {
                    leader,
                    group: Containment { directory, kill },
                });
                connection.write_all(b"R").map_err(|e| e.to_string())?;
            }
        }
    })();
    // Also runs on malformed/truncated control traffic. RAII handles all
    // registered groups; no descriptor/registration survives parent death.
    for terminal in &terminals {
        unsafe {
            libc::write(terminal.group.kill.as_raw_fd(), b"1".as_ptr().cast(), 1);
        }
    }
    drop(terminals);
    result
}

// One byte carries at most two SCM_RIGHTS descriptors. The following bounded
// path frame is serialized on the app side. Aligned storage is required for CMSG.
pub(super) fn send_fds(stream: &UnixStream, files: &[RawFd]) -> Result<(), String> {
    if files.is_empty() || files.len() > 2 {
        return Err("Invalid descriptor count".into());
    }
    let mut byte = [b'F'];
    let mut iov = libc::iovec {
        iov_base: byte.as_mut_ptr().cast(),
        iov_len: 1,
    };
    let mut control = [0usize; 8];
    let mut message: libc::msghdr = unsafe { std::mem::zeroed() };
    message.msg_iov = &mut iov;
    message.msg_iovlen = 1;
    message.msg_control = control.as_mut_ptr().cast();
    message.msg_controllen =
        unsafe { libc::CMSG_SPACE(std::mem::size_of_val(files) as u32) } as usize;
    unsafe {
        let header = libc::CMSG_FIRSTHDR(&message);
        (*header).cmsg_level = libc::SOL_SOCKET;
        (*header).cmsg_type = libc::SCM_RIGHTS;
        (*header).cmsg_len = libc::CMSG_LEN(std::mem::size_of_val(files) as u32) as usize;
        std::ptr::copy_nonoverlapping(
            files.as_ptr().cast::<u8>(),
            libc::CMSG_DATA(header),
            std::mem::size_of_val(files),
        );
    }
    loop {
        let sent = unsafe { libc::sendmsg(stream.as_raw_fd(), &message, libc::MSG_NOSIGNAL) };
        if sent == 1 {
            return Ok(());
        }
        let error = std::io::Error::last_os_error();
        if sent < 0 && error.kind() == std::io::ErrorKind::Interrupted {
            continue;
        }
        return Err(format!("Cannot send watchdog descriptors: {error}"));
    }
}

pub(super) fn receive_fds(stream: &UnixStream, expected: usize) -> Result<Vec<File>, String> {
    let mut byte = [0];
    let mut iov = libc::iovec {
        iov_base: byte.as_mut_ptr().cast(),
        iov_len: 1,
    };
    let mut control = [0usize; 8];
    let mut message: libc::msghdr = unsafe { std::mem::zeroed() };
    message.msg_iov = &mut iov;
    message.msg_iovlen = 1;
    message.msg_control = control.as_mut_ptr().cast();
    message.msg_controllen =
        unsafe { libc::CMSG_SPACE((2 * std::mem::size_of::<RawFd>()) as u32) } as usize;
    let received = loop {
        let count =
            unsafe { libc::recvmsg(stream.as_raw_fd(), &mut message, libc::MSG_CMSG_CLOEXEC) };
        if count < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
            continue;
        }
        break count;
    };
    if received < 0 {
        return Err(format!(
            "Cannot receive watchdog descriptors: {}",
            std::io::Error::last_os_error()
        ));
    }
    let mut files = Vec::new();
    unsafe {
        let mut header = libc::CMSG_FIRSTHDR(&message);
        while !header.is_null() {
            if (*header).cmsg_level == libc::SOL_SOCKET && (*header).cmsg_type == libc::SCM_RIGHTS {
                let size = (*header)
                    .cmsg_len
                    .saturating_sub(libc::CMSG_LEN(0) as usize);
                for index in 0..size / std::mem::size_of::<RawFd>() {
                    let fd = std::ptr::read_unaligned(
                        libc::CMSG_DATA(header).cast::<RawFd>().add(index),
                    );
                    files.push(File::from_raw_fd(fd));
                }
            }
            header = libc::CMSG_NXTHDR(&message, header);
        }
    }
    if received != 1
        || byte != [b'F']
        || message.msg_flags & libc::MSG_CTRUNC != 0
        || files.len() != expected
    {
        return Err("Invalid watchdog descriptor message".into());
    }
    Ok(files)
}
