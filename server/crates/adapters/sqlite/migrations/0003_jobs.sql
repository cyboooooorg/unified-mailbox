-- Long operations (bulk actions, relabelling) and their progress. A job
-- is `running` until it ends; a server that stops under it marks it
-- `interrupted` at the next start.

CREATE TABLE jobs (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind <> ''),
    status TEXT NOT NULL CHECK (status IN ('running', 'succeeded', 'failed', 'interrupted')),
    done INTEGER NOT NULL DEFAULT 0,
    failed INTEGER NOT NULL DEFAULT 0,
    total INTEGER,
    error TEXT,
    created_at TEXT NOT NULL,
    finished_at TEXT
);

CREATE INDEX jobs_status ON jobs (status);
