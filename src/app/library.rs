use crate::{
    app::{App, InputMode, PlaylistFocus},
    types::{playlist::LocalPlaylist, track::Track},
};

impl App {
    pub(super) async fn cache_selected(&mut self) -> anyhow::Result<()> {
        let Some(track) = self.current_selection() else {
            self.status = "nothing selected".to_string();
            return Ok(());
        };
        if self.cache.find(&track.video_id).is_some() {
            self.status = "already cached".to_string();
            return Ok(());
        }
        self.cache.cache_track(&track).await?;
        self.status = format!("caching: {}", track.title);
        Ok(())
    }

    pub(super) fn toggle_favorite_selected(&mut self) -> anyhow::Result<()> {
        let Some(track) = self
            .current_selection()
            .or_else(|| self.queue_state.now_playing.clone())
        else {
            self.status = "nothing selected".to_string();
            return Ok(());
        };
        if let Some(i) = self
            .library_state
            .favorites
            .iter()
            .position(|t| t.video_id == track.video_id)
        {
            self.library_state.favorites.remove(i);
            self.status = "removed from favorites".to_string();
        } else {
            self.library_state.favorites.insert(0, track.clone());
            self.status = format!("favorited: {}", track.title);
        }
        self.storage.save_favorites(&self.library_state.favorites)
    }

    pub(super) fn add_selected_to_default_playlist(&mut self) -> anyhow::Result<()> {
        let Some(track) = self
            .current_selection()
            .or_else(|| self.queue_state.now_playing.clone())
        else {
            self.status = "nothing selected".to_string();
            return Ok(());
        };
        if self.library_state.playlists.is_empty() {
            self.library_state.playlists.push(LocalPlaylist {
                name: "Default".to_string(),
                tracks: Vec::new(),
            });
        }
        if !self.library_state.playlists[0]
            .tracks
            .iter()
            .any(|t| t.video_id == track.video_id)
        {
            self.library_state.playlists[0].tracks.push(track.clone());
            self.status = format!(
                "added to playlist: {}",
                self.library_state.playlists[0].name
            );
        } else {
            self.status = "already in playlist".to_string();
        }
        self.storage
            .save_playlists(&self.library_state.playlists)
    }

    pub(super) fn create_playlist_from_query(&mut self) -> anyhow::Result<()> {
        let name = self.query.trim().to_string();
        if name.is_empty() {
            self.status = "playlist name is empty".to_string();
            return Ok(());
        }
        if self
            .library_state
            .playlists
            .iter()
            .any(|p| p.name == name)
        {
            self.status = "playlist already exists".to_string();
            return Ok(());
        }
        self.library_state.playlists.push(LocalPlaylist {
            name: name.clone(),
            tracks: Vec::new(),
        });
        self.query.clear();
        self.input_mode = InputMode::Normal;
        self.library_state.playlist_selected = self.library_state.playlists.len().saturating_sub(1);
        self.library_state.playlist_focus = PlaylistFocus::Tracks;
        self.status = format!("created playlist: {name}");
        self.storage
            .save_playlists(&self.library_state.playlists)
    }

    /// Persist a freshly-played track to history. Returns nothing — the
    /// caller doesn't care; this just keeps the playback path clean.
    pub(super) fn push_history(&mut self, track: Track) -> anyhow::Result<()> {
        let history = self.library_state.record_play(track);
        self.storage.save_history(&history)
    }

    /// Remove the favorite under the `selected` cursor and save.
    /// Preserves the original "do nothing if out of range" semantics.
    /// Note: `selected` lives on `App` because it's the cross-screen
    /// selection cursor (Search/History/Favorites all share it).
    pub(super) fn remove_favorite_at_selected(&mut self) {
        if self.selected < self.library_state.favorites.len() {
            self.library_state.favorites.remove(self.selected);
            self.selected = self.selected.saturating_sub(1);
            let favorites = self.library_state.favorites.clone();
            let _ = self.storage.save_favorites(&favorites);
            self.status = "removed from favorites".to_string();
        }
    }

    /// Remove the playlist track under `playlist_track_selected`, fix up
    /// the cursor so it stays on the same logical track, and save.
    pub(super) fn remove_playlist_track_at_selected(&mut self) {
        let library = &mut self.library_state;
        let Some(playlist) = library.playlists.get_mut(library.playlist_selected) else {
            return;
        };
        if library.playlist_track_selected >= playlist.tracks.len() {
            return;
        }
        playlist.tracks.remove(library.playlist_track_selected);
        library.playlist_track_selected = library
            .playlist_track_selected
            .min(playlist.tracks.len().saturating_sub(1));
        let playlists = library.playlists.clone();
        let _ = self.storage.save_playlists(&playlists);
        self.status = "removed from playlist".to_string();
    }
}
