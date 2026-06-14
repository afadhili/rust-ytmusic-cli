use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    process::Stdio,
};

use anyhow::Context;
use serde_json::json;
use tokio::{
    process::{Child, Command},
    time::{sleep, Duration},
};

use crate::types::track::Track;

const DEFAULT_VOLUME: u8 = 70;

#[derive(Debug)]
pub struct PlayerService {
    child: Option<Child>,
    socket_path: PathBuf,
    paused: bool,
    volume: u8,
}

impl PlayerService {
    pub fn new() -> Self {
        let socket_path =
            std::env::temp_dir().join(format!("ytmusic-cli-{}.sock", std::process::id()));
        Self {
            child: None,
            socket_path,
            paused: false,
            volume: DEFAULT_VOLUME,
        }
    }

    pub async fn play(&mut self, track: &Track) -> anyhow::Result<()> {
        self.stop().await?;
        let input = track
            .cached_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_else(|| track.url());

        let _ = fs::remove_file(&self.socket_path);
        let child = Command::new("mpv")
            .arg("--no-video")
            .arg("--force-window=no")
            .arg("--terminal=no")
            .arg(format!("--volume={}", self.volume))
            .arg(format!(
                "--input-ipc-server={}",
                self.socket_path.to_string_lossy()
            ))
            .arg(input)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("failed to spawn mpv")?;

        self.child = Some(child);
        self.paused = false;
        sleep(Duration::from_millis(90)).await;
        Ok(())
    }

    pub async fn stop(&mut self) -> anyhow::Result<()> {
        if let Some(mut child) = self.child.take() {
            let _ = self.command(json!({ "command": ["quit"] }));
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        let _ = fs::remove_file(&self.socket_path);
        self.paused = false;
        Ok(())
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }
    pub fn volume(&self) -> u8 {
        self.volume
    }

    pub fn has_exited(&mut self) -> bool {
        let Some(child) = self.child.as_mut() else {
            return false;
        };

        match child.try_wait() {
            Ok(Some(_status)) => {
                self.child = None;
                self.paused = false;
                let _ = fs::remove_file(&self.socket_path);
                true
            }
            Ok(None) => false,
            Err(_) => {
                self.child = None;
                self.paused = false;
                let _ = fs::remove_file(&self.socket_path);
                true
            }
        }
    }

    pub fn toggle_pause(&mut self) -> anyhow::Result<()> {
        self.paused = !self.paused;
        self.command(json!({ "command": ["set_property", "pause", self.paused] }))
    }

    pub fn seek(&self, seconds: i64) -> anyhow::Result<()> {
        self.command(json!({ "command": ["seek", seconds, "relative"] }))
    }

    pub fn set_volume(&mut self, volume: u8) -> anyhow::Result<()> {
        self.volume = volume.min(100);
        self.command(json!({ "command": ["set_property", "volume", self.volume] }))
            .or(Ok(()))
    }

    pub fn volume_up(&mut self) -> anyhow::Result<()> {
        self.set_volume(self.volume.saturating_add(5).min(100))
    }

    pub fn volume_down(&mut self) -> anyhow::Result<()> {
        self.set_volume(self.volume.saturating_sub(5))
    }

    pub fn position(&self) -> Option<f64> {
        self.get_property("time-pos").and_then(|v| v.as_f64())
    }

    pub fn duration(&self) -> Option<f64> {
        self.get_property("duration").and_then(|v| v.as_f64())
    }

    fn get_property(&self, property: &str) -> Option<serde_json::Value> {
        let mut stream = UnixStream::connect(&self.socket_path).ok()?;
        let payload = json!({ "command": ["get_property", property] });
        let mut bytes = serde_json::to_vec(&payload).ok()?;
        bytes.push(b'\n');
        stream.write_all(&bytes).ok()?;
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        let value: serde_json::Value = serde_json::from_str(&line).ok()?;
        value.get("data").cloned()
    }

    fn command(&self, command: serde_json::Value) -> anyhow::Result<()> {
        let mut stream = UnixStream::connect(&self.socket_path).with_context(|| {
            format!("failed to connect mpv IPC: {}", self.socket_path.display())
        })?;
        let mut payload = serde_json::to_vec(&command)?;
        payload.push(b'\n');
        stream.write_all(&payload)?;
        Ok(())
    }
}

impl Drop for PlayerService {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
        }
        let _ = fs::remove_file(&self.socket_path);
    }
}
