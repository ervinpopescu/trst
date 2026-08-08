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

## Automation & Events

`trst` features a highly configurable event system that can automatically execute actions when torrents change state. You define rules with matchers (filters) and actions in your `config.toml` file.

Available event hooks:
- `on_torrent_added`
- `on_download_started`
- `on_download_finished`

### Rule Matchers

Every rule can specify one or more optional matchers. A torrent must match all provided filters for the actions to execute.

- `require_labels`: List of labels the torrent must have.
- `require_tracker`: A substring that must appear in at least one tracker host.
- `name_pattern`: A regex pattern the torrent name must match.

### Available Actions

- `set_sequential`: Sets the torrent to download sequentially (good for streaming).
- `prioritize_files`: Prioritize files using parameters like `pattern` (regex), `first_alphabetical` (N), and `priority` (`high`, `normal`, `low`, `skip`).
- `set_labels`: Adds given labels to the torrent.
- `set_location`: Moves the torrent's data to a new path.
- `execute`: Runs a shell command. Variables like `{id}`, `{name}`, and `{dir}` will be replaced.
- `start`: Starts (resumes) the torrent.
- `stop`: Pauses the torrent.
- `remove`: Removes the torrent. Optional parameter `delete_local_data = true`.

### Examples

Add these sections to your `~/.config/trst/config.toml`:

```toml
[events]

# Automatically set sequential downloading for any torrent labeled "tv" or "movies"
[[events.on_download_started]]
require_labels = ["tv", "movies"]
actions = [
    { action = "set_sequential", enabled = true }
]

# When a torrent with a name like S01E01 finishes, move it and run a script
[[events.on_download_finished]]
name_pattern = '(?i).*S\d{2}E\d{2}.*'
actions = [
    { action = "set_location", path = "/media/tv_shows" },
    { action = "execute", command = "/home/user/.local/bin/organize_media.py", args = ["{dir}/{name}"] }
]

# For anime torrents, prioritize video files over the rest
[[events.on_torrent_added]]
require_tracker = "nyaa.si"
actions = [
    { action = "set_labels", labels = ["anime"] },
    { action = "prioritize_files", pattern = '(?i).*\.mkv$', priority = "high" },
    { action = "prioritize_files", pattern = '(?i).*\.txt$', priority = "skip" }
]
```
