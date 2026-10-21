use super::{Snapshot, TabConfig, TerminalTab, WorkspaceRegistry};

impl WorkspaceRegistry {
    pub fn create_tab(&mut self, workspace_id: u64) -> Result<Snapshot, String> {
        self.insert_tab(workspace_id, None)
    }

    pub fn create_typed_tab(
        &mut self,
        workspace_id: u64,
        name: String,
        config: TabConfig,
    ) -> Result<Snapshot, String> {
        self.insert_tab(workspace_id, Some((name, config)))
    }

    fn insert_tab(
        &mut self,
        workspace_id: u64,
        template: Option<(String, TabConfig)>,
    ) -> Result<Snapshot, String> {
        let auto_start = template.is_some();
        let (name, config) = template.unwrap_or_else(|| {
            (
                format!("Terminal {}", self.next_tab_id + 1),
                TabConfig::default(),
            )
        });
        let workspace = self
            .workspaces
            .iter_mut()
            .find(|w| w.id == workspace_id)
            .ok_or("Workspace is unavailable")?;
        let base_name = name;
        let mut name = base_name.clone();
        let mut suffix = 2;
        while workspace
            .tabs
            .iter()
            .any(|tab| tab.is_open && tab.name == name)
        {
            name = format!("{base_name} {suffix}");
            suffix += 1;
        }
        self.next_tab_id += 1;
        workspace.tabs.push(TerminalTab {
            kind: "terminal".into(),
            is_open: true,
            resume_on_start: auto_start,
            id: self.next_tab_id,
            name,
            display_order: workspace.tabs.len(),
            config,
        });
        workspace.selected_tab_id = Some(self.next_tab_id);
        self.persist()
    }

    pub fn select_tab(&mut self, id: u64) -> Result<Snapshot, String> {
        let workspace = self.tab_workspace_mut(id)?;
        if !workspace.tabs.iter().find(|t| t.id == id).unwrap().is_open {
            return Err("Tab is not open".into());
        }
        workspace.selected_tab_id = Some(id);
        self.persist()
    }

    pub fn rename_tab(&mut self, id: u64, name: String) -> Result<Snapshot, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Tab name is required".into());
        }
        let workspace = self.tab_workspace_mut(id)?;
        workspace.tabs.iter_mut().find(|t| t.id == id).unwrap().name = name.into();
        self.persist()
    }

    pub fn configure_tab(&mut self, id: u64, config: TabConfig) -> Result<Snapshot, String> {
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
        self.tab_workspace_mut(id)?
            .tabs
            .iter_mut()
            .find(|t| t.id == id)
            .unwrap()
            .config = config;
        self.persist()
    }

    pub fn reorder_tab(&mut self, id: u64, position: usize) -> Result<Snapshot, String> {
        let workspace = self.tab_workspace_mut(id)?;
        let open_indices: Vec<_> = workspace
            .tabs
            .iter()
            .enumerate()
            .filter(|(_, t)| t.is_open)
            .map(|(index, _)| index)
            .collect();
        if position >= open_indices.len() {
            return Err("Invalid tab position".into());
        }
        let position = open_indices[position];
        let index = workspace.tabs.iter().position(|t| t.id == id).unwrap();
        if !workspace.tabs[index].is_open {
            return Err("Tab is not open".into());
        }
        let tab = workspace.tabs.remove(index);
        workspace.tabs.insert(position, tab);
        for (order, tab) in workspace.tabs.iter_mut().enumerate() {
            tab.display_order = order;
        }
        self.persist()
    }

    pub fn open_tab(&mut self, id: u64) -> Result<Snapshot, String> {
        let workspace = self.tab_workspace_mut(id)?;
        let tab = workspace.tabs.iter_mut().find(|t| t.id == id).unwrap();
        if !tab.is_open {
            tab.is_open = true;
            tab.resume_on_start = true;
        }
        workspace.selected_tab_id = Some(id);
        self.persist()
    }

    pub fn close_tab(&mut self, id: u64, confirmed: bool) -> Result<Snapshot, String> {
        self.tab_workspace_mut(id)?;
        if let Some(session) = self.sessions.get(&id) {
            self.terminate_session(session.id, confirmed)?;
        }
        let workspace = self.tab_workspace_mut(id)?;
        let index = workspace.tabs.iter().position(|t| t.id == id).unwrap();
        workspace.tabs[index].is_open = false;
        workspace.tabs[index].resume_on_start = false;
        if workspace.selected_tab_id == Some(id) {
            workspace.selected_tab_id = workspace.tabs[index + 1..]
                .iter()
                .find(|t| t.is_open)
                .or_else(|| workspace.tabs[..index].iter().rev().find(|t| t.is_open))
                .map(|t| t.id);
        }
        self.persist()
    }
}
