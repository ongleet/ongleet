use crate::storage::{Metadata, MetadataStore};
use crate::terminal_session::{Status, TerminalSession};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod archives;
pub type PluginData = std::collections::BTreeMap<String, Namespace>;
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Namespace {
    pub version: u32,
    pub revision: u64,
    pub value: serde_json::Value,
}

mod history;
mod projects;
mod tabs;
#[path = "core/pty/lifecycle.rs"]
mod terminals;
pub use projects::Project;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    #[serde(default)]
    pub plugin_data: PluginData,
    pub pull_request: Option<crate::core::pull_request_compat::Association>,
    pub project_id: u64,
    pub id: u64,
    pub name: String,
    pub base_directory: String,
    pub display_order: usize,
    pub tabs: Vec<PluginTab>,
    pub selected_tab_id: Option<u64>,
}

pub use crate::core::pty::TabConfig;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginTab {
    #[serde(default)]
    pub plugin_data: PluginData,
    pub config_version: u32,
    pub kind: String,
    pub is_open: bool,
    pub resume_on_start: bool,
    pub id: u64,
    pub name: String,
    pub display_order: usize,
    pub config: serde_json::Value,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub projects: Vec<Project>,
    pub sessions: Vec<Session>,
    pub selected_id: Option<u64>,
}

// Durable definitions are separate from open tabs and native PTY resources.
#[derive(Default)]
pub struct SessionRegistry {
    data_directory: Option<std::path::PathBuf>,
    actions_directory: Option<std::path::PathBuf>,
    projects: Vec<Project>,
    next_project_id: u64,
    sessions: Vec<Session>,
    store: Option<MetadataStore>,
    last_saved: Option<Metadata>,
    archives: Vec<archives::Archive>,
    next_archive_id: u64,
    archive_cleanup_error: Option<String>,
    selected_id: Option<u64>,
    next_id: u64,
    next_terminal_id: u64,
    next_tab_id: u64,
    // Keyed by tab ID, never by navigation selection or session position.
    pub terminals: HashMap<u64, TerminalSession>,
    pub file_watches: HashMap<u64, crate::core::fs::watch::Watch>,
    pub sdk_attachments: HashMap<u64, String>,
    pub plugin_jobs: HashMap<u64, std::sync::Arc<crate::plugins::Job>>,
}

impl SessionRegistry {
    pub fn set_actions_directory(&mut self, directory: std::path::PathBuf) {
        self.actions_directory = Some(directory);
    }

    pub fn open(directory: &std::path::Path) -> Result<Self, String> {
        let store = MetadataStore::open(directory)?;
        let metadata = store.load()?;
        let mut registry = Self {
            data_directory: Some(directory.to_path_buf()),
            projects: metadata.snapshot.projects.clone(),
            next_project_id: metadata.project_sequence,
            sessions: metadata.snapshot.sessions.clone(),
            selected_id: metadata.snapshot.selected_id,
            next_id: metadata.session_sequence,
            next_tab_id: metadata.tab_sequence,
            store: Some(store),
            archives: metadata.archives.clone(),
            next_archive_id: metadata.archive_sequence,
            archive_cleanup_error: None,
            last_saved: Some(metadata),
            actions_directory: None,
            next_terminal_id: 0,
            terminals: HashMap::new(),
            sdk_attachments: HashMap::new(),
            file_watches: HashMap::new(),
            plugin_jobs: HashMap::new(),
        };
        registry.run_archive_maintenance();
        Ok(registry)
    }

