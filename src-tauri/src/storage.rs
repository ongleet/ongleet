#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredConfig {
    version: u32,
    config: serde_json::Value,
}

use crate::session::{PluginTab, Project, Session, Snapshot};
use diesel::{connection::SimpleConnection, prelude::*};
use std::sync::Mutex;

mod models;
mod schema;
use models::*;
use schema::*;
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
};

#[derive(Clone)]
pub struct Metadata {
    pub archives: Vec<crate::session::archives::Archive>,
    pub archive_sequence: u64,
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
    connection: Mutex<SqliteConnection>,
    // A second writer must not overwrite metadata or relaunch commands twice.
    _lock: MetadataLock,
}

impl MetadataStore {
    pub fn open(directory: &Path) -> Result<Self, String> {
        Self::open_inner(directory).map_err(|e| e.to_string())
    }

    fn open_inner(directory: &Path) -> StorageResult<Self> {
        fs::create_dir_all(directory)?;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(directory.join("metadata.lock"))?;
        lock.try_lock().map_err(|e| {
            format!(
                "Cannot lock session metadata; another application instance may be running: {e}"
            )
        })?;
        let lock = MetadataLock(lock);
        let path = directory.join("workspaces.sqlite3");
        let mut connection =
            SqliteConnection::establish(path.to_str().ok_or("Session database path is not UTF-8")?)
                .map_err(|e| format!("Cannot open session database: {e}"))?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        connection.batch_execute(
            "PRAGMA busy_timeout=500; PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL;",
        )?;
        migrate(&mut connection)?;
        Ok(Self {
            connection: Mutex::new(connection),
            _lock: lock,
        })
    }

    pub fn load(&self) -> Result<Metadata, String> {
        let result = (|| -> StorageResult<Metadata> {
            let mut connection = self
                .connection
                .lock()
                .map_err(|_| "Session database lock poisoned")?;
            let connection = &mut *connection;
            let state = app_state::table
                .find(1_i64)
                .select(AppStateRow::as_select())
                .first(connection)?;
            let projects = projects::table
                .order(projects::display_order)
                .select(ProjectRow::as_select())
                .load(connection)?
                .into_iter()
                .map(|row| {
                    Ok(Project {
                        id: read_id(row.id)?,
                        name: row.name,
                        root_directory: row.root_directory,
                        display_order: usize::try_from(row.display_order)?,
                        collapsed: row.collapsed,
                    })
                })
                .collect::<StorageResult<Vec<_>>>()?;
            let mut sessions = sessions::table
                .order(sessions::display_order)
                .select(SessionRow::as_select())
                .load(connection)?
                .into_iter()
                .map(|row| {
                    // Validate the legacy association even though the namespace is authoritative.
                    let pull_request = row
                        .pull_request_json
                        .as_deref()
                        .map(serde_json::from_str)
                        .transpose()?;
                    Ok(Session {
                        tags: vec![],
                        id: read_id(row.id)?,
                        name: row.name,
                        base_directory: row.base_directory,
                        display_order: usize::try_from(row.display_order)?,
                        selected_tab_id: row.selected_tab_id.map(read_id).transpose()?,
                        project_id: read_id(row.project_id)?,
                        pull_request,
                        plugin_data: serde_json::from_str(&row.plugin_data_json)?,
                        tabs: vec![],
                    })
                })
                .collect::<StorageResult<Vec<_>>>()?;
            for session in &mut sessions {
                session.pull_request = session
                    .plugin_data
                    .get("pull-request")
                    .and_then(|n| serde_json::from_value(n.value.clone()).ok());
                let rows = tab_definitions::table
                    .left_join(open_tabs::table)
                    .filter(tab_definitions::session_id.eq(sql_id(session.id)?))
                    .order(tab_definitions::display_order)
                    .select(<(TabRow, Option<OpeningRow>)>::as_select())
                    .load::<(TabRow, Option<OpeningRow>)>(connection)?;
                session.tabs = rows
                    .into_iter()
                    .map(|(row, opening)| {
                        let stored: StoredConfig = serde_json::from_str(&row.config_json)?;
                        Ok(PluginTab {
                            codex_activity: None,
                            id: read_id(row.id)?,
                            name: row.name,
                            kind: row.kind,
                            config_version: stored.version,
                            config: stored.config,
                            display_order: usize::try_from(row.display_order)?,
                            is_open: opening.is_some(),
                            resume_on_start: opening.is_some_and(|o| o.resume_on_start),
                            plugin_data: serde_json::from_str(&row.plugin_data_json)?,
                        })
                    })
                    .collect::<StorageResult<Vec<_>>>()?;
            }
            let archive_sequence = read_id(
                archive_state::table
                    .find(1_i64)
                    .select(archive_state::sequence)
                    .first(connection)?,
            )?;
            let archives = archives::table
                .order(archives::id)
                .select(ArchiveRow::as_select())
                .load(connection)?
                .into_iter()
                .map(|row| {
                    let id = read_id(row.id)?;
                    let mut archive: crate::session::archives::Archive =
                        serde_json::from_str(&row.payload_json)?;
                    if archive.id != id
                        || id > archive_sequence
                        || !matches!(archive.version, 1 | 2)
                    {
                        return Err("Invalid archive metadata".into());
                    }
                    archive.upgrade_plugin_data();
                    Ok(archive)
                })
                .collect::<StorageResult<Vec<_>>>()?;
            Ok(Metadata {
                archives,
                archive_sequence,
                project_sequence: read_id(state.project_sequence)?,
                snapshot: Snapshot {
                    projects,
                    sessions,
                    selected_id: state.selected_session_id.map(read_id).transpose()?,
                },
                session_sequence: read_id(state.session_sequence)?,
                tab_sequence: read_id(state.tab_sequence)?,
            })
        })();
        result.map_err(|e| format!("Cannot read session metadata; database was not reset: {e}"))
    }

