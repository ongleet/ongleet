use super::schema::*;
use diesel::{Insertable, Queryable, Selectable};

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = projects)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub(super) struct ProjectRow {
    pub id: i64,
    pub name: String,
    pub root_directory: String,
    pub display_order: i64,
    pub collapsed: bool,
}

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = sessions)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub(super) struct SessionRow {
    pub id: i64,
    pub name: String,
    pub base_directory: String,
    pub display_order: i64,
    pub selected_tab_id: Option<i64>,
    pub project_id: i64,
    pub pull_request_json: Option<String>,
    pub plugin_data_json: String,
}

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = tab_definitions)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub(super) struct TabRow {
    pub id: i64,
    pub session_id: i64,
    pub name: String,
    pub kind: String,
    pub config_json: String,
    pub display_order: i64,
    pub plugin_data_json: String,
}

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = open_tabs)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub(super) struct OpeningRow {
    pub definition_id: i64,
    pub resume_on_start: bool,
}

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = app_state)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub(super) struct AppStateRow {
    pub singleton: i64,
    pub selected_session_id: Option<i64>,
    pub session_sequence: i64,
    pub tab_sequence: i64,
    pub project_sequence: i64,
}

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = archives)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub(super) struct ArchiveRow {
    pub id: i64,
    pub payload_json: String,
}

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = archive_state)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub(super) struct ArchiveStateRow {
    pub singleton: i64,
    pub sequence: i64,
}
