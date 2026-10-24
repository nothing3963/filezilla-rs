# Architecture Design

## Goals

1. **Frontend/backend decoupling**: the frontend only depends on a stable set of Tauri commands and events, so anyone can replace the React layer without affecting the protocol implementation.
2. **Protocol-agnostic**: the UI does not know about FTP, FTPS, or SFTP differences; it only deals with the `RemoteFs` abstraction.
3. **Evolvable**: first cover FileZilla's core workflows, then gradually add directory comparison, synchronized browsing, proxies, and other advanced features.
4. **Secure by default**: passwords are never stored in plaintext, TLS is verified by default, and SSH host keys use TOFU.

## Overall Structure

```
┌────────────────────────────────────────────────────────────┐
│ React 19 + TypeScript (WebView2)                           │
│ components / store / i18n / design tokens                  │
│           │  invoke(command, args)        ▲ event(payload) │
└───────────┼───────────────────────────────┼────────────────┘
            ▼                               ▼                 
┌────────────────────────────────────────────────────────────┐
│ Tauri command + event layer (commands.rs / logs.rs)        │
│  parameter validation, DTO conversion, error normalization │
│  event broadcasting                                        │
└───────────┼───────────────────────────────┼────────────────┘
            ▼                               ▼                 
┌────────────────────────┐      ┌───────────────────────────┐
│ SessionManager         │      │ TransferManager           │
│ one ctrl connection/tab│      │queue / concurrency / retry│
│ keepalive, enc cache   │      │ folder expansion, throttle│
└───────────┬────────────┘      └───────────┬───────────────┘
            ▼                               ▼                 
┌────────────────────────────────────────────────────────────┐
│ RemoteFs (async trait)                                     │
│  list / stat / mkdir / rename / chmod / download / upload  │
└───────────┼───────────────────────────────┼────────────────┘
            ▼                               ▼                 
┌────────────────────────┐      ┌───────────────────────────┐
│ FtpRemote              │      │ SftpRemote                │
│ suppaftp + rustls      │      │ russh + russh-sftp        │
│ MLSD / LIST parsing    │      │ SFTP v3                   │
└───────────┬────────────┘      └───────────┬───────────────┘
            ▼                               ▼                 
        TCP / TLS                      SSH / SFTP             
```

## Process & Thread Model

- **Main thread**: Tauri event loop, windows, WebView2.
- **Tokio runtime**: all network, file, and queue operations run as async tasks.
- **UI control connections**: one per tab, serialized inside `tokio::sync::Mutex<Box<dyn RemoteFs>>` to avoid concurrent commands scrambling the FTP control channel.
- **Transfer connections**: each file task opens its own connection, without blocking UI browsing. Concurrency is limited by a semaphore (default 2, configurable 1–10).
- **keepalive**: after a session is established, a background task starts per `keepaliveSecs`, tries to acquire the control-connection lock, and sends NOOP / SFTP stat; it never interrupts an in-flight command.

Why transfers do not reuse the control connection: FTP `RETR/STOR` occupies the data channel; transferring over the control connection would block both browsing and cancellation. Dedicated connections also match FileZilla's behavior more closely.

## Command & Event Contract

All commands are defined in `src-tauri/src/commands.rs`; the frontend wraps them in `src/api/backend.ts`. Parameters use camelCase (Tauri maps them to Rust snake_case automatically).

### Connection

| Command | Description |
| --- | --- |
| `connect(request)` | Establishes a session by `siteId` or inline `config`, returns `SessionInfo` |
| `disconnect(sessionId)` | Closes the control connection and stops keepalive |
| `remote_list(sessionId, path)` | Returns `RemoteListing { path, parent, entries }` |
| `remote_stat / remote_mkdir / remote_rename / remote_chmod / remote_delete` | Remote file operations; delete supports recursion |

### Local

`local_list(path, showHidden)`, `local_mkdir`, `local_rename`, `local_delete`. `path = null` represents the virtual "This PC" root.

### Transfers

| Command | Description |
| --- | --- |
| `enqueue({ sessionId, items })` | `items` may contain directories; the backend expands them into file tasks and pre-creates directories |
| `transfers_list` / `transfer_summary` | Snapshot / statistics |
| `transfer_action(taskId, action)` | `pause / resume / cancel / retry / remove` |
| `transfer_clear` | Clears all terminal-state tasks |

Events:

| Event | Payload |
| --- | --- |
| `transfer:update` | A single `TransferTask` (the frontend upserts by id) |
| `transfer:removed` | task id |
| `transfers:cleared` | task id array |
| `log:entry` | `LogEntry` |

Progress events are throttled to roughly once per 180 ms per task, so large directories / high-speed transfers do not flood the WebView.

### Sites & Settings

`sites_get / site_save / site_delete / site_duplicate`, `group_create / group_rename / group_delete`, `settings_get / settings_set`, `known_hosts_list / known_host_forget`.

## Data Model

Rust DTOs live in `src-tauri/src/models.rs`; the frontend mirrors them in `src/types.ts`. Errors are normalized as:

```json
{ "code": "AUTH", "message": "…", "detail": "…" }
```

Common codes: `NETWORK / TIMEOUT / TLS / AUTH / HOST_KEY / REMOTE / NOT_FOUND / PERMISSION / LOCAL_IO / TRANSFER / CANCELLED / SESSION / INVALID_INPUT / UNSUPPORTED`.

## FTP / FTPS Details

