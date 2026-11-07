use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};

// Stable XDG directory name, shared by the GUI and headless commands.
pub const APPLICATION_ID: &str = "ongleet";
#[allow(dead_code)] // Only GUI startup migrates files; clients remain read-only.
pub const LEGACY_ID: &str = "io.local.terminal-stack-prototype";

#[allow(dead_code)]
pub fn config_directory() -> Result<PathBuf, String> {
    let base = match std::env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        Some(directory) => PathBuf::from(directory),
        None => {
            PathBuf::from(std::env::var_os("HOME").ok_or("HOME is unavailable")?).join(".config")
        }
    };
    if !base.is_absolute() {
        return Err("Application config directory must be absolute".into());
    }
    Ok(base.join(APPLICATION_ID))
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
    Ok(runtime.join(format!("ongleet-{hash:016x}.sock")))
}

impl super::super::SystemPaths for super::Linux {
    fn config_directory(&self) -> Result<PathBuf, String> {
        config_directory()
    }
    fn data_directory(&self, identifier: &str) -> Result<PathBuf, String> {
        data_directory(identifier)
    }
    fn control_endpoint(&self, data: &Path) -> Result<super::super::ControlEndpoint, String> {
        socket_path(data).map(|path| super::super::ControlEndpoint(std::sync::Arc::new(path)))
    }
    fn executable(&self) -> Result<PathBuf, String> {
        super::containment::executable()
    }
    fn application_executable(&self) -> Result<PathBuf, String> {
        std::env::current_exe().map_err(|e| e.to_string())
    }
    fn sibling_executable(&self, name: &str) -> Result<PathBuf, String> {
        Ok(self
            .application_executable()?
            .parent()
            .ok_or("Application directory is unavailable")?
            .join(name))
    }
    fn default_shell(&self) -> String {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into())
    }
}
