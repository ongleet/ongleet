CREATE TABLE workgroups (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    root_directory TEXT NOT NULL,
    display_order INTEGER NOT NULL UNIQUE,
    collapsed INTEGER NOT NULL CHECK (collapsed IN (0, 1))
);
INSERT INTO workgroups VALUES (1, 'Default', COALESCE((SELECT base_directory FROM workspaces ORDER BY display_order LIMIT 1), '/'), 0, 0);
CREATE TABLE workspaces_v2 (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    base_directory TEXT NOT NULL,
    display_order INTEGER NOT NULL UNIQUE,
    selected_tab_id INTEGER REFERENCES tab_definitions(id) DEFERRABLE INITIALLY DEFERRED,
    workgroup_id INTEGER NOT NULL REFERENCES workgroups(id)
);
INSERT INTO workspaces_v2 SELECT id,name,base_directory,display_order,selected_tab_id,1 FROM workspaces;
DROP TABLE workspaces;
ALTER TABLE workspaces_v2 RENAME TO workspaces;
ALTER TABLE app_state ADD COLUMN workgroup_sequence INTEGER NOT NULL DEFAULT 1;
PRAGMA user_version = 2;
