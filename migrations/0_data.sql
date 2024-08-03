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
    UNIQUE (msg_hash, name, value)
);

CREATE INDEX IF NOT EXISTS idx_headers_msg_hash ON headers(msg_hash);


CREATE TABLE IF NOT EXISTS bodies (
    msg_hash TEXT NOT NULL,
    text TEXT NOT NULL,
    FOREIGN KEY (msg_hash) REFERENCES messages(hash),
    UNIQUE (msg_hash)
);

CREATE INDEX IF NOT EXISTS idx_bodies_msg_hash ON bodies(msg_hash);

-------------------------------------------------------------------------------
-- State:
-------------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS last_seen_msg (
    account TEXT NOT NULL,
    mailbox TEXT NOT NULL,
    uid INTEGER NOT NULL,
    UNIQUE (account, mailbox)
);

CREATE INDEX IF NOT EXISTS idx_last_seen_msg_account_mailbox ON last_seen_msg(account, mailbox);