    fn persist(&mut self) -> Result<Snapshot, String> {
        let metadata = Metadata {
            snapshot: self.snapshot(),
            archives: self.archives.clone(),
            archive_sequence: self.next_archive_id,
            project_sequence: self.next_project_id,
            session_sequence: self.next_id,
            tab_sequence: self.next_tab_id,
        };
        if let Some(store) = &mut self.store {
            if let Err(error) = store.save(&metadata) {
                if let Some(previous) = &self.last_saved {
                    self.projects = previous.snapshot.projects.clone();
                    self.next_project_id = previous.project_sequence;
                    self.sessions = previous.snapshot.sessions.clone();
                    self.selected_id = previous.snapshot.selected_id;
                    self.next_id = previous.session_sequence;
                    self.next_tab_id = previous.tab_sequence;
                    self.archives = previous.archives.clone();
                    self.next_archive_id = previous.archive_sequence;
                }
                return Err(error);
            }
            self.last_saved = Some(metadata);
        }
        Ok(self.snapshot())
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            projects: self.projects.clone(),
            sessions: self.sessions.clone(),
            selected_id: self.selected_id,
        }
    }

    pub fn create_from_first_action(
        &mut self,
        project_id: Option<u64>,
        name: String,
        directory: String,
        config_directory: &std::path::Path,
    ) -> Result<Snapshot, String> {
        let group = project_id
            .or_else(|| self.projects.first().map(|g| g.id))
            .ok_or("Create a project first")?;
        self.project(group)?;
        // Validate the YAML before changing metadata. Each opening owns its copy.
        let action = crate::actions::load_project(config_directory, group)?
            .into_iter()
            .find(|action| crate::plugins::has_runtime(&action.kind))
            .ok_or("Project needs a terminal action to initialize a session")?;
        crate::plugins::validate(&action.kind, action.config_version, &action.config)?;
        self.insert_session(group, name, directory, Some(action))
    }

    // Legacy stopped fixtures for restoration and explicit-start integration tests.
    #[cfg(test)]
    pub fn create(&mut self, name: String, directory: String) -> Result<Snapshot, String> {
        valid_name(name.clone())?;
        validate_directory(&directory)?;
        if self.projects.is_empty() {
            self.create_project("Default".into(), directory.clone())?;
        }
        self.create_in_project(self.projects[0].id, name, directory)
    }

    #[cfg(test)]
    pub fn create_in_project(
        &mut self,
        project_id: u64,
        name: String,
        directory: String,
    ) -> Result<Snapshot, String> {
        self.insert_session(project_id, name, directory, None)
    }

    fn insert_session(
        &mut self,
        project_id: u64,
        name: String,
        directory: String,
        initial_action: Option<crate::actions::Action>,
    ) -> Result<Snapshot, String> {
        let name = valid_name(name)?;
        let root = self.project(project_id)?.root_directory.clone();
        let directory = if directory.trim().is_empty() {
            root
        } else {
            directory
        };
        validate_directory(&directory)?;
        self.next_id += 1;
        self.sessions.push(Session {
            plugin_data: Default::default(),
            pull_request: None,
            project_id,
            id: self.next_id,
            name,
            base_directory: directory,
            display_order: self.sessions.len(),
            tabs: vec![],
            selected_tab_id: None,
        });
        self.projects
            .iter_mut()
            .find(|g| g.id == project_id)
            .unwrap()
            .collapsed = false;
        self.selected_id = Some(self.next_id);
        if let Some(action) = initial_action {
            self.create_plugin_tab(
                self.next_id,
                action.name,
                action.kind,
                action.config_version,
                action.config,
            )
        } else {
            self.insert_tab(self.next_id, None)
        }
    }

    pub fn update_in_project(
        &mut self,
        id: u64,
        project_id: u64,
        name: String,
        directory: String,
    ) -> Result<Snapshot, String> {
        self.project(project_id)?;
        let name = valid_name(name)?;
        let session = self
            .sessions
            .iter_mut()
            .find(|w| w.id == id)
            .ok_or("Session is unavailable")?;
        if session.base_directory != directory {
            validate_directory(&directory)?;
        }
        session.name = name;
        session.base_directory = directory;
        session.project_id = project_id;
        self.persist()
    }

    pub fn update(&mut self, id: u64, name: String, directory: String) -> Result<Snapshot, String> {
        let group = self.session_project(id)?;
        self.update_in_project(id, group, name, directory)
    }

    pub fn select(&mut self, id: u64) -> Result<Snapshot, String> {
        if !self.sessions.iter().any(|w| w.id == id) {
            return Err("Session is unavailable".into());
        }
        self.selected_id = Some(id);
        self.persist()
    }

    pub fn reorder(&mut self, id: u64, position: usize) -> Result<Snapshot, String> {
        if position >= self.sessions.len() {
            return Err("Invalid session position".into());
        }
        let index = self
            .sessions
            .iter()
            .position(|w| w.id == id)
            .ok_or("Session is unavailable")?;
        let session = self.sessions.remove(index);
        self.sessions.insert(position, session);
        self.normalize_order();
        self.persist()
    }

    pub fn close(&mut self, id: u64, confirmed: bool) -> Result<Snapshot, String> {
        self.run_archive_maintenance();
        let index = self
            .sessions
            .iter()
            .position(|w| w.id == id)
            .ok_or("Session is unavailable")?;
        let mut archived_session = self.sessions[index].clone();
        self.capture_restart_flags(&mut archived_session.tabs);
        let archived_project = self.project(archived_session.project_id)?.clone();
        let archive = self.prepare_archive(archives::ArchivedItem::Session {
            session: archived_session,
            project: archived_project,
        })?;
        let session_ids: Vec<_> = self.sessions[index]
            .tabs
            .iter()
            .filter_map(|tab| self.terminals.get(&tab.id).map(|s| s.id))
            .collect();
        if !confirmed
            && session_ids.iter().any(|id| {
                self.terminals
                    .values_mut()
                    .find(|s| s.id == *id)
                    .is_some_and(|s| matches!(s.status(), Ok(Status::Running)))
            })
        {
            return Err(
                "Explicit confirmation is required to terminate this session's terminal sessions"
                    .into(),
            );
        }
        for session in session_ids {
            self.terminate_session(session, confirmed)?;
        }
        let plugin_tabs: Vec<_> = self.sessions[index].tabs.iter().map(|t| t.id).collect();
        self.cancel_plugin_jobs(&plugin_tabs)?;
        for id in &plugin_tabs {
            self.file_watches.remove(id);
            self.sdk_attachments.remove(id);
        }
        self.sessions.remove(index);
        self.record_archive(archive);
        self.normalize_order();
        if self.selected_id == Some(id) {
            self.selected_id = self
                .sessions
                .get(index.min(self.sessions.len().saturating_sub(1)))
                .map(|w| w.id);
        }
        self.persist()
    }

    fn normalize_order(&mut self) {
        for (order, session) in self.sessions.iter_mut().enumerate() {
            session.display_order = order;
        }
    }

    fn tab_session_mut(&mut self, tab_id: u64) -> Result<&mut Session, String> {
        self.sessions
            .iter_mut()
            .find(|w| w.tabs.iter().any(|t| t.id == tab_id))
            .ok_or("Tab is unavailable".into())
    }
}

