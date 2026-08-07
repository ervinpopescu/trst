# trst

`trst` is a fast, clean, and modern Terminal User Interface (TUI) client for [Transmission](https://transmissionbt.com/). Written in Rust, it allows you to easily manage, monitor, and configure your torrents directly from your terminal.

## Features

- **Real-time Monitoring:** View active torrents, speeds, ETA, and swarm statistics.
- **Full Management:** Add (via Magnet/URL), pause, resume, reannounce, verify, remove, and delete torrents.
- **Location Control:** Choose download locations when adding torrents, or move existing torrents to new directories.
- **Path Auto-Completion:** Seamless directory path completion when selecting locations using the `Tab` key.
- **Secure Authentication:** Built-in support for Basic Authentication with automatic, secure credential persistence via the OS keyring (integrates with Secret Service, macOS Keychain, and Windows Credential Manager).
- **Configurable UI:** Extensive theming and keybinding configuration via `~/.config/trst/config.toml`.
- **Keyboard-Driven:** Ergonomic, vim-inspired default keybindings.

## Installation

Currently, you can build `trst` from source using `cargo`:

```bash
cargo install --path .
```

## Usage

Simply run `trst` from your terminal. By default, it will attempt to connect to a local Transmission daemon at `http://localhost:9091/transmission/rpc`.

```bash
# Connect to a remote daemon with credentials
trst --url http://192.168.1.100:9091/transmission/rpc --username admin --password secret
```

*(Note: Credentials provided via the CLI are securely and automatically saved to your OS keyring for future runs).*

### Keybindings

| Key | Action |
| --- | --- |
| `↑`/`↓` or `k`/`j` | Navigate torrents |
| `Shift+↑/↓` | Select multiple torrents |
| `space` | Toggle selection |
| `Enter` | Open torrent details/files |
| `a` | Add torrent (Magnet/URL + Location) |
| `m` | Change download location |
| `d` | Remove torrent |
| `D` | Remove torrent + delete files |
| `p` | Pause/Resume |
| `?` | Open Help Menu |
| `q` | Quit |

*You can view the full list of keybindings within the app by pressing `?`.*

## Media Organization

`trst` includes a standalone Python script, `organize_media.py`, designed for automated post-processing of torrents for Jellyfin.

### Features

- **TMDB Integration:** Fetches canonical titles, years, and IDs for reliable matching.
- **Strict Normalization:** Renames existing media folders to the `Title (Year) [tmdbid-XXX]` format.
- **Jellyfin Compatible:** Automatically handles Movie and Series (Season/Episode) structures.
- **Cleanup:** Automatically removes unneeded files (NFOs, samples) after processing.

### Setup with Transmission

To use the script with Transmission:

1. Ensure `requests` is installed (`pip install requests`).
2. Set your TMDB API key. You can either:
   - Add the following to `~/.config/trst/config.toml`:

     ```toml
     [media]
     tmdb_api_key = "your_key_here"
     ```

   - Set the `TMDB_API_KEY` environment variable in your Transmission environment.
3. Copy the script to a permanent location (e.g., `~/.local/bin/organize_media.py`).
4. Update your Transmission `settings.json`:

   ```json
   "script-torrent-done-enabled": true,
   "script-torrent-done-filename": "/home/youruser/.local/bin/organize_media.py"
   ```

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
