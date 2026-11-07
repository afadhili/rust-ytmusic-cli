use crossterm::event::{KeyCode, KeyEvent};

use crate::{
    app::{App, InputMode, PlaylistFocus, Screen},
    types::track::Track,
};

impl App {
    pub(super) async fn handle_search_input(&mut self, key: KeyEvent) -> anyhow::Result<()> {
        match key.code {
            KeyCode::Esc => self.input_mode = InputMode::Normal,
            KeyCode::Enter => self.search().await?,
            KeyCode::Backspace => {
                self.query.pop();
            }
            KeyCode::Char(c) => self.query.push(c),
            _ => {}
        }
        Ok(())
    }

    pub(super) async fn handle_playlist_name_input(&mut self, key: KeyEvent) -> anyhow::Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.query.clear();
                self.input_mode = InputMode::Normal;
            }
            KeyCode::Enter => self.create_playlist_from_query()?,
            KeyCode::Backspace => {
                self.query.pop();
            }
            KeyCode::Char(c) => self.query.push(c),
            _ => {}
        }
        Ok(())
    }

    pub(super) async fn handle_normal_key(&mut self, key: KeyEvent) -> anyhow::Result<()> {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('/') => {
                self.screen = Screen::Search;
                self.input_mode = InputMode::TypingSearch;
                self.query.clear();
                self.status = "search mode".to_string();
            }
            KeyCode::Char('1') => self.screen = Screen::Search,
            KeyCode::Char('2') => self.screen = Screen::Queue,
            KeyCode::Char('3') => self.screen = Screen::History,
            KeyCode::Char('4') => self.screen = Screen::Favorites,
            KeyCode::Char('5') => self.screen = Screen::Playlists,
            KeyCode::Char('6') => self.open_lyrics(),
            KeyCode::Tab => self.toggle_playlist_focus(),
            KeyCode::Down | KeyCode::Char('j') => self.select_next(),
            KeyCode::Up | KeyCode::Char('k') => self.select_prev(),
            KeyCode::Char('J') => self.move_queue_down(),
            KeyCode::Char('K') => self.move_queue_up(),
            KeyCode::Enter => self.play_selected().await?,
            KeyCode::Char('a') => self.enqueue_selected(),
            KeyCode::Char('d') | KeyCode::Delete => self.remove_selected(),
            KeyCode::Char('n') => self.play_next().await?,
            KeyCode::Char('b') => self.play_previous().await?,
            KeyCode::Char('r') => self.refill_queue_if_needed().await?,
            KeyCode::Char(' ') => self.toggle_pause(),
            KeyCode::Char('s') => {
                self.player.stop().await?;
                self.status = "stopped".to_string();
            }
            KeyCode::Char('c') => self.cache_selected().await?,
            KeyCode::Char('f') => self.toggle_favorite_selected()?,
            KeyCode::Char('p') => self.add_selected_to_default_playlist()?,
            KeyCode::Char('P') => {
                self.screen = Screen::Playlists;
                self.input_mode = InputMode::TypingPlaylistName;
                self.query.clear();
                self.status = "type playlist name, Enter to create".to_string();
            }
            KeyCode::Char('+') | KeyCode::Char('=') => self.volume_up(),
            KeyCode::Char('-') => self.volume_down(),
            KeyCode::Char('h') | KeyCode::Left => self
                .player
                .seek(-5)
                .unwrap_or_else(|e| self.status = format!("seek failed: {e}")),
            KeyCode::Char('l') | KeyCode::Right => self
                .player
                .seek(5)
                .unwrap_or_else(|e| self.status = format!("seek failed: {e}")),
            _ => {}
        }
        Ok(())
    }

    pub(super) fn current_selection(&self) -> Option<Track> {
        match self.screen {
            Screen::Search => self.results.get(self.selected).cloned(),
            Screen::Queue => self.queue_state.queue.get(self.queue_state.queue_selected).cloned(),
            Screen::History => self.library_state.history.get(self.selected).cloned(),
            Screen::Favorites => self.library_state.favorites.get(self.selected).cloned(),
            Screen::Playlists => self
                .library_state
                .playlists
                .get(self.library_state.playlist_selected)
                .and_then(|p| p.tracks.get(self.library_state.playlist_track_selected))
                .cloned(),
            _ => None,
        }
    }

    pub(super) fn current_len(&self) -> usize {
        match self.screen {
            Screen::Search => self.results.len(),
            Screen::Queue => self.queue_state.queue.len(),
            Screen::History => self.library_state.history.len(),
            Screen::Favorites => self.library_state.favorites.len(),
            Screen::Playlists => match self.library_state.playlist_focus {
                PlaylistFocus::List => self.library_state.playlists.len(),
                PlaylistFocus::Tracks => self.library_state.focused_playlist_track_len(),
            },
            Screen::Lyrics => self
                .lyrics_state
                .lyrics
                .as_ref()
                .map(|l| l.display_lines().len())
                .unwrap_or(0),
        }
    }

    pub(super) fn toggle_playlist_focus(&mut self) {
        if self.screen != Screen::Playlists {
            return;
        }
        self.library_state.playlist_focus = match self.library_state.playlist_focus {
            PlaylistFocus::List => PlaylistFocus::Tracks,
            PlaylistFocus::Tracks => PlaylistFocus::List,
        };
        self.status = match self.library_state.playlist_focus {
            PlaylistFocus::List => "playlist focus: list".to_string(),
            PlaylistFocus::Tracks => "playlist focus: tracks".to_string(),
        };
    }

    pub(super) fn select_next(&mut self) {
        if self.screen == Screen::Lyrics {
            self.lyrics_state.lyrics_scroll = self.lyrics_state.lyrics_scroll.saturating_add(1);
            return;
        }
        if self.screen == Screen::Queue {
            if !self.queue_state.queue.is_empty() {
                self.queue_state.queue_selected =
                    (self.queue_state.queue_selected + 1).min(self.queue_state.queue.len() - 1);
            }
            return;
        }
        if self.screen == Screen::Playlists {
            match self.library_state.playlist_focus {
                PlaylistFocus::List => {
                    if !self.library_state.playlists.is_empty() {
                        self.library_state.playlist_selected = (self.library_state.playlist_selected
                            + 1)
                        .min(self.library_state.playlists.len() - 1);
                        self.library_state.playlist_track_selected = 0;
                    }
                }
                PlaylistFocus::Tracks => {
                    let len = self.library_state.focused_playlist_track_len();
                    if len > 0 {
                        self.library_state.playlist_track_selected =
                            (self.library_state.playlist_track_selected + 1).min(len - 1);
                    }
                }
            }
            return;
        }
        let len = self.current_len();
        if len > 0 {
            self.selected = (self.selected + 1).min(len - 1);
        }
    }

    pub(super) fn select_prev(&mut self) {
        if self.screen == Screen::Lyrics {
            self.lyrics_state.lyrics_scroll = self.lyrics_state.lyrics_scroll.saturating_sub(1);
            return;
        }
        if self.screen == Screen::Queue {
            self.queue_state.queue_selected = self.queue_state.queue_selected.saturating_sub(1);
            return;
        }
        if self.screen == Screen::Playlists {
            match self.library_state.playlist_focus {
                PlaylistFocus::List => {
                    self.library_state.playlist_selected =
                        self.library_state.playlist_selected.saturating_sub(1);
                    self.library_state.playlist_track_selected = 0;
                }
                PlaylistFocus::Tracks => {
                    self.library_state.playlist_track_selected = self
                        .library_state
                        .playlist_track_selected
                        .saturating_sub(1)
                }
            }
            return;
        }
        self.selected = self.selected.saturating_sub(1);
    }
}
