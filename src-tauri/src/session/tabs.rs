use super::{PluginTab, SessionRegistry, Snapshot, TabConfig};

impl SessionRegistry {
    #[cfg(test)]
    pub fn create_tab(&mut self, session_id: u64) -> Result<Snapshot, String> {
        self.insert_tab(session_id, None)
    }

    #[cfg(test)]
    pub fn create_typed_tab(
        &mut self,
        session_id: u64,
        name: String,
        config: TabConfig,
    ) -> Result<Snapshot, String> {
        self.create_plugin_tab(
            session_id,
            name,
            "terminal".into(),
            1,
            serde_json::to_value(config).map_err(|e| e.to_string())?,
        )
    }

    pub(super) fn insert_tab(
        &mut self,
        session_id: u64,
        template: Option<(String, TabConfig)>,
    ) -> Result<Snapshot, String> {
        let auto_start = template.is_some();
        let (name, config) = template.unwrap_or_else(|| {
            (
                format!("Terminal {}", self.next_tab_id + 1),
                TabConfig::default(),
            )
        });
        let snapshot = self.create_plugin_tab(
            session_id,
            name,
            "terminal".into(),
            1,
            serde_json::to_value(config).map_err(|e| e.to_string())?,
        )?;
        if !auto_start {
            self.sessions
                .iter_mut()
                .flat_map(|s| &mut s.tabs)
                .find(|t| t.id == self.next_tab_id)
                .unwrap()
                .resume_on_start = false;
            return self.persist();
        }
        Ok(snapshot)
    }

    pub fn create_plugin_tab(
        &mut self,
        session_id: u64,
        name: String,
        kind: String,
        config_version: u32,
        config: serde_json::Value,
    ) -> Result<Snapshot, String> {
        let name = name.trim().to_owned();
        if name.is_empty() {
            return Err("Tab name is required".into());
        }
        crate::plugins::validate(&kind, config_version, &config)?;
        let directory = &self
            .sessions
            .iter()
            .find(|s| s.id == session_id)
            .ok_or("Session is unavailable")?
            .base_directory;
        let config = crate::plugins::initialize(&kind, config, directory)?;
        let auto_start = kind == "terminal";
        let session = self
            .sessions
            .iter_mut()
            .find(|w| w.id == session_id)
            .ok_or("Session is unavailable")?;
        let base_name = name;
        let mut name = base_name.clone();
        let mut suffix = 2;
        while session
            .tabs
            .iter()
            .any(|tab| tab.is_open && tab.name == name)
        {
            name = format!("{base_name} {suffix}");
            suffix += 1;
        }
        self.next_tab_id += 1;
        session.tabs.push(PluginTab {
            kind,
            config_version,
            is_open: true,
            resume_on_start: auto_start,
            id: self.next_tab_id,
            name,
            display_order: session.tabs.len(),
            config,
        });
        session.selected_tab_id = Some(self.next_tab_id);
        self.persist()
    }

    pub fn select_tab(&mut self, id: u64) -> Result<Snapshot, String> {
        let session = self.tab_session_mut(id)?;
        if !session.tabs.iter().find(|t| t.id == id).unwrap().is_open {
            return Err("Tab is not open".into());
        }
        session.selected_tab_id = Some(id);
        self.persist()
    }

