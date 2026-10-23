ALTER TABLE sessions ADD COLUMN plugin_data_json TEXT NOT NULL DEFAULT '{}';
ALTER TABLE tab_definitions ADD COLUMN plugin_data_json TEXT NOT NULL DEFAULT '{}';
UPDATE sessions SET plugin_data_json = json_object('pull-request', json_object('version', 1, 'revision', 1, 'value', json(pull_request_json))) WHERE pull_request_json IS NOT NULL;
PRAGMA user_version = 7;
