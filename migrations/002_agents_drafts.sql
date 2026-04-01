CREATE TABLE IF NOT EXISTS agents (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    description TEXT,
    model       TEXT,
    provider    TEXT,
    system      TEXT,
    allowed_tools TEXT,
    created_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS drafts (
    id          TEXT PRIMARY KEY,
    label       TEXT NOT NULL,
    content     TEXT NOT NULL,
    created_at  INTEGER NOT NULL
);
