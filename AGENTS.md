# Agent Instructions for trst

`trst` is a TUI Transmission BitTorrent client written in Rust (edition 2024, MSRV 1.88).

## Architecture

| File | Role |
|------|------|
| `src/main.rs` | Argument parsing, config loading, client construction, App launch |
| `src/app.rs` | `App` struct: state machine, key handling, background refresh loop |
| `src/client.rs` | `TransmissionClient`: thin wrapper around the Transmission RPC over HTTP |
| `src/protocol.rs` | Serde structs for the Transmission JSON-RPC protocol |
| `src/config.rs` | TOML config loading/saving, keybind parsing, theme |
| `src/credentials.rs` | Keyring backend with config-file fallback |
| `src/util.rs` | Formatting helpers and filesystem path-completion utilities |
| `src/ui/` | Ratatui rendering — one module per view plus a `mod.rs` router |

## Key Conventions

- Feature additions go through the `Modal` enum in `app.rs` for overlays, or new `View` variants for full-screen panels.
- Client methods call `self.rpc(method, args)` and return `Result<_, String>`. Error strings are surfaced via `App::set_error`.
- Background data refreshes use the `RefreshMsg` channel; avoid blocking the main event loop.
- Path-completion helpers live in `util.rs`: `get_path_suggestions` / `autocomplete_path` for directory-only completion; `get_torrent_file_suggestions` / `autocomplete_torrent_path` for paths that include `.torrent` files.
- When adding a torrent from a local `.torrent` file, read the bytes client-side and call `client.add_metainfo(&bytes, dir)` (sends base64 `metainfo`). For magnet links and HTTP URLs, call `client.add(url, dir)` (sends `filename`).

## Testing

```sh
cargo test          # run all unit tests
cargo clippy        # lint
```

Tests live alongside source in `#[cfg(test)]` modules. The `tempfile` crate is available for filesystem tests.

## Dependencies

`ureq` (HTTP), `serde`/`serde_json` (JSON-RPC), `ratatui`/`crossterm` (TUI), `base64` (metainfo encoding), `keyring` (credential storage), `toml` (config), `dirs` (XDG paths).

## Commit style

Conventional Commits (`feat:`, `fix:`, `test:`, `refactor:`, etc.). Use the `commit-writer` skill when available. Wrap prose lines to ~72 characters.
