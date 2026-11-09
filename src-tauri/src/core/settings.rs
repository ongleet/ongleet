//! Bounded application-wide plugin defaults. Credentials stay in SecretStore.
use crate::{session::Namespace, system::SystemServices};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
};

fn load(path: &std::path::Path) -> Result<BTreeMap<String, Namespace>, String> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => return Err(error.to_string()),
    };
    let mut bytes = Vec::new();
    file.take(65_537)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 65_536 {
        return Err("Plugin settings exceed 64 KiB".into());
    }
    serde_yaml_ng::from_slice(&bytes).map_err(|e| e.to_string())
}

pub fn read(system: &SystemServices, plugin: &str) -> Result<Option<Namespace>, String> {
    Ok(load(
        &system
            .paths
            .config_directory()?
            .join("plugin-settings.yaml"),
    )?
    .remove(plugin))
}

pub fn replace(
    system: &SystemServices,
    plugin: &str,
    revision: u64,
    version: u32,
    value: serde_json::Value,
) -> Result<Namespace, String> {
    replace_with(
        system.files.as_ref(),
        &system.paths.config_directory()?,
        plugin,
        revision,
        version,
        value,
    )
}

fn replace_with(
    files: &dyn crate::system::SystemFiles,
    directory: &std::path::Path,
    plugin: &str,
    revision: u64,
    version: u32,
    value: serde_json::Value,
) -> Result<Namespace, String> {
    if version == 0 || !value.is_object() {
        return Err("Invalid plugin settings".into());
    }
    files
        .private_directory(&directory, true)
        .map_err(|e| e.to_string())?;
    let _lock = files.try_lock(&directory.join("plugin-settings.lock"))?;
    let path = directory.join("plugin-settings.yaml");
    let mut settings = load(&path)?;
    if settings.get(plugin).map_or(0, |n| n.revision) != revision {
        return Err("revision_conflict: Plugin settings changed; reload before saving".into());
    }
    let namespace = Namespace {
        version,
        revision: revision.checked_add(1).ok_or("Revision overflow")?,
        value,
    };
    settings.insert(plugin.into(), namespace.clone());
    let text = serde_yaml_ng::to_string(&settings).map_err(|e| e.to_string())?;
    if text.len() > 65_536 {
        return Err("Plugin settings exceed 64 KiB".into());
    }
    let temporary = directory.join(format!(".plugin-settings-{}.tmp", files.nonce()?));
    let result = (|| {
        let mut file = files
            .open_private(&temporary, crate::system::PrivateOpen::New)
            .map_err(|e| e.to_string())?;
        file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        files
            .atomic_replace(&temporary, &path)
            .map_err(|e| e.to_string())?;
        files.sync_directory(&directory).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result?;
    Ok(namespace)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn global_settings_retain_siblings_reject_stale_writes_and_preserve_invalid_files() {
        let system = crate::system::services();
        let directory = std::env::temp_dir().join(format!(
            "ongleet-settings-{}",
            system.files.nonce().unwrap()
        ));
        let files = system.files.as_ref();
        let write =
            |plugin, revision, value| replace_with(files, &directory, plugin, revision, 1, value);
        let path = directory.join("plugin-settings.yaml");
        assert!(load(&path).unwrap().is_empty());
        write("files", 0, json!({"showHidden": true})).unwrap();
        write("pull-request", 0, json!({"refreshIntervalSeconds": 300})).unwrap();
        assert!(write("files", 0, json!({"showHidden": false}))
            .unwrap_err()
            .contains("revision_conflict"));
        assert!(write("files", 1, json!({"large": "x".repeat(65_536)}))
            .unwrap_err()
            .contains("64 KiB"));
        let settings = load(&path).unwrap();
        assert_eq!(settings["files"].value, json!({"showHidden": true}));
        assert_eq!(
            settings["pull-request"].value,
            json!({"refreshIntervalSeconds": 300})
        );
        write("files", 1, json!({})).unwrap();
        assert_eq!(load(&path).unwrap()["files"].revision, 2);
        std::fs::write(&path, "invalid: [").unwrap();
        assert!(write("files", 2, json!({})).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "invalid: [");
        std::fs::remove_dir_all(directory).unwrap();
    }
}
