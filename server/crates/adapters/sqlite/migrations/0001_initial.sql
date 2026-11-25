-- Phase 1 schema. Timestamps are written by the code (fixed-width UTC
-- RFC 3339 with milliseconds), never by SQLite. Enum columns hold the
-- domain's stable codes.

CREATE TABLE accounts (
    id INTEGER PRIMARY KEY,
    provider TEXT NOT NULL CHECK (provider IN ('gmail', 'outlook', 'imap')),
    email TEXT NOT NULL CHECK (email = lower(email)),
    -- Encrypted secret; NULL when the account must be reconnected.
    credentials BLOB,
    created_at TEXT NOT NULL,
    last_sync_at TEXT,
    last_error TEXT,
    UNIQUE (provider, email)
);
