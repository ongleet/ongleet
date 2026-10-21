use crate::workspace::{Snapshot, TabConfig, TerminalTab, Workspace};
use rusqlite::{params, Connection, TransactionBehavior};
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
    time::Duration,
};

#[derive(Clone)]
pub struct Metadata {
    pub snapshot: Snapshot,
    pub workspace_sequence: u64,
    pub tab_sequence: u64,
}

pub struct MetadataStore {
    connection: Connection,
    // A second writer must not overwrite metadata or relaunch commands twice.
    _lock: File,
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
                "Cannot lock workspace metadata; another application instance may be running: {e}"
            )
        })?;
        let path = directory.join("workspaces.sqlite3");
        let mut connection =
            Connection::open(&path).map_err(|e| format!("Cannot open workspace database: {e}"))?;
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
        match version {
            0 => {
                let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e| e.to_string())?;
                transaction.execute_batch(include_str!("../migrations/001_metadata.sql")).map_err(|e| format!("Metadata migration failed: {e}"))?;
                transaction.commit().map_err(|e| e.to_string())?;
            }
            1 => {}
            _ => return Err(format!("Unsupported workspace database schema version {version}; the database was not reset")),
        }
        Ok(Self {
            connection,
            _lock: lock,
        })
    }

    pub fn load(&self) -> Result<Metadata, String> {
        let result = (|| -> rusqlite::Result<Metadata> {
            let (selected_id, workspace_sequence, tab_sequence) = self.connection.query_row(
                "SELECT selected_workspace_id, workspace_sequence, tab_sequence FROM app_state WHERE singleton=1",
                [], |row| Ok((optional_id(row, 0)?, read_id(row, 1)?, read_id(row, 2)?)))?;
            let mut statement = self.connection.prepare("SELECT id,name,base_directory,display_order,selected_tab_id FROM workspaces ORDER BY display_order")?;
            let mut workspaces = statement
                .query_map([], |row| {
                    Ok(Workspace {
                        id: read_id(row, 0)?,
                        name: row.get(1)?,
                        base_directory: row.get(2)?,
                        display_order: read_order(row, 3)?,
                        selected_tab_id: optional_id(row, 4)?,
                        tabs: vec![],
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            for workspace in &mut workspaces {
                let mut statement = self.connection.prepare("SELECT d.id,d.name,d.kind,d.config_json,d.display_order,o.definition_id IS NOT NULL,COALESCE(o.resume_on_start,0) FROM tab_definitions d LEFT JOIN open_tabs o ON o.definition_id=d.id WHERE d.workspace_id=? ORDER BY d.display_order")?;
                workspace.tabs = statement
                    .query_map([sql_id(workspace.id)?], |row| {
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
                snapshot: Snapshot {
                    workspaces,
                    selected_id,
                },
                workspace_sequence,
                tab_sequence,
            })
        })();
        result.map_err(|e| format!("Cannot read workspace metadata; database was not reset: {e}"))
    }

    pub fn save(&mut self, metadata: &Metadata) -> Result<(), String> {
        let result = (|| -> rusqlite::Result<()> {
            let transaction = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            // One small metadata snapshot is committed atomically. No PTY data,
            // inherited environment, or historical terminal output is stored.
            transaction.execute_batch(
                "DELETE FROM open_tabs; DELETE FROM tab_definitions; DELETE FROM workspaces;",
            )?;
            for workspace in &metadata.snapshot.workspaces {
                transaction.execute(
                    "INSERT INTO workspaces VALUES (?1,?2,?3,?4,?5)",
                    params![
                        sql_id(workspace.id)?,
                        workspace.name,
                        workspace.base_directory,
                        sql_order(workspace.display_order)?,
                        workspace.selected_tab_id.map(sql_id).transpose()?
                    ],
                )?;
                for tab in &workspace.tabs {
                    let config = serde_json::to_string(&tab.config)
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                    transaction.execute(
                        "INSERT INTO tab_definitions VALUES (?1,?2,?3,?4,?5,?6)",
                        params![
                            sql_id(tab.id)?,
                            sql_id(workspace.id)?,
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
            transaction.execute("UPDATE app_state SET selected_workspace_id=?1,workspace_sequence=?2,tab_sequence=?3 WHERE singleton=1",params![metadata.snapshot.selected_id.map(sql_id).transpose()?,sql_id(metadata.workspace_sequence)?,sql_id(metadata.tab_sequence)?])?;
            transaction.commit()
        })();
        result.map_err(|e| format!("Cannot save workspace metadata: {e}"))
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
