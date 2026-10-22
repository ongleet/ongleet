use super::{SessionRegistry, Snapshot, TabConfig, TerminalTab};

impl SessionRegistry {
    #[cfg(test)]
    pub fn create_tab(&mut self, session_id: u64) -> Result<Snapshot, String> {
        self.insert_tab(session_id, None)
    }

    pub fn create_typed_tab(
        &mut self,
        session_id: u64,
        name: String,
        config: TabConfig,
    ) -> Result<Snapshot, String> {
        self.insert_tab(session_id, Some((name, config)))
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
        session.tabs.push(TerminalTab {
            kind: "terminal".into(),
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

    pub fn configure_tab(&mut self, id: u64, config: TabConfig) -> Result<Snapshot, String> {
        config.validate()?;
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
        if !tab.is_open {
            tab.is_open = true;
            tab.resume_on_start = true;
        }
        session.selected_tab_id = Some(id);
        self.persist()
    }

    pub fn close_tab(&mut self, id: u64, confirmed: bool) -> Result<Snapshot, String> {
        self.tab_session_mut(id)?;
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
        self.persist()
    }
}
