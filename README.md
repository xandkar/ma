ma
===============================================================================

Mail Archivist

Status
------

- [x] fetch all messages from all mailboxes from all accounts
- [x] store messages in content-addressed file tree (`dump/[hash..2]/[hash].eml.gz`)
- [ ] insert metadata into SQLite
- [ ] poll/idle for new messages
- [ ] post-update hooks

Hooks can be used for custom notifications, aggregate query reruns, etc.
