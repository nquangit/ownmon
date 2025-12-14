//! Media tracking using Windows Global System Media Transport Controls.
//!
//! This module provides functionality to detect and track currently playing
//! media (music, videos) from any application that integrates with Windows
//! media controls (Spotify, browsers, VLC, etc.).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession, GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus,
};

/// Playback status of the current media.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlaybackStatus {
    Playing,
    Paused,
    Stopped,
    Changing,
    Unknown,
}

impl From<GlobalSystemMediaTransportControlsSessionPlaybackStatus> for PlaybackStatus {
    fn from(status: GlobalSystemMediaTransportControlsSessionPlaybackStatus) -> Self {
        match status {
            GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing => Self::Playing,
            GlobalSystemMediaTransportControlsSessionPlaybackStatus::Paused => Self::Paused,
            GlobalSystemMediaTransportControlsSessionPlaybackStatus::Stopped => Self::Stopped,
            GlobalSystemMediaTransportControlsSessionPlaybackStatus::Changing => Self::Changing,
            _ => Self::Unknown,
        }
    }
}

/// Information about currently playing media.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaInfo {
    /// Title of the media (song name, video title).
    pub title: String,

    /// Artist or channel name.
    pub artist: String,

    /// Album name (if available).
    pub album: String,

    /// Source application ID (e.g., "Spotify.exe").
    pub source_app_id: String,

    /// Current playback status.
    pub playback_status: PlaybackStatus,

    /// When this media info was captured.
    pub timestamp: DateTime<Utc>,
}

impl MediaInfo {
    /// Creates a new MediaInfo with the given details.
    pub fn new(
        title: String,
        artist: String,
        album: String,
        source_app_id: String,
        playback_status: PlaybackStatus,
    ) -> Self {
        Self {
            title,
            artist,
            album,
            source_app_id,
            playback_status,
            timestamp: Utc::now(),
        }
    }

    /// Returns true if this represents actual playing media.
    pub fn is_playing(&self) -> bool {
        self.playback_status == PlaybackStatus::Playing && !self.title.is_empty()
    }
}

/// A tracked media session with timing information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaSession {
    /// The media that was/is playing.
    pub media_info: MediaInfo,

    /// When playback started.
    pub start_time: DateTime<Utc>,

    /// When playback ended (None if still playing).
    pub end_time: Option<DateTime<Utc>>,
}

impl MediaSession {
    /// Creates a new media session starting now.
    pub fn new(media_info: MediaInfo) -> Self {
        Self {
            start_time: Utc::now(),
            end_time: None,
            media_info,
        }
    }

    /// Finalizes the session.
    pub fn finalize(&mut self) {
        self.end_time = Some(Utc::now());
    }

    /// Returns the duration in seconds.
    pub fn duration_secs(&self) -> i64 {
        match self.end_time {
            Some(end) => (end - self.start_time).num_seconds().max(0),
            None => (Utc::now() - self.start_time).num_seconds().max(0),
        }
    }

    /// Returns true if this is the same media (by title and artist).
    pub fn is_same_media(&self, other: &MediaInfo) -> bool {
        self.media_info.title == other.title && self.media_info.artist == other.artist
    }
}

/// Gets the current media session manager.
pub fn get_session_manager(
) -> windows::core::Result<GlobalSystemMediaTransportControlsSessionManager> {
    GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?.get()
}

/// Gets the currently active media session.
pub fn get_current_session(
    manager: &GlobalSystemMediaTransportControlsSessionManager,
) -> Option<GlobalSystemMediaTransportControlsSession> {
    manager.GetCurrentSession().ok()
}

/// Fetches media info from the current session.
pub fn fetch_current_media() -> Option<MediaInfo> {
    let manager = get_session_manager().ok()?;
    let session = get_current_session(&manager)?;

    // Get playback info
    let playback_info = session.GetPlaybackInfo().ok()?;
    let playback_status: PlaybackStatus = playback_info.PlaybackStatus().ok()?.into();

    // Get media properties (this is async)
    let properties = session.TryGetMediaPropertiesAsync().ok()?.get().ok()?;

    let title = properties.Title().ok()?.to_string();
    let artist = properties.Artist().ok()?.to_string();
    let album = properties.AlbumTitle().ok()?.to_string();

    // Get source app ID
    let source_app_id = session.SourceAppUserModelId().ok()?.to_string();

    Some(MediaInfo::new(
        title,
        artist,
        album,
        source_app_id,
        playback_status,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_media_info_is_playing() {
        let playing = MediaInfo::new(
            "Song Title".to_string(),
            "Artist".to_string(),
            "Album".to_string(),
            "Spotify.exe".to_string(),
            PlaybackStatus::Playing,
        );
        assert!(playing.is_playing());

        let paused = MediaInfo::new(
            "Song Title".to_string(),
            "Artist".to_string(),
            "Album".to_string(),
            "Spotify.exe".to_string(),
            PlaybackStatus::Paused,
        );
        assert!(!paused.is_playing());

        let empty = MediaInfo::new(
            "".to_string(),
            "".to_string(),
            "".to_string(),
            "".to_string(),
            PlaybackStatus::Playing,
        );
        assert!(!empty.is_playing());
    }

    #[test]
    fn test_media_session_same_media() {
        let info1 = MediaInfo::new(
            "Song A".to_string(),
            "Artist X".to_string(),
            "Album 1".to_string(),
            "app".to_string(),
            PlaybackStatus::Playing,
        );
        let session = MediaSession::new(info1);

        let info2 = MediaInfo::new(
            "Song A".to_string(),
            "Artist X".to_string(),
            "Album 2".to_string(), // Different album but same song/artist
            "app2".to_string(),
            PlaybackStatus::Paused,
        );
        assert!(session.is_same_media(&info2));

        let info3 = MediaInfo::new(
            "Song B".to_string(),
            "Artist X".to_string(),
            "Album 1".to_string(),
            "app".to_string(),
            PlaybackStatus::Playing,
        );
        assert!(!session.is_same_media(&info3));
    }

    #[test]
    fn test_playback_status_serialization() {
        let status = PlaybackStatus::Playing;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"Playing\"");
    }
}