- **Explicit FTPS**: TCP → `AUTH TLS` → login; **implicit FTPS**: direct TLS on port 990.
- **Proxy**: HTTP CONNECT / SOCKS5 establish a tunnel at the TCP layer, then hand off to FTP/SFTP; implicit FTPS over a proxy is not yet supported.
- TLS uses rustls + `webpki-roots` and does not depend on the platform TLS stack; certificate verification can be disabled per site.
- Listing prefers `MLSD`, falling back to `LIST` on failure or empty results. The parser supports:
  - RFC 3659 MLSD facts (type/size/modify/unix.mode/owner/group)
  - Unix `ls -l` (including filenames with spaces and symlinks)
  - DOS/IIS format (`<DIR>`, AM/PM)
- Paths are uniformly normalized to POSIX to keep Windows backslashes out of FTP commands.
- **Encoding**: `suppaftp::Encoding` is driven by `encoding_rs`. When UTF-8 decoding yields `U+FFFD`, the client automatically re-lists the directory with GB18030 and records the encoding on the session; `SessionManager::resolved_for` carries that encoding to subsequent transfer connections, so Chinese filenames can be downloaded/uploaded.

## SFTP Details

- russh is a pure-Rust implementation and does not depend on libssh2.
- Passwords and OpenSSH private keys are supported (PEM/OpenSSH; PuTTY `.ppk` is not).
- On first connection, the SHA256 fingerprint is recorded in `known_hosts.json`; later mismatches reject the connection outright and return `HOST_KEY`.
- Directory entries come from `read_dir` lstat metadata; symlinks are not expanded recursively to avoid cycles.

## Transfer Queue

`TransferManager` responsibilities:

1. **Expansion**: recursively traverse local directories; recursively `list` remote directories. Local and remote directories are pre-created, with a cap of 20,000 files.
2. **Scheduling**: each file task is `tokio::spawn`ed, with concurrency controlled by a shared `Semaphore`.
3. **Lifecycle**: `Queued → Connecting → Active → Completed / Failed / Cancelled`, and `Paused` while suspended.
4. **Progress**: 64–128 KiB chunked copies; `TransferControl` checks cancel/pause between chunks and computes sleep based on the speed limit.
5. **Resume**: downloads use `REST` with the existing local size, uploads use `REST + APPE` with the remote size; SFTP uses seek.
6. **Failure handling**: the error code/message is preserved and retryable; cancellation `abort`s the data connection to avoid misaligning the control connection.

## Persistence

- `sites.json`: sites and groups; the password field is never written.
- `settings.json`: theme, language, concurrency, speed limits, show-hidden-files, etc.
- `known_hosts.json`: SSH host fingerprints.
- `queue.json`: unfinished transfer tasks; restored in a paused state after restart and resumed automatically when reconnecting to the same site.
- Passwords / private-key passphrases / proxy passwords: system credential store (Windows Credential Manager).
- Writes use atomic "temp file + rename" replacement.
- During development, `FILEZILLA_RS_CONFIG_DIR` overrides the config directory.

## Frontend Design

- **Design tokens**: CSS variables at the top of `src/styles/index.css` centrally define colors, radii, density, and fonts; light/dark themes only switch variables.
- **Layout**: `react-resizable-panels` v4 `Group / Panel / Separator`; dual panes + collapsible queue + log drawer.
- **Virtual list**: `@tanstack/react-virtual` renders only the visible rows for large directories.
- **State**: a single Zustand store partitioned by "tabs / local pane / transfers / sites / UI"; async actions are centralized in the store and components stay side-effect free.
- **Browser mock**: `src/api/mock.ts` implements the same command set, so `pnpm dev` gives the full UI and transfer animations without Rust — ideal for standalone frontend development and screenshot regression.
- **i18n**: `src/i18n/locales/<code>.json` (one file per language, aggregated in `index.ts`); `useT()` returns a translation function with interpolation. Only `en-US` (fallback) and `zh-CN` (default) are bundled statically; other languages are dynamically `import`ed via `loadLanguage()` on switch. Adding a language only requires adding one JSON file and re-running the generation script.

### Reskinning / Customizing the Frontend

- Change colors: only edit the variables under `:root[data-theme]`.
- Change layout: `src/App.tsx` composes `FilePane` / `TransferPanel` / `LogPanel`; components communicate only through the store and props.
- Change the framework: keep the command contract of `src/types.ts` and `src/api/backend.ts`; Vue/Svelte/Solid can all plug in directly.

## Extension Points

- **New protocol**: implement `RemoteFs` and dispatch in `remote::connect`; the frontend only needs one more entry in the protocol enum.
- **New command**: add a function in `commands.rs` and register it in `generate_handler!`; add a typed wrapper in `backend.ts` on the frontend.
- **New event**: `AppHandle::emit` on the backend, subscribe via `onBackendEvent` on the frontend.
- **New setting**: add a field to `AppSettings` (with a serde default); the settings page control stays backward compatible.
- **Theme pack**: extract CSS variables into a standalone stylesheet and switch `data-theme` at runtime.

## Testing Strategy

- `cargo test --lib`: path normalization, Unix/MLSD/DOS listing parsing, local directories.
- Manual end-to-end: FTP, explicit FTPS, and SFTP connect/list/download verified against `test.rebex.net`; GB18030 fallback verified against a real Chinese FTP server.
- Frontend: `tsc --noEmit` + Vite build; the browser mock is used for interaction regression.

## Next Steps

See [`roadmap.md`](roadmap.md). Near-term priorities: active mode, queue persistence, directory comparison / synchronized browsing, FileZilla XML import, proxies, remote editing, SSH host key confirmation UI, installers and auto-update.
