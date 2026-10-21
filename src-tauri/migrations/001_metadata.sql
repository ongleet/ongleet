CREATE TABLE workspaces (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    base_directory TEXT NOT NULL,
    display_order INTEGER NOT NULL UNIQUE,
    selected_tab_id INTEGER REFERENCES tab_definitions(id) DEFERRABLE INITIALLY DEFERRED
);
CREATE TABLE tab_definitions (
    id INTEGER PRIMARY KEY,
    workspace_id INTEGER NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    config_json TEXT NOT NULL,
    display_order INTEGER NOT NULL,
    UNIQUE (workspace_id, display_order)
);
CREATE TABLE open_tabs (
    definition_id INTEGER PRIMARY KEY REFERENCES tab_definitions(id) ON DELETE CASCADE,
    resume_on_start INTEGER NOT NULL CHECK (resume_on_start IN (0, 1))
);
CREATE TABLE app_state (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    selected_workspace_id INTEGER REFERENCES workspaces(id) DEFERRABLE INITIALLY DEFERRED,
    workspace_sequence INTEGER NOT NULL,
    tab_sequence INTEGER NOT NULL
);
INSERT INTO app_state VALUES (1, NULL, 0, 0);
PRAGMA user_version = 1;
