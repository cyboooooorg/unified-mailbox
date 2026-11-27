-- Settings chosen by the user, as key/value pairs so that a new setting
-- needs no migration. The `language` key holds a language code.

CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
