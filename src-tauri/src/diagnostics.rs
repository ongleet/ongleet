//! Bounded, opt-in breadcrumbs for native crashes. Never log request bodies or URLs.
use std::{
    fs::{File, OpenOptions},
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tauri::Manager;
use webkit2gtk::WebViewExt;

const LIMIT: u64 = 4 * 1024 * 1024;
static LOG: OnceLock<Mutex<Log>> = OnceLock::new();
struct Log {
    file: File,
    path: PathBuf,
    bytes: u64,
    started: Instant,
}

pub fn enabled() -> bool {
    std::env::var_os("DEVCENTER_DEBUG").as_deref() == Some(std::ffi::OsStr::new("1"))
}

pub fn init(directory: &Path) -> std::io::Result<()> {
    if !enabled() {
        return Ok(());
    }
    let path = directory.join("debug.log");
    rotate(&path)?;
    let file = open(&path)?;
    let _ = LOG.set(Mutex::new(Log {
        file,
        path: path.clone(),
        bytes: 0,
        started: Instant::now(),
    }));
    eprintln!("DevCenter diagnostics: {}", path.display());
    record(format_args!(
        "startup version={} pid={} executable={:?}",
        env!("CARGO_PKG_VERSION"),
        std::process::id(),
        std::env::current_exe().ok()
    ));
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        record(format_args!("rust-panic location={:?}", info.location()));
        previous(info);
    }));
    Ok(())
}

fn rotate(path: &Path) -> std::io::Result<()> {
    match std::fs::rename(path, path.with_extension("previous.log")) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn open(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
}

pub fn record(message: std::fmt::Arguments<'_>) {
    let Some(log) = LOG.get() else { return };
    let Ok(mut log) = log.try_lock() else { return };

    log.write(message);
}

impl Log {
    fn write(&mut self, message: std::fmt::Arguments<'_>) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let line = format!(
            "unix_ms={timestamp} elapsed_ms={} {message}\n",
            self.started.elapsed().as_millis()
        );
        if self.bytes + line.len() as u64 > LIMIT {
            if rotate(&self.path).is_err() {
                return;
            }
            let Ok(file) = open(&self.path) else { return };
            self.file = file;
            self.bytes = 0;
        }
        // File is unbuffered: completed breadcrumbs survive SIGABRT without a handler.
        if self.file.write_all(line.as_bytes()).is_ok() {
            self.bytes += line.len() as u64;
        }
    }
}

pub fn install(app: &tauri::App) -> tauri::Result<()> {
    if !enabled() {
        return Ok(());
    }
    for (_, window) in app.webview_windows() {
        window.with_webview(|platform| {
            let webview = platform.inner();
            webview.connect_load_changed(|_, event| record(format_args!("webview load={event:?}")));
            webview.connect_load_failed(|_, event, _, error| {
                record(format_args!(
                    "webview load_failed stage={event:?} domain={:?}",
                    error.domain()
                ));
                false
            });
            webview.connect_web_process_terminated(|_, reason| {
                record(format_args!("webview process_terminated reason={reason:?}"))
            });
            webview.connect_resource_load_started(|_, _, _| {
                record(format_args!("webview resource_started"))
            });
        })?;
    }
    let mut ticks = 0u64;
    gtk::glib::timeout_add_seconds_local(30, move || {
        ticks += 1;
        // Only selected process counters, never environment or terminal contents.
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status
                .lines()
                .filter(|line| line.starts_with("VmRSS:") || line.starts_with("Threads:"))
            {
                record(format_args!("heartbeat tick={ticks} {line}"));
            }
        }
        gtk::glib::ControlFlow::Continue
    });
    record(format_args!("native diagnostics installed"));
    Ok(())
}

/// Completion breadcrumb also runs on early error returns; contents stay private.
pub struct Operation {
    name: &'static str,
    id: u64,
    started: Instant,
}
impl Operation {
    pub fn start(name: &'static str, id: u64) -> Self {
        record(format_args!("sdk begin operation={name} tab={id}"));
        Self {
            name,
            id,
            started: Instant::now(),
        }
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        record(format_args!(
            "sdk end operation={} tab={} duration_ms={}",
            self.name,
            self.id,
            self.started.elapsed().as_millis()
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn rotation_keeps_recent_breadcrumbs_bounded_and_private() {
        let directory =
            std::env::temp_dir().join(format!("devcenter-diagnostic-test-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("debug.log");
        let mut log = Log {
            file: open(&path).unwrap(),
            path: path.clone(),
            bytes: 0,
            started: Instant::now(),
        };
        let payload = "x".repeat(8192);
        for _ in 0..1100 {
            log.write(format_args!("{payload}"));
        }
        log.write(format_args!("last breadcrumb"));
        drop(log);
        let backup = path.with_extension("previous.log");
        for file in [&path, &backup] {
            let metadata = std::fs::metadata(file).unwrap();
            assert!(metadata.len() <= LIMIT);
            assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        }
        assert!(std::fs::read_to_string(&path)
            .unwrap()
            .ends_with("last breadcrumb\n"));
        // Restart preserves the final breadcrumbs from the preceding run.
        rotate(&path).unwrap();
        drop(open(&path).unwrap());
        assert!(std::fs::read_to_string(&backup)
            .unwrap()
            .ends_with("last breadcrumb\n"));
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
