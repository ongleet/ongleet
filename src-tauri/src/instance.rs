//! User-private single-instance and control socket. Registry operations run off GTK.
pub use crate::control::paths::{data_directory, socket_path};
use crate::control::{
    client,
    protocol::{encode_response, Request, Response, RpcError, REQUEST_LIMIT, VERSION},
};
use crate::control_dispatch::{self, Effect};
use std::{
    fs,
    io::Write,
    os::unix::{
        fs::{FileTypeExt, MetadataExt, PermissionsExt},
        net::UnixListener,
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

pub fn show(path: &Path) -> Result<(), String> {
    let response =
        client::call(path, crate::control::protocol::Command::Show).map_err(|e| e.message)?;
    if response.ok {
        Ok(())
    } else {
        Err(response
            .error
            .map(|e| e.message)
            .unwrap_or_else(|| "Activation failed".into()))
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
                        let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                        if client::verify_peer(&stream).is_err() {
                            continue;
                        }
                        let mut effect = Effect::None;
                        let result = (|| {
                            let bytes = client::read_message(
                                &mut stream,
                                REQUEST_LIMIT,
                                Duration::from_secs(2),
                            )?;
                            let request: Request =
                                serde_json::from_slice(&bytes).map_err(|_| {
                                    RpcError::new(
                                        "invalid_request",
                                        "Malformed or unsupported request",
                                    )
                                })?;
                            if request.version != VERSION {
                                return Err(RpcError::new(
                                    "protocol_error",
                                    "Unsupported socket protocol version",
                                ));
                            }
                            if thread_stop.load(Ordering::Acquire) {
                                return Err(RpcError::new(
                                    "unavailable",
                                    "Application is shutting down",
                                ));
                            }
                            control_dispatch::handle(&app, request.command)
                        })();
                        let response = match result {
                            Ok((data, next)) => {
                                effect = next;
                                Response::success(data)
                            }
                            Err(error) => Response::failure(error),
                        };
                        let bytes = encode_response(&response);
                        let _ = stream.write_all(&bytes);
                        let _ = stream.shutdown(std::net::Shutdown::Write);
                        match effect {
                            Effect::None => {}
                            Effect::Show => {
                                let handle = app.clone();
                                let _ = app.run_on_main_thread(move || {
                                    if let Some(window) = handle.get_webview_window("main") {
                                        if crate::window_state::activate(&window).is_ok() {
                                            handle
                                                .state::<crate::Lifecycle>()
                                                .visible
                                                .store(true, Ordering::Release);
                                        }
                                    }
                                });
                            }
                            Effect::Quit => {
                                let handle = app.clone();
                                let _ = app.run_on_main_thread(move || handle.exit(0));
                            }
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
