//! Bounded, opt-in breadcrumbs for native crashes. Never log request bodies or URLs.
use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tauri::Manager;

const LIMIT: u64 = 4 * 1024 * 1024;
static LOG: OnceLock<Mutex<Log>> = OnceLock::new();
struct Log {
    files: std::sync::Arc<dyn crate::system::SystemFiles>,
    file: File,
    path: PathBuf,
    bytes: u64,
    started: Instant,
}

pub fn enabled() -> bool {
    std::env::var_os("ONGLEET_DEBUG").as_deref() == Some(std::ffi::OsStr::new("1"))
}

pub fn init(
    files: std::sync::Arc<dyn crate::system::SystemFiles>,
    paths: &dyn crate::system::SystemPaths,
    directory: &Path,
) -> std::io::Result<()> {
    if !enabled() {
        return Ok(());
    }
    let path = directory.join("debug.log");
    rotate(files.as_ref(), &path)?;
    let file = open(files.as_ref(), &path)?;
    let _ = LOG.set(Mutex::new(Log {
        files,
        file,
        path: path.clone(),
        bytes: 0,
        started: Instant::now(),
    }));
    eprintln!("Ongleet diagnostics: {}", path.display());
    record(format_args!(
        "startup version={} pid={} executable={:?}",
        env!("CARGO_PKG_VERSION"),
        std::process::id(),
        paths.application_executable().ok()
    ));
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        record(format_args!("rust-panic location={:?}", info.location()));
        previous(info);
    }));
    Ok(())
}

fn rotate(files: &dyn crate::system::SystemFiles, path: &Path) -> std::io::Result<()> {
    match files.atomic_replace(path, &path.with_extension("previous.log")) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn open(files: &dyn crate::system::SystemFiles, path: &Path) -> std::io::Result<File> {
    files.open_private(path, crate::system::PrivateOpen::Diagnostic)
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
            if rotate(self.files.as_ref(), &self.path).is_err() {
                return;
            }
            let Ok(file) = open(self.files.as_ref(), &self.path) else {
                return;
            };
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
    app.state::<crate::system::SystemServices>()
        .desktop
        .install_diagnostics(app, std::sync::Arc::new(record_native))
        .map_err(std::io::Error::other)?;
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
#[cfg(unix)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn rotation_keeps_recent_breadcrumbs_bounded_and_private() {
        let directory =
            std::env::temp_dir().join(format!("ongleet-diagnostic-test-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("debug.log");
        let mut log = Log {
            files: crate::system::services().files,
            file: open(crate::system::services().files.as_ref(), &path).unwrap(),
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
        rotate(crate::system::services().files.as_ref(), &path).unwrap();
        drop(open(crate::system::services().files.as_ref(), &path).unwrap());
        assert!(std::fs::read_to_string(&backup)
            .unwrap()
            .ends_with("last breadcrumb\n"));
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
        std::fs::remove_dir_all(directory).unwrap();
    }
}

fn record_native(event: crate::system::NativeDiagnostic) {
    use crate::system::NativeDiagnostic;
    match event {
        NativeDiagnostic::Load(event) => record(format_args!("webview load={event}")),
        NativeDiagnostic::LoadFailed { stage, domain } => record(format_args!(
            "webview load_failed stage={stage} domain={domain}"
        )),
        NativeDiagnostic::Terminated(reason) => {
            record(format_args!("webview process_terminated reason={reason}"))
        }
        NativeDiagnostic::ResourceStarted => record(format_args!("webview resource_started")),
        NativeDiagnostic::Heartbeat { tick, counter } => {
            record(format_args!("heartbeat tick={tick} {counter}"))
        }
        NativeDiagnostic::Installed => record(format_args!("native diagnostics installed")),
    }
}
