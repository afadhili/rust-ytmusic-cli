use std::{fs, path::PathBuf};

use anyhow::Context;
use reqwest::StatusCode;
use serde::Deserialize;

use crate::{
    services::{music::MusicService, storage::StorageService},
    types::{
        lyrics::{LyricsData, LyricsSource},
        track::Track,
    },
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LrcLibLyrics {
    id: u64,
    track_name: String,
    artist_name: String,
    album_name: Option<String>,
    duration: Option<f64>,
    instrumental: bool,
    plain_lyrics: Option<String>,
    synced_lyrics: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LrcLibSearchItem {
    id: u64,
    track_name: String,
    artist_name: String,
    album_name: Option<String>,
    duration: Option<f64>,
    instrumental: bool,
    plain_lyrics: Option<String>,
    synced_lyrics: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LyricsService {
    client: reqwest::Client,
    cache_dir: PathBuf,
}

impl LyricsService {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent("ytmusic-cli/0.4 (+https://github.com/afadhili/ytmusic-cli)")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let cache_dir = dirs::audio_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ytmusic.cli");

        let _ = fs::create_dir_all(&cache_dir);

        Self { client, cache_dir }
    }

    pub fn cache_path(&self, video_id: &str) -> PathBuf {
        self.cache_dir.join(format!("{video_id}-lyrics.txt"))
    }

    pub async fn get_lyrics(
        &self,
        track: &Track,
        music: &MusicService,
        storage: &StorageService,
    ) -> anyhow::Result<Option<LyricsData>> {
        if let Some(cached) = self.read_cached_lyrics(&track.video_id)? {
            return Ok(Some(cached));
        }

        // Backward compatibility with the old JSON cache location. If found, migrate it
        // to ~/Music/ytmusic.cli/{id}-lyrics.txt.
        if let Some(cached) = storage.get_cached_lyrics(&track.video_id) {
            self.write_cached_lyrics(&cached)?;
            return Ok(Some(cached));
        }

        if let Some(data) = self.fetch_lrclib_exact(track).await? {
            self.write_cached_lyrics(&data)?;
            let _ = storage.save_lyrics(&data);
            return Ok(Some(data));
        }

        if let Some(data) = self.fetch_lrclib_search(track).await? {
            self.write_cached_lyrics(&data)?;
            let _ = storage.save_lyrics(&data);
            return Ok(Some(data));
        }

        let Some(lines) = music.youtube_lyrics(&track.video_id).await? else {
            return Ok(None);
        };

        let data = LyricsData {
            video_id: track.video_id.clone(),
            track_name: track.title.clone(),
            artist_name: track.artist.clone(),
            album_name: None,
            duration: track.duration.map(|duration| duration as f64),
            instrumental: false,
            source: LyricsSource::YoutubeFallback,
            plain_lyrics: Some(lines.join("\n")),
            synced_lyrics: None,
        };

        self.write_cached_lyrics(&data)?;
        let _ = storage.save_lyrics(&data);
        Ok(Some(data))
    }

    pub fn read_cached_lyrics(&self, video_id: &str) -> anyhow::Result<Option<LyricsData>> {
        let path = self.cache_path(video_id);
        if !path.exists() {
            return Ok(None);
        }

        let data = fs::read_to_string(&path)
            .with_context(|| format!("failed to read lyrics cache: {}", path.display()))?;

        Ok(parse_cached_lyrics_file(video_id, &data))
    }

    pub fn write_cached_lyrics(&self, lyrics: &LyricsData) -> anyhow::Result<()> {
        fs::create_dir_all(&self.cache_dir).with_context(|| {
            format!(
                "failed to create lyrics cache dir: {}",
                self.cache_dir.display()
            )
        })?;

        let path = self.cache_path(&lyrics.video_id);
        let data = format_cached_lyrics_file(lyrics);
        fs::write(&path, data)
            .with_context(|| format!("failed to write lyrics cache: {}", path.display()))
    }

    async fn fetch_lrclib_exact(&self, track: &Track) -> anyhow::Result<Option<LyricsData>> {
        let duration_string = track.duration.map(|duration| duration.to_string());

        let mut params = vec![
            ("track_name", track.title.as_str()),
            ("artist_name", track.artist.as_str()),
        ];

        if let Some(duration) = duration_string.as_deref() {
            params.push(("duration", duration));
        }

        let res = self
            .client
            .get("https://lrclib.net/api/get")
            .query(&params)
            .send()
            .await
            .context("failed to request LRCLIB /api/get")?;

        if res.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !res.status().is_success() {
            return Ok(None);
        }

        let fetched = res
            .json::<LrcLibLyrics>()
            .await
            .context("failed to parse LRCLIB /api/get response")?;

        Ok(Some(Self::from_lrclib_exact(track, fetched)))
    }

    async fn fetch_lrclib_search(&self, track: &Track) -> anyhow::Result<Option<LyricsData>> {
        let query = format!("{} {}", track.artist, track.title);
        let res = self
            .client
            .get("https://lrclib.net/api/search")
            .query(&[("q", query.as_str())])
            .send()
            .await
            .context("failed to request LRCLIB /api/search")?;

        if !res.status().is_success() {
            return Ok(None);
        }

        let items = res
            .json::<Vec<LrcLibSearchItem>>()
            .await
            .context("failed to parse LRCLIB /api/search response")?;

        let Some(best) = Self::best_search_match(track, items) else {
            return Ok(None);
        };

        Ok(Some(Self::from_lrclib_search(track, best)))
    }

    fn best_search_match(track: &Track, items: Vec<LrcLibSearchItem>) -> Option<LrcLibSearchItem> {
        items.into_iter().max_by_key(|item| {
            let mut score = 0i32;

            if normalize(&item.track_name) == normalize(&track.title) {
                score += 40;
            }
            if normalize(&item.artist_name).contains(&normalize(&track.artist))
                || normalize(&track.artist).contains(&normalize(&item.artist_name))
            {
                score += 25;
            }
            if item
                .synced_lyrics
                .as_deref()
                .is_some_and(|lyrics| !lyrics.trim().is_empty())
            {
                score += 20;
            }
            if item
                .plain_lyrics
                .as_deref()
                .is_some_and(|lyrics| !lyrics.trim().is_empty())
            {
                score += 8;
            }
            if let (Some(expected), Some(actual)) = (track.duration, item.duration) {
                let diff = (expected as f64 - actual).abs();
                if diff <= 2.0 {
                    score += 12;
                } else if diff <= 5.0 {
                    score += 6;
                } else if diff > 15.0 {
                    score -= 20;
                }
            }

            score
        })
    }

    fn from_lrclib_exact(track: &Track, fetched: LrcLibLyrics) -> LyricsData {
        let _ = fetched.id;
        LyricsData {
            video_id: track.video_id.clone(),
            track_name: fetched.track_name,
            artist_name: fetched.artist_name,
            album_name: fetched.album_name,
            duration: fetched.duration,
            instrumental: fetched.instrumental,
            source: LyricsSource::LrcLib,
            plain_lyrics: fetched.plain_lyrics,
            synced_lyrics: fetched.synced_lyrics,
        }
    }

    fn from_lrclib_search(track: &Track, fetched: LrcLibSearchItem) -> LyricsData {
        let _ = fetched.id;
        LyricsData {
            video_id: track.video_id.clone(),
            track_name: fetched.track_name,
            artist_name: fetched.artist_name,
            album_name: fetched.album_name,
            duration: fetched.duration,
            instrumental: fetched.instrumental,
            source: LyricsSource::LrcLib,
            plain_lyrics: fetched.plain_lyrics,
            synced_lyrics: fetched.synced_lyrics,
        }
    }
}

fn format_cached_lyrics_file(lyrics: &LyricsData) -> String {
    let kind = if lyrics.instrumental {
        "instrumental"
    } else if lyrics
        .synced_lyrics
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty())
    {
        "synced"
    } else {
        "plain"
    };

    let mut out = String::new();
    out.push_str("# ytmusic-cli lyrics cache v1\n");
    out.push_str(&format!("# video_id: {}\n", lyrics.video_id));
    out.push_str(&format!(
        "# track_name: {}\n",
        lyrics.track_name.replace('\n', " ")
    ));
    out.push_str(&format!(
        "# artist_name: {}\n",
        lyrics.artist_name.replace('\n', " ")
    ));
    if let Some(album) = lyrics.album_name.as_deref() {
        out.push_str(&format!("# album_name: {}\n", album.replace('\n', " ")));
    }
    if let Some(duration) = lyrics.duration {
        out.push_str(&format!("# duration: {}\n", duration));
    }
    out.push_str(&format!("# source: {}\n", lyrics.source.as_cache_str()));
    out.push_str(&format!("# kind: {kind}\n"));
    out.push('\n');

    if lyrics.instrumental {
        out.push_str("Instrumental\n");
    } else if let Some(synced) = lyrics
        .synced_lyrics
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        out.push_str(synced.trim());
        out.push('\n');
    } else if let Some(plain) = lyrics
        .plain_lyrics
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        out.push_str(plain.trim());
        out.push('\n');
    }

    out
}

