use super::WorkspaceRegistry;
use crate::session::{Attachment, Launch, Output, Status, TerminalSession};
use tauri::ipc::Channel;

impl WorkspaceRegistry {
    pub fn connect(
        &mut self,
        tab_id: u64,
        cols: u16,
        rows: u16,
        output: Channel<Output>,
        start_if_stopped: bool,
    ) -> Result<Option<Attachment>, String> {
        if let Some(session) = self.sessions.get(&tab_id) {
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
        if self.sessions.contains_key(&tab_id) {
            return Err("Stop this tab's existing terminal session first".into());
        }
        let workspace = self
            .workspaces
            .iter()
            .find(|w| w.tabs.iter().any(|t| t.id == tab_id))
            .ok_or("Tab is unavailable")?;
        if !workspace
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .unwrap()
            .is_open
        {
            return Err("Open this tab definition before starting it".into());
        }
        let config = &workspace
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .unwrap()
            .config;
        let (executable, args) = if let Some(command) = &config.shell_command {
            (
                Some(std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into())),
                vec!["-lc".into(), command.clone()],
            )
        } else {
            (config.executable.clone(), config.args.clone())
        };
        let mut env = if let Some(directory) = &self.actions_directory {
            let yaml = crate::actions::read_workgroup_yaml(directory, workspace.workgroup_id)?;
            crate::actions::parse_config(&yaml)?.env
        } else {
            std::collections::BTreeMap::new()
        };
        // Process environment < common workgroup variables < action overrides.
        env.extend(config.env.clone());
        let launch = Launch {
            executable,
            args,
            cwd: config
                .cwd_override
                .clone()
                .unwrap_or_else(|| workspace.base_directory.clone()),
            env,
            cols,
            rows,
        };
        self.next_session_id += 1;
        let id = self.next_session_id;
        let session = match TerminalSession::spawn(id, launch, output) {
            Ok(session) => session,
            Err(error) => {
                self.tab_workspace_mut(tab_id)?
                    .tabs
                    .iter_mut()
                    .find(|t| t.id == tab_id)
                    .unwrap()
                    .resume_on_start = false;
                self.persist()?;
                return Err(error);
            }
        };
        self.sessions.insert(tab_id, session);
        self.tab_workspace_mut(tab_id)?
            .tabs
            .iter_mut()
            .find(|t| t.id == tab_id)
            .unwrap()
            .resume_on_start = true;
        if let Err(error) = self.persist() {
            let mut session = self.sessions.remove(&tab_id).unwrap();
            let _ = session.close();
            return Err(error);
        }
        Ok(id)
    }

    pub(super) fn terminate_session(&mut self, id: u64, confirmed: bool) -> Result<u64, String> {
        let key = self
            .sessions
            .iter()
            .find(|(_, s)| s.id == id)
            .map(|(key, _)| *key)
            .ok_or("Session is unavailable")?;
        if !confirmed
            && matches!(
                self.sessions.get_mut(&key).unwrap().status()?,
                Status::Running
            )
        {
            return Err("Explicit confirmation is required to terminate session processes".into());
        }
        self.sessions.get_mut(&key).unwrap().close()?;
        self.sessions.remove(&key);
        Ok(key)
    }

    pub fn close_session(&mut self, id: u64, confirmed: bool) -> Result<(), String> {
        let tab = self.terminate_session(id, confirmed)?;
        self.tab_workspace_mut(tab)?
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
            .sessions
            .iter_mut()
            .find(|(_, s)| s.id == id)
            .ok_or("Session is unavailable")?;
        let tab_id = *tab_id;
        let status = session.status()?;
        if matches!(status, Status::Exited { .. }) {
            let tab = self
                .tab_workspace_mut(tab_id)?
                .tabs
                .iter_mut()
                .find(|t| t.id == tab_id)
                .unwrap();
            if tab.resume_on_start {
                tab.resume_on_start = false;
                self.persist()?;
            }
        }
        Ok(status)
    }

    pub fn shutdown(&mut self, confirmed: bool) -> Result<(), String> {
        if !confirmed {
            for session in self.sessions.values_mut() {
                if matches!(session.status()?, Status::Running) {
                    return Err(
                        "Explicit confirmation is required to terminate session processes".into(),
                    );
                }
            }
        }
        // Persist the last running set before killing processes for app exit.
        // Explicit Stop and tab close clear the flag through their own paths.
        let mut exited = Vec::new();
        for (tab, session) in &mut self.sessions {
            if matches!(session.status()?, Status::Exited { .. }) {
                exited.push(*tab);
            }
        }
        for id in exited {
            self.tab_workspace_mut(id)?
                .tabs
                .iter_mut()
                .find(|t| t.id == id)
                .unwrap()
                .resume_on_start = false;
        }
        self.persist()?;
        let ids: Vec<_> = self.sessions.values().map(|s| s.id).collect();
        for id in ids {
            self.terminate_session(id, confirmed)?;
        }
        Ok(())
    }
}
