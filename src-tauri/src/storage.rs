use crate::session::{Project, Session, Snapshot, TabConfig, TerminalTab};
use rusqlite::{params, Connection, TransactionBehavior};
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
    time::Duration,
};

#[derive(Clone)]
pub struct Metadata {
    pub project_sequence: u64,
    pub snapshot: Snapshot,
    pub session_sequence: u64,
    pub tab_sequence: u64,
}

// Explicit unlock releases ownership even if another thread briefly forked a
// CLOEXEC duplicate during PTY launch. Closing our descriptor alone can leave
// that duplicate holding flock until its exec completes.
struct MetadataLock(File);
impl Drop for MetadataLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

pub struct MetadataStore {
    connection: Connection,
    // A second writer must not overwrite metadata or relaunch commands twice.
    _lock: MetadataLock,
}

impl MetadataStore {
    pub fn open(directory: &Path) -> Result<Self, String> {
        fs::create_dir_all(directory)
            .map_err(|e| format!("Cannot create application data directory: {e}"))?;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(directory.join("metadata.lock"))
            .map_err(|e| e.to_string())?;
        lock.try_lock().map_err(|e| {
            format!(
                "Cannot lock session metadata; another application instance may be running: {e}"
            )
        })?;
        let lock = MetadataLock(lock);
        let path = directory.join("workspaces.sqlite3");
        let mut connection =
            Connection::open(&path).map_err(|e| format!("Cannot open session database: {e}"))?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
        connection
            .busy_timeout(Duration::from_millis(500))
            .map_err(|e| e.to_string())?;
        connection
            .execute_batch("PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL;")
            .map_err(|e| e.to_string())?;
        let version: u32 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|e| e.to_string())?;
        if version > 3 {
            return Err(format!(
                "Unsupported session database schema version {version}; the database was not reset"
            ));
        }
        if version < 2 {
            // Rebuild the referenced workspace table without cascading deletion
            // of its tabs. Verify all relationships before committing migration.
            connection
                .execute_batch("PRAGMA foreign_keys=OFF;")
                .map_err(|e| e.to_string())?;
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;
            if version == 0 {
                transaction
                    .execute_batch(include_str!("../migrations/001_metadata.sql"))
                    .map_err(|e| e.to_string())?;
            }
            transaction
                .execute_batch(include_str!("../migrations/002_workgroups.sql"))
                .map_err(|e| format!("Workgroup migration failed: {e}"))?;
            if transaction
                .query_row("SELECT COUNT(*) FROM workspaces", [], |r| {
                    r.get::<_, i64>(0)
                })
                .map_err(|e| e.to_string())?
                == 0
            {
                let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
                transaction
                    .execute("UPDATE workgroups SET root_directory=?1 WHERE id=1", [home])
                    .map_err(|e| e.to_string())?;
            }
            let violations: i64 = transaction
                .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
                    row.get(0)
                })
                .map_err(|e| e.to_string())?;
            if violations != 0 {
                return Err("Workgroup migration found invalid metadata relationships; database was not changed".into());
            }
            transaction.commit().map_err(|e| e.to_string())?;
            connection
                .execute_batch("PRAGMA foreign_keys=ON;")
                .map_err(|e| e.to_string())?;
        }
        if version < 3 {
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;
            transaction
                .execute_batch(include_str!("../migrations/003_projects_sessions.sql"))
                .map_err(|e| format!("Project/session migration failed: {e}"))?;
            let violations: i64 = transaction
                .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
                    row.get(0)
                })
                .map_err(|e| e.to_string())?;
            if violations != 0 {
                return Err("Project/session migration found invalid metadata relationships; database was not changed".into());
            }
            transaction.commit().map_err(|e| e.to_string())?;
        }
        Ok(Self {
            connection,
            _lock: lock,
        })
    }

    pub fn load(&self) -> Result<Metadata, String> {
        let result = (|| -> rusqlite::Result<Metadata> {
            let (selected_id, session_sequence, tab_sequence, project_sequence) = self.connection.query_row(
                "SELECT selected_session_id, session_sequence, tab_sequence, project_sequence FROM app_state WHERE singleton=1",
                [], |row| Ok((optional_id(row, 0)?, read_id(row, 1)?, read_id(row, 2)?, read_id(row, 3)?)))?;
            let mut groups = self.connection.prepare("SELECT id,name,root_directory,display_order,collapsed FROM projects ORDER BY display_order")?;
            let projects = groups
                .query_map([], |row| {
                    Ok(Project {
                        id: read_id(row, 0)?,
                        name: row.get(1)?,
                        root_directory: row.get(2)?,
                        display_order: read_order(row, 3)?,
                        collapsed: row.get(4)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let mut statement = self.connection.prepare("SELECT id,name,base_directory,display_order,selected_tab_id,project_id FROM sessions ORDER BY display_order")?;
            let mut sessions = statement
                .query_map([], |row| {
                    Ok(Session {
                        project_id: read_id(row, 5)?,
                        id: read_id(row, 0)?,
                        name: row.get(1)?,
                        base_directory: row.get(2)?,
                        display_order: read_order(row, 3)?,
                        selected_tab_id: optional_id(row, 4)?,
                        tabs: vec![],
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            for session in &mut sessions {
                let mut statement = self.connection.prepare("SELECT d.id,d.name,d.kind,d.config_json,d.display_order,o.definition_id IS NOT NULL,COALESCE(o.resume_on_start,0) FROM tab_definitions d LEFT JOIN open_tabs o ON o.definition_id=d.id WHERE d.session_id=? ORDER BY d.display_order")?;
                session.tabs = statement
                    .query_map([sql_id(session.id)?], |row| {
                        let json: String = row.get(3)?;
                        let config: TabConfig = serde_json::from_str(&json).map_err(|e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                3,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        })?;
                        Ok(TerminalTab {
                            id: read_id(row, 0)?,
                            name: row.get(1)?,
                            kind: row.get(2)?,
                            config,
                            display_order: read_order(row, 4)?,
                            is_open: row.get(5)?,
                            resume_on_start: row.get(6)?,
                        })
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
            }
            Ok(Metadata {
                project_sequence,
                snapshot: Snapshot {
                    projects,
                    sessions,
                    selected_id,
                },
                session_sequence,
                tab_sequence,
            })
        })();
        result.map_err(|e| format!("Cannot read session metadata; database was not reset: {e}"))
    }

    pub fn save(&mut self, metadata: &Metadata) -> Result<(), String> {
        let result = (|| -> rusqlite::Result<()> {
            let transaction = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            // One small metadata snapshot is committed atomically. No PTY data,
            // inherited environment, or historical terminal output is stored.
            transaction.execute_batch(
                "DELETE FROM open_tabs; DELETE FROM tab_definitions; DELETE FROM sessions; DELETE FROM projects;",
            )?;
            for group in &metadata.snapshot.projects {
                transaction.execute(
                    "INSERT INTO projects VALUES (?1,?2,?3,?4,?5)",
                    params![
                        sql_id(group.id)?,
                        group.name,
                        group.root_directory,
                        sql_order(group.display_order)?,
                        group.collapsed
                    ],
                )?;
            }
            for session in &metadata.snapshot.sessions {
                transaction.execute(
                    "INSERT INTO sessions VALUES (?1,?2,?3,?4,?5,?6)",
                    params![
                        sql_id(session.id)?,
                        session.name,
                        session.base_directory,
                        sql_order(session.display_order)?,
                        session.selected_tab_id.map(sql_id).transpose()?,
                        sql_id(session.project_id)?
                    ],
                )?;
                for tab in &session.tabs {
                    let config = serde_json::to_string(&tab.config)
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                    transaction.execute(
                        "INSERT INTO tab_definitions VALUES (?1,?2,?3,?4,?5,?6)",
                        params![
                            sql_id(tab.id)?,
                            sql_id(session.id)?,
                            tab.name,
                            tab.kind,
                            config,
                            sql_order(tab.display_order)?
                        ],
                    )?;
                    if tab.is_open {
                        transaction.execute(
                            "INSERT INTO open_tabs VALUES (?1,?2)",
                            params![sql_id(tab.id)?, tab.resume_on_start],
                        )?;
                    }
                }
            }
            transaction.execute("UPDATE app_state SET selected_session_id=?1,session_sequence=?2,tab_sequence=?3,project_sequence=?4 WHERE singleton=1",params![metadata.snapshot.selected_id.map(sql_id).transpose()?,sql_id(metadata.session_sequence)?,sql_id(metadata.tab_sequence)?,sql_id(metadata.project_sequence)?])?;
            transaction.commit()
        })();
        result.map_err(|e| format!("Cannot save session metadata: {e}"))
    }

    #[cfg(test)]
    pub fn reject_writes(&self) {
        self.connection
            .execute_batch("PRAGMA query_only=ON;")
            .unwrap();
    }
}

fn sql_id(value: u64) -> rusqlite::Result<i64> {
    i64::try_from(value).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}
fn sql_order(value: usize) -> rusqlite::Result<i64> {
    i64::try_from(value).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}
fn read_id(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u64> {
    u64::try_from(row.get::<_, i64>(index)?).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Integer,
            Box::new(e),
        )
    })
}
fn read_order(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<usize> {
    usize::try_from(row.get::<_, i64>(index)?).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Integer,
            Box::new(e),
        )
    })
}
fn optional_id(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Option<u64>> {
    row.get::<_, Option<i64>>(index)?
        .map(u64::try_from)
        .transpose()
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                index,
                rusqlite::types::Type::Integer,
                Box::new(e),
            )
        })
}

#[cfg(test)]
mod tests;
