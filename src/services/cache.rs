use std::{path::PathBuf, process::Stdio};

use anyhow::Context;
use tokio::process::Command;

use crate::types::track::Track;

const AUDIO_EXTS: &[&str; 5] = &["m4a", "mp3", "opus", "webm", "ogg"];
const DEFAULT_AUDIO_FORMAT: &str = "m4a";

#[derive(Debug, Clone)]
pub struct CacheService {
    dir: PathBuf,
    audio_format: String,
}

impl CacheService {
    pub fn new() -> anyhow::Result<Self> {
        let music_dir = dirs::audio_dir()
            .or_else(dirs::home_dir)
            .context("failed to resolve home or music directory")?;
        let dir = music_dir.join("ytmusic.cli");

        std::fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create cache directory: {}", dir.display()))?;

        Ok(Self {
            dir,
            audio_format: DEFAULT_AUDIO_FORMAT.to_string(),
        })
    }

    pub fn find(&self, video_id: &str) -> Option<PathBuf> {
        AUDIO_EXTS
            .iter()
            .map(|ext| self.dir.join(format!("{video_id}.{ext}")))
            .find(|path| path.exists())
    }

    pub fn enrich(&self, mut track: Track) -> Track {
        track.cached_path = self.find(&track.video_id);
        track
    }

    pub async fn cache_track(&self, track: &Track) -> anyhow::Result<()> {
        let output_template = self.dir.join("%(id)s.%(ext)s");
        Command::new("yt-dlp")
            .arg("-f")
            .arg("bestaudio")
            .arg("-x")
            .arg("--audio-format")
            .arg(&self.audio_format)
            .arg("--audio-quality")
            .arg("0")
            .arg("--embed-metadata")
            .arg("--embed-thumbnail")
            .arg("--convert-thumbnails")
            .arg("jpg")
            .arg("-o")
            .arg(output_template)
            .arg(track.url())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("failed to spawn yt-dlp")?;
        Ok(())
    }
}
