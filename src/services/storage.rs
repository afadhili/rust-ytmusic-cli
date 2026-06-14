use std::{fs, path::PathBuf};

use anyhow::Context;
use serde::{de::DeserializeOwned, Serialize};

use crate::types::{lyrics::LyricsData, playlist::LocalPlaylist, track::Track};

#[derive(Debug, Clone)]
pub struct StorageService {
    data_dir: PathBuf,
}

impl StorageService {
    pub fn new() -> anyhow::Result<Self> {
        let base = dirs::data_local_dir()
            .or_else(dirs::data_dir)
            .or_else(dirs::home_dir)
            .context("failed to resolve data directory")?;
        let data_dir = base.join("ytmusic-cli");
        fs::create_dir_all(&data_dir)
            .with_context(|| format!("failed to create data dir: {}", data_dir.display()))?;
        Ok(Self { data_dir })
    }

    pub fn load_history(&self) -> Vec<Track> {
        self.read_json("history.json").unwrap_or_default()
    }

    pub fn save_history(&self, history: &[Track]) -> anyhow::Result<()> {
        self.write_json("history.json", history)
    }

    pub fn load_favorites(&self) -> Vec<Track> {
        self.read_json("favorites.json").unwrap_or_default()
    }

    pub fn save_favorites(&self, favorites: &[Track]) -> anyhow::Result<()> {
        self.write_json("favorites.json", favorites)
    }

    pub fn load_playlists(&self) -> Vec<LocalPlaylist> {
        self.read_json("playlists.json").unwrap_or_default()
    }

    pub fn save_playlists(&self, playlists: &[LocalPlaylist]) -> anyhow::Result<()> {
        self.write_json("playlists.json", playlists)
    }

    pub fn load_lyrics_cache(&self) -> Vec<LyricsData> {
        self.read_json("lyrics.json").unwrap_or_default()
    }

    pub fn get_cached_lyrics(&self, video_id: &str) -> Option<LyricsData> {
        self.load_lyrics_cache()
            .into_iter()
            .find(|item| item.video_id == video_id)
    }

    pub fn save_lyrics(&self, lyrics: &LyricsData) -> anyhow::Result<()> {
        let mut cache = self.load_lyrics_cache();
        cache.retain(|item| item.video_id != lyrics.video_id);
        cache.insert(0, lyrics.clone());
        cache.truncate(500);
        self.write_json("lyrics.json", &cache)
    }

    fn read_json<T: DeserializeOwned>(&self, name: &str) -> anyhow::Result<T> {
        let path = self.data_dir.join(name);
        let data = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        serde_json::from_str(&data).with_context(|| format!("failed to parse {}", path.display()))
    }

    fn write_json<T: Serialize + ?Sized>(&self, name: &str, value: &T) -> anyhow::Result<()> {
        let path = self.data_dir.join(name);
        let data = serde_json::to_string_pretty(value)?;
        fs::write(&path, data).with_context(|| format!("failed to write {}", path.display()))
    }
}
