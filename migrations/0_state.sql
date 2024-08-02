CREATE TABLE IF NOT EXISTS last_seen_msg (
    account TEXT NOT NULL,
    mailbox TEXT NOT NULL,
    uid INTEGER NOT NULL,
    UNIQUE (account, mailbox)
);
