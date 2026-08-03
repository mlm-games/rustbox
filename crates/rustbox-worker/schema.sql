-- rustbox online level sharing schema (D1).
-- Apply with:  npx wrangler d1 execute rustbox --remote --file=./schema.sql

CREATE TABLE IF NOT EXISTS levels (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    author         TEXT    NOT NULL,
    name           TEXT    NOT NULL,
    description    TEXT    NOT NULL DEFAULT '',
    tags           TEXT    NOT NULL DEFAULT '[]',   -- JSON array of strings
    format_version INTEGER NOT NULL,
    game_version   TEXT    NOT NULL DEFAULT '',
    size_bytes     INTEGER NOT NULL DEFAULT 0,
    sha256         TEXT    NOT NULL DEFAULT '',
    status         TEXT    NOT NULL DEFAULT 'published', -- published | hidden | deleted
    likes          INTEGER NOT NULL DEFAULT 0,
    plays          INTEGER NOT NULL DEFAULT 0,
    reports        INTEGER NOT NULL DEFAULT 0,
    owner_id       TEXT    NOT NULL DEFAULT '',     -- sha256 of the creator's recovery key ('' = admin upload)
    created_at     TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at     TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_levels_status_created ON levels (status, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_levels_author ON levels (author);
CREATE INDEX IF NOT EXISTS idx_levels_owner ON levels (owner_id);

-- Per-IP sliding-window counters for rate limiting (report/like).
CREATE TABLE IF NOT EXISTS rate_limits (
    key    TEXT PRIMARY KEY,          -- "{bucket}:{ip}:{window}"
    count  INTEGER NOT NULL DEFAULT 0,
    window INTEGER NOT NULL           -- unix seconds of the window start
);

-- Anonymous creator identities. The recovery key is the account.
-- the server only ever sees its sha256 hash (owner_id), never the raw key.
CREATE TABLE IF NOT EXISTS owners (
    owner_id     TEXT    PRIMARY KEY,
    created_at   INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS devices (
    device_id    TEXT    PRIMARY KEY,
    owner_id     TEXT    NOT NULL,
    created_at   INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL
);

-- Exact upload accounting (source of truth for the weekly quota).
CREATE TABLE IF NOT EXISTS upload_events (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    owner_id   TEXT    NOT NULL,
    device_id  TEXT    NOT NULL,
    ip_bucket  TEXT    NOT NULL,       -- IPv4 addr or IPv6 /64 prefix (not sure if this helps prevent my storage filling up from duplicates, but certainly increases friction)
    created_at INTEGER NOT NULL        -- unix seconds
);

CREATE INDEX IF NOT EXISTS idx_upload_events_owner_time ON upload_events (owner_id, created_at);
CREATE INDEX IF NOT EXISTS idx_upload_events_ip_time ON upload_events (ip_bucket, created_at);
