# FileZilla Modern

A FileZilla FTP / FTPS / SFTP client rewritten with **Rust + Tauri 2 + React 19**. The frontend and backend are fully decoupled: Rust handles connections, protocols, the filesystem, and the transfer queue, while React only consumes data through a typed IPC contract, so swapping the frontend skin does not require touching the backend.

## Tech Stack

- **Desktop shell**: Tauri 2.12 / WebView2
- **Backend**: Rust 1.88+, Tokio, suppaftp 12 (rustls), russh 0.64 + russh-sftp 3
- **Frontend**: React 19, TypeScript 5.9, Vite 8, Tailwind CSS 4, Zustand 5, TanStack Virtual
- **Storage**: sites / settings / known_hosts / queue as JSON; passwords only go to the system credential store

## Development

Prerequisites: Node.js 20.19+, pnpm 11.7, Rust 1.88+, WebView2 Runtime (currently verified on Windows only).

```bash
pnpm install
pnpm dev          # Frontend + mock backend, open http://localhost:1420 in a browser
pnpm app:dev      # Full Tauri desktop app
pnpm app:build    # Generate installer / executable
pnpm check        # TypeScript type checking
```

Rust checks:

```bash
cargo check --manifest-path src-tauri/Cargo.toml
cargo test  --manifest-path src-tauri/Cargo.toml --lib
```

## Data & Security

- Configuration: `%APPDATA%\org.filezilla.modern\` (sites, settings, known_hosts, queue)
- Passwords / private-key passphrases / proxy passwords: Windows Credential Manager, never written to JSON
- TLS: rustls + WebPKI root certificates; unknown SFTP host keys require confirmation, fingerprint changes are rejected

## Known Limitations

- Automatic updates and launch-at-startup are not implemented yet.
- Implicit FTPS compatibility is limited: some servers (e.g. `test.rebex.net`) return 426 on the data connection, and proxying is not yet supported.
- Active mode requires the server to be able to connect back to the client; behind NAT / firewalls, passive mode should usually be used.
- Folder transfers are expanded into per-file tasks; empty directories do not appear in the queue.
- Queue recovery relies on saved sites; tasks created via Quick Connect cannot be restored automatically after a restart.
- Only 25 mainstream languages are provided; less common languages no longer appear in the language list.

## License

GPL-3.0, same as the original FileZilla client.