    pub fn save(&mut self, metadata: &Metadata) -> Result<(), String> {
        let result = (|| -> StorageResult<()> {
            let connection = self
                .connection
                .get_mut()
                .map_err(|_| "Session database lock poisoned")?;
            connection.immediate_transaction::<_, StorageError, _>(|connection| {
                // Commit the metadata snapshot atomically, without PTY data or inherited environment.
                diesel::delete(open_tabs::table).execute(connection)?;
                diesel::delete(tab_definitions::table).execute(connection)?;
                diesel::delete(sessions::table).execute(connection)?;
                diesel::delete(projects::table).execute(connection)?;
                for group in &metadata.snapshot.projects {
                    diesel::insert_into(projects::table)
                        .values(ProjectRow {
                            id: sql_id(group.id)?,
                            name: group.name.clone(),
                            root_directory: group.root_directory.clone(),
                            display_order: i64::try_from(group.display_order)?,
                            collapsed: group.collapsed,
                        })
                        .execute(connection)?;
                }
                for session in &metadata.snapshot.sessions {
                    diesel::insert_into(sessions::table)
                        .values(SessionRow {
                            id: sql_id(session.id)?,
                            name: session.name.clone(),
                            base_directory: session.base_directory.clone(),
                            display_order: i64::try_from(session.display_order)?,
                            selected_tab_id: session.selected_tab_id.map(sql_id).transpose()?,
                            project_id: sql_id(session.project_id)?,
                            pull_request_json: session
                                .pull_request
                                .as_ref()
                                .map(serde_json::to_string)
                                .transpose()?,
                            plugin_data_json: serde_json::to_string(&session.plugin_data)?,
                        })
                        .execute(connection)?;
                    for tab in &session.tabs {
                        diesel::insert_into(tab_definitions::table)
                            .values(TabRow {
                                id: sql_id(tab.id)?,
                                session_id: sql_id(session.id)?,
                                name: tab.name.clone(),
                                kind: tab.kind.clone(),
                                config_json: serde_json::to_string(&StoredConfig {
                                    version: tab.config_version,
                                    config: tab.config.clone(),
                                })?,
                                display_order: i64::try_from(tab.display_order)?,
                                plugin_data_json: serde_json::to_string(&tab.plugin_data)?,
                            })
                            .execute(connection)?;
                        if tab.is_open {
                            diesel::insert_into(open_tabs::table)
                                .values(OpeningRow {
                                    definition_id: sql_id(tab.id)?,
                                    resume_on_start: tab.resume_on_start,
                                })
                                .execute(connection)?;
                        }
                    }
                }
                diesel::update(app_state::table.find(1_i64))
                    .set((
                        app_state::selected_session_id.eq(metadata
                            .snapshot
                            .selected_id
                            .map(sql_id)
                            .transpose()?),
                        app_state::session_sequence.eq(sql_id(metadata.session_sequence)?),
                        app_state::tab_sequence.eq(sql_id(metadata.tab_sequence)?),
                        app_state::project_sequence.eq(sql_id(metadata.project_sequence)?),
                    ))
                    .execute(connection)?;
                diesel::delete(archives::table).execute(connection)?;
                for archive in &metadata.archives {
                    diesel::insert_into(archives::table)
                        .values(ArchiveRow {
                            id: sql_id(archive.id)?,
                            payload_json: serde_json::to_string(archive)?,
                        })
                        .execute(connection)?;
                }
                diesel::update(archive_state::table.find(1_i64))
                    .set(archive_state::sequence.eq(sql_id(metadata.archive_sequence)?))
                    .execute(connection)?;
                Ok(())
            })
        })();
        result.map_err(|e| format!("Cannot save session metadata: {e}"))
    }

