use std::{
    collections::{HashSet, VecDeque},
    io,
    time::Duration,
};

use tokio::task::JoinHandle;

use anyhow::Context;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::{
    services::{
        cache::CacheService, lyrics::LyricsService, music::MusicService, player::PlayerService,
        storage::StorageService,
    },
    tui,
    types::{lyrics::LyricsData, playlist::LocalPlaylist, track::Track},
};

const MIN_QUEUE_LEN: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Search,
    Queue,
    History,
    Favorites,
    Playlists,
    Lyrics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    TypingSearch,
    TypingPlaylistName,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueSource {
    Auto,
    Playlist,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistFocus {
    List,
    Tracks,
}

pub struct App {
    pub screen: Screen,
    pub input_mode: InputMode,
    pub query: String,
    pub results: Vec<Track>,
    pub selected: usize,
    pub queue: VecDeque<Track>,
    pub queue_selected: usize,
    pub queue_index: Option<usize>,
    pub queue_source: QueueSource,
    pub queued_video_ids: HashSet<String>,
    pub active_playlist: Option<usize>,
    pub history: Vec<Track>,
    pub favorites: Vec<Track>,
    pub playlists: Vec<LocalPlaylist>,
    pub playlist_selected: usize,
    pub playlist_track_selected: usize,
    pub playlist_focus: PlaylistFocus,
    pub now_playing: Option<Track>,
    pub previous: Vec<Track>,
    pub lyrics: Option<LyricsData>,
    pub lyrics_scroll: usize,
    pub lyrics_loading: bool,
    pub lyrics_loading_track_id: Option<String>,
    pub lyrics_task: Option<JoinHandle<anyhow::Result<Option<LyricsData>>>>,
    pub status: String,
    pub should_quit: bool,
    pub storage: StorageService,
    pub cache: CacheService,
    pub player: PlayerService,
    pub music: MusicService,
    pub lyrics_service: LyricsService,
}

impl App {
    pub async fn new() -> anyhow::Result<Self> {
        let storage = StorageService::new()?;
        let cache = CacheService::new()?;
        let history = storage
            .load_history()
            .into_iter()
            .map(|t| cache.enrich(t))
            .collect();
        let favorites = storage
            .load_favorites()
            .into_iter()
            .map(|t| cache.enrich(t))
            .collect();
        let playlists = storage.load_playlists();

        Ok(Self {
            screen: Screen::Search,
            input_mode: InputMode::TypingSearch,
            query: String::new(),
            results: Vec::new(),
            selected: 0,
            queue: VecDeque::new(),
            queue_selected: 0,
            queue_index: None,
            queue_source: QueueSource::Auto,
            queued_video_ids: HashSet::new(),
            active_playlist: None,
            history,
            favorites,
            playlists,
            playlist_selected: 0,
            playlist_track_selected: 0,
            playlist_focus: PlaylistFocus::List,
            now_playing: None,
            previous: Vec::new(),
            lyrics: None,
            lyrics_scroll: 0,
            lyrics_loading: false,
            lyrics_loading_track_id: None,
            lyrics_task: None,
            status: "type search query, then Enter".to_string(),
            should_quit: false,
            player: PlayerService::new(),
            storage,
            cache,
            music: MusicService::new().await?,
            lyrics_service: LyricsService::new(),
        })
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        enable_raw_mode().context("failed to enable raw mode")?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen).context("failed to enter alternate screen")?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend).context("failed to create terminal")?;

        let result = self.event_loop(&mut terminal).await;

        disable_raw_mode().ok();
        execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
        terminal.show_cursor().ok();
        self.player.stop().await.ok();
        result
    }

    async fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> anyhow::Result<()> {
        while !self.should_quit {
            self.poll_lyrics_task().await;
            self.poll_player_end().await?;
            terminal.draw(|frame| tui::draw(frame, self))?;
            if event::poll(Duration::from_millis(120))? {
                if let Event::Key(key) = event::read()? {
                    self.handle_key(key).await?;
                }
            }
        }
        Ok(())
    }

    async fn handle_key(&mut self, key: KeyEvent) -> anyhow::Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return Ok(());
        }

        match self.input_mode {
            InputMode::TypingSearch => self.handle_search_input(key).await,
            InputMode::TypingPlaylistName => self.handle_playlist_name_input(key).await,
            InputMode::Normal => self.handle_normal_key(key).await,
        }
    }

    async fn handle_search_input(&mut self, key: KeyEvent) -> anyhow::Result<()> {
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

    async fn handle_playlist_name_input(&mut self, key: KeyEvent) -> anyhow::Result<()> {
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

    async fn handle_normal_key(&mut self, key: KeyEvent) -> anyhow::Result<()> {
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

    async fn search(&mut self) -> anyhow::Result<()> {
        let query = self.query.trim().to_string();
        if query.is_empty() {
            self.status = "query is empty".to_string();
            return Ok(());
        }
        self.status = format!("searching: {query}");
        let songs = self.music.search_songs(&query).await?;
        self.results = songs
            .into_iter()
            .map(|track| self.cache.enrich(track))
            .collect();
        self.selected = 0;
        self.screen = Screen::Search;
        self.input_mode = InputMode::Normal;
        self.status = format!("{} result(s)", self.results.len());
        Ok(())
    }

    async fn play_selected(&mut self) -> anyhow::Result<()> {
        match self.screen {
            Screen::Queue => self.play_queue_at(self.queue_selected).await,
            Screen::Playlists => self.play_playlist_selection().await,
            _ => {
                let Some(track) = self.current_selection() else {
                    self.status = "nothing selected".to_string();
                    return Ok(());
                };
                self.play_track_detached(track).await
            }
        }
    }

    async fn play_playlist_selection(&mut self) -> anyhow::Result<()> {
        let Some(playlist) = self.playlists.get(self.playlist_selected).cloned() else {
            self.status = "no playlist selected".to_string();
            return Ok(());
        };

        if playlist.tracks.is_empty() {
            self.status = "playlist is empty".to_string();
            return Ok(());
        }

        let index = self.playlist_track_selected.min(playlist.tracks.len() - 1);
        self.queue = playlist
            .tracks
            .into_iter()
            .map(|track| self.cache.enrich(track))
            .collect();
        self.queue_source = QueueSource::Playlist;
        self.queued_video_ids.clear();
        for track in self.queue.iter() {
            if !track.video_id.is_empty() {
                self.queued_video_ids.insert(track.video_id.clone());
            }
        }
        self.active_playlist = Some(self.playlist_selected);
        self.queue_selected = index;
        self.play_queue_at(index).await
    }

    async fn play_track_detached(&mut self, track: Track) -> anyhow::Result<()> {
        self.queue_source = QueueSource::Auto;
        self.active_playlist = None;

        let track = self.cache.enrich(track);
        let index = self.ensure_track_in_auto_queue(track.clone());
        self.queue_index = Some(index);
        self.queue_selected = index;

        self.set_now_playing(track).await?;
        self.refill_queue_if_needed().await?;
        Ok(())
    }

    async fn play_queue_at(&mut self, index: usize) -> anyhow::Result<()> {
        let Some(track) = self.queue.get(index).cloned() else {
            self.status = "queue index is empty".to_string();
            return Ok(());
        };

        self.queue_index = Some(index);
        self.queue_selected = index;
        self.set_now_playing(track).await?;

        if self.queue_source == QueueSource::Auto {
            self.refill_queue_if_needed().await?;
        }

        Ok(())
    }

    async fn set_now_playing(&mut self, track: Track) -> anyhow::Result<()> {
        if let Some(current) = self.now_playing.take() {
            if current.video_id != track.video_id {
                self.previous.push(current);
            }
        }

        let track = self.cache.enrich(track);
        self.remember_track(&track);
        self.player.play(&track).await?;
        if track.cached_path.is_none() {
            let _ = self.cache.cache_track(&track).await;
        }
        self.push_history(track.clone())?;
        self.now_playing = Some(track.clone());
        self.lyrics = None;
        if let Some(task) = self.lyrics_task.take() {
            task.abort();
        }
        self.lyrics_loading = false;
        self.lyrics_loading_track_id = None;
        self.status = if track.cached_path.is_some() {
            "playing cached track".to_string()
        } else {
            "playing remote, caching in background".to_string()
        };
        Ok(())
    }

    async fn play_next(&mut self) -> anyhow::Result<()> {
        if self.queue_source == QueueSource::Auto {
            self.refill_queue_if_needed().await?;
        }

        let next_index = match self.queue_index {
            Some(index) => index + 1,
            None => 0,
        };

        if next_index >= self.queue.len() {
            if self.queue_source == QueueSource::Auto {
                self.refill_queue_if_needed().await?;
            }
        }

        if next_index >= self.queue.len() {
            self.status = "no next track".to_string();
            return Ok(());
        }

        self.play_queue_at(next_index).await
    }

    async fn poll_player_end(&mut self) -> anyhow::Result<()> {
        if self.player.has_exited() {
            self.handle_track_finished().await?;
        }

        Ok(())
    }

    async fn handle_track_finished(&mut self) -> anyhow::Result<()> {
        if self.now_playing.is_none() {
            return Ok(());
        }

        match self.queue_source {
            QueueSource::Playlist => {
                let next_index = self.queue_index.map(|index| index + 1).unwrap_or(0);

                if next_index < self.queue.len() {
                    self.status = "autoplay next playlist track".to_string();
                    self.play_queue_at(next_index).await?;
                } else {
                    self.status = "playlist finished".to_string();
                }
            }
            QueueSource::Auto => {
                self.refill_queue_if_needed().await?;

                let next_index = self.queue_index.map(|index| index + 1).unwrap_or(0);

                if next_index < self.queue.len() {
                    self.status = "autoplay next track".to_string();
                    self.play_queue_at(next_index).await?;
                    self.refill_queue_if_needed().await?;
                } else {
                    self.status = "queue finished".to_string();
                }
            }
        }

        Ok(())
    }

    fn ensure_track_in_auto_queue(&mut self, track: Track) -> usize {
        if let Some(index) = self
            .queue
            .iter()
            .position(|item| item.video_id == track.video_id)
        {
            self.remember_track(&track);
            return index;
        }

        let insert_index = self
            .queue_index
            .map(|index| index + 1)
            .unwrap_or(self.queue.len());
        let insert_index = insert_index.min(self.queue.len());
        self.remember_track(&track);
        self.queue.insert(insert_index, track);
        insert_index
    }

    async fn play_previous(&mut self) -> anyhow::Result<()> {
        if let Some(index) = self.queue_index {
            if index > 0 && index - 1 < self.queue.len() {
                return self.play_queue_at(index - 1).await;
            }
        }

        let Some(track) = self.previous.pop() else {
            self.status = "no previous track".to_string();
            return Ok(());
        };
        self.set_now_playing(track).await
    }

    fn remaining_after_current(&self) -> usize {
        match self.queue_index {
            Some(index) => self.queue.len().saturating_sub(index + 1),
            None => self.queue.len(),
        }
    }

    fn should_refill_queue(&self) -> bool {
        self.queue_source == QueueSource::Auto && self.remaining_after_current() < MIN_QUEUE_LEN
    }

    async fn refill_queue_if_needed(&mut self) -> anyhow::Result<()> {
        if self.queue_source == QueueSource::Playlist {
            return Ok(());
        }

        if !self.should_refill_queue() {
            return Ok(());
        }

        let Some(current) = self.now_playing.as_ref() else {
            return Ok(());
        };

        let seed_video_id = current.video_id.clone();
        let mut added = 0usize;

        while self.should_refill_queue() {
            let before = self.remaining_after_current();
            let batch_added = self.refill_queue_from(&seed_video_id).await?;
            added += batch_added;

            if batch_added == 0 || self.remaining_after_current() == before {
                break;
            }
        }

        if added > 0 {
            self.status = format!(
                "auto queued {added} track(s); {} upcoming track(s)",
                self.remaining_after_current()
            );
        }

        Ok(())
    }

    async fn refill_queue_from(&mut self, video_id: &str) -> anyhow::Result<usize> {
        let up_next = self.music.watch_queue(video_id).await?;
        let mut added = 0;

        for track in up_next {
            if self.remaining_after_current() >= MIN_QUEUE_LEN {
                break;
            }

            let track = self.cache.enrich(track);

            if track.video_id.is_empty() {
                continue;
            }

            if self.has_track_anywhere(&track.video_id) {
                continue;
            }

            self.remember_track(&track);
            self.queue.push_back(track);
            added += 1;
        }

        Ok(added)
    }

    fn has_track_anywhere(&self, video_id: &str) -> bool {
        if video_id.is_empty() {
            return true;
        }

        self.queued_video_ids.contains(video_id)
            || self
                .now_playing
                .as_ref()
                .is_some_and(|t| t.video_id == video_id)
            || self.queue.iter().any(|t| t.video_id == video_id)
            || self.previous.iter().any(|t| t.video_id == video_id)
    }

    fn remember_track(&mut self, track: &Track) {
        if !track.video_id.is_empty() {
            self.queued_video_ids.insert(track.video_id.clone());
        }
    }

    fn enqueue_selected(&mut self) {
        if let Some(track) = self.current_selection() {
            if self.queue.iter().any(|t| t.video_id == track.video_id) {
                self.status = "already in queue".to_string();
                return;
            }
            self.remember_track(&track);
            self.queue.push_back(track.clone());
            self.queue_source = QueueSource::Auto;
            self.active_playlist = None;
            self.status = format!("queued: {}", track.title);
            self.screen = Screen::Queue;
        } else {
            self.status = "nothing selected".to_string();
        }
    }

    fn remove_selected(&mut self) {
        match self.screen {
            Screen::Queue => {
                if self.queue_selected < self.queue.len() {
                    self.queue.remove(self.queue_selected);
                    if let Some(index) = self.queue_index {
                        self.queue_index = if self.queue.is_empty() {
                            None
                        } else if self.queue_selected < index {
                            Some(index.saturating_sub(1))
                        } else if self.queue_selected == index {
                            Some(index.min(self.queue.len() - 1))
                        } else {
                            Some(index)
                        };
                    }
                    self.queue_selected =
                        self.queue_selected.min(self.queue.len().saturating_sub(1));
                    self.status = "removed from queue".to_string();
                }
            }
            Screen::Favorites => {
                if self.selected < self.favorites.len() {
                    self.favorites.remove(self.selected);
                    let _ = self.storage.save_favorites(&self.favorites);
                    self.selected = self.selected.saturating_sub(1);
                    self.status = "removed from favorites".to_string();
                }
            }
            Screen::Playlists => self.remove_selected_from_playlist(),
            _ => {}
        }
    }

    fn remove_selected_from_playlist(&mut self) {
        let Some(playlist) = self.playlists.get_mut(self.playlist_selected) else {
            return;
        };
        if self.playlist_track_selected >= playlist.tracks.len() {
            return;
        }
        playlist.tracks.remove(self.playlist_track_selected);
        self.playlist_track_selected = self
            .playlist_track_selected
            .min(playlist.tracks.len().saturating_sub(1));
        let _ = self.storage.save_playlists(&self.playlists);
        self.status = "removed from playlist".to_string();
    }

    fn move_queue_up(&mut self) {
        if self.screen != Screen::Queue || self.queue_selected == 0 || self.queue.len() < 2 {
            return;
        }
        self.queue
            .swap(self.queue_selected, self.queue_selected - 1);
        if let Some(index) = self.queue_index {
            if index == self.queue_selected {
                self.queue_index = Some(index - 1);
            } else if index + 1 == self.queue_selected {
                self.queue_index = Some(index + 1);
            }
        }
        self.queue_selected -= 1;
    }

    fn move_queue_down(&mut self) {
        if self.screen != Screen::Queue || self.queue_selected + 1 >= self.queue.len() {
            return;
        }
        self.queue
            .swap(self.queue_selected, self.queue_selected + 1);
        if let Some(index) = self.queue_index {
            if index == self.queue_selected {
                self.queue_index = Some(index + 1);
            } else if index == self.queue_selected + 1 {
                self.queue_index = Some(index - 1);
            }
        }
        self.queue_selected += 1;
    }

    async fn cache_selected(&mut self) -> anyhow::Result<()> {
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

    fn toggle_favorite_selected(&mut self) -> anyhow::Result<()> {
        let Some(track) = self
            .current_selection()
            .or_else(|| self.now_playing.clone())
        else {
            self.status = "nothing selected".to_string();
            return Ok(());
        };
        if let Some(i) = self
            .favorites
            .iter()
            .position(|t| t.video_id == track.video_id)
        {
            self.favorites.remove(i);
            self.status = "removed from favorites".to_string();
        } else {
            self.favorites.insert(0, track.clone());
            self.status = format!("favorited: {}", track.title);
        }
        self.storage.save_favorites(&self.favorites)
    }

    fn add_selected_to_default_playlist(&mut self) -> anyhow::Result<()> {
        let Some(track) = self
            .current_selection()
            .or_else(|| self.now_playing.clone())
        else {
            self.status = "nothing selected".to_string();
            return Ok(());
        };
        if self.playlists.is_empty() {
            self.playlists.push(LocalPlaylist {
                name: "Default".to_string(),
                tracks: Vec::new(),
            });
        }
        if !self.playlists[0]
            .tracks
            .iter()
            .any(|t| t.video_id == track.video_id)
        {
            self.playlists[0].tracks.push(track.clone());
            self.status = format!("added to playlist: {}", self.playlists[0].name);
        } else {
            self.status = "already in playlist".to_string();
        }
        self.storage.save_playlists(&self.playlists)
    }

    fn create_playlist_from_query(&mut self) -> anyhow::Result<()> {
        let name = self.query.trim().to_string();
        if name.is_empty() {
            self.status = "playlist name is empty".to_string();
            return Ok(());
        }
        if self.playlists.iter().any(|p| p.name == name) {
            self.status = "playlist already exists".to_string();
            return Ok(());
        }
        self.playlists.push(LocalPlaylist {
            name: name.clone(),
            tracks: Vec::new(),
        });
        self.query.clear();
        self.input_mode = InputMode::Normal;
        self.playlist_selected = self.playlists.len().saturating_sub(1);
        self.playlist_focus = PlaylistFocus::Tracks;
        self.status = format!("created playlist: {name}");
        self.storage.save_playlists(&self.playlists)
    }

    fn open_lyrics(&mut self) {
        self.screen = Screen::Lyrics;
        self.lyrics_scroll = 0;

        let Some(track) = self.now_playing.clone() else {
            self.status = "no track playing".to_string();
            return;
        };

        if self
            .lyrics
            .as_ref()
            .is_some_and(|lyrics| lyrics.video_id == track.video_id)
        {
            self.status = "lyrics already loaded".to_string();
            return;
        }

        if self.lyrics_loading_track_id.as_deref() == Some(track.video_id.as_str()) {
            self.status = "lyrics still loading".to_string();
            return;
        }

        let lyrics_service = self.lyrics_service.clone();
        let storage = self.storage.clone();
        let video_id = track.video_id.clone();

        self.lyrics = None;
        self.lyrics_loading = true;
        self.lyrics_loading_track_id = Some(video_id.clone());
        self.status = format!("loading lyrics: {}", track.label());

        self.lyrics_task = Some(tokio::spawn(async move {
            let music = MusicService::new().await?;
            lyrics_service.get_lyrics(&track, &music, &storage).await
        }));
    }

    async fn poll_lyrics_task(&mut self) {
        let Some(task) = self.lyrics_task.as_ref() else {
            return;
        };

        if !task.is_finished() {
            return;
        }

        let Some(task) = self.lyrics_task.take() else {
            return;
        };

        self.lyrics_loading = false;
        self.lyrics_loading_track_id = None;

        match task.await {
            Ok(Ok(lyrics)) => {
                self.lyrics = lyrics;
                self.status = match &self.lyrics {
                    Some(data) if data.instrumental => "instrumental track from LRCLIB".to_string(),
                    Some(data) if data.is_synced() => {
                        "synced lyrics loaded from cache/LRCLIB".to_string()
                    }
                    Some(data)
                        if data.source == crate::types::lyrics::LyricsSource::YoutubeFallback =>
                    {
                        "plain lyrics loaded from YouTube fallback".to_string()
                    }
                    Some(_) => "plain lyrics loaded".to_string(),
                    None => "lyrics not available".to_string(),
                };
            }
            Ok(Err(err)) => {
                self.lyrics = None;
                self.status = format!("lyrics failed: {err}");
            }
            Err(err) => {
                self.lyrics = None;
                self.status = format!("lyrics task failed: {err}");
            }
        }
    }

    fn push_history(&mut self, track: Track) -> anyhow::Result<()> {
        self.history.retain(|t| t.video_id != track.video_id);
        self.history.insert(0, track);
        self.history.truncate(100);
        self.storage.save_history(&self.history)
    }

    fn toggle_pause(&mut self) {
        if let Err(err) = self.player.toggle_pause() {
            self.status = format!("pause failed: {err}");
        } else {
            self.status = if self.player.is_paused() {
                "paused"
            } else {
                "playing"
            }
            .to_string();
        }
    }

    fn volume_up(&mut self) {
        if let Err(err) = self.player.volume_up() {
            self.status = format!("volume failed: {err}");
        } else {
            self.status = format!("volume: {}", self.player.volume());
        }
    }

    fn volume_down(&mut self) {
        if let Err(err) = self.player.volume_down() {
            self.status = format!("volume failed: {err}");
        } else {
            self.status = format!("volume: {}", self.player.volume());
        }
    }

    fn current_selection(&self) -> Option<Track> {
        match self.screen {
            Screen::Search => self.results.get(self.selected).cloned(),
            Screen::Queue => self.queue.get(self.queue_selected).cloned(),
            Screen::History => self.history.get(self.selected).cloned(),
            Screen::Favorites => self.favorites.get(self.selected).cloned(),
            Screen::Playlists => self
                .playlists
                .get(self.playlist_selected)
                .and_then(|p| p.tracks.get(self.playlist_track_selected))
                .cloned(),
            _ => None,
        }
    }

    fn current_len(&self) -> usize {
        match self.screen {
            Screen::Search => self.results.len(),
            Screen::Queue => self.queue.len(),
            Screen::History => self.history.len(),
            Screen::Favorites => self.favorites.len(),
            Screen::Playlists => match self.playlist_focus {
                PlaylistFocus::List => self.playlists.len(),
                PlaylistFocus::Tracks => self
                    .playlists
                    .get(self.playlist_selected)
                    .map(|p| p.tracks.len())
                    .unwrap_or(0),
            },
            Screen::Lyrics => self
                .lyrics
                .as_ref()
                .map(|l| l.display_lines().len())
                .unwrap_or(0),
        }
    }

    fn toggle_playlist_focus(&mut self) {
        if self.screen != Screen::Playlists {
            return;
        }
        self.playlist_focus = match self.playlist_focus {
            PlaylistFocus::List => PlaylistFocus::Tracks,
            PlaylistFocus::Tracks => PlaylistFocus::List,
        };
        self.status = match self.playlist_focus {
            PlaylistFocus::List => "playlist focus: list".to_string(),
            PlaylistFocus::Tracks => "playlist focus: tracks".to_string(),
        };
    }

    fn select_next(&mut self) {
        if self.screen == Screen::Lyrics {
            self.lyrics_scroll = self.lyrics_scroll.saturating_add(1);
            return;
        }
        if self.screen == Screen::Queue {
            if !self.queue.is_empty() {
                self.queue_selected = (self.queue_selected + 1).min(self.queue.len() - 1);
            }
            return;
        }
        if self.screen == Screen::Playlists {
            match self.playlist_focus {
                PlaylistFocus::List => {
                    if !self.playlists.is_empty() {
                        self.playlist_selected =
                            (self.playlist_selected + 1).min(self.playlists.len() - 1);
                        self.playlist_track_selected = 0;
                    }
                }
                PlaylistFocus::Tracks => {
                    let len = self
                        .playlists
                        .get(self.playlist_selected)
                        .map(|p| p.tracks.len())
                        .unwrap_or(0);
                    if len > 0 {
                        self.playlist_track_selected =
                            (self.playlist_track_selected + 1).min(len - 1);
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

    fn select_prev(&mut self) {
        if self.screen == Screen::Lyrics {
            self.lyrics_scroll = self.lyrics_scroll.saturating_sub(1);
            return;
        }
        if self.screen == Screen::Queue {
            self.queue_selected = self.queue_selected.saturating_sub(1);
            return;
        }
        if self.screen == Screen::Playlists {
            match self.playlist_focus {
                PlaylistFocus::List => {
                    self.playlist_selected = self.playlist_selected.saturating_sub(1);
                    self.playlist_track_selected = 0;
                }
                PlaylistFocus::Tracks => {
                    self.playlist_track_selected = self.playlist_track_selected.saturating_sub(1)
                }
            }
            return;
        }
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn player_position_label(&self) -> String {
        let pos = self.player.position().unwrap_or(0.0) as u64;
        let dur = self
            .player
            .duration()
            .or_else(|| {
                self.now_playing
                    .as_ref()
                    .and_then(|t| t.duration.map(|d| d as f64))
            })
            .unwrap_or(0.0) as u64;
        format!("{}:{:02}/{}:{:02}", pos / 60, pos % 60, dur / 60, dur % 60)
    }

    pub fn queue_source_label(&self) -> &'static str {
        match self.queue_source {
            QueueSource::Auto => "auto",
            QueueSource::Playlist => "playlist",
        }
    }

    pub fn active_lyrics_index(&self) -> Option<usize> {
        let lyrics = self.lyrics.as_ref()?;
        let position_ms = (self.player.position().unwrap_or(0.0) * 1000.0) as u64;
        lyrics.active_synced_index(position_ms)
    }
}
