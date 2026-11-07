//! User-private single-instance and control socket. Registry operations run off GTK.
use crate::control::{
    client,
    protocol::{encode_response, Request, Response, RpcError, REQUEST_LIMIT, VERSION},
};
use crate::control_dispatch::{self, Effect};
use std::{
    io::Write,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::Duration,
};
use tauri::Manager;

pub fn show(
    transport: &dyn crate::system::ControlTransport,
    path: &crate::system::ControlEndpoint,
) -> Result<(), String> {
    let response = client::call(transport, path, crate::control::protocol::Command::Show)
        .map_err(|e| e.message)?;
    if response.ok {
        Ok(())
    } else {
        Err(response
            .error
            .map(|e| e.message)
            .unwrap_or_else(|| "Activation failed".into()))
    }
}

/// Return the shell prompt while the explicitly launched desktop app runs.
/// Development builds and --foreground bypass this path in the caller.
pub fn detach_terminal_launch(
    system: &crate::system::SystemServices,
    socket: &crate::system::ControlEndpoint,
) -> Result<bool, String> {
    use std::time::Instant;
    if !system.processes.has_terminal() {
        return Ok(false);
    }
    let mut child = system
        .processes
        .spawn(
            crate::system::ProcessLaunch {
                executable: system.paths.executable()?,
                args: vec!["--foreground".into()],
                ..Default::default()
            },
            crate::system::ProcessMode::TerminalApplication,
        )
        .map_err(|e| format!("Cannot start application: {e}"))?
        .child;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if client::call_with_timeout(
            system.transport.as_ref(),
            socket,
            crate::control::protocol::Command::Status,
            Duration::from_secs(1),
        )
        .is_ok_and(|response| response.ok)
        {
            return Ok(true);
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return Err(format!("Application exited before becoming available: {status}. Run ongleet-gui --foreground for startup diagnostics."));
        }
        if Instant::now() >= deadline {
            return Err("Application startup exceeded 15 seconds; it may still start. Check ongleet app status before retrying.".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

pub struct Instance {
    stop: Arc<AtomicBool>,
    thread: Mutex<Option<JoinHandle<()>>>,
}
impl Instance {
    // The caller must already hold the exclusive SQLite writer lock. Only then
    // can a stale socket be removed without racing a live application instance.
    pub fn start(
        transport: &dyn crate::system::ControlTransport,
        path: crate::system::ControlEndpoint,
        app: tauri::AppHandle,
    ) -> Result<Self, String> {
        let listener = transport.listen(&path)?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let thread = std::thread::spawn(move || {
            while !thread_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok(mut stream) => {
                        let _ = stream.write_timeout(Duration::from_secs(2));
                        if stream.verify_owner().is_err() {
                            continue;
                        }
                        let mut effect = Effect::None;
                        let result = (|| {
                            let bytes = client::read_message(
                                stream.as_mut(),
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
                        let _ = stream.finish_write();
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
        })
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.lock().unwrap().take() {
            if thread.thread().id() != std::thread::current().id() {
                let _ = thread.join();
            }
        }
    }
}
impl Drop for Instance {
    fn drop(&mut self) {
        self.stop();
    }
}
