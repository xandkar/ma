-- Raw:
CREATE TABLE IF NOT EXISTS messages (
    hash TEXT PRIMARY KEY,
    raw BLOB NOT NULL
);

-- Parsed:
CREATE TABLE IF NOT EXISTS headers (
    msg_hash TEXT NOT NULL,
    name TEXT NOT NULL,
    value TEXT NOT NULL,
    FOREIGN KEY (msg_hash) REFERENCES messages(hash),
    UNIQUE (msg_hash, name)
);

CREATE TABLE IF NOT EXISTS bodies (
    msg_hash TEXT NOT NULL,
    text TEXT NOT NULL,
    FOREIGN KEY (msg_hash) REFERENCES messages(hash),
    UNIQUE (msg_hash)
);
