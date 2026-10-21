use crate::session::{Launch, Output, TerminalSession};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use tauri::ipc::Channel;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: u64,
    pub name: String,
    pub base_directory: String,
    pub display_order: usize,
    pub tabs: Vec<TerminalTab>,
    pub selected_tab_id: Option<u64>,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
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
    pub id: u64,
    pub name: String,
    pub display_order: usize,
    pub config: TabConfig,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub workspaces: Vec<Workspace>,
    pub selected_id: Option<u64>,
}

// In-memory metadata for the sidebar stage. SQLite will persist these same
// identities and settings later; navigation never owns native PTY resources.
#[derive(Default)]
pub struct WorkspaceRegistry {
    workspaces: Vec<Workspace>,
    selected_id: Option<u64>,
    next_id: u64,
    next_session_id: u64,
    next_tab_id: u64,
    // Keyed by tab ID, never by navigation selection or workspace position.
    pub sessions: HashMap<u64, TerminalSession>,
}

impl WorkspaceRegistry {
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            workspaces: self.workspaces.clone(),
            selected_id: self.selected_id,
        }
    }

    pub fn create(&mut self, name: String, directory: String) -> Result<Snapshot, String> {
        let name = valid_name(name)?;
        validate_directory(&directory)?;
        self.next_id += 1;
        self.workspaces.push(Workspace {
            id: self.next_id,
            name,
            base_directory: directory,
            display_order: self.workspaces.len(),
            tabs: vec![],
            selected_tab_id: None,
        });
        self.selected_id = Some(self.next_id);
        self.create_tab(self.next_id)
    }

    pub fn update(&mut self, id: u64, name: String, directory: String) -> Result<Snapshot, String> {
        let name = valid_name(name)?;
        let workspace = self
            .workspaces
            .iter_mut()
            .find(|w| w.id == id)
            .ok_or("Workspace is unavailable")?;
        // Renaming must still work if a previously saved directory disappears.
        if directory != workspace.base_directory {
            validate_directory(&directory)?;
        }
        workspace.name = name;
        workspace.base_directory = directory;
        Ok(self.snapshot())
    }

    pub fn select(&mut self, id: u64) -> Result<Snapshot, String> {
        if !self.workspaces.iter().any(|w| w.id == id) {
            return Err("Workspace is unavailable".into());
        }
        self.selected_id = Some(id);
        Ok(self.snapshot())
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
        Ok(self.snapshot())
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
        let launch = Launch {
            executable,
            args,
            cwd: config
                .cwd_override
                .clone()
                .unwrap_or_else(|| workspace.base_directory.clone()),
            env: config.env.clone(),
            cols,
            rows,
        };
        self.next_session_id += 1;
        let id = self.next_session_id;
        self.sessions
            .insert(tab_id, TerminalSession::spawn(id, launch, output)?);
        Ok(id)
    }

    pub fn close_session(&mut self, id: u64, confirmed: bool) -> Result<(), String> {
        let key = self
            .sessions
            .iter()
            .find(|(_, s)| s.id == id)
            .map(|(key, _)| *key)
            .ok_or("Session is unavailable")?;
        if !confirmed {
            return Err("Explicit confirmation is required to terminate session processes".into());
        }
        self.sessions.get_mut(&key).unwrap().close()?;
        self.sessions.remove(&key);
        Ok(())
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
        if !session_ids.is_empty() && !confirmed {
            return Err(
                "Explicit confirmation is required to terminate this workspace's terminal sessions"
                    .into(),
            );
        }
        for session in session_ids {
            self.close_session(session, confirmed)?;
        }
        self.workspaces.remove(index);
        self.normalize_order();
        if self.selected_id == Some(id) {
            self.selected_id = self
                .workspaces
                .get(index.min(self.workspaces.len().saturating_sub(1)))
                .map(|w| w.id);
        }
        Ok(self.snapshot())
    }

    pub fn shutdown(&mut self, confirmed: bool) -> Result<(), String> {
        if !self.sessions.is_empty() && !confirmed {
            return Err("Explicit confirmation is required to terminate session processes".into());
        }
        let ids: Vec<_> = self.sessions.values().map(|s| s.id).collect();
        for id in ids {
            self.close_session(id, confirmed)?;
        }
        Ok(())
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

    pub fn create_tab(&mut self, workspace_id: u64) -> Result<Snapshot, String> {
        let workspace = self
            .workspaces
            .iter_mut()
            .find(|w| w.id == workspace_id)
            .ok_or("Workspace is unavailable")?;
        self.next_tab_id += 1;
        workspace.tabs.push(TerminalTab {
            id: self.next_tab_id,
            name: format!("Terminal {}", self.next_tab_id),
            display_order: workspace.tabs.len(),
            config: TabConfig::default(),
        });
        workspace.selected_tab_id = Some(self.next_tab_id);
        Ok(self.snapshot())
    }

    pub fn select_tab(&mut self, id: u64) -> Result<Snapshot, String> {
        self.tab_workspace_mut(id)?.selected_tab_id = Some(id);
        Ok(self.snapshot())
    }

    pub fn rename_tab(&mut self, id: u64, name: String) -> Result<Snapshot, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Tab name is required".into());
        }
        let workspace = self.tab_workspace_mut(id)?;
        workspace.tabs.iter_mut().find(|t| t.id == id).unwrap().name = name.into();
        Ok(self.snapshot())
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
        Ok(self.snapshot())
    }

    pub fn reorder_tab(&mut self, id: u64, position: usize) -> Result<Snapshot, String> {
        let workspace = self.tab_workspace_mut(id)?;
        if position >= workspace.tabs.len() {
            return Err("Invalid tab position".into());
        }
        let index = workspace.tabs.iter().position(|t| t.id == id).unwrap();
        let tab = workspace.tabs.remove(index);
        workspace.tabs.insert(position, tab);
        for (order, tab) in workspace.tabs.iter_mut().enumerate() {
            tab.display_order = order;
        }
        Ok(self.snapshot())
    }

    pub fn close_tab(&mut self, id: u64, confirmed: bool) -> Result<Snapshot, String> {
        self.tab_workspace_mut(id)?;
        if let Some(session) = self.sessions.get(&id) {
            self.close_session(session.id, confirmed)?;
        }
        let workspace = self.tab_workspace_mut(id)?;
        let index = workspace.tabs.iter().position(|t| t.id == id).unwrap();
        workspace.tabs.remove(index);
        for (order, tab) in workspace.tabs.iter_mut().enumerate() {
            tab.display_order = order;
        }
        if workspace.selected_tab_id == Some(id) {
            workspace.selected_tab_id = workspace
                .tabs
                .get(index.min(workspace.tabs.len().saturating_sub(1)))
                .map(|t| t.id);
        }
        Ok(self.snapshot())
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
mod tests {
    use super::*;
    use crate::session::Status;
    use std::{
        fs,
        sync::{
            atomic::{AtomicU64, Ordering},
            mpsc,
        },
        time::Duration,
    };
    use tauri::ipc::InvokeResponseBody;

    struct Directory(std::path::PathBuf);
    impl Directory {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(1);
            let path = std::env::temp_dir().join(format!(
                "workspace-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn path(&self) -> String {
            self.0.to_str().unwrap().into()
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn metadata_operations_preserve_identity_order_selection_and_directories() {
        let directory = Directory::new();
        fs::write(directory.0.join("keep.txt"), b"untouched").unwrap();
        let other = Directory::new();
        let mut registry = WorkspaceRegistry::default();
        let first = registry
            .create("First".into(), directory.path())
            .unwrap()
            .selected_id
            .unwrap();
        let second = registry
            .create("Second".into(), directory.path())
            .unwrap()
            .selected_id
            .unwrap();
        assert_ne!(first, second);
        registry.select(first).unwrap();
        let state = registry.reorder(first, 1).unwrap();
        assert_eq!(state.selected_id, Some(first));
        assert_eq!(
            state
                .workspaces
                .iter()
                .map(|w| (w.id, w.display_order))
                .collect::<Vec<_>>(),
            vec![(second, 0), (first, 1)]
        );
        let state = registry
            .update(first, "Renamed".into(), other.path())
            .unwrap();
        assert_eq!(state.workspaces[1].id, first);
        assert_eq!(state.workspaces[1].name, "Renamed");
        assert_eq!(state.workspaces[1].base_directory, other.path());
        let state = registry.close(first, false).unwrap();
        assert_eq!(state.selected_id, Some(second));
        assert_eq!(state.workspaces[0].display_order, 0);
        assert!(registry.close(second, false).unwrap().selected_id.is_none());
        assert_eq!(
            fs::read(directory.0.join("keep.txt")).unwrap(),
            b"untouched"
        );
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
        assert!(other.0.is_dir());
    }

    #[test]
    fn invalid_updates_are_atomic_and_missing_directory_does_not_prevent_rename() {
        let directory = Directory::new();
        let mut registry = WorkspaceRegistry::default();
        assert!(registry.create(" ".into(), directory.path()).is_err());
        let id = registry
            .create("Keep".into(), directory.path())
            .unwrap()
            .selected_id
            .unwrap();
        let missing = directory.0.join("missing").to_str().unwrap().to_owned();
        assert!(registry.create("Invalid".into(), missing.clone()).is_err());
        assert!(registry
            .update(id, "Wrong".into(), missing.clone())
            .is_err());
        assert!(registry.reorder(id, 8).is_err());
        assert!(registry.select(999).is_err());
        assert_eq!(registry.snapshot().selected_id, Some(id));
        assert_eq!(registry.snapshot().workspaces[0].name, "Keep");
        assert!(!std::path::Path::new(&missing).exists());
        fs::remove_dir(&directory.0).unwrap();
        let state = registry
            .update(id, "Renamed".into(), directory.path())
            .unwrap();
        assert_eq!(state.workspaces[0].name, "Renamed");
        assert_eq!(state.workspaces[0].base_directory, directory.path());
        assert!(!directory.0.exists());
    }

    fn start_test(
        registry: &mut WorkspaceRegistry,
        tab: u64,
        directory: String,
        channel: Channel<Output>,
    ) -> u64 {
        registry.configure_tab(tab, TabConfig {
            executable: Some("/usr/bin/python3".into()),
            args: vec!["-u".into(), "-c".into(), "import os,time; print(os.getcwd(),flush=True); input(); print(os.getcwd(),flush=True); time.sleep(60)".into()],
            cwd_override: Some(directory), ..Default::default()
        }).unwrap();
        registry.start(tab, 80, 24, channel).unwrap()
    }

    fn output() -> (Channel<Output>, mpsc::Receiver<Output>) {
        let (send, receive) = mpsc::channel();
        let channel = Channel::new(move |body| {
            if let InvokeResponseBody::Json(json) = body {
                let _ = send.send(serde_json::from_str(&json).unwrap());
            }
            Ok(())
        });
        (channel, receive)
    }

    fn read_cwd(
        registry: &WorkspaceRegistry,
        workspace: u64,
        receive: &mpsc::Receiver<Output>,
    ) -> String {
        let mut collected = Vec::new();
        loop {
            let Output::Data { seq, bytes } = receive.recv_timeout(Duration::from_secs(5)).unwrap()
            else {
                panic!("Expected working directory")
            };
            collected.extend(bytes);
            registry
                .sessions
                .get(&workspace)
                .unwrap()
                .acknowledge(seq)
                .unwrap();
            if collected.ends_with(b"\n") {
                if let Some(path) = String::from_utf8_lossy(&collected)
                    .lines()
                    .find(|line| line.starts_with('/'))
                {
                    return path.trim().to_owned();
                }
            }
        }
    }

    #[test]
    fn workspace_navigation_keeps_independent_ptys_and_closure_requires_confirmation() {
        let first_directory = Directory::new();
        let second_directory = Directory::new();
        let mut registry = WorkspaceRegistry::default();
        let first = registry
            .create("First".into(), first_directory.path())
            .unwrap()
            .selected_id
            .unwrap();
        let second = registry
            .create("Second".into(), second_directory.path())
            .unwrap()
            .selected_id
            .unwrap();
        let first_tab = registry.workspaces[0].tabs[0].id;
        let second_tab = registry.workspaces[1].tabs[0].id;
        let (channel, receive) = output();
        let first_session = start_test(&mut registry, first_tab, first_directory.path(), channel);
        assert_eq!(
            read_cwd(&registry, first_tab, &receive),
            first_directory.path()
        );
        let session_id = registry.sessions.get(&first_tab).unwrap().id;
        registry.select(second).unwrap();
        registry
            .update(first, "Renamed".into(), second_directory.path())
            .unwrap();
        registry
            .sessions
            .get(&first_tab)
            .unwrap()
            .queue_input(b"\n".to_vec())
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        assert_eq!(
            read_cwd(&registry, first_tab, &receive),
            first_directory.path(),
            "Editing workspace metadata changed a running process's cwd"
        );
        let (channel, _) = output();
        let second_session =
            start_test(&mut registry, second_tab, second_directory.path(), channel);
        assert_ne!(first_session, second_session);
        registry.select(first).unwrap();
        assert_eq!(registry.sessions.get(&first_tab).unwrap().id, session_id);
        assert!(matches!(
            registry
                .sessions
                .get_mut(&first_tab)
                .unwrap()
                .status()
                .unwrap(),
            Status::Running
        ));
        assert!(registry.close(first, false).is_err());
        assert!(registry.shutdown(false).is_err());
        assert_eq!(registry.snapshot().workspaces.len(), 2);
        registry.close(first, true).unwrap();
        assert!(matches!(
            registry
                .sessions
                .get_mut(&second_tab)
                .unwrap()
                .status()
                .unwrap(),
            Status::Running
        ));
        assert_eq!(registry.snapshot().selected_id, Some(second));
        registry.shutdown(true).unwrap();
        assert!(registry.sessions.is_empty());
        assert!(first_directory.0.is_dir() && second_directory.0.is_dir());
    }

    #[test]
    fn tab_order_names_selection_and_last_tab_close_are_workspace_local() {
        let directory = Directory::new();
        let mut registry = WorkspaceRegistry::default();
        let first = registry
            .create("First".into(), directory.path())
            .unwrap()
            .selected_id
            .unwrap();
        let a = registry.workspaces[0].tabs[0].id;
        registry.create_tab(first).unwrap();
        let b = registry.workspaces[0].selected_tab_id.unwrap();
        let second = registry
            .create("Second".into(), directory.path())
            .unwrap()
            .selected_id
            .unwrap();
        let c = registry.workspaces[1].selected_tab_id.unwrap();
        assert!(a != b && b != c && a != c);
        registry.rename_tab(a, "Shell".into()).unwrap();
        assert!(registry.rename_tab(a, " ".into()).is_err());
        registry.select_tab(a).unwrap();
        registry.reorder_tab(a, 1).unwrap();
        assert_eq!(
            registry.workspaces[0]
                .tabs
                .iter()
                .map(|t| (t.id, t.display_order))
                .collect::<Vec<_>>(),
            vec![(b, 0), (a, 1)]
        );
        assert_eq!(registry.workspaces[0].selected_tab_id, Some(a));
        assert_eq!(registry.workspaces[1].selected_tab_id, Some(c));
        assert_eq!(registry.selected_id, Some(second));
        registry.select(first).unwrap();
        assert_eq!(registry.workspaces[0].tabs[1].name, "Shell");
        assert!(registry.reorder_tab(a, 9).is_err());
        registry.close_tab(a, false).unwrap();
        assert_eq!(registry.workspaces[0].selected_tab_id, Some(b));
        registry.close_tab(b, false).unwrap();
        assert!(registry.workspaces[0].tabs.is_empty());
        assert_eq!(registry.workspaces[0].selected_tab_id, None);
        assert_eq!(registry.workspaces[1].tabs[0].id, c);
        registry.create_tab(first).unwrap();
        assert!(registry.workspaces[0].selected_tab_id.unwrap() > c);
    }

    #[test]
    fn sibling_ptys_survive_tab_changes_and_workspace_close_preflights_all_sessions() {
        let directory = Directory::new();
        let mut registry = WorkspaceRegistry::default();
        let workspace = registry
            .create("Workspace".into(), directory.path())
            .unwrap()
            .selected_id
            .unwrap();
        let a = registry.workspaces[0].tabs[0].id;
        registry.create_tab(workspace).unwrap();
        let b = registry.workspaces[0].selected_tab_id.unwrap();
        let (channel, receive_a) = output();
        let session_a = start_test(&mut registry, a, directory.path(), channel);
        assert_eq!(read_cwd(&registry, a, &receive_a), directory.path());
        let (channel, receive_b) = output();
        let session_b = start_test(&mut registry, b, directory.path(), channel);
        assert_eq!(read_cwd(&registry, b, &receive_b), directory.path());
        assert_ne!(session_a, session_b);
        registry.rename_tab(a, "Live shell".into()).unwrap();
        registry.reorder_tab(a, 1).unwrap();
        registry.select_tab(a).unwrap();
        let (channel, _) = output();
        assert!(registry.start(a, 80, 24, channel).is_err());
        assert!(registry.close(workspace, false).is_err());
        assert!(registry.close_tab(a, false).is_err());
        for (tab, session) in [(a, session_a), (b, session_b)] {
            assert_eq!(registry.sessions[&tab].id, session);
            assert!(matches!(
                registry.sessions.get_mut(&tab).unwrap().status().unwrap(),
                Status::Running
            ));
        }
        registry.close_tab(a, true).unwrap();
        assert!(!registry.sessions.contains_key(&a));
        assert!(matches!(
            registry.sessions.get_mut(&b).unwrap().status().unwrap(),
            Status::Running
        ));
        registry.close(workspace, true).unwrap();
        assert!(registry.sessions.is_empty());
        assert!(registry.workspaces.is_empty());
        assert!(directory.0.is_dir());
    }

    fn read_line(
        registry: &WorkspaceRegistry,
        tab: u64,
        receive: &mpsc::Receiver<Output>,
    ) -> String {
        let mut collected = Vec::new();
        loop {
            let Output::Data { seq, bytes } = receive.recv_timeout(Duration::from_secs(5)).unwrap()
            else {
                panic!("Expected fixture output")
            };
            collected.extend(bytes);
            registry.sessions[&tab].acknowledge(seq).unwrap();
            if collected.ends_with(b"\n") {
                return String::from_utf8(collected).unwrap().trim().to_owned();
            }
        }
    }

    #[test]
    fn tab_config_launches_direct_args_overrides_default_directory_and_explicit_shell() {
        let old_directory = Directory::new();
        let new_directory = Directory::new();
        let mut registry = WorkspaceRegistry::default();
        let workspace = registry
            .create("Workspace".into(), old_directory.path())
            .unwrap()
            .selected_id
            .unwrap();
        let a = registry.workspaces[0].tabs[0].id;
        let mut config = TabConfig {
            executable: Some("/usr/bin/python3".into()),
            args: vec!["-u".into(), "-c".into(), "import os,sys,json,time; print(json.dumps([os.getcwd(),sys.argv[1],os.environ['TAB_VALUE'],'PATH' in os.environ]),flush=True); input(); print(os.getcwd(),flush=True); time.sleep(60)".into(), "literal; echo $(shell-syntax)".into()],
            env: BTreeMap::from([("TAB_VALUE".into(), "explicit".into())]), ..Default::default()
        };
        registry.configure_tab(a, config.clone()).unwrap();
        let (channel, receive) = output();
        registry.start(a, 80, 24, channel).unwrap();
        let payload: serde_json::Value =
            serde_json::from_str(&read_line(&registry, a, &receive)).unwrap();
        assert_eq!(
            payload,
            serde_json::json!([
                old_directory.path(),
                "literal; echo $(shell-syntax)",
                "explicit",
                true
            ])
        );
        registry
            .update(workspace, "Updated".into(), new_directory.path())
            .unwrap();
        registry.sessions[&a]
            .queue_input(b"\n".to_vec())
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        assert_eq!(read_cwd(&registry, a, &receive), old_directory.path());
        registry.create_tab(workspace).unwrap();
        let b = registry.workspaces[0].selected_tab_id.unwrap();
        registry.configure_tab(b, config.clone()).unwrap();
        let (channel, receive) = output();
        registry.start(b, 80, 24, channel).unwrap();
        let payload: serde_json::Value =
            serde_json::from_str(&read_line(&registry, b, &receive)).unwrap();
        assert_eq!(payload[0], new_directory.path());
        config.cwd_override = Some(old_directory.path());
        registry.create_tab(workspace).unwrap();
        let c = registry.workspaces[0].selected_tab_id.unwrap();
        registry.configure_tab(c, config).unwrap();
        let (channel, receive) = output();
        registry.start(c, 80, 24, channel).unwrap();
        let payload: serde_json::Value =
            serde_json::from_str(&read_line(&registry, c, &receive)).unwrap();
        assert_eq!(payload[0], old_directory.path());
        assert_eq!(registry.workspaces[0].tabs[0].config.env.len(), 1);
        registry.create_tab(workspace).unwrap();
        let d = registry.workspaces[0].selected_tab_id.unwrap();
        registry
            .configure_tab(
                d,
                TabConfig {
                    shell_command: Some("printf '%s\\n' explicit-shell".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        let (channel, receive) = output();
        registry.start(d, 80, 24, channel).unwrap();
        assert_eq!(read_line(&registry, d, &receive), "explicit-shell");
        assert!(registry
            .configure_tab(
                d,
                TabConfig {
                    env: BTreeMap::from([("INVALID=KEY".into(), "value".into())]),
                    ..Default::default()
                }
            )
            .is_err());
        registry.shutdown(true).unwrap();
    }
}
