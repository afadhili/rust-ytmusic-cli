use std::path::PathBuf;

use rs_ytmusic_api::{SongDetailed, UpNextDetails};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Track {
    pub video_id: String,
    pub title: String,
    pub artist: String,
    pub duration: Option<u64>,
    pub thumbnail_url: Option<String>,
    #[serde(skip)]
    pub cached_path: Option<PathBuf>,
}

impl Track {
    pub fn url(&self) -> String {
        format!("https://music.youtube.com/watch?v={}", self.video_id)
    }

    pub fn label(&self) -> String {
        format!("{} - {}", self.title, self.artist)
    }
}

impl From<SongDetailed> for Track {
    fn from(song: SongDetailed) -> Self {
        let thumbnail_url = song.thumbnails.last().map(|t| t.url.clone());
        Self {
            video_id: song.video_id,
            title: song.name,
            artist: song.artist.name,
            duration: song.duration,
            thumbnail_url,
            cached_path: None,
        }
    }
}

impl From<UpNextDetails> for Track {
    fn from(item: UpNextDetails) -> Self {
        let thumbnail_url = item.thumbnails.last().map(|t| t.url.clone());
        Self {
            video_id: item.video_id,
            title: item.title,
            artist: item.artists.name,
            duration: item.duration,
            thumbnail_url,
            cached_path: None,
        }
    }
}

pub fn format_duration(duration: Option<u64>) -> String {
    let Some(total) = duration else {
        return "--:--".to_string();
    };
    let minutes = total / 60;
    let seconds = total % 60;
    format!("{minutes}:{seconds:02}")
}
