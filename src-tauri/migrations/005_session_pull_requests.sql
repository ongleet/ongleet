-- Keep session associations separate from the plugin payload migration.
ALTER TABLE sessions ADD COLUMN pull_request_json TEXT;
PRAGMA user_version = 5;
