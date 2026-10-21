use crate::session::{Status, TerminalSession};
use crate::storage::{Metadata, MetadataStore};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

mod sessions;
mod tabs;
mod workgroups;
pub use workgroups::Workgroup;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub workgroup_id: u64,
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
    pub workgroups: Vec<Workgroup>,
    pub workspaces: Vec<Workspace>,
    pub selected_id: Option<u64>,
}

// Durable definitions are separate from open tabs and native PTY resources.
#[derive(Default)]
pub struct WorkspaceRegistry {
    workgroups: Vec<Workgroup>,
    next_workgroup_id: u64,
    workspaces: Vec<Workspace>,
    store: Option<MetadataStore>,
    last_saved: Option<Metadata>,
    selected_id: Option<u64>,
    next_id: u64,
    next_session_id: u64,
    next_tab_id: u64,
    // Keyed by tab ID, never by navigation selection or workspace position.
    pub sessions: HashMap<u64, TerminalSession>,
}

impl WorkspaceRegistry {
    pub fn open(directory: &std::path::Path) -> Result<Self, String> {
        let store = MetadataStore::open(directory)?;
        let metadata = store.load()?;
        if metadata
            .snapshot
            .workspaces
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
            workgroups: metadata.snapshot.workgroups.clone(),
            next_workgroup_id: metadata.workgroup_sequence,
            workspaces: metadata.snapshot.workspaces.clone(),
            selected_id: metadata.snapshot.selected_id,
            next_id: metadata.workspace_sequence,
            next_tab_id: metadata.tab_sequence,
            store: Some(store),
            last_saved: Some(metadata),
            ..Default::default()
        })
    }

    fn persist(&mut self) -> Result<Snapshot, String> {
        let metadata = Metadata {
            snapshot: self.snapshot(),
            workgroup_sequence: self.next_workgroup_id,
            workspace_sequence: self.next_id,
            tab_sequence: self.next_tab_id,
        };
        if let Some(store) = &mut self.store {
            if let Err(error) = store.save(&metadata) {
                if let Some(previous) = &self.last_saved {
                    self.workgroups = previous.snapshot.workgroups.clone();
                    self.next_workgroup_id = previous.workgroup_sequence;
                    self.workspaces = previous.snapshot.workspaces.clone();
                    self.selected_id = previous.snapshot.selected_id;
                    self.next_id = previous.workspace_sequence;
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
            workgroups: self.workgroups.clone(),
            workspaces: self.workspaces.clone(),
            selected_id: self.selected_id,
        }
    }

    pub fn create_from_first_action(
        &mut self,
        workgroup_id: Option<u64>,
        name: String,
        directory: String,
        config_directory: &std::path::Path,
    ) -> Result<Snapshot, String> {
        let group = workgroup_id
            .or_else(|| self.workgroups.first().map(|g| g.id))
            .ok_or("Create a workgroup first")?;
        self.workgroup(group)?;
        // Validate the YAML before changing metadata. Each opening owns its copy.
        let action = crate::tab_types::load_workgroup(config_directory, group)?
            .into_iter()
            .next()
            .ok_or("Workgroup has no actions")?;
        self.insert_workspace(group, name, directory, Some((action.name, action.config)))
    }

    // Legacy stopped fixtures for restoration and explicit-start integration tests.
    #[cfg(test)]
    pub fn create(&mut self, name: String, directory: String) -> Result<Snapshot, String> {
        valid_name(name.clone())?;
        validate_directory(&directory)?;
        if self.workgroups.is_empty() {
            self.create_workgroup("Default".into(), directory.clone())?;
        }
        self.create_in_workgroup(self.workgroups[0].id, name, directory)
    }

    #[cfg(test)]
    pub fn create_in_workgroup(
        &mut self,
        workgroup_id: u64,
        name: String,
        directory: String,
    ) -> Result<Snapshot, String> {
        self.insert_workspace(workgroup_id, name, directory, None)
    }

    fn insert_workspace(
        &mut self,
        workgroup_id: u64,
        name: String,
        directory: String,
        initial_action: Option<(String, TabConfig)>,
    ) -> Result<Snapshot, String> {
        let name = valid_name(name)?;
        let root = self.workgroup(workgroup_id)?.root_directory.clone();
        let directory = if directory.trim().is_empty() {
            root
        } else {
            directory
        };
        validate_directory(&directory)?;
        self.next_id += 1;
        self.workspaces.push(Workspace {
            workgroup_id,
            id: self.next_id,
            name,
            base_directory: directory,
            display_order: self.workspaces.len(),
            tabs: vec![],
            selected_tab_id: None,
        });
        self.workgroups
            .iter_mut()
            .find(|g| g.id == workgroup_id)
            .unwrap()
            .collapsed = false;
        self.selected_id = Some(self.next_id);
        self.insert_tab(self.next_id, initial_action)
    }

    pub fn update_in_workgroup(
        &mut self,
        id: u64,
        workgroup_id: u64,
        name: String,
        directory: String,
    ) -> Result<Snapshot, String> {
        self.workgroup(workgroup_id)?;
        let name = valid_name(name)?;
        let workspace = self
            .workspaces
            .iter_mut()
            .find(|w| w.id == id)
            .ok_or("Workspace is unavailable")?;
        if workspace.base_directory != directory {
            validate_directory(&directory)?;
        }
        workspace.name = name;
        workspace.base_directory = directory;
        workspace.workgroup_id = workgroup_id;
        self.persist()
    }

    pub fn update(&mut self, id: u64, name: String, directory: String) -> Result<Snapshot, String> {
        let group = self.workspace_workgroup(id)?;
        self.update_in_workgroup(id, group, name, directory)
    }

    pub fn select(&mut self, id: u64) -> Result<Snapshot, String> {
        if !self.workspaces.iter().any(|w| w.id == id) {
            return Err("Workspace is unavailable".into());
        }
        self.selected_id = Some(id);
        self.persist()
    }

    pub fn reorder(&mut self, id: u64, position: usize) -> Result<Snapshot, String> {
        if position >= self.workspaces.len() {
            return Err("Invalid workspace position".into());
        }
        let index = self
            .workspaces
            .iter()
            .position(|w| w.id == id)
            .ok_or("Workspace is unavailable")?;
        let workspace = self.workspaces.remove(index);
        self.workspaces.insert(position, workspace);
        self.normalize_order();
        self.persist()
    }

    pub fn close(&mut self, id: u64, confirmed: bool) -> Result<Snapshot, String> {
        let index = self
            .workspaces
            .iter()
            .position(|w| w.id == id)
            .ok_or("Workspace is unavailable")?;
        let session_ids: Vec<_> = self.workspaces[index]
            .tabs
            .iter()
            .filter_map(|tab| self.sessions.get(&tab.id).map(|s| s.id))
            .collect();
        if !confirmed
            && session_ids.iter().any(|id| {
                self.sessions
                    .values_mut()
                    .find(|s| s.id == *id)
                    .is_some_and(|s| matches!(s.status(), Ok(Status::Running)))
            })
        {
            return Err(
                "Explicit confirmation is required to terminate this workspace's terminal sessions"
                    .into(),
            );
        }
        for session in session_ids {
            self.terminate_session(session, confirmed)?;
        }
        self.workspaces.remove(index);
        self.normalize_order();
        if self.selected_id == Some(id) {
            self.selected_id = self
                .workspaces
                .get(index.min(self.workspaces.len().saturating_sub(1)))
                .map(|w| w.id);
        }
        self.persist()
    }

    fn normalize_order(&mut self) {
        for (order, workspace) in self.workspaces.iter_mut().enumerate() {
            workspace.display_order = order;
        }
    }

    fn tab_workspace_mut(&mut self, tab_id: u64) -> Result<&mut Workspace, String> {
        self.workspaces
            .iter_mut()
            .find(|w| w.tabs.iter().any(|t| t.id == tab_id))
            .ok_or("Tab is unavailable".into())
    }
}

fn valid_name(name: String) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Workspace name is required".into());
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
