use crate::storage::{Metadata, MetadataStore};
use crate::terminal_session::{Status, TerminalSession};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

mod history;
mod projects;
mod tabs;
mod terminals;
pub use projects::Project;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub project_id: u64,
    pub id: u64,
    pub name: String,
    pub base_directory: String,
    pub display_order: usize,
    pub tabs: Vec<TerminalTab>,
    pub selected_tab_id: Option<u64>,
}

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

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalTab {
    pub kind: String,
    pub is_open: bool,
    pub resume_on_start: bool,
    pub id: u64,
    pub name: String,
    pub display_order: usize,
    pub config: TabConfig,
}

#[derive(Clone, Serialize)]
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
    selected_id: Option<u64>,
    next_id: u64,
    next_terminal_id: u64,
    next_tab_id: u64,
    // Keyed by tab ID, never by navigation selection or session position.
    pub terminals: HashMap<u64, TerminalSession>,
}

impl SessionRegistry {
    pub fn set_actions_directory(&mut self, directory: std::path::PathBuf) {
        self.actions_directory = Some(directory);
    }

    pub fn open(directory: &std::path::Path) -> Result<Self, String> {
        let store = MetadataStore::open(directory)?;
        let metadata = store.load()?;
        if metadata
            .snapshot
            .sessions
            .iter()
            .flat_map(|w| &w.tabs)
            .any(|t| t.kind != "terminal")
        {
            return Err(
                "Saved configuration contains an unsupported tab type; database was not reset"
                    .into(),
            );
        }
        Ok(Self {
            data_directory: Some(directory.to_path_buf()),
            projects: metadata.snapshot.projects.clone(),
            next_project_id: metadata.project_sequence,
            sessions: metadata.snapshot.sessions.clone(),
            selected_id: metadata.snapshot.selected_id,
            next_id: metadata.session_sequence,
            next_tab_id: metadata.tab_sequence,
            store: Some(store),
            last_saved: Some(metadata),
            ..Default::default()
        })
    }

    fn persist(&mut self) -> Result<Snapshot, String> {
        let metadata = Metadata {
            snapshot: self.snapshot(),
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
            .find(|action| action.kind == "terminal")
            .ok_or("Project needs a terminal action to initialize a session")?;
        self.insert_session(group, name, directory, Some((action.name, action.config)))
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
        initial_action: Option<(String, TabConfig)>,
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
        self.insert_tab(self.next_id, initial_action)
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
        let index = self
            .sessions
            .iter()
            .position(|w| w.id == id)
            .ok_or("Session is unavailable")?;
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
        self.sessions.remove(index);
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
