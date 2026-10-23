-- Wrap existing payloads atomically; IDs/openings/order/selection/restart flags stay in place.
UPDATE tab_definitions SET config_json = json_object('version', 1, 'config', json(config_json));
PRAGMA user_version = 4;
