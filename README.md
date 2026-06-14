# ytmusic-cli

A terminal YouTube Music player built with Rust, Ratatui, mpv, yt-dlp, and [`rs-ytmusic-api`](https://crates.io/crates/rs-ytmusic-api).

This project provides a keyboard-driven TUI for searching, playing, caching, queuing, and managing YouTube Music tracks directly from the terminal.

> This project is unofficial and is not affiliated with YouTube, Google, or YouTube Music.

## Features

- Search YouTube Music tracks
- Play music with `mpv`
- Cache audio with `yt-dlp`
- Queue management
- Non-destructive queue playback
- Auto-refill queue using YouTube Music watch queue
- Autoplay next track when the current track finishes
- Playlist mode
- Playlist-local queue playback
- History
- Favorites
- Local playlists
- Synced lyrics using LRCLIB
- YouTube Music lyrics fallback
- File-based lyrics cache
- mpv IPC control
- Keyboard-driven Ratatui interface

## Requirements

Install runtime dependencies:

```bash
sudo pacman -S mpv yt-dlp
```

For other distributions:

```bash
# Debian / Ubuntu
sudo apt install mpv yt-dlp

# Fedora
sudo dnf install mpv yt-dlp
```

## Installation

From source:

```bash
git clone https://github.com/afadhili/rust-ytmusic-cli
cd ytmusic-cli
cargo install --path .
```

Or run directly:

```bash
cargo run
```

## Dependencies

This project uses:

- [`ratatui`](https://crates.io/crates/ratatui) for terminal UI
- [`crossterm`](https://crates.io/crates/crossterm) for terminal events
- [`tokio`](https://crates.io/crates/tokio) for async runtime
- [`rs-ytmusic-api`](https://crates.io/crates/rs-ytmusic-api) for YouTube Music metadata/search
- `mpv` for playback
- `yt-dlp` for audio caching
- LRCLIB for synced lyrics

## Usage

Start the TUI:

```bash
ytmusic-cli
```

Or with Cargo:

```bash
cargo run
```

## Keybindings

| Key | Action |
|---|---|
| `/` | Enter search mode |
| `Enter` | Search or play selected item |
| `j` / `Down` | Move selection down |
| `k` / `Up` | Move selection up |
| `1` | Search tab |
| `2` | Queue tab |
| `3` | History tab |
| `4` | Favorites tab |
| `5` | Playlists tab |
| `6` | Lyrics tab |
| `a` | Add selected track to queue |
| `n` | Play next track |
| `b` | Play previous track |
| `d` | Remove selected queue/favorite item |
| `J` | Move queue item down |
| `K` | Move queue item up |
| `f` | Toggle favorite |
| `p` | Add selected track to default playlist |
| `P` | Create playlist |
| `c` | Cache selected track |
| `r` | Refill auto queue |
| `Space` | Pause/resume |
| `s` | Stop playback |
| `h` / `Left` | Seek backward |
| `l` / `Right` | Seek forward |
| `+` | Volume up |
| `-` | Volume down |
| `q` / `Esc` | Quit |

## Cache

Audio cache is stored in:

```txt
~/Music/ytmusic.cli/
```

Cached audio files are created using `yt-dlp`.

The cache command uses metadata and thumbnail embedding when possible.

## Lyrics Cache

Lyrics are cached as plain text files in:

```txt
~/Music/ytmusic.cli/{video_id}-lyrics.txt
```

Synced lyrics are stored in LRC format when available.

Lyrics priority:

1. Read local cache
2. Fetch synced lyrics from LRCLIB
3. Fallback to YouTube Music lyrics
4. Save result to local cache

## Queue Behavior

The queue is non-destructive.

When a track is played, it stays in the queue. Playback position is tracked with an internal queue index.

Auto queue behavior:

- The first played track is added to the queue
- The queue is refilled when the number of tracks after the currently playing track is less than 5
- Refill uses `fetch_watch_queue` from `rs-ytmusic-api`
- Duplicate tracks are skipped
- Tracks already played or already queued in the current session are not re-added

Playlist mode behavior:

- Playing from a playlist replaces the queue with that playlist's tracks
- Autoplay only moves inside the playlist
- YouTube Music watch queue refill is disabled in playlist mode

## Data Storage

Application data is stored in:

```txt
~/.local/share/ytmusic-cli/
```

Files:

```txt
history.json
favorites.json
playlists.json
```

Audio and lyrics cache are stored in:

```txt
~/Music/ytmusic.cli/
```

## Development

Run:

```bash
cargo run
```

Check:

```bash
cargo check
```

Format:

```bash
cargo fmt
```

Clippy:

```bash
cargo clippy --all-targets
```

Build release:

```bash
cargo build --release
```

The compiled binary will be available at:

```txt
target/release/ytmusic-cli
```

## Project Structure

```txt
src/
├── main.rs
├── app.rs
├── tui.rs
├── services/
│   ├── cache.rs
│   ├── lyrics.rs
│   ├── mod.rs
│   ├── music.rs
│   ├── player.rs
│   └── storage.rs
├── types/
│   ├── lyrics.rs
│   ├── mod.rs
│   ├── playlist.rs
│   └── track.rs
└── ui/
    └── mod.rs
```

## Notes

This project depends on YouTube Music's internal API through `rs-ytmusic-api`.

YouTube Music response structures may change over time. If search, lyrics, queue, or metadata parsing breaks, update `rs-ytmusic-api` first.

## Disclaimer

This project is unofficial.

YouTube Music is a trademark of Google LLC. This project is not affiliated with Google, YouTube, or YouTube Music.

Use responsibly and respect YouTube's Terms of Service.

## License

GPL-3.0
