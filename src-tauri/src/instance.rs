//! Single-instance window activation. Full workspace/terminal CLI control is later work.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::{
            fs::{FileTypeExt, MetadataExt, PermissionsExt},
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::Duration,
};
use tauri::Manager;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u32,
    command: String,
}

pub fn data_directory(identifier: &str) -> Result<PathBuf, String> {
    let base = match std::env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
        Some(directory) => PathBuf::from(directory),
        None => PathBuf::from(std::env::var_os("HOME").ok_or("HOME is unavailable")?)
            .join(".local/share"),
    };
    if !base.is_absolute() {
        return Err("Application data directory must be absolute".into());
    }
    Ok(base.join(identifier))
}

pub fn socket_path(data: &Path) -> Result<PathBuf, String> {
    let uid = unsafe { libc::geteuid() };
    let runtime = PathBuf::from(format!("/run/user/{uid}"));
    let metadata = fs::metadata(&runtime)
        .map_err(|e| format!("Private user runtime directory unavailable: {e}"))?;
    if metadata.uid() != uid || metadata.permissions().mode() & 0o077 != 0 {
        return Err(
            "User runtime directory must be owned by the current user and mode 0700".into(),
        );
    }
    // Stable across launches and short enough for sockaddr_un even with a long
    // XDG_DATA_HOME. Separate data directories get separate application instances.
    let hash = data
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .fold(0xcbf29ce484222325u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });
    Ok(runtime.join(format!("terminal-workspace-{hash:016x}.sock")))
}

pub fn show(path: &Path) -> Result<(), String> {
    let mut stream =
        UnixStream::connect(path).map_err(|e| format!("Application unavailable: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(1)))
        .map_err(|e| e.to_string())?;
    let request = serde_json::to_vec(&Request {
        version: 1,
        command: "show".into(),
    })
    .map_err(|e| e.to_string())?;
    stream.write_all(&request).map_err(|e| e.to_string())?;
    stream
        .shutdown(std::net::Shutdown::Write)
        .map_err(|e| e.to_string())?;
    let mut response = String::new();
    stream
        .take(1024)
        .read_to_string(&mut response)
        .map_err(|e| e.to_string())?;
    if response == "{\"ok\":true}\n" {
        Ok(())
    } else {
        Err(format!("Application activation failed: {response}"))
    }
}

pub struct Instance {
    stop: Arc<AtomicBool>,
    thread: Mutex<Option<JoinHandle<()>>>,
    path: PathBuf,
}
impl Instance {
    // The caller must already hold the exclusive SQLite writer lock. Only then
    // can a stale socket be removed without racing a live application instance.
    pub fn start(path: PathBuf, app: tauri::AppHandle) -> Result<Self, String> {
        match fs::symlink_metadata(&path) {
            Ok(metadata)
                if metadata.file_type().is_socket()
                    && metadata.uid() == unsafe { libc::geteuid() } =>
            {
                fs::remove_file(&path).map_err(|e| e.to_string())?
            }
            Ok(_) => return Err("Refusing to replace an unexpected application socket path".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        let listener = UnixListener::bind(&path).map_err(|e| e.to_string())?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let thread = std::thread::spawn(move || {
            while !thread_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
                        let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
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
                            continue;
                        }
                        let mut bytes = Vec::new();
                        if Read::by_ref(&mut stream)
                            .take(1025)
                            .read_to_end(&mut bytes)
                            .is_err()
                        {
                            continue;
                        }
                        let request = serde_json::from_slice::<Request>(&bytes);
                        let accepted = bytes.len() <= 1024
                            && request.is_ok_and(|request| {
                                request.version == 1 && request.command == "show"
                            });
                        if accepted && !thread_stop.load(Ordering::Acquire) {
                            let handle = app.clone();
                            if app
                                .run_on_main_thread(move || {
                                    if let Some(window) = handle.get_webview_window("main") {
                                        let _ = window.show();
                                        let _ = window.unminimize();
                                        let _ = window.set_focus();
                                    }
                                })
                                .is_ok()
                            {
                                let _ = stream.write_all(b"{\"ok\":true}\n");
                            }
                        } else {
                            let _ =
                                stream.write_all(b"{\"ok\":false,\"error\":\"invalid_request\"}\n");
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(25))
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            stop,
            thread: Mutex::new(Some(thread)),
            path,
        })
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.lock().unwrap().take() {
            if thread.thread().id() != std::thread::current().id() {
                let _ = thread.join();
            }
        }
        let _ = fs::remove_file(&self.path);
    }
}
impl Drop for Instance {
    fn drop(&mut self) {
        self.stop();
    }
}
