//! Explicitly launched desktop commands are independent of managed PTY sessions.
use super::Action;
use crate::workspace::{TabConfig, WorkspaceRegistry};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

static CHILDREN: AtomicUsize = AtomicUsize::new(0);
const MAX_CHILDREN: usize = 64;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Exit {
    pub workspace_id: u64,
    pub name: String,
    pub code: Option<i32>,
    pub error: Option<String>,
}

// One pass: replacement values (including braces or shell metacharacters) stay literal.
fn expand(text: &str, values: &BTreeMap<&str, String>) -> Result<String, String> {
    let mut rest = text;
    let mut result = String::new();
    while let Some(start) = rest.find("{{") {
        result.push_str(&rest[..start]);
        let tail = &rest[start + 2..];
        let end = tail.find("}}").ok_or("Unclosed action placeholder")?;
        let key = tail[..end].trim();
        result.push_str(
            values
                .get(key)
                .ok_or_else(|| format!("Unknown action placeholder: {key}"))?,
        );
        rest = &tail[end + 2..];
    }
    result.push_str(rest);
    Ok(result)
}

pub struct Prepared {
    pub config: TabConfig,
    pub name: String,
    pub workspace_id: u64,
}

impl WorkspaceRegistry {
    pub fn prepare_external(
        &self,
        workspace_id: u64,
        action_id: &str,
        directory: &std::path::Path,
    ) -> Result<Prepared, String> {
        let snapshot = self.snapshot();
        let workspace = snapshot
            .workspaces
            .iter()
            .find(|w| w.id == workspace_id)
            .ok_or("Workspace is unavailable")?;
        let group = self.workgroup(workspace.workgroup_id)?;
        let yaml = super::read_workgroup_yaml(directory, group.id)?;
        let settings = super::parse_config(&yaml)?;
        let Action {
            name,
            kind,
            mut config,
            ..
        } = settings
            .actions
            .into_iter()
            .find(|a| a.id == action_id)
            .ok_or("Action is unavailable")?;
        if kind != "command" {
            return Err("This action opens a terminal tab".into());
        }
        let values = BTreeMap::from([
            ("workspace.path", workspace.base_directory.clone()),
            ("workspace.name", workspace.name.clone()),
            ("workspace.id", workspace.id.to_string()),
            ("workgroup.path", group.root_directory.clone()),
            ("workgroup.name", group.name.clone()),
            ("workgroup.id", group.id.to_string()),
        ]);
        if config
            .shell_command
            .as_ref()
            .is_some_and(|s| s.contains("{{"))
        {
            return Err("Use quoted $WORKSPACE_PATH / $WORKGROUP_PATH environment variables in shell commands, not placeholders".into());
        }
        config.executable = config
            .executable
            .as_deref()
            .map(|s| expand(s, &values))
            .transpose()?;
        config.args = config
            .args
            .iter()
            .map(|s| expand(s, &values))
            .collect::<Result<_, _>>()?;
        config.cwd_override = Some(expand(
            config
                .cwd_override
                .as_deref()
                .unwrap_or("{{workspace.path}}"),
            &values,
        )?);
        let mut env = settings.env;
        env.extend(config.env);
        config.env = env
            .into_iter()
            .map(|(key, value)| Ok((key, expand(&value, &values)?)))
            .collect::<Result<_, String>>()?;
        config
            .env
            .insert("WORKSPACE_PATH".into(), workspace.base_directory.clone());
        config
            .env
            .insert("WORKSPACE_NAME".into(), workspace.name.clone());
        config
            .env
            .insert("WORKGROUP_PATH".into(), group.root_directory.clone());
        config
            .env
            .insert("WORKGROUP_NAME".into(), group.name.clone());
        config.validate()?;
        Ok(Prepared {
            config,
            name,
            workspace_id,
        })
    }
}

pub fn launch(prepared: Prepared, exited: Arc<dyn Fn(Exit) + Send + Sync>) -> Result<u32, String> {
    use std::os::unix::process::CommandExt;
    if CHILDREN
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
            (n < MAX_CHILDREN).then_some(n + 1)
        })
        .is_err()
    {
        return Err("Too many external commands are still running (limit 64)".into());
    }
    struct Slot;
    impl Drop for Slot {
        fn drop(&mut self) {
            CHILDREN.fetch_sub(1, Ordering::AcqRel);
        }
    }
    let slot = Slot;
    let config = prepared.config;
    let cwd = config
        .cwd_override
        .ok_or("External action directory is unavailable")?;
    if !std::path::Path::new(&cwd).is_dir() {
        return Err("Working directory must be an existing directory".into());
    }
    let mut command = if let Some(script) = config.shell_command {
        let mut command = Command::new(std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into()));
        command.args(["-lc", &script]);
        command
    } else {
        let mut command = Command::new(config.executable.ok_or("Executable is required")?);
        command.args(config.args);
        command
    };
    command
        .current_dir(cwd)
        .envs(config.env)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // External applications intentionally outlive this app and its PTY cgroups.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("Unable to launch {}: {e}", prepared.name))?;
    let pid = child.id();
    std::thread::spawn(move || {
        let _slot = slot;
        let (code, error) = match child.wait() {
            Ok(status) => (status.code(), None),
            Err(e) => (None, Some(e.to_string())),
        };
        exited(Exit {
            workspace_id: prepared.workspace_id,
            name: prepared.name,
            code,
            error,
        });
    });
    Ok(pid)
}

#[cfg(test)]
mod tests;