    pub fn rename_tab(&mut self, id: u64, name: String) -> Result<Snapshot, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Tab name is required".into());
        }
        let session = self.tab_session_mut(id)?;
        session.tabs.iter_mut().find(|t| t.id == id).unwrap().name = name.into();
        self.persist()
    }

    pub fn remember_files_folder(&mut self, id: u64, folder: String) -> Result<Snapshot, String> {
        let mut config = self.plugin_tab(id, "files")?.config.clone();
        if config["folder"].as_str() == Some(folder.as_str()) {
            return Ok(self.snapshot());
        }
        config["folder"] = serde_json::Value::String(folder);
        self.configure_plugin_tab(id, config)
    }

    pub fn remember_files_editor(
        &mut self,
        id: u64,
        file: Option<String>,
    ) -> Result<Snapshot, String> {
        let mut config = self.plugin_tab(id, "files")?.config.clone();
        let value = serde_json::to_value(file).map_err(|e| e.to_string())?;
        if config["editorFile"] == value {
            return Ok(self.snapshot());
        }
        config["editorFile"] = value;
        self.configure_plugin_tab(id, config)
    }

    pub fn configure_tab(&mut self, id: u64, config: TabConfig) -> Result<Snapshot, String> {
        self.configure_plugin_tab(id, serde_json::to_value(config).map_err(|e| e.to_string())?)
    }

    pub fn configure_plugin_tab(
        &mut self,
        id: u64,
        config: serde_json::Value,
    ) -> Result<Snapshot, String> {
        let tab = self
            .tab_session_mut(id)?
            .tabs
            .iter()
            .find(|t| t.id == id)
            .unwrap();
        crate::plugins::validate(&tab.kind, tab.config_version, &config)?;
        self.tab_session_mut(id)?
            .tabs
            .iter_mut()
            .find(|t| t.id == id)
            .unwrap()
            .config = config;
        self.persist()
    }

    pub fn reorder_tab(&mut self, id: u64, position: usize) -> Result<Snapshot, String> {
        let session = self.tab_session_mut(id)?;
        let open_indices: Vec<_> = session
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
        let index = session.tabs.iter().position(|t| t.id == id).unwrap();
        if !session.tabs[index].is_open {
            return Err("Tab is not open".into());
        }
        let tab = session.tabs.remove(index);
        session.tabs.insert(position, tab);
        for (order, tab) in session.tabs.iter_mut().enumerate() {
            tab.display_order = order;
        }
        self.persist()
    }

    pub fn open_tab(&mut self, id: u64) -> Result<Snapshot, String> {
        let session = self.tab_session_mut(id)?;
        let tab = session.tabs.iter_mut().find(|t| t.id == id).unwrap();
        crate::plugins::validate(&tab.kind, tab.config_version, &tab.config)?;
        if !tab.is_open {
            tab.is_open = true;
            tab.resume_on_start = tab.kind == "terminal";
        }
        session.selected_tab_id = Some(id);
        self.persist()
    }

    pub fn delete_tab(&mut self, id: u64, confirmed: bool) -> Result<Snapshot, String> {
        self.run_archive_maintenance();
        let mut parent = self.tab_session_mut(id)?.clone();
        let mut tab = parent.tabs.iter().find(|tab| tab.id == id).unwrap().clone();
        self.capture_restart_flags(std::slice::from_mut(&mut tab));
        let project = self.project(parent.project_id)?.clone();
        parent.tabs.clear();
        parent.selected_tab_id = None;
        let archive = self.prepare_archive(super::archives::ArchivedItem::Tab {
            session: parent,
            project,
            tab,
        })?;
        self.close_tab_state(id, confirmed)?;
        self.record_archive(archive);
        let session = self.tab_session_mut(id)?;
        session.tabs.retain(|tab| tab.id != id);
        for (order, tab) in session.tabs.iter_mut().enumerate() {
            tab.display_order = order;
        }
        self.persist()
    }

    pub fn close_tab(&mut self, id: u64, confirmed: bool) -> Result<Snapshot, String> {
        self.close_tab_state(id, confirmed)?;
        self.persist()
    }

    fn close_tab_state(&mut self, id: u64, confirmed: bool) -> Result<(), String> {
        self.tab_session_mut(id)?;
        self.cancel_plugin_work(id)?;
        if let Some(session) = self.terminals.get(&id) {
            self.terminate_session(session.id, confirmed)?;
        }
        let session = self.tab_session_mut(id)?;
        let index = session.tabs.iter().position(|t| t.id == id).unwrap();
        session.tabs[index].is_open = false;
        session.tabs[index].resume_on_start = false;
        if session.selected_tab_id == Some(id) {
            session.selected_tab_id = session.tabs[index + 1..]
                .iter()
                .find(|t| t.is_open)
                .or_else(|| session.tabs[..index].iter().rev().find(|t| t.is_open))
                .map(|t| t.id);
        }
        Ok(())
    }
}

impl SessionRegistry {
    pub fn plugin_tab(&self, id: u64, kind: &str) -> Result<&super::PluginTab, String> {
        let tab = self
            .sessions
            .iter()
            .flat_map(|s| &s.tabs)
            .find(|t| t.id == id && t.is_open)
            .ok_or("Open tab is unavailable")?;
        if tab.kind != kind {
            return Err(format!("Tab does not use the {kind} plugin"));
        }
        crate::plugins::validate(&tab.kind, tab.config_version, &tab.config)?;
        Ok(tab)
    }
}
