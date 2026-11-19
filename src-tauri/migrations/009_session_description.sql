ALTER TABLE sessions ADD COLUMN description TEXT NOT NULL DEFAULT '';
PRAGMA user_version=9;
