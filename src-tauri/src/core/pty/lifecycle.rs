use super::SessionRegistry;
use crate::terminal_session::{Attachment, Launch, Output, Status, TerminalSession};
use tauri::ipc::Channel;

impl SessionRegistry {
    pub fn connect(
        &mut self,
        tab_id: u64,
        cols: u16,
        rows: u16,
        output: Channel<Output>,
        start_if_stopped: bool,
    ) -> Result<Option<Attachment>, String> {
        self.runtime_tab(tab_id)?;
        if let Some(session) = self.terminals.get(&tab_id) {
            return session.attach(output).map(Some);
        }
        if !start_if_stopped {
            return Ok(None);
        }
        let id = self.start(tab_id, cols, rows, output)?;
        Ok(Some(Attachment {
            id,
            cols,
            rows,
            replay: false,
        }))
    }

    pub fn start(
        &mut self,
        tab_id: u64,
        cols: u16,
        rows: u16,
        output: Channel<Output>,
    ) -> Result<u64, String> {
        if self.terminals.contains_key(&tab_id) {
            return Err("Stop this tab's existing terminal session first".into());
        }
        let session = self
            .sessions
            .iter()
            .find(|w| w.tabs.iter().any(|t| t.id == tab_id))
            .ok_or("Tab is unavailable")?;
        if !session
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .unwrap()
            .is_open
        {
            return Err("Open this tab definition before starting it".into());
        }
        let tab = session.tabs.iter().find(|t| t.id == tab_id).unwrap();
        if !crate::plugins::has_runtime(&tab.kind) {
            return Err("This tab is not a terminal".into());
        }
        crate::plugins::validate(&tab.kind, tab.config_version, &tab.config)?;
        let config: super::TabConfig =
            serde_json::from_value(tab.config.clone()).map_err(|e| e.to_string())?;
        let (mut executable, mut args) = if let Some(command) = &config.shell_command {
            (
                Some(std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into())),
                vec!["-lc".into(), command.clone()],
            )
        } else {
            (config.executable.clone(), config.args.clone())
        };
        let mut env = if let Some(directory) = &self.actions_directory {
            let yaml = crate::actions::read_project_yaml(directory, session.project_id)?;
            crate::actions::parse_project_config(&yaml)?.env
        } else {
            std::collections::BTreeMap::new()
        };
        // Process environment < common project variables < action overrides.
        env.extend(config.env.clone());
        // Stable launch context is reserved and cannot be overridden by project/action env.
        env.insert("DEVCENTER_SESSION_ID".into(), session.id.to_string());
        env.insert(
            "DEVCENTER_PROJECT_ID".into(),
            session.project_id.to_string(),
        );
        env.insert("DEVCENTER_TAB_ID".into(), tab_id.to_string());
        // Resolve the opt-in wrapper through the running image, including after
        // rebuilds unlink that image. Never depend on a separately built helper.
        if config.shell_command.is_none()
            && matches!(executable.as_deref(), Some("dcodex" | "devcenter-codex"))
        {
            let data = self
                .data_directory
                .as_ref()
                .ok_or("Codex resume requires persisted application data")?;
            executable = Some(
                crate::containment::executable()?
                    .to_string_lossy()
                    .into_owned(),
            );
            args.insert(0, crate::codex::WRAPPER_FLAG.into());
            env.insert(
                "DEVCENTER_CODEX_DATA".into(),
                data.to_string_lossy().into_owned(),
            );
            env.insert(
                "DEVCENTER_CODEX_HOOK_EXE".into(),
                crate::containment::executable()?
                    .to_string_lossy()
                    .into_owned(),
            );
        }
        let launch = Launch {
            executable,
            args,
            cwd: config
                .cwd_override
                .clone()
                .unwrap_or_else(|| session.base_directory.clone()),
            env,
            cols,
            rows,
        };
        self.next_terminal_id += 1;
        let id = self.next_terminal_id;
        let previous_history = if self.data_directory.is_some() {
            self.terminal_history(tab_id)?
        } else {
            String::new()
        };
        let mut session = match TerminalSession::spawn(id, launch, output) {
            Ok(session) => session,
            Err(error) => {
                self.tab_session_mut(tab_id)?
                    .tabs
                    .iter_mut()
                    .find(|t| t.id == tab_id)
                    .unwrap()
                    .resume_on_start = false;
                self.persist()?;
                return Err(error);
            }
        };
        session.previous_history = previous_history;
        self.terminals.insert(tab_id, session);
        self.tab_session_mut(tab_id)?
            .tabs
            .iter_mut()
            .find(|t| t.id == tab_id)
            .unwrap()
            .resume_on_start = true;
        if let Err(error) = self.persist() {
            let mut session = self.terminals.remove(&tab_id).unwrap();
            let _ = session.close();
            return Err(error);
        }
        Ok(id)
    }

    pub(super) fn terminate_session(&mut self, id: u64, confirmed: bool) -> Result<u64, String> {
        let key = self
            .terminals
            .iter()
            .find(|(_, s)| s.id == id)
            .map(|(key, _)| *key)
            .ok_or("Session is unavailable")?;
        if !confirmed
            && matches!(
                self.terminals.get_mut(&key).unwrap().status()?,
                Status::Running
            )
        {
            return Err("Explicit confirmation is required to terminate session processes".into());
        }
        self.terminals.get_mut(&key).unwrap().close()?;
        self.terminals.remove(&key);
        if self.codex_activity.remove(&key).is_some() {
            let owner = self.tab_session_mut(key)?.id;
            self.refresh_codex_tag(owner)?;
        }
        Ok(key)
    }

    pub fn close_terminal_session(&mut self, id: u64, confirmed: bool) -> Result<(), String> {
        let tab = self.terminate_session(id, confirmed)?;
        self.tab_session_mut(tab)?
            .tabs
            .iter_mut()
            .find(|t| t.id == tab)
            .unwrap()
            .resume_on_start = false;
        self.persist()?;
        Ok(())
    }

    pub fn status(&mut self, id: u64) -> Result<Status, String> {
        let (tab_id, session) = self
            .terminals
            .iter_mut()
            .find(|(_, s)| s.id == id)
            .ok_or("Session is unavailable")?;
        let tab_id = *tab_id;
        let status = session.status()?;
        if matches!(status, Status::Exited { .. }) {
            let tab = self
                .tab_session_mut(tab_id)?
                .tabs
                .iter_mut()
                .find(|t| t.id == tab_id)
                .unwrap();
            if tab.resume_on_start {
                tab.resume_on_start = false;
                self.persist()?;
            }
        }
        if matches!(status, Status::Exited { .. })
            && self
                .codex_activity
                .get(&tab_id)
                .is_some_and(|s| *s != crate::codex::Activity::Stopped)
        {
            self.codex_activity
                .insert(tab_id, crate::codex::Activity::Stopped);
            let owner = self.tab_session_mut(tab_id)?.id;
            self.refresh_codex_tag(owner)?;
        }
        Ok(status)
    }

    pub fn shutdown(&mut self, confirmed: bool) -> Result<(), String> {
        if !confirmed {
            for session in self.terminals.values_mut() {
                if matches!(session.status()?, Status::Running) {
                    return Err(
                        "Explicit confirmation is required to terminate session processes".into(),
                    );
                }
            }
        }
        let plugin_tabs: Vec<_> = self
            .plugin_jobs
            .keys()
            .copied()
            .chain(self.http_jobs.keys().map(|(id, _)| *id))
            .collect();
        self.cancel_plugin_jobs(&plugin_tabs)?;
        self.file_watches.clear();
        self.sdk_attachments.clear();
        // Persist the last running set before killing processes for app exit.
        // Explicit Stop and tab close clear the flag through their own paths.
        let mut exited = Vec::new();
        for (tab, session) in &mut self.terminals {
            if matches!(session.status()?, Status::Exited { .. }) {
                exited.push(*tab);
            }
        }
        for id in exited {
            self.tab_session_mut(id)?
                .tabs
                .iter_mut()
                .find(|t| t.id == id)
                .unwrap()
                .resume_on_start = false;
        }
        self.persist()?;
        let ids: Vec<_> = self.terminals.values().map(|s| s.id).collect();
        for id in ids {
            self.terminate_session(id, confirmed)?;
        }
        Ok(())
    }
}
