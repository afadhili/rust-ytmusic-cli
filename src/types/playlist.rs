use serde::{Deserialize, Serialize};

use super::track::Track;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LocalPlaylist {
    pub name: String,
    pub tracks: Vec<Track>,
}
