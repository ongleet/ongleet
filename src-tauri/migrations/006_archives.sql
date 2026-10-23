-- Archives are independent of active foreign keys so parents can be removed.
CREATE TABLE archives (
    id INTEGER PRIMARY KEY CHECK(id > 0),
    payload_json TEXT NOT NULL
);
CREATE TABLE archive_state (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
    sequence INTEGER NOT NULL CHECK(sequence >= 0)
);
INSERT INTO archive_state VALUES (1, 0);
PRAGMA user_version = 6;
