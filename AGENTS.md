# AGENTS.md

Single-binary Rust CLI (`aweme-db-decrypt`) that decrypts 抖音极速版 (aweme.lite) WCDB/SQLCipher v3 IM databases. Depends on `rusqlite` with `bundled-sqlcipher-vendored-openssl` (vendored crypto — no system OpenSSL).

## Commands
- `cargo test` — all tests are in-module `#[cfg(test)]` unit tests; no fixtures or real DBs. Fast (19 tests).
- `cargo fmt` — repo is rustfmt-clean; keep it that way.
- `cargo build --release` — release profile uses `lto=thin`, `codegen-units=1`, `strip`; slow. Use plain `cargo build` for iteration.
- `./build-all.sh` — host + every installed cross target, staged as platform-tagged binaries into `dist/`. Skips targets with missing toolchains (logs why); Android needs `ANDROID_NDK_HOME`/`ANDROID_NDK_ROOT` + `cargo-ndk`, windows-gnu needs mingw, musl needs musl-cross. Normal dev rarely needs this.
- `.cargo/config.toml` only sets linkers for cross-target builds; native macOS/Linux builds ignore it.
- CI is release-only (`.github/workflows/release.yml`): no test/lint gate. Releases are cut by pushing `v*` tags — bump `version` in `Cargo.toml` first.

## Architecture
- `src/cmd/` — one file per subcommand (`decrypt`, `query`, `shell`, `watch`).
- `src/db/cipher.rs` — the crown jewels: passphrase = `byte<uid>imwcdb<uid>dance`; SQLCipher v3 pragmas (page 4096, kdf_iter 64000, HMAC-SHA1, PBKDF2-SHA1); filename→(kind, uid) detection.
- `src/db/open.rs` — `open_encrypted` (copy into temp `WorkDir`, returns decrypted conn) for query/shell; `open_encrypted_direct` (read-only live open, no copy) for `watch`.
- `src/db/workdir.rs` — `WorkDir` copies the DB plus `-wal`/`-shm`/`-journal` sidecars into a scratch dir, deleted on Drop. **Source files are never modified.** `--write` only mutates the scratch copy and is discarded on exit.
- `src/fmt/` — `exec.rs` is the shared statement-runner used by both `query` and `shell` (prepare → render, status lines go to `status_w`); `output.rs` renders table/csv/json — **CSV/JSON stream rows off the prepared statement, only table mode materializes** (column-width alignment); `sql_split.rs` is a hand-rolled UTF-8-aware SQL statement splitter — do not assume sqlite `prepare`-based splitting works for multi-statement input.

## Conventions / gotchas
- Adding a subcommand: create `src/cmd/x.rs`, register in `src/cmd/mod.rs` and `main.rs`, **and** add its name to the `known` list in `rewrite_argv` (src/main.rs) — otherwise legacy `aweme-db-decrypt <file>` (implicit `decrypt`) breaks.
- `query`/`shell` default to read-only; `--write` changes are lost on exit by design.
- `watch` prints status to stderr, message data to stdout; `--post-url` accepts only `http://` (no TLS).
- README, user-facing strings, and most comments are Chinese — match that. Commits use conventional style (`feat:`, `fix(android):`), bodies mix Chinese/English.
- `-wal` sidecars must travel with the DB or decrypted results are stale; the tool copies them automatically.
