use crate::workspace::TabConfig;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    io::{Read, Write},
    path::Path,
};

const DEFAULTS: &str = include_str!("../../config/tab-types.yaml");
const MAX_BYTES: u64 = 65_536;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    tab_types: Vec<TabType>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TabType {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub config: TabConfig,
}

fn parse(text: &str) -> Result<Vec<TabType>, String> {
    let config: Config = serde_yaml_ng::from_str(text).map_err(|e| e.to_string())?;
    if config.tab_types.is_empty() || config.tab_types.len() > 100 {
        return Err("Expected between 1 and 100 tab types".into());
    }
    let mut ids = HashSet::new();
    for template in &config.tab_types {
        if template.id.trim().is_empty()
            || template.name.trim().is_empty()
            || !ids.insert(&template.id)
        {
            return Err("Tab types require unique nonempty IDs and nonempty names".into());
        }
        if template.kind != "terminal" {
            return Err(format!("Unsupported tab kind: {}", template.kind));
        }
        let launch = &template.config;
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
    Ok(config.tab_types)
}

// Reload for each menu opening and launch. Never overwrite an invalid user file.
#[cfg(test)]
pub fn load(directory: &Path) -> Result<Vec<TabType>, String> {
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let path = directory.join("tab-types.yaml");
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

pub fn load_workgroup(directory: &Path, id: u64) -> Result<Vec<TabType>, String> {
    let path = workgroup_path(directory, id);
    let result = (|| {
        if !path.exists() {
            // Preserve the existing global choices when introducing workgroups.
            let legacy = directory.join("tab-types.yaml");
            let seed = if legacy.exists() {
                read_config(&legacy)?
            } else {
                DEFAULTS.into()
            };
            parse(&seed)?;
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
