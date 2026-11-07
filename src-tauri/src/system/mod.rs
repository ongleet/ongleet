//! Injected native mechanisms. Feature policy belongs to callers.
use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
    sync::Arc,
};
#[cfg(not(target_os = "linux"))]
compile_error!("Ongleet currently supports Linux only");
#[cfg(target_os = "linux")]
mod linux;

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
    fn spawn(&self, launch: Launch) -> Result<TerminalIo, String>;
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
    let native = Arc::new(linux::Linux);
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
    linux::containment::run_helper_if_requested()
}
#[cfg(test)]
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
    fn private_directory(&self, path: &Path, recursive: bool) -> io::Result<()>;
    fn private_permissions(&self, path: &Path, directory: bool) -> io::Result<()>;
    fn open_private(&self, path: &Path, mode: PrivateOpen) -> io::Result<std::fs::File>;
    fn try_lock(&self, path: &Path) -> Result<Box<dyn FileLock>, String>;
    fn nonce(&self) -> Result<String, String>;
    fn open_editor(&self, path: &Path) -> io::Result<std::fs::File>;
    fn revision(&self, bytes: &[u8], metadata: &std::fs::Metadata) -> String;
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
pub trait DesktopService: Send + Sync {
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
