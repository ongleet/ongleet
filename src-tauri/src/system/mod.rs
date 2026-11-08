//! Injected native mechanisms. Feature policy belongs to callers.
use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
    sync::Arc,
};
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
compile_error!("Ongleet supports Linux, macOS and Windows x64");
#[cfg(all(target_os = "macos", not(target_arch = "aarch64")))]
compile_error!("The macOS preview requires Apple Silicon");
#[cfg(all(target_os = "windows", not(target_arch = "x86_64")))]
compile_error!("The Windows backend requires x64");
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Launch {
    pub executable: Option<String>,
    pub args: Vec<String>,
    pub cwd: String,
    pub env: BTreeMap<String, String>,
    pub cols: u16,
    pub rows: u16,
}

pub trait SystemPaths: Send + Sync {
    fn config_directory(&self) -> Result<PathBuf, String>;
    fn data_directory(&self, identifier: &str) -> Result<PathBuf, String>;
    fn control_endpoint(&self, data: &Path) -> Result<ControlEndpoint, String>;
    /// Live image for internal helpers, including after replacement on disk.
    fn executable(&self) -> Result<PathBuf, String>;
    fn application_executable(&self) -> Result<PathBuf, String>;
    #[allow(dead_code)] // Used by the CLI composition, not the GUI.
    fn sibling_executable(&self, name: &str) -> Result<PathBuf, String>;
    fn default_shell(&self) -> String;
    /// An optional platform-specific seed for newly created shell actions.
    fn shell_action_seed(&self) -> Option<(&'static str, String)> {
        None
    }
    /// Construct explicit shell mode; literal executable launches bypass this.
    fn shell_command(&self, script: &str) -> (String, Vec<String>);
    /// A command interpreted by the native Codex hook shell.
    fn hook_command(&self, executable: &str, args: &[&str]) -> Result<String, String>;
}
pub trait Cancellation: Send + Sync {
    fn cancel(&self);
    fn is_cancelled(&self) -> bool;
}
pub trait TerminalReader: Send {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize>;
    fn wait(&self) -> Result<bool, String>;
}
pub trait TerminalWriter: Send {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize>;
    fn wait(&self) -> Result<bool, String>;
}
pub trait TerminalControl: Send + Sync {
    fn resize(&self, cols: u16, rows: u16) -> Result<(), String>;
    fn stop(&self) -> Result<(), String>;
    fn release(&self);
    #[cfg(test)]
    fn group_directory(&self) -> PathBuf;
    #[cfg(test)]
    fn released(&self) -> bool;
}
pub trait TerminalWait: Send {
    fn wait(self: Box<Self>) -> Result<u32, String>;
}
pub struct TerminalIo {
    pub reader: Box<dyn TerminalReader>,
    pub writer: Box<dyn TerminalWriter>,
    pub cancel_input: Arc<dyn Cancellation>,
    pub cancel_read: Arc<dyn Cancellation>,
    pub control: Arc<dyn TerminalControl>,
    pub wait: Box<dyn TerminalWait>,
}
pub trait TerminalService: Send + Sync {
    fn cleanup_guarantee(&self) -> CleanupGuarantee {
        CleanupGuarantee::Strict
    }
    /// Independent of containment: a pseudoconsole may retain output after exit.
    fn post_exit_drain_timeout(&self) -> Option<std::time::Duration> {
        None
    }
    fn spawn(&self, launch: Launch) -> Result<TerminalIo, String>;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupGuarantee {
    Strict,
    #[allow(dead_code)] // Constructed by the Darwin backend only.
    BestEffort,
}

#[derive(Clone)]
pub struct SystemServices {
    pub processes: Arc<dyn ProcessService>,
    pub secrets: Arc<dyn SecretStore>,
    pub files: Arc<dyn SystemFiles>,
    pub desktop: Arc<dyn DesktopService>,
    pub paths: Arc<dyn SystemPaths>,
    pub terminal: Arc<dyn TerminalService>,
    pub transport: Arc<dyn ControlTransport>,
}
/// Called once by each process entry point. Expensive resources stay lazy.
pub fn services() -> SystemServices {
    #[cfg(target_os = "linux")]
    let native = Arc::new(linux::Linux);
    #[cfg(target_os = "macos")]
    let native = Arc::new(macos::Macos::new());
    #[cfg(target_os = "windows")]
    let native = Arc::new(windows::Windows::new());
    SystemServices {
        desktop: native.clone(),
        secrets: native.clone(),
        processes: native.clone(),
        files: native.clone(),
        paths: native.clone(),
        terminal: native.clone(),
        transport: native,
    }
}
pub fn run_helper_if_requested() -> bool {
    #[cfg(target_os = "windows")]
    {
        windows::run_helper_if_requested()
    }
    #[cfg(target_os = "linux")]
    {
        linux::containment::run_helper_if_requested()
    }
    #[cfg(target_os = "macos")]
    {
        macos::containment::run_helper_if_requested()
    }
}
#[cfg(all(test, target_os = "linux"))]
#[allow(unused_imports)]
pub(crate) use linux::containment::watchdog_pid;

#[derive(Clone)]
pub struct ControlEndpoint(Arc<dyn std::any::Any + Send + Sync>);

pub trait ControlConnection: io::Read + io::Write + Send {
    fn verify_owner(&self) -> Result<(), String>;
    fn read_timeout(&self, timeout: std::time::Duration) -> io::Result<()>;
    fn write_timeout(&self, timeout: std::time::Duration) -> io::Result<()>;
    fn finish_write(&self) -> io::Result<()>;
}
pub trait ControlListener: Send {
    fn accept(&self) -> io::Result<Box<dyn ControlConnection>>;
}
pub trait ControlTransport: Send + Sync {
    fn connect(&self, endpoint: &ControlEndpoint) -> Result<Box<dyn ControlConnection>, String>;
    fn listen(&self, endpoint: &ControlEndpoint) -> Result<Box<dyn ControlListener>, String>;
}

pub enum PrivateOpen {
    New,
    Replace,
    Lock,
    Diagnostic,
}
pub trait FileLock: Send + Sync {}
pub trait FileWatch: Send {
    fn acknowledge(&self);
}
pub trait SystemFiles: Send + Sync {
    /// Atomically install a same-volume temporary file, replacing an existing file.
    /// The caller flushes the temporary file before committing it.
    fn atomic_replace(&self, source: &Path, destination: &Path) -> io::Result<()>;
    fn sync_directory(&self, directory: &Path) -> io::Result<()>;
    fn private_directory(&self, path: &Path, recursive: bool) -> io::Result<()>;
    fn private_permissions(&self, path: &Path, directory: bool) -> io::Result<()>;
    fn open_private(&self, path: &Path, mode: PrivateOpen) -> io::Result<std::fs::File>;
    fn try_lock(&self, path: &Path) -> Result<Box<dyn FileLock>, String>;
    fn nonce(&self) -> Result<String, String>;
    fn open_editor(&self, path: &Path) -> io::Result<std::fs::File>;
    fn revision(&self, bytes: &[u8], metadata: &std::fs::Metadata, path: &Path) -> String;
    fn editable(&self, metadata: &std::fs::Metadata) -> bool;
    fn preserve_attributes(
        &self,
        file: &std::fs::File,
        metadata: &std::fs::Metadata,
    ) -> Result<(), String>;
    fn watch(
        &self,
        path: String,
        changed: Box<dyn Fn() + Send>,
    ) -> Result<Box<dyn FileWatch>, String>;
}

#[derive(Default)]
pub struct ProcessLaunch {
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub cwd: Option<PathBuf>,
}
pub enum ProcessMode {
    Captured,
    Detached,
    TerminalApplication,
    Inherited,
}
pub struct ProcessExit {
    pub code: Option<i32>,
    pub exit_code: i32,
    pub description: String,
}
impl std::fmt::Display for ProcessExit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.description)
    }
}
pub trait ProcessChild: Send {
    fn id(&self) -> u32;
    fn try_wait(&mut self) -> Result<Option<ProcessExit>, String>;
    fn wait(&mut self) -> Result<ProcessExit, String>;
}
pub struct ProcessResources {
    pub child: Box<dyn ProcessChild>,
    pub stdout: Option<Box<dyn io::Read + Send>>,
    pub stderr: Option<Box<dyn io::Read + Send>>,
}
pub trait ProcessService: Send + Sync {
    fn spawn(&self, launch: ProcessLaunch, mode: ProcessMode) -> Result<ProcessResources, String>;
    fn prepare_supervisor(&self);
    fn has_terminal(&self) -> bool;
}

