ma
===============================================================================

Mail Archivist. Downloads all mail from multiple accounts into a SQLite
database, for backup and analysis.

Usage
-----

### Environment

1. install rust: <https://www.rust-lang.org/tools/install>
2. ensure `$HOME/.cargo/bin` is in your `$PATH`
3. `cargo install ma`

### Initial

1. `cd $mail_archive_directory` (any directory in which you want `ma` to
   maintain the mail archive)
2. `ma fetch` (will generate initial config file and then exit with a failure
   to connect)
3. Create an App Password with your provider, for example:
    - Google/Gmail: <https://myaccount.google.com/apppasswords>
    - Fastmail: <https://app.fastmail.com/settings/security/integrations>
    - Runbox: <https://runbox.com/mail/account_security>
4. `$EDITOR ma.toml` (fill in the correct values in the generated config file)
5. `ma fetch` (should now work)

### Routine

Like with `git`, you can either `cd` into `$mail_archive_directory` or provide
it to `ma` as an argument, like so:

- `ma --dir $mail_archive_directory fetch` to update
- `sqlite3 $mail_archive_directory/ma.db` to enjoy exploring your mail archive
  with SQL!

TODO
----

- [x] fetch all messages from all mailboxes from all accounts
- [x] store raw messages in content-addressed file tree (`dump/[hash..2]/[hash].eml.gz`)
- [x] insert headers and text body into SQLite
- [x] store state, the highest seen msg per account per mailbox, and avoid re-downloads
- [x] fetch directly to database and rebrand file-tree storing as `export`
- [ ] snapshot (log?) mailboxes and message locations
- [ ] poll/idle for new messages (maybe not necessary, since once can just
      periodically re-fetch)
- [ ] post-update hooks
      (Can be used for custom notifications, aggregate query reruns, etc.)
- [ ] timeouts
- [x] parallelize fetch
- [ ] parallelize import/export
- [ ] example analytics
  - [ ] `Received` based route trace graph
  - [ ] `From -> To` directed graph with edges weighted by
    + [ ] frequency
    + [ ] msg size
- [ ] export into `InfluxDB` or something similar that Grafana can read from

Questions
---------

### Automatic Address Book

How to measure confidence in sender/receiver identity, given that:

- any name can have any address
- any address can have any name
