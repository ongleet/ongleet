use crate::session::TabConfig;
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
    #[serde(default = "default_config_version", rename = "configVersion")]
    pub config_version: u32,
    pub config: serde_json::Value,
}

fn default_config_version() -> u32 {
    1
}

pub fn parse(text: &str) -> Result<Vec<Action>, String> {
    Ok(parse_config(text)?.actions)
}

pub fn parse_config(text: &str) -> Result<Config, String> {
    parse_config_policy(text, false)
}

// Missing plugin actions remain opaque when reading existing project files.
// Saving edited YAML still requires all configurations to be validated.
pub fn parse_project_config(text: &str) -> Result<Config, String> {
    parse_config_policy(text, true)
}

fn parse_config_policy(text: &str, allow_unavailable: bool) -> Result<Config, String> {
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
            || template.kind.trim().is_empty()
            || !ids.insert(&template.id)
        {
            return Err("Actions require unique nonempty IDs and nonempty names".into());
        }
        if allow_unavailable
            && template.kind != "command"
            && !crate::plugins::available(&template.kind, template.config_version)
        {
            continue;
        }
        if template.kind != "command" {
            crate::plugins::validate(&template.kind, template.config_version, &template.config)?;
            if !crate::plugins::has_runtime(&template.kind) {
                continue;
            }
        }
        let launch: TabConfig =
            serde_json::from_value(template.config.clone()).map_err(|e| e.to_string())?;
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

/// Called only while holding the application metadata writer lock.
/// Rename the complete tree so custom files and stable project IDs survive.
pub fn migrate_projects(directory: &Path) -> Result<(), String> {
    let source = directory.join("workgroups");
    let target = directory.join("projects");
    match std::fs::symlink_metadata(&source) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
        Ok(metadata) if !metadata.is_dir() => {
            return Err("Legacy workgroups configuration is not a directory".into())
        }
        Ok(_) => {}
    }
    if target.try_exists().map_err(|e| e.to_string())? {
        return Err("Both workgroups and projects configuration directories exist; resolve the conflict explicitly. Neither was changed".into());
    }
    std::fs::rename(&source, &target)
        .map_err(|e| format!("Cannot migrate project configuration: {e}"))
}

pub fn project_path(directory: &Path, id: u64) -> std::path::PathBuf {
    directory
        .join("projects")
        .join(id.to_string())
        .join("actions.yaml")
}

pub fn load_project(directory: &Path, id: u64) -> Result<Vec<Action>, String> {
    let path = project_path(directory, id);
    let result = (|| {
        if !path.exists() {
            let seed = read_default_yaml(directory)?;
            parse_project_config(&seed)?;
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
        parse_project_config(&read_config(&path)?).map(|config| config.actions)
    })();
    result.map_err(|error| format!("{}: {error}", path.display()))
}

pub fn format(config: Config) -> Result<String, String> {
    let yaml = serde_yaml_ng::to_string(&config).map_err(|e| e.to_string())?;
    parse(&yaml)?;
    Ok(yaml)
}

pub fn read_project_yaml(directory: &Path, id: u64) -> Result<String, String> {
    let path = project_path(directory, id);
    if !path.exists() {
        load_project(directory, id)?;
    }
    // Read existing malformed YAML too, so the editor can repair it.
    read_config(&path).map_err(|error| format!("{}: {error}", path.display()))
}

pub fn save_project_yaml(directory: &Path, id: u64, text: &str) -> Result<(), String> {
    save_yaml(&project_path(directory, id), text)
}

pub fn read_default_yaml(directory: &Path) -> Result<String, String> {
    let path = directory.join("actions.yaml");
    if !path.try_exists().map_err(|e| e.to_string())? {
        save_yaml(&path, DEFAULTS)?;
    }
    read_config(&path)
}

pub fn save_default_yaml(directory: &Path, text: &str) -> Result<(), String> {
    save_yaml(&directory.join("actions.yaml"), text)
}

fn save_yaml(path: &Path, text: &str) -> Result<(), String> {
    parse(text)?;
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

#[cfg(test)]
impl Action {
    pub fn terminal_config(&self) -> TabConfig {
        serde_json::from_value(self.config.clone()).unwrap()
    }
}
