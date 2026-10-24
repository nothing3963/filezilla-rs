# Migration Roadmap

The original FileZilla client carries more than 20 years of accumulated history. This project proceeds in the order "usable first, then aligned, then surpassing", avoiding a one-shot rewrite of every edge case.

## Phase 1 — Core Usability (Done)

- [x] Tauri 2 + React 19 project skeleton, frontend/backend decoupled
- [x] FTP / FTPS (explicit, implicit) / SFTP connection and browsing
- [x] Dual-pane file management, multiple tabs, Site Manager, Quick Connect
- [x] Upload, download, folder recursion, queue, concurrency, pause/cancel/retry
- [x] Resume, speed limits, keepalive
- [x] Light/dark theme, Mica, Chinese/English, logs, status bar
- [x] System credential store, SSH host key TOFU, TLS verification
- [x] FileZilla `sitemanager.xml` import/export, queue persistence, HTTP/SOCKS5 proxies
- [x] All 58 original languages overlay, `fzdefaults.xml` enterprise policies
- [x] Filename encoding selection + automatic GB18030 fallback
- [x] Browser mock backend so the frontend can be developed independently
- [x] Migrated 88 C++ directory-listing test cases, 88/88 passing; local path + full server path test cases migrated
- [x] Session directory cache and invalidation strategy

## Phase 2 — Aligning FileZilla Daily Workflows

- [x] Active mode (PORT/EPRT) and per-site mode selection
- [x] Versioned queue-persistence schema (v2, v1 array compatible)
- [x] FileZilla `recentservers.xml` import
- [x] Transfer timestamp preservation (download sets mtime, upload MFMT/SFTP attrs)
- [x] Directory comparison (name/type/size/time)
- [x] Synchronized browsing (follows same-named subdirectories; not a full mirror)
- [x] Remote file search (by filename, depth 8 / limit 1000)
- [x] Remote editing: download to temp dir → watch saves → auto upload back
- [ ] Proxy: FTP over HTTP, advanced proxy authentication options
- [x] HTTP CONNECT / SOCKS5 proxies (except implicit FTPS)
- [x] SSH host key confirmation dialog and known_hosts management page
- [x] Drag files in from Explorer
- [x] Right-click "Show in Explorer", copy URL, bookmarks
- [x] Queue completion action (in-app notification, sound; no auto-shutdown)

## Phase 3 — Experience & Platforms

- [x] Per-site connection cap for queue/transfers (site max_connections)
- [ ] Multi-connection parallel segmented downloads of one file (optional)
- [x] System tray (show/quit), close-to-tray; launch at startup pending
- [ ] Auto-update (Tauri updater) and signing
- [ ] macOS notarization, Linux AppImage/deb packaging
- [x] Custom accent colors (6 presets); full theme packs pending
- [x] Command palette Ctrl+K; full keyboard mapping pending
- [ ] Performance: 100k-entry directories, incremental refresh, directory watching
- [ ] Plugin protocol adapters (WebDAV, S3, OneDrive, etc.)

## Technical Debt

- FTP control commands lack a unified per-command timeout; it currently relies mostly on the connection timeout and UI cancellation.
- Transfer connections are created per file, so directory-heavy tasks repeat SSH handshakes; a per-session connection pool could be introduced later.
- Transfer tasks live entirely in memory; queue persistence is implemented as `queue.json`, but the schema is not yet versioned.
- Automatic Chinese/Japanese encoding detection currently only does a GB18030 fallback; it could be extended to a configurable candidate encoding sequence.
- The frontend bundle is not code-split; initial load is about 1 MB (gzip ~216 KB), acceptable for a desktop app.

## Acceptance Criteria

Before each phase ends, at least the following must be done:

1. `cargo test --lib` and `cargo check` pass;
2. `tsc --noEmit` and `vite build` pass;
3. Run connect, list, upload, and download once each against `test.rebex.net` over FTP / FTPS / SFTP;
4. Verify Chinese filename browsing and transfers against a GBK/GB18030 Chinese FTP server;
5. Screenshot the frontend in both light/dark themes and both densities to check for layout overflow.
