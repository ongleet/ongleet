//! Bounded session cleanup scripts, shared by GUI, CLI and archive expiry.
use crate::session::{Project, Session, SessionRegistry};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::{atomic::AtomicBool, Arc},
};

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Hooks {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_archive: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_delete: Option<String>,
}
impl Hooks {
    pub fn validate(&self) -> Result<(), String> {
        for script in [&self.before_archive, &self.before_delete]
            .into_iter()
            .flatten()
        {
            if script.trim().is_empty() || script.contains('\0') {
                return Err("Session hooks require nonempty scripts without NUL bytes".into());
            }
        }
        Ok(())
    }
}
impl SessionRegistry {
    pub(crate) fn run_session_hook(
        &self,
        session: &Session,
        project: &Project,
        deleting: bool,
    ) -> Result<(), String> {
        let Some(directory) = &self.actions_directory else {
            return Ok(());
        };
        // A deleted project's existing archives must still be purgeable. Never seed
        // a new configuration (and new cleanup commands) during archive maintenance.
        let path = super::project_path(directory, project.id);
        if !path.try_exists().map_err(|e| e.to_string())? {
            return Ok(());
        }
        let config = super::parse_project_config(&super::read_config(&path)?)?;
        let (event, script) = if deleting {
            ("beforeDelete", config.hooks.before_delete)
        } else {
            ("beforeArchive", config.hooks.before_archive)
        };
        let Some(script) = script else {
            return Ok(());
        };
        let mut env = config.env;
        self.system.paths.merge_environment(
            &mut env,
            BTreeMap::from([
                ("SESSION_ID".into(), session.id.to_string()),
                ("SESSION_NAME".into(), session.name.clone()),
                ("SESSION_PATH".into(), session.base_directory.clone()),
                ("PROJECT_ID".into(), project.id.to_string()),
                ("PROJECT_NAME".into(), project.name.clone()),
                ("PROJECT_PATH".into(), project.root_directory.clone()),
                ("ONGLEET_SESSION_EVENT".into(), event.into()),
            ]),
        );
        let (executable, args) = self.system.paths.shell_command(&script);
        let result = crate::core::process::execute_with(
            self.system.processes.as_ref(),
            crate::core::process::Request {
                executable,
                args,
                env,
                cwd: Some(session.base_directory.clone()),
                timeout_ms: Some(20_000),
                output_limit: Some(65_536),
            },
            Arc::new(AtomicBool::new(false)),
        )
        .map_err(|e| format!("Session {event} hook failed: {e}"))?;
        if result.code != 0 {
            return Err(format!(
                "Session {event} hook exited with code {}: {}",
                result.code,
                result.stderr.trim()
            ));
        }
        Ok(())
    }
}
