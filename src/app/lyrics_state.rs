use tokio::task::JoinHandle;

use crate::types::lyrics::LyricsData;

/// State for the lyrics view: the loaded lyrics, the manual scroll offset,
/// the in-flight loading task, and which track is currently being loaded.
///
/// The previous design kept both `lyrics_loading: bool` and
/// `lyrics_loading_track_id: Option<String>` in lockstep on every write.
/// They've been merged into a single `loading_for: Option<String>`;
/// `is_loading()` exposes the predicate that the renderer used to read.
pub struct LyricsState {
    pub lyrics: Option<LyricsData>,
    pub lyrics_scroll: usize,
    loading_for: Option<String>,
    task: Option<JoinHandle<anyhow::Result<Option<LyricsData>>>>,
}

impl LyricsState {
    pub fn new() -> Self {
        Self {
            lyrics: None,
            lyrics_scroll: 0,
            loading_for: None,
            task: None,
        }
    }

    pub fn is_loading(&self) -> bool {
        self.loading_for.is_some()
    }

    /// True iff a fetch for `video_id` is already in flight.
    pub fn is_loading_for(&self, video_id: &str) -> bool {
        self.loading_for.as_deref() == Some(video_id)
    }

    /// Clear any loaded lyrics and mark `video_id` as currently loading.
    /// Does not abort a previously running task; the caller does that.
    pub fn begin_loading(&mut self, video_id: String) {
        self.lyrics = None;
        self.loading_for = Some(video_id);
    }

    /// Forget that we were loading anything. Caller should have already
    /// taken/aborted the task.
    pub fn clear_loading(&mut self) {
        self.loading_for = None;
    }

    /// Replace the in-flight task (e.g. when starting a new fetch).
    pub fn set_task(&mut self, handle: JoinHandle<anyhow::Result<Option<LyricsData>>>) {
        self.task = Some(handle);
    }

    /// Take the task handle out so the caller can `abort` it.
    pub fn take_task(&mut self) -> Option<JoinHandle<anyhow::Result<Option<LyricsData>>>> {
        self.task.take()
    }

    /// Look at the in-flight task without removing it.
    pub fn peek_task(&self) -> Option<&JoinHandle<anyhow::Result<Option<LyricsData>>>> {
        self.task.as_ref()
    }

    /// Reset to a freshly-constructed state (used by `set_now_playing` when
    /// the track changes — abort the old task and forget the loading
    /// marker).
    pub fn reset_for_new_track(&mut self) {
        self.lyrics = None;
        if let Some(task) = self.task.take() {
            task.abort();
        }
        self.loading_for = None;
    }
}
