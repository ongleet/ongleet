//! Durable, plugin-independent archives. Purging only touches application-owned artifacts.
use super::{PluginTab, Project, Session, SessionRegistry, Snapshot};
use crate::terminal_session::Status;
use serde::{Deserialize, Serialize};

pub const RETENTION_SECONDS: u64 = 30 * 24 * 60 * 60;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Archive {
    pub version: u32,
    pub id: u64,
    pub archived_at: u64,
    pub purging: bool,
    pub item: ArchivedItem,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum ArchivedItem {
    Session {
        session: Session,
        project: Project,
    },
    Tab {
        session: Session,
        project: Project,
        tab: PluginTab,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveSummary {
    id: u64,
    kind: &'static str,
    name: String,
    session_name: String,
    project_name: String,
    archived_at: u64,
    expires_at: u64,
    purging: bool,
    restart_terminals: usize,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveList {
    pub entries: Vec<ArchiveSummary>,
    pub cleanup_error: Option<String>,
}

pub fn now() -> Result<u64, String> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|time| time.as_secs())
        .map_err(|e| format!("Cannot determine archive age: {e}"))
}
impl Archive {
    fn context(&self) -> (&Session, &Project) {
        match &self.item {
            ArchivedItem::Session { session, project }
            | ArchivedItem::Tab {
                session, project, ..
            } => (session, project),
        }
    }
    fn tabs(&self) -> Vec<&PluginTab> {
        match &self.item {
            ArchivedItem::Session { session, .. } => session.tabs.iter().collect(),
            ArchivedItem::Tab { tab, .. } => vec![tab],
        }
    }
}

impl SessionRegistry {
    pub fn archive_exists(&self, id: u64) -> bool {
        self.archives.iter().any(|archive| archive.id == id)
    }

    pub fn archives(&self) -> ArchiveList {
        ArchiveList {
            cleanup_error: self.archive_cleanup_error.clone(),
            entries: self
                .archives
                .iter()
                .rev()
                .map(|archive| {
                    let (session, project) = archive.context();
                    let (kind, name) = match &archive.item {
                        ArchivedItem::Session { session, .. } => ("session", session.name.clone()),
                        ArchivedItem::Tab { tab, .. } => ("tab", tab.name.clone()),
                    };
                    ArchiveSummary {
                        id: archive.id,
                        kind,
                        name,
                        session_name: session.name.clone(),
                        project_name: project.name.clone(),
                        archived_at: archive.archived_at,
                        expires_at: archive.archived_at.saturating_add(RETENTION_SECONDS),
                        purging: archive.purging,
                        restart_terminals: archive
                            .tabs()
                            .iter()
                            .filter(|tab| {
                                tab.kind == "terminal" && tab.is_open && tab.resume_on_start
                            })
                            .count(),
                    }
                })
                .collect(),
        }
    }

    pub(super) fn capture_restart_flags(&mut self, tabs: &mut [PluginTab]) {
        for tab in tabs {
            if tab.kind == "terminal" {
                tab.resume_on_start = tab.is_open
                    && self
                        .terminals
                        .get_mut(&tab.id)
                        .is_some_and(|terminal| matches!(terminal.status(), Ok(Status::Running)));
            }
        }
    }
    pub(super) fn prepare_archive(&self, item: ArchivedItem) -> Result<Archive, String> {
        Ok(Archive {
            version: 1,
            id: self
                .next_archive_id
                .checked_add(1)
                .ok_or("Archive ID limit reached")?,
            archived_at: now()?,
            purging: false,
            item,
        })
    }
    pub(super) fn record_archive(&mut self, archive: Archive) {
        self.next_archive_id = archive.id;
        self.archives.push(archive);
    }

    pub fn restore_archive(&mut self, id: u64) -> Result<Snapshot, String> {
        let archive = self
            .archives
            .iter()
            .find(|archive| archive.id == id)
            .ok_or("Archive is unavailable")?
            .clone();
        if archive.purging {
            return Err("Archive deletion has started and cannot be undone".into());
        }
        if now()? >= archive.archived_at.saturating_add(RETENTION_SECONDS) {
            return Err("Archive has expired and cannot be restored".into());
        }
        let (parent, project) = archive.context();
        let parent_id = parent.id;
        if let ArchivedItem::Session { session, .. } = &archive.item {
            if self.sessions.iter().any(|existing| existing.id == session.id) {
                return Err("A session with this ID already exists".into());
            }
        } else if !self.sessions.iter().any(|session| session.id == parent_id)
            && self.archives.iter().any(|entry| matches!(&entry.item, ArchivedItem::Session { session, .. } if session.id == parent_id)) {
            return Err("Restore the archived parent session before restoring its tab".into());
        }
        if archive.tabs().iter().any(|tab| {
            self.sessions
                .iter()
                .flat_map(|session| &session.tabs)
                .any(|existing| existing.id == tab.id)
        }) {
            return Err("An archived tab ID is already active".into());
        }
        if !self
            .projects
            .iter()
            .any(|existing| existing.id == project.id)
        {
            let mut project = project.clone();
            project.display_order = self.projects.len();
            project.collapsed = false;
            self.projects.push(project);
        }
        match archive.item {
            ArchivedItem::Session { mut session, .. } => {
                session.display_order = self.sessions.len();
                self.sessions.push(session);
            }
            ArchivedItem::Tab {
                mut session,
                mut tab,
                ..
            } => {
                if !self
                    .sessions
                    .iter()
                    .any(|existing| existing.id == parent_id)
                {
                    session.display_order = self.sessions.len();
                    self.sessions.push(session);
                }
                let parent = self
                    .sessions
                    .iter_mut()
                    .find(|session| session.id == parent_id)
                    .unwrap();
                tab.display_order = parent.tabs.len();
                // Preserve the saved opening and restart state; no PTY is created here.
                if tab.is_open {
                    parent.selected_tab_id = Some(tab.id);
                }
                parent.tabs.push(tab);
            }
        }
        self.selected_id = Some(parent_id);
        if let Some(project_id) = self
            .sessions
            .iter()
            .find(|session| session.id == parent_id)
            .map(|session| session.project_id)
        {
            if let Some(project) = self
                .projects
                .iter_mut()
                .find(|project| project.id == project_id)
            {
                project.collapsed = false;
            }
        }
        self.archives.retain(|archive| archive.id != id);
        self.normalize_order();
        self.persist()
    }

    pub fn purge_archive(&mut self, id: u64, confirmed: bool) -> Result<Snapshot, String> {
        if !confirmed {
            return Err(
                "Explicit confirmation is required to permanently delete an archive".into(),
            );
        }
        let index = self
            .archives
            .iter()
            .position(|archive| archive.id == id)
            .ok_or("Archive is unavailable")?;
        // Commit a non-restorable tombstone before removing artifacts. Failures
        // remain retryable across restarts, without resurrecting a partial archive.
        if !self.archives[index].purging {
            self.archives[index].purging = true;
            self.persist()?;
        }
        self.remove_archive_artifacts(&self.archives[index])?;
        self.archives.retain(|archive| archive.id != id);
        let snapshot = self.persist()?;
        if !self.archives.iter().any(|archive| archive.purging) {
            self.archive_cleanup_error = None;
        }
        Ok(snapshot)
    }

    fn remove_archive_artifacts(&self, archive: &Archive) -> Result<(), String> {
        let Some(root) = &self.data_directory else {
            return Ok(());
        };
        let session_id = archive.context().0.id;
        for kind in ["terminal-history", "codex-sessions"] {
            let base = root.join(kind);
            let folder = base.join(session_id.to_string());
            for directory in [&base, &folder] {
                match std::fs::symlink_metadata(directory) {
                    Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
                    Ok(_) => {
                        return Err(format!(
                            "Refusing archive cleanup through a non-directory or symlink: {}",
                            directory.display()
                        ))
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.to_string()),
                }
            }
            let entries = match std::fs::read_dir(&folder) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.to_string()),
            };
            let tabs = archive.tabs();
            for entry in entries {
                let entry = entry.map_err(|e| e.to_string())?;
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                let owned = tabs.iter().any(|tab| {
                    let id = tab.id;
                    if kind == "terminal-history" {
                        name == format!("{id}.txt") || name == format!("{id}.tmp")
                    } else {
                        name == format!("{id}.json")
                            || name == format!("{id}.lock")
                            || (name.starts_with(&format!("{id}.")) && name.ends_with(".tmp"))
                    }
                });
                if owned {
                    match std::fs::remove_file(entry.path()) {
                        Ok(()) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(error) => {
                            return Err(format!("Cannot purge {}: {error}", entry.path().display()))
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn purge_expired_archives(&mut self, timestamp: u64) -> Result<usize, String> {
        let ids: Vec<_> = self
            .archives
            .iter()
            .filter(|archive| {
                archive.purging
                    || timestamp >= archive.archived_at.saturating_add(RETENTION_SECONDS)
            })
            .map(|archive| archive.id)
            .collect();
        let mut count = 0;
        let mut failures = Vec::new();
        for id in ids {
            match self.purge_archive(id, true) {
                Ok(_) => count += 1,
                Err(error) => failures.push(format!("Archive {id}: {error}")),
            }
        }
        if failures.is_empty() {
            self.archive_cleanup_error = None;
            Ok(count)
        } else {
            let error = failures.join("; ");
            self.archive_cleanup_error = Some(error.clone());
            Err(error)
        }
    }
    pub(super) fn run_archive_maintenance(&mut self) {
        if let Err(error) = now().and_then(|timestamp| self.purge_expired_archives(timestamp)) {
            self.archive_cleanup_error = Some(error);
        }
    }
}

#[cfg(test)]
#[path = "archives_tests.rs"]
mod tests;
