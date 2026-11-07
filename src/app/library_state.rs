use crate::{
    app::PlaylistFocus,
    types::{playlist::LocalPlaylist, track::Track},
};

/// User library data: history, favorites, and local playlists, plus the
/// cursors that select within them on the Playlists screen.
///
/// No tricky invariants — just pure data plus the save helpers that persist
/// the right slice to disk.
pub struct LibraryState {
    pub history: Vec<Track>,
    pub favorites: Vec<Track>,
    pub playlists: Vec<LocalPlaylist>,
    pub playlist_selected: usize,
    pub playlist_track_selected: usize,
    pub playlist_focus: PlaylistFocus,
}

impl LibraryState {
    pub fn new(history: Vec<Track>, favorites: Vec<Track>, playlists: Vec<LocalPlaylist>) -> Self {
        Self {
            history,
            favorites,
            playlists,
            playlist_selected: 0,
            playlist_track_selected: 0,
            playlist_focus: PlaylistFocus::List,
        }
    }

    /// Drop a track from the front of `history`, push it back at index 0,
    /// cap the list, and return the result so the caller can persist.
    /// Behavior matches the original `push_history` exactly.
    pub fn record_play(&mut self, track: Track) -> Vec<Track> {
        self.history.retain(|t| t.video_id != track.video_id);
        self.history.insert(0, track);
        self.history.truncate(100);
        self.history.clone()
    }

    /// Number of tracks in the currently focused playlist sub-view. Used by
    /// the selection navigation logic so callers don't need to know the
    /// focus-state encoding.
    pub fn focused_playlist_track_len(&self) -> usize {
        self.playlists
            .get(self.playlist_selected)
            .map(|p| p.tracks.len())
            .unwrap_or(0)
    }
}