pub type SystemFuture<'a, T> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, String>> + Send + 'a>>;
pub type SecretAttributes = BTreeMap<String, String>;
pub struct SecretRecord {
    pub identity: String,
    pub attributes: SecretAttributes,
}
pub struct SecretSearch {
    pub locked: bool,
    pub records: Vec<SecretRecord>,
}
pub trait SecretStore: Send + Sync {
    fn read<'a>(&'a self, record: &'a SecretRecord) -> SystemFuture<'a, Vec<u8>>;
    fn search<'a>(&'a self, attributes: &'a SecretAttributes) -> SystemFuture<'a, SecretSearch>;
    fn ensure_available(&self) -> SystemFuture<'_, ()>;
    fn create<'a>(
        &'a self,
        attributes: &'a SecretAttributes,
        bytes: &'a [u8],
        replace: bool,
    ) -> SystemFuture<'a, ()>;
    fn remove<'a>(&'a self, record: &'a SecretRecord) -> SystemFuture<'a, ()>;
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Geometry {
    pub width: i32,
    pub height: i32,
    pub position: Option<(i32, i32)>,
    pub maximized: bool,
}

impl Default for Geometry {
    fn default() -> Self {
        Self {
            width: 1100,
            height: 760,
            position: None,
            maximized: false,
        }
    }
}

