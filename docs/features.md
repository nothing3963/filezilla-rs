# Feature Matrix

Status legend: ✅ implemented and verified · 🟡 partially implemented / known limitations · ⏳ planned

## Connection & Protocols

| Feature | Status | Notes |
| --- | --- | --- |
| FTP | ✅ | Passive mode PASV/EPSV; verified against public servers |
| FTPS (explicit AUTH TLS) | ✅ | rustls, certificate verification can be disabled |
| FTPS (implicit 990) | 🟡 | Control connection works; test.rebex.net returns 426 on the data connection, compatibility pending |
| SFTP | ✅ | russh, password and OpenSSH private key support |
| IPv6 | 🟡 | Uses system resolution; no dedicated IPv6 testing yet |
| Active mode (PORT/EPRT) | ✅ | Passive mode can be disabled per site; unavailable behind a proxy |
| HTTP CONNECT proxy | ✅ | Per-site configuration, Basic authentication supported |
| SOCKS5 proxy | ✅ | Per-site configuration, username/password authentication; implicit FTPS not yet supported |
| Keep-alive | ✅ | Background NOOP / SFTP stat per site configuration |
| Connection timeout | ✅ | Shared timeout for FTP/FTPS handshake, login, FEAT, PWD |
| Reconnect | 🟡 | Toolbar "Reconnect" rebuilds the tab; no automatic reconnect |
| Multiple tabs | ✅ | Independent control connection per tab |

## Sites & Credentials

| Feature | Status | Notes |
| --- | --- | --- |
| Site Manager | ✅ | Create, edit, copy, delete |
| Groups / nested groups | ✅ | Tree view, context menu support |
| Quick Connect | ✅ | Protocol, host, port, username, password |
| Save password | ✅ | Windows Credential Manager (keyring equivalent on other platforms) |
| SSH private key + passphrase | ✅ | OpenSSH/PEM; PuTTY `.ppk` not supported |
| SSH host key verification | ✅ | Fingerprint confirmation on first connect; changes rejected; removable from settings |
| FileZilla XML import/export | ✅ | `sitemanager.xml` and `recentservers.xml`, nested groups, passwords imported into the credential store |
| Enterprise `fzdefaults.xml` | ✅ | Config directory, Kiosk, read-only, disable updates, default sites |
| Master password | ⏳ | Depends on the system credential store |

## File Browsing

| Feature | Status | Notes |
| --- | --- | --- |
| Local / remote dual pane | ✅ | Draggable ratio |
| Breadcrumbs, path editing, back/forward/up | ✅ | Separate history for local and remote |
| Sort by name/size/time/permissions | ✅ | Directories first, natural sorting |
| Name filter | ✅ | Independent per pane |
| Keyboard quick navigation | ✅ | Type-ahead to locate; arrow keys move selection, Shift+arrows range-select, Enter enters directory/opens file |
| Show hidden files | ✅ | Off by default; Windows hidden/system attributes and dotfiles |
| Enter folder on double-click | ✅ | Enter also works with a directory selected |
| Directory cache | ✅ | 30-second session cache; falls back to stale cache when listing fails |
| Symlink display | ✅ | Marked LINK, not expanded recursively |
| Directory comparison | ✅ | Toolbar toggle; marks missing/different entries by name/type/size/time |
| Synchronized browsing | ✅ | Follows same-named subdirectories; toolbar "Synchronized browsing" |
| Remote search | ✅ | Recursive filename search from the current directory; results can jump directly |
| File editor integration | ✅ | Downloads to the config directory, watches saves, and uploads back automatically |
| Bookmarks | ✅ | Local/remote directory bookmarks, persisted in settings |

## Transfers

| Feature | Status | Notes |
| --- | --- | --- |
| Upload / download | ✅ | Chunked streaming, large files supported |
| Folder upload / download | 🟡 | Recursively expanded into file tasks with pre-created directories; download verified against public SFTP, upload awaits a writable-server regression |
| Drag to the other pane | ✅ | Internal drag + drag-in from Explorer for upload |
| Transfer queue | ✅ | Virtual-scroll list, status, progress, speed, ETA |
| Concurrent transfers | ✅ | Global 1–10, default 2; per-site max_connections additionally limits |
| Pause / resume / cancel / retry / remove | ✅ | Cooperative cancel and pause |
| Global speed limits | ✅ | Independent KB/s for download/upload |
| Resume | ✅ | FTP REST/APPE, SFTP seek |
| Transfer timestamps | ✅ | Download sets local mtime; upload FTP MFMT / SFTP setstat (server must support) |
| Queue persistence | ✅ | `queue.json` v2 schema, v1-compatible; auto-continues on reconnect |
| Queue completion action | ✅ | In-app notification, sound; no auto-shutdown |
| Auto shutdown/action on completion | ⏳ | |

## UI & Experience

| Feature | Status | Notes |
| --- | --- | --- |
| Windows 11 style | ✅ | Borderless, Mica, rounded containers, Fluent controls |
| Light / dark / follow system | ✅ | CSS variable switching, Mica synced |
| Accent color | ✅ | 6 presets: blue/cyan/purple/orange/pink/green |
| Command palette | ✅ | Ctrl+K, search and run common commands |
| Chinese / English | ✅ | Full built-in dictionaries |
| All 58 original languages | ✅ | Auto-generated from `.po` files, falls back to English when missing |
| Comfortable / compact density | ✅ | Row height and toolbar height variables |
| Log panel | ✅ | Same bottom container as the transfer queue, tab switching; level filter, search, auto-scroll, clear |
| Status bar | ✅ | Connection, item count, session count, transfer speed |
| System tray | ✅ | Show/quit; close-to-tray configurable; launch at startup pending |
| Auto-update | ⏳ | |
| High DPI | ✅ | Uses logical pixels and system scaling |
| Accessibility | 🟡 | Basic aria/keyboard support, no full audit |

## Platforms

| Platform | Status |
| --- | --- |
| Windows 10/11 | ✅ Verified |
| macOS | ⏳ Code is conditionally compiled per platform, not verified |
| Linux | ⏳ Code is conditionally compiled per platform, not verified |
