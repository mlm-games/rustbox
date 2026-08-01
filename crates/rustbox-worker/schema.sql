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
    created_at     TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at     TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_levels_status_created ON levels (status, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_levels_author ON levels (author);

-- Per-IP sliding-window counters for rate limiting (upload/report/like).
CREATE TABLE IF NOT EXISTS rate_limits (
    key    TEXT PRIMARY KEY,          -- "{bucket}:{ip}:{window}"
    count  INTEGER NOT NULL DEFAULT 0,
    window INTEGER NOT NULL           -- unix seconds of the window start
);