fn parse_cached_lyrics_file(video_id: &str, data: &str) -> Option<LyricsData> {
    let mut track_name = String::new();
    let mut artist_name = String::new();
    let mut album_name = None;
    let mut duration = None;
    let mut source = LyricsSource::Unknown;
    let mut kind = "plain".to_string();
    let mut body = Vec::new();
    let mut in_body = false;

    for line in data.lines() {
        if !in_body && line.trim().is_empty() {
            in_body = true;
            continue;
        }

        if !in_body && line.starts_with('#') {
            let Some((key, value)) = line.trim_start_matches('#').trim().split_once(':') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "track_name" => track_name = value.to_string(),
                "artist_name" => artist_name = value.to_string(),
                "album_name" => album_name = Some(value.to_string()),
                "duration" => duration = value.parse::<f64>().ok(),
                "source" => source = LyricsSource::from_cache_str(value),
                "kind" => kind = value.to_string(),
                _ => {}
            }
            continue;
        }

        body.push(line);
    }

    let text = body.join("\n").trim().to_string();
    if text.is_empty() && kind != "instrumental" {
        return None;
    }

    Some(LyricsData {
        video_id: video_id.to_string(),
        track_name,
        artist_name,
        album_name,
        duration,
        instrumental: kind == "instrumental",
        source,
        plain_lyrics: if kind == "plain" {
            Some(text.clone())
        } else {
            None
        },
        synced_lyrics: if kind == "synced" { Some(text) } else { None },
    })
}

fn normalize(input: &str) -> String {
    input
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