fn valid_name(name: String) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Session name is required".into());
    }
    Ok(name.into())
}

fn validate_directory(directory: &str) -> Result<(), String> {
    let path = std::path::Path::new(directory);
    if !path.is_absolute() || !path.is_dir() {
        return Err("Base directory must be an absolute path to an existing directory".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
impl PluginTab {
    pub fn terminal_config(&self) -> TabConfig {
        serde_json::from_value(self.config.clone()).unwrap()
    }
}

impl SessionRegistry {
    #[cfg(test)]
    pub fn associate_pull_request(
        &mut self,
        tab_id: u64,
        association: Option<crate::core::pull_request_compat::Association>,
    ) -> Result<Snapshot, String> {
        self.open_plugin_tab(tab_id)?;
        let session_id = self.tab_session_mut(tab_id)?.id;
        self.associate_session_pull_request(session_id, association)
    }
    pub fn associate_session_pull_request(
        &mut self,
        session_id: u64,
        association: Option<crate::core::pull_request_compat::Association>,
    ) -> Result<Snapshot, String> {
        if let Some(association) = &association {
            association.validate()?;
        }
        let tabs: Vec<_> = self
            .sessions
            .iter()
            .find(|s| s.id == session_id)
            .ok_or("Session is unavailable")?
            .tabs
            .iter()
            .map(|t| t.id)
            .collect();
        self.cancel_plugin_jobs(&tabs)?;
        let data = &mut self
            .sessions
            .iter_mut()
            .find(|s| s.id == session_id)
            .unwrap()
            .plugin_data;
        let revision = data.get("pull-request").map_or(0, |n| n.revision);
        self.replace_plugin_data(
            session_id,
            None,
            "pull-request",
            revision,
            1,
            serde_json::to_value(association).map_err(|e| e.to_string())?,
        )?;
        Ok(self.snapshot())
    }
    pub fn cancel_plugin_work(&mut self, tab: u64) -> Result<(), String> {
        self.file_watches.remove(&tab);
        self.cancel_plugin_jobs(&[tab])
    }
    pub fn cancel_plugin_jobs(&mut self, tabs: &[u64]) -> Result<(), String> {
        use std::{
            sync::atomic::Ordering,
            time::{Duration, Instant},
        };
        let jobs: Vec<_> = tabs
            .iter()
            .filter_map(|id| self.plugin_jobs.get(id).cloned())
            .collect();
        for job in &jobs {
            job.cancel.store(true, Ordering::Release);
        }
        // Workers signal completion after subprocess cleanup, before acquiring
        // this registry lock. Quit must not exit with a live gh subprocess.
        let deadline = Instant::now() + Duration::from_secs(2);
        while jobs.iter().any(|job| !job.finished.load(Ordering::Acquire)) {
            if Instant::now() >= deadline {
                return Err(
                    "Plugin subprocess cleanup timed out; retry after pending work stops".into(),
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        for id in tabs {
            self.plugin_jobs.remove(id);
        }
        Ok(())
    }
}
impl Drop for SessionRegistry {
    fn drop(&mut self) {
        for cancel in self.plugin_jobs.values() {
            cancel
                .cancel
                .store(true, std::sync::atomic::Ordering::Release);
        }
    }
}

impl SessionRegistry {
    pub fn replace_plugin_data(
        &mut self,
        session_id: u64,
        tab_id: Option<u64>,
        plugin: &str,
        expected: u64,
        version: u32,
        value: serde_json::Value,
    ) -> Result<Namespace, String> {
        if version == 0 {
            return Err("Invalid namespace version".into());
        }
        let session = self
            .sessions
            .iter_mut()
            .find(|s| s.id == session_id)
            .ok_or("Session is unavailable")?;
        let data = if let Some(id) = tab_id {
            &mut session
                .tabs
                .iter_mut()
                .find(|t| t.id == id && t.is_open)
                .ok_or("Open tab is unavailable")?
                .plugin_data
        } else {
            &mut session.plugin_data
        };
        if data.get(plugin).map_or(0, |n| n.revision) != expected {
            return Err("revision_conflict: Plugin data changed; reload before saving".into());
        }
        let namespace = Namespace {
            version,
            revision: expected.checked_add(1).ok_or("Revision overflow")?,
            value,
        };
        let mut replacement = data.clone();
        replacement.insert(plugin.into(), namespace.clone());
        if serde_json::to_vec(&replacement)
            .map_err(|e| e.to_string())?
            .len()
            > 65_536
        {
            return Err("Plugin data exceeds owner's 64 KiB limit".into());
        }
        *data = replacement;
        if tab_id.is_none() {
            // Legacy response field is derived, never the source of truth.
            session.pull_request = session
                .plugin_data
                .get("pull-request")
                .and_then(|n| serde_json::from_value(n.value.clone()).ok());
        }
        self.persist()?;
        Ok(namespace)
    }
}
