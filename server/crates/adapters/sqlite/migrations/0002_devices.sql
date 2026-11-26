-- Paired devices (revocable per-device bearer tokens, stored as hashes)
-- and the one-time pairing codes that enrol them.

CREATE TABLE devices (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL CHECK (name <> ''),
    token_hash TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    last_seen_at TEXT,
    revoked_at TEXT
);

CREATE TABLE pairing_codes (
    code_hash TEXT PRIMARY KEY,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    created_by INTEGER REFERENCES devices (id),
    used_at TEXT
);
