use crate::workspace::TabConfig;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    io::{Read, Write},
    path::Path,
};

pub mod external;

const DEFAULTS: &str = include_str!("../../config/actions.yaml");
const MAX_BYTES: u64 = 65_536;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    pub actions: Vec<Action>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Action {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub config: TabConfig,
}

pub fn parse(text: &str) -> Result<Vec<Action>, String> {
    Ok(parse_config(text)?.actions)
}

pub fn parse_config(text: &str) -> Result<Config, String> {
    if text.len() as u64 > MAX_BYTES {
        return Err("Configuration exceeds 64 KiB".into());
    }
    let config: Config = serde_yaml_ng::from_str(text).map_err(|e| e.to_string())?;
    if config.actions.is_empty() || config.actions.len() > 100 {
        return Err("Expected between 1 and 100 actions".into());
    }
    TabConfig {
        env: config.env.clone(),
        ..Default::default()
    }
    .validate()?;
    let mut ids = HashSet::new();
    for template in &config.actions {
        if template.id.trim().is_empty()
            || template.name.trim().is_empty()
            || !ids.insert(&template.id)
        {
            return Err("Actions require unique nonempty IDs and nonempty names".into());
        }
        if !matches!(template.kind.as_str(), "terminal" | "command") {
            return Err(format!("Unsupported action kind: {}", template.kind));
        }
        let launch = &template.config;
        let mut validation = launch.clone();
        if template.kind == "command"
            && validation
                .cwd_override
                .as_ref()
                .is_some_and(|s| s.contains("{{"))
        {
            // The resolved path is validated again immediately before launch.
            validation.cwd_override = None;
        }
        validation.validate()?;
        if launch.shell_command.is_some() {
            if launch.executable.is_some()
                || !launch.args.is_empty()
                || launch.shell_command.as_ref().unwrap().trim().is_empty()
            {
                return Err(
                    "Shell command requires nonempty shellCommand and no executable/args".into(),
                );
            }
        } else if launch
            .executable
            .as_ref()
            .is_none_or(|s| s.trim().is_empty())
        {
            return Err("Terminal type requires an executable or explicit shellCommand".into());
        }
    }
    Ok(config)
}

// Reload for each menu opening and launch. Never overwrite an invalid user file.
#[cfg(test)]
pub fn load(directory: &Path) -> Result<Vec<Action>, String> {
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let path = directory.join("actions.yaml");
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(mut file) => file
            .write_all(DEFAULTS.as_bytes())
            .map_err(|e| e.to_string())?,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.to_string()),
    }
    let result = (|| {
        let mut text = String::new();
        std::fs::File::open(&path)
            .map_err(|e| e.to_string())?
            .take(MAX_BYTES + 1)
            .read_to_string(&mut text)
            .map_err(|e| e.to_string())?;
        if text.len() as u64 > MAX_BYTES {
            return Err("Configuration exceeds 64 KiB".into());
        }
        parse(&text)
    })();
    result.map_err(|e| format!("{}: {e}", path.display()))
}

pub fn workgroup_path(directory: &Path, id: u64) -> std::path::PathBuf {
    directory
        .join("workgroups")
        .join(id.to_string())
        .join("actions.yaml")
}

pub fn load_workgroup(directory: &Path, id: u64) -> Result<Vec<Action>, String> {
    let path = workgroup_path(directory, id);
    let result = (|| {
        if !path.exists() {
            let seed = DEFAULTS;
            std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut file) => file.write_all(seed.as_bytes()).map_err(|e| e.to_string())?,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        parse(&read_config(&path)?)
    })();
    result.map_err(|error| format!("{}: {error}", path.display()))
}

pub fn format(config: Config) -> Result<String, String> {
    let yaml = serde_yaml_ng::to_string(&config).map_err(|e| e.to_string())?;
    parse(&yaml)?;
    Ok(yaml)
}

pub fn read_workgroup_yaml(directory: &Path, id: u64) -> Result<String, String> {
    let path = workgroup_path(directory, id);
    if !path.exists() {
        load_workgroup(directory, id)?;
    }
    // Read existing malformed YAML too, so the editor can repair it.
    read_config(&path).map_err(|error| format!("{}: {error}", path.display()))
}

pub fn save_workgroup_yaml(directory: &Path, id: u64, text: &str) -> Result<(), String> {
    if text.len() as u64 > MAX_BYTES {
        return Err("Configuration exceeds 64 KiB".into());
    }
    parse(text)?;
    let path = workgroup_path(directory, id);
    let parent = path.parent().unwrap();
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let temporary = parent.join(format!(".actions-{}-{stamp}.tmp", std::process::id()));
    let result = (|| {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        std::fs::rename(&temporary, &path).map_err(|e| e.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map_err(|error: String| format!("{}: {error}", path.display()))
}

fn read_config(path: &Path) -> Result<String, String> {
    let mut text = String::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(MAX_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    if text.len() as u64 > MAX_BYTES {
        return Err("Configuration exceeds 64 KiB".into());
    }
    Ok(text)
}

#[cfg(test)]
mod tests;
