# ytmusic-cli

Rust terminal YouTube Music player using Ratatui, `rust-ytmusic-api`, mpv IPC, yt-dlp cache, queue, history, favorites, playlists, and synced lyrics.

## Runtime dependencies

```bash
sudo pacman -S mpv yt-dlp
```

## Run

```bash
cargo run
```

## Features

- Search songs through `rust-ytmusic-api`
- Play tracks with `mpv`
- Control mpv through JSON IPC over Unix socket
- Cache audio to `~/Music/ytmusic.cli` using `yt-dlp`
- Queue with non-destructive playback index
- First played track is inserted into the queue and becomes the active queue item
- Auto queue refill when upcoming tracks after the current queue index are below 5
- Autoplay next track when mpv reports the current track has finished
- Auto queue mode refills from `fetch_watch_queue()` when the remaining upcoming list is below 5
- Playlist playback mode stays limited to selected playlist tracks and never refills from YouTube Music
- History and favorites saved locally
- Local playlists saved locally
- Lyrics tab with LRCLIB synced lyrics first, YouTube Music lyrics fallback second
- Non-blocking lyrics fetch; the TUI stays responsive while lyrics load
- File-based lyrics cache at `~/Music/ytmusic.cli/{video_id}-lyrics.txt`
- Synced lyric line highlight based on mpv `time-pos`

## Lyrics behavior

Lyrics are resolved in this order:

1. Local lyrics text file from `~/Music/ytmusic.cli/{video_id}-lyrics.txt`
2. Legacy JSON cache from `~/.local/share/ytmusic-cli/lyrics.json` if it exists, then migrate to the text file cache
3. LRCLIB `/api/get` with `track_name`, `artist_name`, and `duration`
4. LRCLIB `/api/search` fallback
5. YouTube Music lyrics through `rust-ytmusic-api`

LRCLIB synced lyrics are parsed from LRC timestamps and highlighted in the lyrics tab according to mpv playback position. Fetching runs in a Tokio task, so pressing `6` shows a loading message instead of freezing the UI.

## Keybinds

```txt
/        search mode
Enter    search / play selected
1        search tab
2        queue tab
3        history tab
4        favorites tab
5        playlists tab
6        lyrics tab
Tab      switch playlist focus between playlist list and playlist tracks
j/Down   next item
k/Up     previous item
J        move queue item down
K        move queue item up
a        enqueue selected
n        next track
b        previous track
r        refill auto queue from current track
f        toggle favorite
p        add selected/current track to default playlist
P        create playlist
c        cache selected track
Space    pause/resume
s        stop
h/Left   seek -5s
l/Right  seek +5s
+        volume up
-        volume down
q/Esc    quit
```

## Data paths

```txt
~/.local/share/ytmusic-cli/history.json
~/.local/share/ytmusic-cli/favorites.json
~/.local/share/ytmusic-cli/playlists.json
~/.local/share/ytmusic-cli/lyrics.json   # legacy/migration cache
~/Music/ytmusic.cli/{video_id}-lyrics.txt
~/Music/ytmusic.cli/{video_id}.m4a
```

## Auto queue dedupe

Auto refill uses a session-level `queued_video_ids` set. A track that was already added to the queue, played in the current session, or exists in previous/current queue state will not be added again by `fetch_watch_queue()`.

Playlist playback does not call `fetch_watch_queue()` and keeps queue limited to the selected playlist.
# rust-ytmusic-cli
