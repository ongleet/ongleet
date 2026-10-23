use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct TabConfig {
    pub cwd_override: Option<String>,
    pub executable: Option<String>,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub shell_command: Option<String>,
}

impl TabConfig {
    pub fn validate(&self) -> Result<(), String> {
        let config = self;
        if config.shell_command.is_some()
            && (config.executable.is_some() || !config.args.is_empty())
        {
            return Err("Shell-command mode cannot also specify an executable or arguments".into());
        }
        if config
            .cwd_override
            .as_ref()
            .is_some_and(|p| !std::path::Path::new(p).is_absolute())
        {
            return Err("Working directory override must be absolute".into());
        }
        if config
            .shell_command
            .as_ref()
            .is_some_and(|c| c.trim().is_empty())
        {
            return Err("Shell command cannot be empty".into());
        }
        if config
            .executable
            .as_ref()
            .is_some_and(|e| e.trim().is_empty())
        {
            return Err("Executable cannot be empty".into());
        }
        if config
            .env
            .iter()
            .any(|(key, value)| key.is_empty() || key.contains(['=', '\0']) || value.contains('\0'))
        {
            return Err("Invalid environment override".into());
        }
        if config.args.iter().any(|arg| arg.contains('\0'))
            || config.executable.as_ref().is_some_and(|s| s.contains('\0'))
            || config
                .shell_command
                .as_ref()
                .is_some_and(|s| s.contains('\0'))
        {
            return Err("Commands and arguments cannot contain NUL bytes".into());
        }
        Ok(())
    }
}
