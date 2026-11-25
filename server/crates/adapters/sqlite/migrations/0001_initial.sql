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

CREATE TABLE tags (
    -- Stable key derived once from the name (domain `TagKey`).
    key TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    -- The name lowercased by the code, accents kept: the uniqueness rule.
    -- SQLite's lower() and NOCASE fold ASCII only.
    name_folded TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL,
    color TEXT NOT NULL
        CHECK (color GLOB '#[0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f]'),
    created_at TEXT NOT NULL
);

CREATE TABLE mails (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    provider_id TEXT NOT NULL,
    -- Message-ID: stable across moves (Outlook ids change on every move).
    message_key TEXT,
    thread_id TEXT,

    received_at TEXT NOT NULL,
    -- NULL when the From header holds no address that parses (spam).
    sender TEXT,
    sender_name TEXT,
    recipients TEXT NOT NULL DEFAULT '[]',
    subject TEXT NOT NULL DEFAULT '',
    dmarc_passed INTEGER NOT NULL DEFAULT 0 CHECK (dmarc_passed IN (0, 1)),

    -- Text part, cut to 50,000 characters; NULL until fetched.
    body TEXT CHECK (length(body) <= 50000),

    location TEXT NOT NULL
        CHECK (location IN ('inbox', 'archive', 'junk', 'trash', 'sent', 'folder')),
    folder_path TEXT,
    -- Where the mail was first seen.
    source TEXT NOT NULL CHECK (source IN ('inbox', 'junk')),
    -- NULL: unknown.
    is_read INTEGER CHECK (is_read IN (0, 1)),
    locked INTEGER NOT NULL DEFAULT 0 CHECK (locked IN (0, 1)),
    -- The lock last seen in the mailbox; the mailbox wins when it differs.
    locked_remote INTEGER NOT NULL DEFAULT 0 CHECK (locked_remote IN (0, 1)),
    gone_since TEXT,

    review TEXT CHECK (review IN ('rescued', 'confirmed')),
    -- Written by the code with `review`; the confirmed-spam purge delay
    -- counts from it.
    review_at TEXT,

    -- A tag key, 'unknown' for Category::Unknown, NULL when not classified.
    category TEXT,
    priority TEXT CHECK (priority IN ('high', 'normal', 'low')),
    needs_reply INTEGER NOT NULL DEFAULT 0 CHECK (needs_reply IN (0, 1)),
    is_phishing INTEGER NOT NULL DEFAULT 0 CHECK (is_phishing IN (0, 1)),
    spam_confidence INTEGER CHECK (spam_confidence BETWEEN 0 AND 100),
    summary TEXT,
    reason TEXT,
    classified_at TEXT,
    model TEXT,
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    billing TEXT CHECK (billing IN ('api', 'batch', 'subscription')),

    UNIQUE (account_id, provider_id),
    CHECK ((location = 'folder') = (folder_path IS NOT NULL)),
    CHECK ((review IS NULL) = (review_at IS NULL))
);

CREATE INDEX mails_message_key ON mails (account_id, message_key)
    WHERE message_key IS NOT NULL;
CREATE INDEX mails_thread ON mails (account_id, thread_id, received_at);
CREATE INDEX mails_location ON mails (location, received_at);
CREATE INDEX mails_received ON mails (received_at);
CREATE INDEX mails_sender ON mails (sender COLLATE NOCASE);
CREATE INDEX mails_category ON mails (category);

CREATE TABLE trusted_senders (
    -- An address or a domain, stored lowercased by the code.
    pattern TEXT PRIMARY KEY CHECK (pattern = lower(pattern)),
    created_at TEXT NOT NULL
);

CREATE TABLE muted_threads (
    account_id INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    thread_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (account_id, thread_id)
);

CREATE TABLE sync_state (
    account_id INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    location TEXT NOT NULL
        CHECK (location IN ('inbox', 'archive', 'junk', 'trash', 'sent', 'folder')),
    -- '' outside folders: a NULL in a primary key would allow duplicates.
    folder_path TEXT NOT NULL DEFAULT '',
    -- The window already fully indexed, in days.
    days INTEGER NOT NULL,
    PRIMARY KEY (account_id, location, folder_path),
    CHECK ((location = 'folder') = (folder_path <> ''))
);

CREATE TABLE audit (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    at TEXT NOT NULL,
    actor TEXT NOT NULL CHECK (actor IN ('user', 'device', 'system', 'claude', 'rule')),
    actor_id TEXT,
    -- Codes; #26 adds their CHECK lists.
    resource TEXT NOT NULL CHECK (resource <> ''),
    resource_id TEXT,
    -- No foreign key: the log outlives the account.
    account_id INTEGER,
    action TEXT NOT NULL CHECK (action <> ''),
    -- JSON {field: [before, after]}.
    changes TEXT,
    note TEXT
);

CREATE INDEX audit_at ON audit (at);
CREATE INDEX audit_resource ON audit (resource, resource_id);
CREATE INDEX audit_actor ON audit (actor, actor_id);
CREATE INDEX audit_action ON audit (action);

-- Full-text search over mails, accent-insensitive. External content: the
-- text stays in `mails` only, and the triggers keep the index in step on
-- every write, cascades included.
CREATE VIRTUAL TABLE mails_fts USING fts5 (
    subject,
    sender,
    sender_name,
    body,
    summary,
    content = 'mails',
    content_rowid = 'id',
    tokenize = 'unicode61 remove_diacritics 2'
);

CREATE TRIGGER mails_fts_insert AFTER INSERT ON mails BEGIN
    INSERT INTO mails_fts (rowid, subject, sender, sender_name, body, summary)
    VALUES (new.id, new.subject, new.sender, new.sender_name, new.body, new.summary);
END;

CREATE TRIGGER mails_fts_delete AFTER DELETE ON mails BEGIN
    INSERT INTO mails_fts (mails_fts, rowid, subject, sender, sender_name, body, summary)
    VALUES ('delete', old.id, old.subject, old.sender, old.sender_name, old.body, old.summary);
END;

CREATE TRIGGER mails_fts_update
AFTER UPDATE OF subject, sender, sender_name, body, summary ON mails BEGIN
    INSERT INTO mails_fts (mails_fts, rowid, subject, sender, sender_name, body, summary)
    VALUES ('delete', old.id, old.subject, old.sender, old.sender_name, old.body, old.summary);
    INSERT INTO mails_fts (rowid, subject, sender, sender_name, body, summary)
    VALUES (new.id, new.subject, new.sender, new.sender_name, new.body, new.summary);
END;
