use std::io;
use std::time::Duration;

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
    types::track::Track,
};

pub mod input;
pub mod library;
pub mod library_state;
pub mod lyrics;
pub mod lyrics_state;
pub mod playback;
pub mod queue_state;

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
    pub library_state: library_state::LibraryState,
    pub queue_state: queue_state::QueueState,
    pub lyrics_state: lyrics_state::LyricsState,
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
            library_state: library_state::LibraryState::new(history, favorites, playlists),
            queue_state: queue_state::QueueState::new(),
            lyrics_state: lyrics_state::LyricsState::new(),
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
}