pub enum ClipboardContent {
    Text(String),
    Image,
    Files(Vec<PathBuf>),
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformCapabilities {
    pub command_shortcuts: bool,
    pub primary_selection: &'static str,
}
pub trait DesktopService: Send + Sync {
    fn configure_builder(&self, builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
        builder
    }
    fn reopen_requested(&self, _event: &tauri::RunEvent) -> bool {
        false
    }
    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities {
            command_shortcuts: false,
            primary_selection: "system",
        }
    }
    fn install_menu(&self, _app: &tauri::App) -> Result<(), String> {
        Ok(())
    }
    fn page_load(&self, _event: String) {}
    fn initialize(&self);
    fn show(&self, window: &tauri::WebviewWindow) -> Result<(), String>;
    fn hide(&self, window: &tauri::WebviewWindow) -> Result<(), String>;
    fn activate(&self, window: &tauri::WebviewWindow) -> Result<(), String>;
    fn install_geometry(
        &self,
        window: &tauri::WebviewWindow,
        geometry: Arc<std::sync::Mutex<Geometry>>,
    ) -> Result<(), String>;
    fn clipboard(&self, app: tauri::AppHandle, primary: bool)
        -> SystemFuture<'_, ClipboardContent>;
    fn write_primary(&self, app: tauri::AppHandle, text: String) -> SystemFuture<'_, ()>;
    fn local_file(&self, uri: &str) -> Option<PathBuf>;
    fn file_uri(&self, path: &Path) -> String;
    fn open_uri(&self, uri: String) -> SystemFuture<'_, ()>;
    fn install_diagnostics(
        &self,
        app: &tauri::App,
        record: Arc<dyn Fn(NativeDiagnostic) + Send + Sync>,
    ) -> Result<(), String>;
    fn install_input_diagnostics(
        &self,
        app: &tauri::App,
        record: Arc<dyn Fn(NativeInputDiagnostic) + Send + Sync>,
    ) -> Result<(), String>;
}

/// Native facts only; common diagnostics own their format and destination.
pub enum NativeDiagnostic {
    Load(String),
    LoadFailed { stage: String, domain: String },
    Terminated(String),
    ResourceStarted,
    Heartbeat { tick: u64, counter: String },
    Installed,
}
pub enum NativeInputDiagnostic {
    Enabled {
        label: String,
        active: bool,
        visible: bool,
    },
    Event(NativeInputEvent),
}
pub struct NativeInputEvent {
    pub elapsed_ms: u128,
    pub label: String,
    pub event: String,
    pub active: bool,
    pub focused: bool,
    pub visible: bool,
    pub send_event: bool,
    pub key: Option<NativeKey>,
}
pub struct NativeKey {
    pub time: u32,
    pub hardware_code: u16,
    pub keyval: String,
    pub group: u8,
    pub modifiers: String,
}

#[cfg(test)]
mod contract_tests {
    use super::*;

    #[test]
    fn replacement_commits_existing_and_missing_files_without_removing_failed_sources() {
        let files = services().files;
        let directory = std::env::temp_dir().join(format!(
            "ongleet-replace-{}-{}",
            std::process::id(),
            files.nonce().unwrap(),
        ));
        std::fs::create_dir(&directory).unwrap();
        let temporary = directory.join("temporary");
        let destination = directory.join("destination");
        for text in ["first", "replacement 日本語"] {
            let mut file = files.open_private(&temporary, PrivateOpen::New).unwrap();
            std::io::Write::write_all(&mut file, text.as_bytes()).unwrap();
            file.sync_all().unwrap();
            files.atomic_replace(&temporary, &destination).unwrap();
            assert_eq!(std::fs::read_to_string(&destination).unwrap(), text);
            assert!(!temporary.exists());
        }
        std::fs::write(&temporary, "keep on failure").unwrap();
        assert!(files.atomic_replace(&temporary, &directory).is_err());
        assert_eq!(
            std::fs::read_to_string(&temporary).unwrap(),
            "keep on failure"
        );
        files.sync_directory(&directory).unwrap();
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn hook_command_preserves_literal_arguments_through_the_native_shell() {
        // Both currently supported backends use POSIX hooks. Windows will need
        // its own native shell round-trip fixture when its backend is added.
        let paths = services().paths;
        let arguments = [
            "",
            "space and café 日本語",
            "it's \"quoted\"",
            "$HOME; $(exit 1) & | < >",
            "a\nb",
            "trailing\\",
        ];
        let mut args = vec!["%s\\0"];
        args.extend(arguments);
        let command = paths.hook_command("/usr/bin/printf", &args).unwrap();
        let output = std::process::Command::new("/bin/sh")
            .args(["-c", &command])
            .output()
            .unwrap();
        assert!(output.status.success());
        let expected: Vec<u8> = arguments
            .iter()
            .flat_map(|arg| arg.as_bytes().iter().copied().chain(std::iter::once(0)))
            .collect();
        assert_eq!(output.stdout, expected);
        assert!(paths.hook_command("bad\0executable", &[]).is_err());
        assert!(paths
            .hook_command("executable", &["bad\0argument"])
            .is_err());
    }
}
