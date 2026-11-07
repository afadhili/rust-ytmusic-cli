use crate::{
    app::{App, QueueSource, Screen},
    types::track::Track,
};

impl App {
    pub(super) async fn search(&mut self) -> anyhow::Result<()> {
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
        self.input_mode = crate::app::InputMode::Normal;
        self.status = format!("{} result(s)", self.results.len());
        Ok(())
    }

    pub(super) async fn play_selected(&mut self) -> anyhow::Result<()> {
        match self.screen {
            Screen::Queue => self.play_queue_at(self.queue_state.queue_selected).await,
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

    pub(super) async fn play_playlist_selection(&mut self) -> anyhow::Result<()> {
        let Some(playlist) = self
            .library_state
            .playlists
            .get(self.library_state.playlist_selected)
            .cloned()
        else {
            self.status = "no playlist selected".to_string();
            return Ok(());
        };

        if playlist.tracks.is_empty() {
            self.status = "playlist is empty".to_string();
            return Ok(());
        }

        let index = self
            .library_state
            .playlist_track_selected
            .min(playlist.tracks.len() - 1);

        let enriched: Vec<Track> = playlist
            .tracks
            .into_iter()
            .map(|track| self.cache.enrich(track))
            .collect();

        self.queue_state.queue_source = QueueSource::Playlist;
        self.queue_state.active_playlist = Some(self.library_state.playlist_selected);
        self.queue_state.replace_with(enriched);

        self.play_queue_at(index).await
    }

    pub(super) async fn play_track_detached(&mut self, track: Track) -> anyhow::Result<()> {
        self.queue_state.queue_source = QueueSource::Auto;
        self.queue_state.active_playlist = None;

        let track = self.cache.enrich(track);
        let index = self.queue_state.insert_after_current(track.clone());
        self.queue_state.queue_index = Some(index);
        self.queue_state.queue_selected = index;

        self.set_now_playing(track).await?;
        self.refill_queue_if_needed().await?;
        Ok(())
    }

    pub(super) async fn play_queue_at(&mut self, index: usize) -> anyhow::Result<()> {
        let Some(track) = self.queue_state.queue.get(index).cloned() else {
            self.status = "queue index is empty".to_string();
            return Ok(());
        };

        self.queue_state.queue_index = Some(index);
        self.queue_state.queue_selected = index;
        self.set_now_playing(track).await?;

        if self.queue_state.queue_source == QueueSource::Auto {
            self.refill_queue_if_needed().await?;
        }

        Ok(())
    }

    pub(super) async fn set_now_playing(&mut self, track: Track) -> anyhow::Result<()> {
        self.queue_state.push_previous_if_different(&track);

        let track = self.cache.enrich(track);
        self.queue_state.remember_track(&track);
        self.player.play(&track).await?;
        if track.cached_path.is_none() {
            let _ = self.cache.cache_track(&track).await;
        }
        self.push_history(track.clone())?;
        self.queue_state.now_playing = Some(track.clone());
        self.lyrics_state.reset_for_new_track();
        self.status = if track.cached_path.is_some() {
            "playing cached track".to_string()
        } else {
            "playing remote, caching in background".to_string()
        };
        Ok(())
    }

    pub(super) async fn play_next(&mut self) -> anyhow::Result<()> {
        if self.queue_state.queue_source == QueueSource::Auto {
            self.refill_queue_if_needed().await?;
        }

        let next_index = match self.queue_state.queue_index {
            Some(index) => index + 1,
            None => 0,
        };

        if next_index >= self.queue_state.queue.len()
            && self.queue_state.queue_source == QueueSource::Auto
        {
            self.refill_queue_if_needed().await?;
        }

        if next_index >= self.queue_state.queue.len() {
            self.status = "no next track".to_string();
            return Ok(());
        }

        self.play_queue_at(next_index).await
    }

    pub(super) async fn poll_player_end(&mut self) -> anyhow::Result<()> {
        if self.player.has_exited() {
            self.handle_track_finished().await?;
        }

        Ok(())
    }

    pub(super) async fn handle_track_finished(&mut self) -> anyhow::Result<()> {
        if self.queue_state.now_playing.is_none() {
            return Ok(());
        }

        match self.queue_state.queue_source {
            QueueSource::Playlist => {
                let next_index = self
                    .queue_state
                    .queue_index
                    .map(|index| index + 1)
                    .unwrap_or(0);

                if next_index < self.queue_state.queue.len() {
                    self.status = "autoplay next playlist track".to_string();
                    self.play_queue_at(next_index).await?;
                } else {
                    self.status = "playlist finished".to_string();
                }
            }
            QueueSource::Auto => {
                self.refill_queue_if_needed().await?;

                let next_index = self
                    .queue_state
                    .queue_index
                    .map(|index| index + 1)
                    .unwrap_or(0);

                if next_index < self.queue_state.queue.len() {
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

    pub(super) async fn play_previous(&mut self) -> anyhow::Result<()> {
        if let Some(index) = self.queue_state.queue_index {
            if index > 0 && index - 1 < self.queue_state.queue.len() {
                return self.play_queue_at(index - 1).await;
            }
        }

        let Some(track) = self.queue_state.previous.pop() else {
            self.status = "no previous track".to_string();
            return Ok(());
        };
        self.set_now_playing(track).await
    }

    pub(super) async fn refill_queue_if_needed(&mut self) -> anyhow::Result<()> {
        if self.queue_state.queue_source == QueueSource::Playlist {
            return Ok(());
        }

        if !self.queue_state.should_refill_queue() {
            return Ok(());
        }

        let Some(current) = self.queue_state.now_playing.as_ref() else {
            return Ok(());
        };

        let seed_video_id = current.video_id.clone();
        let mut added = 0usize;

        while self.queue_state.should_refill_queue() {
            let before = self.queue_state.remaining_after_current();
            let batch_added = self.refill_queue_from(&seed_video_id).await?;
            added += batch_added;

            if batch_added == 0 || self.queue_state.remaining_after_current() == before {
                break;
            }
        }

        if added > 0 {
            self.status = format!(
                "auto queued {added} track(s); {} upcoming track(s)",
                self.queue_state.remaining_after_current()
            );
        }

        Ok(())
    }

    pub(super) async fn refill_queue_from(&mut self, video_id: &str) -> anyhow::Result<usize> {
        let up_next = self.music.watch_queue(video_id).await?;
        let mut added = 0;

        for track in up_next {
            if self.queue_state.remaining_after_current()
                >= crate::app::MIN_QUEUE_LEN
            {
                break;
            }

            let track = self.cache.enrich(track);

            if track.video_id.is_empty() {
                continue;
            }

            if self.queue_state.has_track_anywhere(&track.video_id) {
                continue;
            }

            self.queue_state.enqueue(track);
            added += 1;
        }

        Ok(added)
    }

    pub(super) fn enqueue_selected(&mut self) {
        if let Some(track) = self.current_selection() {
            if self
                .queue_state
                .queue
                .iter()
                .any(|t| t.video_id == track.video_id)
            {
                self.status = "already in queue".to_string();
                return;
            }
            self.queue_state.queue_source = QueueSource::Auto;
            self.queue_state.active_playlist = None;
            self.queue_state.enqueue(track.clone());
            self.status = format!("queued: {}", track.title);
            self.screen = Screen::Queue;
        } else {
            self.status = "nothing selected".to_string();
        }
    }

    pub(super) fn remove_selected(&mut self) {
        match self.screen {
            Screen::Queue => {
                if self.queue_state.remove_at_selected() {
                    self.status = "removed from queue".to_string();
                }
            }
            Screen::Favorites => self.remove_favorite_at_selected(),
            Screen::Playlists => self.remove_playlist_track_at_selected(),
            _ => {}
        }
    }

    pub(super) fn move_queue_up(&mut self) {
        if self.screen != Screen::Queue {
            return;
        }
        self.queue_state.move_selected_up();
    }

    pub(super) fn move_queue_down(&mut self) {
        if self.screen != Screen::Queue {
            return;
        }
        self.queue_state.move_selected_down();
    }

    pub(super) fn toggle_pause(&mut self) {
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

    pub(super) fn volume_up(&mut self) {
        if let Err(err) = self.player.volume_up() {
            self.status = format!("volume failed: {err}");
        } else {
            self.status = format!("volume: {}", self.player.volume());
        }
    }

    pub(super) fn volume_down(&mut self) {
        if let Err(err) = self.player.volume_down() {
            self.status = format!("volume failed: {err}");
        } else {
            self.status = format!("volume: {}", self.player.volume());
        }
    }
}
