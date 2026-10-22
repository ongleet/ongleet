-- Rename metadata in place: stable IDs, relationships and launch settings survive.
ALTER TABLE workgroups RENAME TO projects;
ALTER TABLE workspaces RENAME TO sessions;
ALTER TABLE sessions RENAME COLUMN workgroup_id TO project_id;
ALTER TABLE tab_definitions RENAME COLUMN workspace_id TO session_id;
ALTER TABLE app_state RENAME COLUMN selected_workspace_id TO selected_session_id;
ALTER TABLE app_state RENAME COLUMN workspace_sequence TO session_sequence;
ALTER TABLE app_state RENAME COLUMN workgroup_sequence TO project_sequence;
PRAGMA user_version = 3;
