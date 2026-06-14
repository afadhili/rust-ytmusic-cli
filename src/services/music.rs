use anyhow::Context;
use rs_ytmusic_api::MusicClient;

use crate::types::track::Track;

pub struct MusicService {
    client: MusicClient,
}

impl MusicService {
    pub async fn new() -> anyhow::Result<Self> {
        let client = MusicClient::new()
            .init()
            .await
            .context("failed to initialize rust-ytmusic-api client")?;

        Ok(Self { client })
    }

    pub async fn search_songs(&self, query: &str) -> anyhow::Result<Vec<Track>> {
        let songs = self
            .client
            .find_songs(query)
            .await
            .with_context(|| format!("failed to search songs for query: {query}"))?;

        Ok(songs.into_iter().map(Track::from).collect())
    }

    pub async fn watch_queue(&self, video_id: &str) -> anyhow::Result<Vec<Track>> {
        let items = self
            .client
            .fetch_watch_queue(video_id)
            .await
            .with_context(|| format!("failed to fetch watch queue for video id: {video_id}"))?;

        Ok(items.into_iter().map(Track::from).collect())
    }

    pub async fn youtube_lyrics(&self, video_id: &str) -> anyhow::Result<Option<Vec<String>>> {
        self.client
            .fetch_lyrics(video_id)
            .await
            .with_context(|| format!("failed to fetch YouTube lyrics for video id: {video_id}"))
    }
}
