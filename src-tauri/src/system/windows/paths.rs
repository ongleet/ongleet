use super::{
    super::{ControlEndpoint, SystemPaths},
    native::*,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

pub struct Image(Result<Vec<(PathBuf, Option<super::files::Identity>)>, String>);
impl Image {
    pub fn capture() -> Self {
        Self((|| {
            let current = std::env::current_exe().map_err(|e| e.to_string())?;
            #[cfg(test)]
            let current = current
                .parent()
                .and_then(Path::parent)
                .ok_or("Test binary directory unavailable")?
                .join("terminal-pty-helper.exe");
            let directory = current.parent().ok_or("Executable directory unavailable")?;
            let mut entries = vec![(
                current.clone(),
                Some(super::files::identity(&current).map_err(|e| e.to_string())?),
            )];
            entries.extend(
                ["ongleet-gui.exe", "ongleet.exe", "ong-codex.exe"].map(|name| {
                    let path = directory.join(name);
                    let identity = super::files::identity(&path).ok();
                    (path, identity)
                }),
            );
            Ok(entries)
        })())
    }
    pub fn launch_guard(&self, executable: &Path) -> Result<Vec<std::fs::File>, String> {
        use std::os::windows::fs::OpenOptionsExt;
        let entries = self.0.as_ref().map_err(Clone::clone)?;
        let bundled = entries
            .iter()
            .any(|(path, _)| std::fs::canonicalize(path).is_ok_and(|path| path == executable));
        if !bundled {
            return Ok(Vec::new());
        }
        let mut guards = Vec::new();
        for (path, original) in entries {
            match original {
                Some(original) => {
                    // Deny writes and replacement while CreateProcess binds the
                    // verified image. No inheritable file handles reach children.
                    let file = std::fs::OpenOptions::new()
                        .read(true)
                        .share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ)
                        .open(path)
                        .map_err(|e| {
                            format!("Cannot guard application image; restart Ongleet: {e}")
                        })?;
                    if super::files::file_identity(&file).map_err(|e| e.to_string())? != *original {
                        return Err("Application executable was replaced; restart Ongleet before launching helpers".into());
                    }
                    guards.push(file);
                }
                None if path.exists() => {
                    return Err(
                        "Application executables changed; restart Ongleet before launching helpers"
                            .into(),
                    )
                }
                None => (),
            }
        }
        Ok(guards)
    }
    pub fn bootstrap(&self) -> Result<PathBuf, String> {
        let current = self.verified()?;
        let entries = self.0.as_ref().map_err(Clone::clone)?;
        if let Some((path, _)) = entries.iter().find(|(path, identity)| {
            identity.is_some()
                && path
                    .file_name()
                    .is_some_and(|name| name.eq_ignore_ascii_case("ong-codex.exe"))
        }) {
            return Ok(path.clone());
        }
        if current
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("ongleet-gui.exe"))
        {
            return Err("The bundled ong-codex.exe console helper is missing".into());
        }
        Ok(current)
    }
    pub fn verified(&self) -> Result<PathBuf, String> {
        let entries = self.0.as_ref().map_err(Clone::clone)?;
        if entries
            .iter()
            .any(|(path, identity)| super::files::identity(path).ok() != *identity)
        {
            return Err(
                "Application executables were replaced; restart Ongleet before launching helpers"
                    .into(),
            );
        }
        Ok(entries[0].0.clone())
    }
}
fn directory(xdg: &str, fallback: &str, identifier: &str) -> Result<PathBuf, String> {
    let (base, name) = if let Some(base) = std::env::var_os(xdg).filter(|s| !s.is_empty()) {
        (PathBuf::from(base), identifier)
    } else {
        (
            PathBuf::from(
                std::env::var_os(fallback).ok_or_else(|| format!("{fallback} is unavailable"))?,
            ),
            if identifier == "ongleet" {
                "Ongleet"
            } else {
                identifier
            },
        )
    };
    if !base.is_absolute() {
        return Err("Application directories must be absolute".into());
    }
    Ok(base.join(name))
}
pub fn powershell() -> String {
    PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()))
        .join("System32/WindowsPowerShell/v1.0/powershell.exe")
        .to_string_lossy()
        .into_owned()
}
pub fn ps_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
pub fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}
impl SystemPaths for super::Windows {
    fn config_directory(&self) -> Result<PathBuf, String> {
        directory("XDG_CONFIG_HOME", "APPDATA", "ongleet")
    }
    fn data_directory(&self, id: &str) -> Result<PathBuf, String> {
        directory("XDG_DATA_HOME", "LOCALAPPDATA", id)
    }
    fn control_endpoint(&self, data: &Path) -> Result<ControlEndpoint, String> {
        if !data.is_absolute() {
            return Err("Data directory must be absolute".into());
        }
        let identity = current_identity().map_err(|e| e.to_string())?;
        let key = format!("{}:{:?}:{}", identity.sid, identity.logon, data.display());
        Ok(ControlEndpoint(Arc::new(format!(
            r"\\.\pipe\ongleet-{:016x}",
            hash(key.as_bytes())
        ))))
    }
    fn executable(&self) -> Result<PathBuf, String> {
        self.image.bootstrap()
    }
    fn application_executable(&self) -> Result<PathBuf, String> {
        self.image.verified()
    }
    fn sibling_executable(&self, name: &str) -> Result<PathBuf, String> {
        if !matches!(name, "ongleet-gui" | "ongleet" | "ong-codex") {
            return Err("Unknown bundled executable".into());
        }
        Ok(self
            .image
            .verified()?
            .parent()
            .ok_or("Executable directory unavailable")?
            .join(format!("{name}.exe")))
    }
    fn merge_environment(
        &self,
        base: &mut std::collections::BTreeMap<String, String>,
        overrides: std::collections::BTreeMap<String, String>,
    ) {
        for (key, value) in overrides {
            base.retain(|existing, _| {
                environment_order(existing.as_ref(), key.as_ref()) != std::cmp::Ordering::Equal
            });
            base.insert(key, value);
        }
    }
    fn quote_shell_argument(&self, value: &str) -> String {
        ps_quote(value)
    }
    fn shell_action_seed(&self) -> Option<(&'static str, String)> {
        Some(("PowerShell", powershell()))
    }
    fn default_shell(&self) -> String {
        powershell()
    }
    fn shell_command(&self, script: &str) -> (String, Vec<String>) {
        (
            powershell(),
            vec![
                "-NoLogo".into(),
                "-NoProfile".into(),
                "-Command".into(),
                script.into(),
            ],
        )
    }
    fn hook_command(&self, executable: &str, args: &[&str]) -> Result<String, String> {
        // Native Codex hooks use the configured Windows shell (PowerShell).
        for value in std::iter::once(executable).chain(args.iter().copied()) {
            wide(value).map_err(|e| e.to_string())?;
        }
        // Redirect the native child's input explicitly. Inheriting stdin from
        // PowerShell can bind a console handle instead of the hook payload pipe.
        // CopyTo streams raw UTF-8 bytes without line buffering or re-encoding.
        let arguments = args
            .iter()
            .map(|arg| super::process::quote(arg))
            .collect::<Vec<_>>()
            .join(" ");
        Ok(format!("$ErrorActionPreference='Stop';$s=[Diagnostics.ProcessStartInfo]::new();$s.UseShellExecute=$false;$s.CreateNoWindow=$true;$s.RedirectStandardInput=$true;$s.StandardInputEncoding=[Text.UTF8Encoding]::new($false);$s.FileName={};$s.Arguments={};$p=[Diagnostics.Process]::Start($s);[Console]::OpenStandardInput().CopyTo($p.StandardInput.BaseStream);$p.StandardInput.BaseStream.Close();$p.WaitForExit();exit $p.ExitCode", ps_quote(executable),ps_quote(&arguments)))
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::SystemFiles;
    use super::*;
    #[test]
    fn replaced_or_new_sibling_images_require_restart_before_helpers() {
        let native = super::super::Windows::new();
        let root = std::env::temp_dir().join(format!("ongleet-image-{}", native.nonce().unwrap()));
        native.private_directory(&root, true).unwrap();
        let current = root.join("ongleet-gui.exe");
        let helper = root.join("ong-codex.exe");
        let cli = root.join("ongleet.exe");
        std::fs::write(&current, b"original GUI").unwrap();
        std::fs::write(&helper, b"original CLI").unwrap();
        let capture = || {
            Image(Ok(vec![
                (
                    current.clone(),
                    Some(super::super::files::identity(&current).unwrap()),
                ),
                (
                    helper.clone(),
                    Some(super::super::files::identity(&helper).unwrap()),
                ),
                (cli.clone(), super::super::files::identity(&cli).ok()),
            ]))
        };
        let image = capture();
        assert_eq!(image.verified().unwrap(), current);
        let replacement = root.join("replacement");
        std::fs::write(&replacement, b"changed helper").unwrap();
        let guards = image
            .launch_guard(&std::fs::canonicalize(&current).unwrap())
            .unwrap();
        assert!(native.atomic_replace(&replacement, &helper).is_err());
        drop(guards);
        native.atomic_replace(&replacement, &helper).unwrap();
        assert!(image.verified().unwrap_err().contains("restart"));
        let image = capture();
        std::fs::write(&replacement, b"changed GUI").unwrap();
        native.atomic_replace(&replacement, &current).unwrap();
        assert!(image.verified().is_err());
        let image = capture();
        std::fs::write(&cli, b"added sibling").unwrap();
        assert!(image.verified().is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
