-- updated_at and expires_at are epoch milliseconds.

CREATE TABLE teachers (
  sub TEXT PRIMARY KEY,
  email TEXT NOT NULL,
  name TEXT NOT NULL,
  refresh_token_enc TEXT NOT NULL,
  doc TEXT,
  updated_at INTEGER NOT NULL
);

CREATE TABLE sessions (
  token_hash TEXT PRIMARY KEY,
  sub TEXT NOT NULL REFERENCES teachers(sub) ON DELETE CASCADE,
  expires_at INTEGER NOT NULL
);
CREATE INDEX sessions_expires_at ON sessions (expires_at);
