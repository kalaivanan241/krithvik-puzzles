CREATE TABLE IF NOT EXISTS categories (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS media (
    url TEXT PRIMARY KEY,
    kind TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS items (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    category_id INTEGER NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    name TEXT NOT NULL,
    sound TEXT NOT NULL DEFAULT '',
    image TEXT NOT NULL DEFAULT '',
    audio_url TEXT NOT NULL DEFAULT '',
    color TEXT NOT NULL DEFAULT '#ffe7b8',
    UNIQUE(category_id, key)
);
CREATE TABLE IF NOT EXISTS migrations (name TEXT PRIMARY KEY);
-- Generated voice clips, keyed by the exact phrase spoken.
CREATE TABLE IF NOT EXISTS speech (
    text TEXT PRIMARY KEY,
    url TEXT NOT NULL REFERENCES media(url)
);
-- OAuth for remote MCP clients. Secrets and tokens are stored only as SHA-256
-- hashes; admin_fingerprint ties grants to the admin token that approved them.
CREATE TABLE IF NOT EXISTS oauth_clients (
    client_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    redirect_uris TEXT NOT NULL,
    auth_method TEXT NOT NULL,
    secret_hash TEXT,
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS oauth_codes (
    code_hash TEXT PRIMARY KEY,
    client_id TEXT NOT NULL REFERENCES oauth_clients(client_id) ON DELETE CASCADE,
    redirect_uri TEXT NOT NULL,
    challenge TEXT NOT NULL,
    admin_fingerprint TEXT NOT NULL,
    expires_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS oauth_tokens (
    token_hash TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('access', 'refresh')),
    client_id TEXT NOT NULL REFERENCES oauth_clients(client_id) ON DELETE CASCADE,
    admin_fingerprint TEXT NOT NULL,
    expires_at INTEGER NOT NULL
);
