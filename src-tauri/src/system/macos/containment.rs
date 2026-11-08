//! Best-effort session cleanup. Darwin has no equivalent of cgroup.kill.
use super::{events, paths};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::{
            net::{UnixListener, UnixStream},
            process::CommandExt,
        },
    },
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::Duration,
};
const WATCHDOG: &str = "--ongleet-darwin-watchdog";
pub const HELPER_FLAG: &str = "--ongleet-darwin-terminal";
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
struct Identity {
    pid: i32,
    seconds: u64,
    micros: u64,
}
fn identity(pid: i32) -> Option<Identity> {
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of_val(&info) as i32;
    if unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            size,
        )
    } != size
        || info.pbi_uid != unsafe { libc::geteuid() }
    {
        return None;
    }
    Some(Identity {
        pid,
        seconds: info.pbi_start_tvsec,
        micros: info.pbi_start_tvusec,
    })
}
fn cleanup(leader: Identity) {
    cleanup_until(leader, std::time::Instant::now() + Duration::from_secs(3));
}
fn cleanup_until(leader: Identity, deadline: std::time::Instant) {
    // Refuse a reused session leader. Once reaped, the session ID remains useful
    // for ordinary descendants, but enumeration and identity checks are best effort.
    if identity(leader.pid).is_some_and(|current| current != leader) {
        return;
    }
    let mut pids = vec![0i32; 65536];
    let bytes = unsafe {
        libc::proc_listpids(
            1, // PROC_ALL_PIDS from libproc.h
            0,
            pids.as_mut_ptr().cast(),
            (pids.len() * 4) as i32,
        )
    };
    if bytes <= 0 {
        return;
    }
    for pid in pids
        .into_iter()
        .take(bytes as usize / std::mem::size_of::<i32>())
    {
        if std::time::Instant::now() >= deadline {
            break;
        }
        if pid <= 1 || unsafe { libc::getsid(pid) } != leader.pid {
            continue;
        }
        let Some(member) = identity(pid) else {
            continue;
        };
        if identity(member.pid) == Some(member) && unsafe { libc::getsid(member.pid) } == leader.pid
        {
            unsafe { libc::kill(member.pid, libc::SIGKILL) };
        }
    }
}
pub struct Containment {
    leader: Identity,
    stopped: AtomicBool,
}
impl Containment {
    pub fn stop(&self) -> Result<(), String> {
        if !self.stopped.swap(true, Ordering::AcqRel) {
            cleanup(self.leader);
            if let Some(Ok(w)) = INSTANCE.get() {
                w.groups.lock().unwrap().remove(&self.leader.pid);
            }
        }
        Ok(())
    }
    #[cfg(test)]
    pub fn directory(&self) -> PathBuf {
        PathBuf::new()
    }
}
impl Drop for Containment {
    fn drop(&mut self) {
        let _ = self.stop();
        if let Some(Ok(w)) = INSTANCE.get() {
            w.groups.lock().unwrap().remove(&self.leader.pid);
        }
    }
}
fn peer_pid(stream: &UnixStream) -> Result<i32, String> {
    let mut pid = 0i32;
    let mut size = std::mem::size_of_val(&pid) as libc::socklen_t;
    if unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_LOCAL,
            libc::LOCAL_PEERPID,
            (&mut pid as *mut i32).cast(),
            &mut size,
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(pid)
}
struct Watchdog {
    #[cfg(test)]
    pid: u32,
    path: PathBuf,
    alive: Arc<AtomicBool>,
    groups: Arc<Mutex<BTreeMap<i32, Identity>>>,
    _connection: UnixStream,
}
static INSTANCE: OnceLock<Result<Watchdog, String>> = OnceLock::new();
fn watchdog(executable: PathBuf) -> Result<&'static Watchdog, String> {
    let result = INSTANCE.get_or_init(|| {
        use super::super::SystemFiles;
        let nonce = super::Macos::new().nonce()?;
        let path = paths::runtime()?.join(format!("w-{}.sock", &nonce[..16]));
        let register = paths::runtime()?.join(format!("r-{}.sock", &nonce[..16]));
        let listener = UnixListener::bind(&path).map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let mut child = Command::new(executable)
            .arg(WATCHDOG)
            .arg(&path)
            .arg(std::process::id().to_string())
            .arg(&register)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        #[cfg(test)]
        let watchdog_pid = child.id();
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        let connection = loop {
            match listener.accept() {
                Ok((s, _)) => break s,
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        && std::time::Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = std::fs::remove_file(&path);
                    return Err(e.to_string());
                }
            }
        };
        // Darwin accepts inherit O_NONBLOCK from the listening socket. The
        // lifetime monitor must block until watchdog EOF, not report WouldBlock.
        connection
            .set_nonblocking(false)
            .map_err(|e| e.to_string())?;
        drop(listener);
        let _ = std::fs::remove_file(&path);
        connection
            .set_read_timeout(Some(Duration::from_secs(3)))
            .map_err(|e| e.to_string())?;
        if peer_pid(&connection)? != child.id() as i32 {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Watchdog peer identity mismatch".into());
        }
        let mut check = &connection;
        let mut ack = [0];
        check.read_exact(&mut ack).map_err(|e| e.to_string())?;
        if ack != [1] {
            return Err("Watchdog startup refused".into());
        }
        connection
            .set_read_timeout(None)
            .map_err(|e| e.to_string())?;
        let alive = Arc::new(AtomicBool::new(true));
        let groups = Arc::new(Mutex::new(BTreeMap::<i32, Identity>::new()));
        let mut monitor = connection.try_clone().map_err(|e| e.to_string())?;
        let status = alive.clone();
        let tracked = groups.clone();
        std::thread::spawn(move || {
            let mut b = [0];
            loop {
                match monitor.read(&mut b) {
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    _ => break,
                }
            }
            status.store(false, Ordering::Release);
            let groups: Vec<_> = tracked.lock().unwrap().values().copied().collect();
            let deadline = std::time::Instant::now() + Duration::from_secs(3);
            for group in groups {
                cleanup_until(group, deadline);
            }
            let _ = child.wait();
        });
        Ok(Watchdog {
            #[cfg(test)]
            pid: watchdog_pid,
            path: register,
            alive,
            groups,
            _connection: connection,
        })
    });
    result.as_ref().map_err(Clone::clone)
}
pub fn prepare(executable: PathBuf) -> Result<PathBuf, String> {
    let w = watchdog(executable)?;
    if !w.alive.load(Ordering::Acquire) {
        return Err("Terminal protection lost; restart Ongleet".into());
    }
    Ok(w.path.clone())
}
pub fn registered(pid: u32) -> Result<Arc<Containment>, String> {
    let w = INSTANCE
        .get()
        .ok_or("Watchdog unavailable")?
        .as_ref()
        .map_err(Clone::clone)?;
    let mut groups = w.groups.lock().unwrap();
    if !w.alive.load(Ordering::Acquire) {
        return Err("Terminal protection lost; restart Ongleet".into());
    }
    let leader = identity(pid as i32).ok_or("Terminal identity unavailable")?;
    groups.insert(pid as i32, leader);
    Ok(Arc::new(Containment {
        leader,
        stopped: AtomicBool::new(false),
    }))
}
fn watchdog_main(path: PathBuf, parent: i32, register: PathBuf) -> Result<(), String> {
    let mut app = UnixStream::connect(path).map_err(|e| e.to_string())?;
    if peer_pid(&app)? != parent {
        return Err("Application peer identity mismatch".into());
    }
    let listener = UnixListener::bind(&register).map_err(|e| e.to_string())?;
    let q = events::queue(&[
        events::event(
            parent as usize,
            libc::EVFILT_PROC,
            libc::EV_ADD,
            libc::NOTE_EXIT,
        ),
        events::event(app.as_raw_fd() as usize, libc::EVFILT_READ, libc::EV_ADD, 0),
        events::event(
            listener.as_raw_fd() as usize,
            libc::EVFILT_READ,
            libc::EV_ADD,
            0,
        ),
    ])?;
    app.write_all(&[1]).map_err(|e| e.to_string())?;
    let mut groups = BTreeMap::new();
    loop {
        let event = events::wait(&q)?;
        if event.filter == libc::EVFILT_PROC {
            if event.ident == parent as usize {
                break;
            }
            if let Some(id) = groups.remove(&(event.ident as i32)) {
                cleanup(id);
            }
            continue;
        }
        if event.ident == app.as_raw_fd() as usize {
            break;
        }
        let (mut stream, _) = listener.accept().map_err(|e| e.to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .map_err(|e| e.to_string())?;
        stream
            .set_write_timeout(Some(Duration::from_secs(1)))
            .map_err(|e| e.to_string())?;
        let mut uid = 0;
        let mut gid = 0;
        if unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) } != 0
            || uid != unsafe { libc::geteuid() }
        {
            continue;
        }
        let mut bytes = [0u8; 4];
        if stream.read_exact(&mut bytes).is_err() {
            continue;
        }
        let pid = i32::from_ne_bytes(bytes);
        if peer_pid(&stream).ok() != Some(pid) || groups.len() >= 4096 {
            continue;
        }
        if let Some(id) = identity(pid).filter(|_| unsafe { libc::getsid(pid) } == pid) {
            let change = events::event(
                pid as usize,
                libc::EVFILT_PROC,
                libc::EV_ADD | libc::EV_ONESHOT,
                libc::NOTE_EXIT,
            );
            if unsafe {
                libc::kevent(
                    q.as_raw_fd(),
                    &change,
                    1,
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null(),
                )
            } < 0
            {
                continue;
            }
            groups.insert(pid, id);
            let _ = stream.write_all(&[1]);
        }
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    for id in groups.values() {
        cleanup_until(*id, deadline);
    }
    let _ = std::fs::remove_file(register);
    Ok(())
}
fn bootstrap(
    path: PathBuf,
    handshake: PathBuf,
    executable: std::ffi::OsString,
    args: Vec<std::ffi::OsString>,
) -> Result<(), String> {
    let pid = unsafe { libc::getpid() };
    if unsafe { libc::getsid(0) } != pid {
        return Err("Terminal bootstrap has no independent native session".into());
    }
    let mut socket = UnixStream::connect(path).map_err(|e| e.to_string())?;
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|e| e.to_string())?;
    socket
        .set_write_timeout(Some(Duration::from_secs(3)))
        .map_err(|e| e.to_string())?;
    socket
        .write_all(&pid.to_ne_bytes())
        .map_err(|e| e.to_string())?;
    let mut ack = [0];
    socket.read_exact(&mut ack).map_err(|e| e.to_string())?;
    if ack != [1] {
        return Err("Watchdog registration refused".into());
    }
    drop(socket);
    let mut ready = UnixStream::connect(handshake).map_err(|e| e.to_string())?;
    ready
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|e| e.to_string())?;
    ready.write_all(&[1]).map_err(|e| e.to_string())?;
    ready.read_exact(&mut ack).map_err(|e| e.to_string())?;
    if ack != [1] {
        return Err("Application registration refused".into());
    }
    // Keep the CLOEXEC socket alive through exec. EOF acknowledges successful
    // execution; a failed exec sends its error before closing the connection.
    let error = Command::new(executable).args(args).exec().to_string();
    let _ = ready.write_all(error.as_bytes());
    Err(error)
}
pub fn run_helper_if_requested() -> bool {
    let mut args = std::env::args_os().skip(1);
    let Some(flag) = args.next() else {
        return false;
    };
    let result = if flag == WATCHDOG {
        let path = args.next().map(PathBuf::from);
        let pid = args.next().and_then(|v| v.to_str()?.parse().ok());
        match (path, pid, args.next()) {
            (Some(p), Some(id), Some(register)) => watchdog_main(p, id, register.into()),
            _ => Err("Invalid watchdog arguments".into()),
        }
    } else if flag == HELPER_FLAG {
        match (args.next(), args.next(), args.next()) {
            (Some(path), Some(handshake), Some(executable)) => {
                bootstrap(path.into(), handshake.into(), executable, args.collect())
            }
            _ => Err("Invalid terminal bootstrap arguments".into()),
        }
    } else {
        return false;
    };
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
    true
}

