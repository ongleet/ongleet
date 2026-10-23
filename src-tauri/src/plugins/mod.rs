//! Trusted built-in plugins. Native byte transport remains terminal-specific.
mod catalog;
pub use catalog::catalog;
pub mod commands;
pub mod files;
pub mod pull_request;
pub mod terminal;

pub fn validate(kind: &str, version: u32, config: &serde_json::Value) -> Result<(), String> {
    if version != 1 {
        return Err(format!(
            "Unsupported {kind} configuration version {version}"
        ));
    }
    if serde_json::to_vec(config).map_err(|e| e.to_string())?.len() > 65_536 {
        return Err("Plugin configuration exceeds 64 KiB".into());
    }
    match kind {
        "terminal" => serde_json::from_value::<terminal::TabConfig>(config.clone())
            .map_err(|e| e.to_string())?
            .validate(),
        "files" => serde_json::from_value::<files::Config>(config.clone())
            .map_err(|e| e.to_string())?
            .validate(),
        "pull-request" => pull_request::validate(config),
        _ => Err(format!("Plugin unavailable: {kind}")),
    }
}

#[derive(Default)]
pub struct Job {
    pub cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub finished: std::sync::atomic::AtomicBool,
}

/// Resolve per-plugin defaults when copying an action into a durable tab.
pub fn initialize(
    kind: &str,
    config: serde_json::Value,
    directory: &str,
) -> Result<serde_json::Value, String> {
    match kind {
        "files" => files::initialize(config, directory),
        _ => Ok(config),
    }
}

pub fn available(kind: &str, version: u32) -> bool {
    version == 1 && matches!(kind, "terminal" | "files" | "pull-request")
}
