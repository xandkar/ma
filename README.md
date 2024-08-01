ma
===============================================================================

Mail Archivist

Status
------

- [x] fetch all messages from all mailboxes from all accounts
- [x] store raw messages in content-addressed file tree (`dump/[hash..2]/[hash].eml.gz`)
- [x] insert headers and text body into SQLite
- [x] store state, the highest seen msg per account per mailbox, and avoid re-downloads
- [ ] poll/idle for new messages
- [ ] post-update hooks

Hooks can be used for custom notifications, aggregate query reruns, etc.
