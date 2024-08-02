-------------------------------------------------------------------------------
-- Msgs raw:
-------------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS messages (
    hash TEXT PRIMARY KEY,
    raw BLOB NOT NULL
);

-------------------------------------------------------------------------------
-- Msgs parsed:
-------------------------------------------------------------------------------
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

-------------------------------------------------------------------------------
-- State:
-------------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS last_seen_msg (
    account TEXT NOT NULL,
    mailbox TEXT NOT NULL,
    uid INTEGER NOT NULL,
    UNIQUE (account, mailbox)
);
