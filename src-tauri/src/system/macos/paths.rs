use super::super::{ControlEndpoint, SystemPaths};
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};

// Capture executable identities before any helper launch. A replaced bundle
// requires an application restart; Darwin cannot re-exec Linux's live /proc image.
#[derive(PartialEq, Eq)]
struct FileIdentity {
    device: u64,
    inode: u64,
    size: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}
fn file_identity(path: &Path) -> Result<FileIdentity, String> {
    let m = fs::metadata(path).map_err(|e| e.to_string())?;
    if !m.is_file() {
        return Err("Bundled executable is not a regular file".into());
    }
    Ok(FileIdentity {
        device: m.dev(),
        inode: m.ino(),
        size: m.len(),
        modified: (m.mtime(), m.mtime_nsec()),
        changed: (m.ctime(), m.ctime_nsec()),
    })
}
pub struct Image(Result<(PathBuf, FileIdentity, Vec<(PathBuf, Option<FileIdentity>)>), String>);
impl Image {
    pub fn capture() -> Self {
        let path = std::env::current_exe().map_err(|e| e.to_string());
        #[cfg(test)]
        let path = path.and_then(|path| {
            Ok(path
                .parent()
                .and_then(Path::parent)
                .ok_or("Test binary directory unavailable")?
                .join("terminal-pty-helper"))
        });
        Self::at_path(path)
    }
    fn at_path(path: Result<PathBuf, String>) -> Self {
        Self((|| {
            let path = path?;
            let identity = file_identity(&path)?;
            let directory = path.parent().ok_or("Application directory unavailable")?;
            let siblings = ["ongleet-gui", "ongleet", "ong-codex"]
                .into_iter()
                .map(|name| {
                    let path = directory.join(name);
                    let identity = file_identity(&path).ok();
                    (path, identity)
                })
                .collect();
            Ok((path, identity, siblings))
        })())
    }
    pub fn verified(&self) -> Result<PathBuf, String> {
        let (path, original, siblings) = self.0.as_ref().map_err(Clone::clone)?;
        if file_identity(path)? != *original
            || siblings
                .iter()
                .any(|(path, original)| file_identity(path).ok() != *original)
        {
            return Err(
                "Application bundle was replaced; restart Ongleet before launching helpers".into(),
            );
        }
        Ok(path.clone())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacement_of_running_image_or_sibling_refuses_helpers() {
        let root = std::env::temp_dir().join(format!("ongleet-image-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("ongleet-gui");
        fs::write(&path, "original").unwrap();
        let sibling = root.join("ongleet");
        fs::write(&sibling, "CLI").unwrap();
        let image = Image::at_path(Ok(path.clone()));
        assert_eq!(image.verified().unwrap(), path);
        let replacement = root.join("replacement");
        fs::write(&replacement, "new CLI").unwrap();
        fs::rename(&replacement, &sibling).unwrap();
        assert!(image.verified().unwrap_err().contains("replaced"));
        let image = Image::at_path(Ok(path.clone()));
        fs::write(&replacement, "new GUI").unwrap();
        fs::rename(&replacement, &path).unwrap();
        assert!(image.verified().unwrap_err().contains("replaced"));
        fs::remove_dir_all(root).unwrap();
    }
}
fn directory(variable: &str, fallback: &str, identifier: &str) -> Result<PathBuf, String> {
    let path = if let Some(base) = std::env::var_os(variable).filter(|s| !s.is_empty()) {
        PathBuf::from(base).join(identifier)
    } else {
        let home = std::env::var_os("HOME").ok_or("HOME is unavailable")?;
        PathBuf::from(home)
            .join("Library")
            .join(fallback)
            .join(if identifier == "ongleet" {
                "Ongleet"
            } else {
                identifier
            })
    };
    if !path.is_absolute() {
        return Err("Application directory must be absolute".into());
    }
    Ok(path)
}
pub fn runtime() -> Result<PathBuf, String> {
    let base = std::env::temp_dir();
    let path = base.join(format!("ongleet-{}", unsafe { libc::geteuid() }));
    use std::os::unix::fs::DirBuilderExt;
    match fs::DirBuilder::new().mode(0o700).create(&path) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.to_string()),
    }
    let m = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
    if !m.is_dir() || m.uid() != unsafe { libc::geteuid() } || m.permissions().mode() & 0o077 != 0 {
        return Err("Runtime directory must be private and owned by the current user".into());
    }
    Ok(path)
}
impl SystemPaths for super::Macos {
    fn shell_command(&self, script: &str) -> (String, Vec<String>) {
        (self.default_shell(), vec!["-lc".into(), script.into()])
    }
    fn hook_command(&self, executable: &str, args: &[&str]) -> Result<String, String> {
        if std::iter::once(executable)
            .chain(args.iter().copied())
            .any(|s| s.contains('\0'))
        {
            return Err("Hook command contains a NUL byte".into());
        }
        Ok(std::iter::once(executable)
            .chain(args.iter().copied())
            .map(|s| format!("'{}'", s.replace('\'', "'\\''")))
            .collect::<Vec<_>>()
            .join(" "))
    }

    fn config_directory(&self) -> Result<PathBuf, String> {
        directory("XDG_CONFIG_HOME", "Preferences", "ongleet")
    }
    fn data_directory(&self, id: &str) -> Result<PathBuf, String> {
        directory("XDG_DATA_HOME", "Application Support", id)
    }
    fn control_endpoint(&self, data: &Path) -> Result<ControlEndpoint, String> {
        let hash = data
            .as_os_str()
            .as_encoded_bytes()
            .iter()
            .fold(0xcbf29ce484222325u64, |h, b| {
                (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
            });
        let path = runtime()?.join(format!("{hash:016x}.sock"));
        if path.as_os_str().as_encoded_bytes().len() >= 104 {
            return Err("macOS runtime socket path is too long".into());
        }
        Ok(ControlEndpoint(std::sync::Arc::new(path)))
    }
    fn executable(&self) -> Result<PathBuf, String> {
        self.image.verified()
    }
    fn application_executable(&self) -> Result<PathBuf, String> {
        self.image.verified()
    }
    fn sibling_executable(&self, name: &str) -> Result<PathBuf, String> {
        if !matches!(name, "ongleet" | "ongleet-gui" | "ong-codex") {
            return Err("Unknown bundled executable".into());
        }
        Ok(self
            .image
            .verified()?
            .parent()
            .ok_or("Application directory unavailable")?
            .join(name))
    }
    fn default_shell(&self) -> String {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into())
    }
}
