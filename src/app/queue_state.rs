use std::collections::{HashSet, VecDeque};

use crate::{
    app::{QueueSource, MIN_QUEUE_LEN},
    types::track::Track,
};

/// State for the queue and the currently playing / previous tracks.
///
/// Pure state transitions live here; anything that needs a service (fetching
/// watch-queue, persisting history, driving the player) stays on `App` and
/// calls into these primitives. This keeps `queue_index` fixups in one place.
pub struct QueueState {
    pub queue: VecDeque<Track>,
    pub queue_selected: usize,
    pub queue_index: Option<usize>,
    pub queue_source: QueueSource,
    pub queued_video_ids: HashSet<String>,
    pub active_playlist: Option<usize>,
    pub now_playing: Option<Track>,
    pub previous: Vec<Track>,
}

impl QueueState {
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            queue_selected: 0,
            queue_index: None,
            queue_source: QueueSource::Auto,
            queued_video_ids: HashSet::new(),
            active_playlist: None,
            now_playing: None,
            previous: Vec::new(),
        }
    }

    /// Number of tracks remaining *after* the currently playing index. If
    /// nothing is playing, the whole queue is "upcoming".
    pub fn remaining_after_current(&self) -> usize {
        match self.queue_index {
            Some(index) => self.queue.len().saturating_sub(index + 1),
            None => self.queue.len(),
        }
    }

    pub fn should_refill_queue(&self) -> bool {
        self.queue_source == QueueSource::Auto && self.remaining_after_current() < MIN_QUEUE_LEN
    }

    /// Remember a track by `video_id` so future `has_track_anywhere` checks
    /// reject duplicates.
    pub fn remember_track(&mut self, track: &Track) {
        if !track.video_id.is_empty() {
            self.queued_video_ids.insert(track.video_id.clone());
        }
    }

    pub fn has_track_anywhere(&self, video_id: &str) -> bool {
        if video_id.is_empty() {
            return true;
        }

        self.queued_video_ids.contains(video_id)
            || self
                .now_playing
                .as_ref()
                .is_some_and(|t| t.video_id == video_id)
            || self.queue.iter().any(|t| t.video_id == video_id)
            || self.previous.iter().any(|t| t.video_id == video_id)
    }

    /// Push the previous `now_playing` onto the `previous` stack when it
    /// differs from the new track.
    pub fn push_previous_if_different(&mut self, new_track: &Track) {
        if let Some(current) = self.now_playing.take() {
            if current.video_id != new_track.video_id {
                self.previous.push(current);
            }
        }
    }

    /// Replace the queue wholesale (used by `play_playlist_selection`).
    /// Resets `queue_index`/`queue_selected` to 0 and rebuilds
    /// `queued_video_ids` from the new contents.
    pub fn replace_with(&mut self, tracks: impl IntoIterator<Item = Track>) {
        self.queue = tracks.into_iter().collect();
        self.queued_video_ids.clear();
        for track in self.queue.iter() {
            if !track.video_id.is_empty() {
                self.queued_video_ids.insert(track.video_id.clone());
            }
        }
        self.queue_index = None;
        self.queue_selected = 0;
    }

    /// Remove the track at `queue_selected` and fix up `queue_index` so it
    /// keeps pointing at the same logical track if possible. Returns
    /// `true` when something was removed.
    ///
    /// Preserves the pre-refactor behavior exactly: `queued_video_ids` is
    /// intentionally *not* cleaned on removal.
    pub fn remove_at_selected(&mut self) -> bool {
        if self.queue_selected >= self.queue.len() {
            return false;
        }
        self.queue.remove(self.queue_selected);
        if let Some(index) = self.queue_index {
            self.queue_index = if self.queue.is_empty() {
                None
            } else if self.queue_selected < index {
                Some(index.saturating_sub(1))
            } else if self.queue_selected == index {
                Some(index.min(self.queue.len() - 1))
            } else {
                Some(index)
            };
        }
        self.queue_selected = self.queue_selected.min(self.queue.len().saturating_sub(1));
        true
    }

    /// Swap `queue_selected` with its neighbor one step up. Preserves the
    /// pre-refactor behavior: `queue_index` is adjusted only when it sits
    /// on either of the two swapped positions.
    pub fn move_selected_up(&mut self) -> bool {
        if self.queue_selected == 0 || self.queue.len() < 2 {
            return false;
        }
        self.queue
            .swap(self.queue_selected, self.queue_selected - 1);
        if let Some(index) = self.queue_index {
            if index == self.queue_selected {
                self.queue_index = Some(index - 1);
            } else if index + 1 == self.queue_selected {
                self.queue_index = Some(index + 1);
            }
        }
        self.queue_selected -= 1;
        true
    }

    /// Swap `queue_selected` with its neighbor one step down. Mirror of
    /// `move_selected_up`.
    pub fn move_selected_down(&mut self) -> bool {
        if self.queue_selected + 1 >= self.queue.len() {
            return false;
        }
        self.queue
            .swap(self.queue_selected, self.queue_selected + 1);
        if let Some(index) = self.queue_index {
            if index == self.queue_selected {
                self.queue_index = Some(index + 1);
            } else if index == self.queue_selected + 1 {
                self.queue_index = Some(index - 1);
            }
        }
        self.queue_selected += 1;
        true
    }

    /// Append `track` to the back of the queue (used by `enqueue_selected`
    /// and `refill_queue_from`). Caller is responsible for the duplicate
    /// check; this method just pushes and remembers.
    pub fn enqueue(&mut self, track: Track) {
        self.remember_track(&track);
        self.queue.push_back(track);
    }

    /// Insert `track` immediately after `queue_index` (or at the tail when
    /// nothing is playing). Returns the index of the inserted track.
    /// Used by `play_track_detached` so the user can hit `n` to keep going.
    pub fn insert_after_current(&mut self, track: Track) -> usize {
        let insert_index = self
            .queue_index
            .map(|index| index + 1)
            .unwrap_or(self.queue.len());
        let insert_index = insert_index.min(self.queue.len());
        self.remember_track(&track);
        self.queue.insert(insert_index, track);
        insert_index
    }
}