    #[cfg(test)]
    pub fn reject_writes(&self) {
        self.connection
            .lock()
            .unwrap()
            .batch_execute("PRAGMA query_only=ON;")
            .unwrap();
    }
}

type StorageError = Box<dyn std::error::Error + Send + Sync>;
type StorageResult<T> = Result<T, StorageError>;

fn sql_id(value: u64) -> StorageResult<i64> {
    Ok(i64::try_from(value)?)
}
fn read_id(value: i64) -> StorageResult<u64> {
    Ok(u64::try_from(value)?)
}

// Raw SQL is confined to historical migrations and SQLite-specific pragmas.
#[derive(diesel::QueryableByName)]
struct Scalar {
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    value: i64,
}
fn scalar(connection: &mut SqliteConnection, sql: &str) -> StorageResult<i64> {
    Ok(diesel::sql_query(sql)
        .get_result::<Scalar>(connection)?
        .value)
}

fn migrate(connection: &mut SqliteConnection) -> StorageResult<()> {
    let version = scalar(
        connection,
        "SELECT user_version AS value FROM pragma_user_version",
    )?;
    if version > 7 {
        return Err(format!(
            "Unsupported session database schema version {version}; the database was not reset"
        )
        .into());
    }
    if version < 2 {
        // Rebuild the referenced table without cascading deletion of its tabs.
        connection.batch_execute("PRAGMA foreign_keys=OFF;")?;
        connection.immediate_transaction::<_, StorageError, _>(|connection| {
            if version == 0 {
                connection.batch_execute(include_str!("../migrations/001_metadata.sql"))?;
            }
            connection
                .batch_execute(include_str!("../migrations/002_workgroups.sql"))
                .map_err(|e| format!("Workgroup migration failed: {e}"))?;
            if scalar(connection, "SELECT count(*) AS value FROM workspaces")? == 0 {
                let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
                diesel::sql_query("UPDATE workgroups SET root_directory=? WHERE id=1")
                    .bind::<diesel::sql_types::Text, _>(home)
                    .execute(connection)?;
            }
            check_relationships(connection, "Workgroup")
        })?;
        connection.batch_execute("PRAGMA foreign_keys=ON;")?;
    }
    if version < 3 {
        connection.immediate_transaction::<_, StorageError, _>(|connection| {
            connection
                .batch_execute(include_str!("../migrations/003_projects_sessions.sql"))
                .map_err(|e| format!("Project/session migration failed: {e}"))?;
            check_relationships(connection, "Project/session")
        })?;
    }
    for (target, name, sql) in [
        (4, "Plugin", include_str!("../migrations/004_plugins.sql")),
        (
            5,
            "Session PR",
            include_str!("../migrations/005_session_pull_requests.sql"),
        ),
        (6, "Archive", include_str!("../migrations/006_archives.sql")),
        (
            7,
            "Plugin data",
            include_str!("../migrations/007_plugin_data.sql"),
        ),
    ] {
        if version >= target {
            continue;
        }
        connection.immediate_transaction::<_, StorageError, _>(|connection| {
            // Both schema-4 variants exist; do not wrap plugin payloads twice.
            let sql = if target == 5 && scalar(connection,
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('sessions') WHERE name='pull_request_json') AS value")? != 0 {
                "PRAGMA user_version=5;"
            } else { sql };
            connection.batch_execute(sql).map_err(|e| format!("{name} migration failed: {e}"))?;
            Ok(())
        })?;
    }
    Ok(())
}

fn check_relationships(connection: &mut SqliteConnection, name: &str) -> StorageResult<()> {
    if scalar(
        connection,
        "SELECT count(*) AS value FROM pragma_foreign_key_check",
    )? != 0
    {
        return Err(format!(
            "{name} migration found invalid metadata relationships; database was not changed"
        )
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
