use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LyricsData {
    pub video_id: String,
    pub track_name: String,
    pub artist_name: String,
    pub album_name: Option<String>,
    pub duration: Option<f64>,
    pub instrumental: bool,
    pub source: LyricsSource,
    pub plain_lyrics: Option<String>,
    pub synced_lyrics: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LyricsSource {
    LrcLib,
    YoutubeFallback,
    #[default]
    Unknown,
}

impl LyricsSource {
    pub fn as_cache_str(self) -> &'static str {
        match self {
            LyricsSource::LrcLib => "lrclib",
            LyricsSource::YoutubeFallback => "youtube_fallback",
            LyricsSource::Unknown => "unknown",
        }
    }

    pub fn from_cache_str(value: &str) -> Self {
        match value {
            "lrclib" => LyricsSource::LrcLib,
            "youtube_fallback" => LyricsSource::YoutubeFallback,
            _ => LyricsSource::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncedLine {
    pub time_ms: u64,
    pub text: String,
}

impl LyricsData {
    pub fn is_synced(&self) -> bool {
        self.synced_lyrics
            .as_deref()
            .is_some_and(|lyrics| !parse_lrc(lyrics).is_empty())
    }

    pub fn synced_lines(&self) -> Vec<SyncedLine> {
        self.synced_lyrics
            .as_deref()
            .map(parse_lrc)
            .unwrap_or_default()
    }

    pub fn display_lines(&self) -> Vec<String> {
        if self.instrumental {
            return vec!["Instrumental".to_string()];
        }

        let synced = self.synced_lines();
        if !synced.is_empty() {
            return synced.into_iter().map(|line| line.text).collect();
        }

        if let Some(plain) = self
            .plain_lyrics
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            return plain
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(ToOwned::to_owned)
                .collect();
        }

        vec!["Lyrics not available".to_string()]
    }

    pub fn active_synced_index(&self, position_ms: u64) -> Option<usize> {
        let lines = self.synced_lines();
        if lines.is_empty() {
            return None;
        }

        Some(
            lines
                .iter()
                .enumerate()
                .take_while(|(_, line)| line.time_ms <= position_ms)
                .map(|(index, _)| index)
                .last()
                .unwrap_or(0),
        )
    }
}

pub fn parse_lrc(input: &str) -> Vec<SyncedLine> {
    let mut out = Vec::new();

    for raw_line in input.lines() {
        let mut rest = raw_line.trim();
        let mut times = Vec::new();

        while let Some(stripped) = rest.strip_prefix('[') {
            let Some(end) = stripped.find(']') else {
                break;
            };

            let timestamp = &stripped[..end];
            if let Some(time_ms) = parse_lrc_timestamp(timestamp) {
                times.push(time_ms);
            }

            rest = stripped[end + 1..].trim_start();
        }

        if times.is_empty() {
            continue;
        }

        let text = rest.trim().to_string();
        for time_ms in times {
            out.push(SyncedLine {
                time_ms,
                text: text.clone(),
            });
        }
    }

    out.sort_by_key(|line| line.time_ms);
    out
}

fn parse_lrc_timestamp(timestamp: &str) -> Option<u64> {
    let (minutes, rest) = timestamp.split_once(':')?;
    let minutes = minutes.parse::<u64>().ok()?;

    let (seconds, millis) = match rest.split_once('.') {
        Some((seconds, fraction)) => {
            let seconds = seconds.parse::<u64>().ok()?;
            let millis = match fraction.len() {
                0 => 0,
                1 => fraction.parse::<u64>().ok()? * 100,
                2 => fraction.parse::<u64>().ok()? * 10,
                _ => fraction[..3].parse::<u64>().ok()?,
            };
            (seconds, millis)
        }
        None => (rest.parse::<u64>().ok()?, 0),
    };

    Some(minutes * 60_000 + seconds * 1_000 + millis)
}
