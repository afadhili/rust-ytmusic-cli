use crate::{
    app::{App, QueueSource, Screen},
    services::music::MusicService,
};

impl App {
    pub(super) fn open_lyrics(&mut self) {
        self.screen = Screen::Lyrics;
        self.lyrics_state.lyrics_scroll = 0;

        let Some(track) = self.queue_state.now_playing.clone() else {
            self.status = "no track playing".to_string();
            return;
        };

        if self
            .lyrics_state
            .lyrics
            .as_ref()
            .is_some_and(|lyrics| lyrics.video_id == track.video_id)
        {
            self.status = "lyrics already loaded".to_string();
            return;
        }

        if self.lyrics_state.is_loading_for(&track.video_id) {
            self.status = "lyrics still loading".to_string();
            return;
        }

        let lyrics_service = self.lyrics_service.clone();
        let storage = self.storage.clone();
        let video_id = track.video_id.clone();

        self.lyrics_state.begin_loading(video_id.clone());
        self.status = format!("loading lyrics: {}", track.label());

        let track_for_task = track.clone();
        let handle = tokio::spawn(async move {
            let music = MusicService::new().await?;
            lyrics_service
                .get_lyrics(&track_for_task, &music, &storage)
                .await
        });
        self.lyrics_state.set_task(handle);
    }

    pub(super) async fn poll_lyrics_task(&mut self) {
        let Some(task) = self.lyrics_state.peek_task() else {
            return;
        };

        if !task.is_finished() {
            return;
        }

        let Some(task) = self.lyrics_state.take_task() else {
            return;
        };

        self.lyrics_state.clear_loading();

        match task.await {
            Ok(Ok(lyrics)) => {
                self.lyrics_state.lyrics = lyrics;
                self.status = match &self.lyrics_state.lyrics {
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
                self.lyrics_state.lyrics = None;
                self.status = format!("lyrics failed: {err}");
            }
            Err(err) => {
                self.lyrics_state.lyrics = None;
                self.status = format!("lyrics task failed: {err}");
            }
        }
    }

    pub fn player_position_label(&self) -> String {
        let pos = self.player.position().unwrap_or(0.0) as u64;
        let dur = self
            .player
            .duration()
            .or_else(|| {
                self.queue_state
                    .now_playing
                    .as_ref()
                    .and_then(|t| t.duration.map(|d| d as f64))
            })
            .unwrap_or(0.0) as u64;
        format!("{}:{:02}/{}:{:02}", pos / 60, pos % 60, dur / 60, dur % 60)
    }

    pub fn queue_source_label(&self) -> &'static str {
        match self.queue_state.queue_source {
            QueueSource::Auto => "auto",
            QueueSource::Playlist => "playlist",
        }
    }

    pub fn active_lyrics_index(&self) -> Option<usize> {
        let lyrics = self.lyrics_state.lyrics.as_ref()?;
        let position_ms = (self.player.position().unwrap_or(0.0) * 1000.0) as u64;
        lyrics.active_synced_index(position_ms)
    }
}