pub struct Handshake {
    path: PathBuf,
    listener: UnixListener,
}
impl Handshake {
    pub fn new() -> Result<Self, String> {
        use super::super::SystemFiles;
        let name = super::Macos::new().nonce()?;
        let path = paths::runtime()?.join(format!("h-{}.sock", &name[..16]));
        let listener = UnixListener::bind(&path).map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        Ok(Self { path, listener })
    }
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
    pub fn wait(&self, pid: u32) -> Result<Arc<Containment>, String> {
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            match self.listener.accept() {
                Ok((mut s, _)) => {
                    s.set_nonblocking(false).map_err(|e| e.to_string())?;
                    s.set_read_timeout(Some(Duration::from_secs(1)))
                        .map_err(|e| e.to_string())?;
                    if peer_pid(&s)? != pid as i32 {
                        return Err("Bootstrap peer identity mismatch".into());
                    }
                    let mut ack = [0];
                    s.read_exact(&mut ack).map_err(|e| e.to_string())?;
                    if ack != [1] {
                        return Err("Invalid bootstrap acknowledgment".into());
                    }
                    let group = registered(pid)?;
                    s.write_all(&[1]).map_err(|e| e.to_string())?;
                    let mut failure = Vec::new();
                    (&mut s)
                        .take(4097)
                        .read_to_end(&mut failure)
                        .map_err(|e| e.to_string())?;
                    if !failure.is_empty() {
                        return Err(format!(
                            "Terminal launch failed: {}",
                            String::from_utf8_lossy(&failure)
                        ));
                    }
                    return Ok(group);
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        && std::time::Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(e) => return Err(format!("Terminal registration failed: {e}")),
            }
        }
    }
}
impl Drop for Handshake {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
pub fn watchdog_pid() -> u32 {
    INSTANCE.get().unwrap().as_ref().unwrap().pid
}
#[cfg(test)]
pub fn protection_lost() -> bool {
    !INSTANCE
        .get()
        .unwrap()
        .as_ref()
        .unwrap()
        .alive
        .load(Ordering::Acquire)
}
