//! A service with nothing behind it, for tests and examples: it signs in
//! to no account, every request fails, and nothing touches the network.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Result};
use async_trait::async_trait;
use music::{
    Album, AlbumDetail, Artist, ArtistProfile, Capabilities, InputSource, MediaKind, MusicApi,
    MusicProvider, PlaybackConfig, PlaybackEvent, PlaybackEvents, PlaybackFactory, Player,
    Playlist, PlaylistDetail, PromptSink, ProviderSession, SavedArtist, Shape, SignIn, Track,
    UserProfile,
};

pub(super) struct Sample;

/// What every request answers with.
macro_rules! nothing {
    () => {
        bail!("a sample session has no service behind it")
    };
}

impl Sample {
    pub(super) fn session() -> ProviderSession {
        ProviderSession {
            profile: UserProfile::default(),
            api: Arc::new(Sample),
            playback: Arc::new(Sample),
            shape: Shape::Saved,
            authenticated: true,
            capabilities: Capabilities::ALL,
        }
    }
}

#[async_trait]
impl MusicProvider for Sample {
    fn name(&self) -> &'static str {
        "Sample"
    }
    fn slug(&self) -> &'static str {
        "sample"
    }
    fn sign_in_options(&self) -> Vec<SignIn> {
        vec![SignIn::Default]
    }
    fn stored(&self) -> bool {
        false
    }
    async fn restore(&self) -> Result<Option<ProviderSession>> {
        Ok(None)
    }
    async fn sign_in(&self, _: SignIn, _: PromptSink, _: InputSource) -> Result<ProviderSession> {
        nothing!()
    }
    fn sign_out(&self) {}
}

#[async_trait]
impl MusicApi for Sample {
    fn share_url(&self, _: MediaKind, _: &str) -> Option<String> {
        None
    }
    async fn profile(&self) -> Result<UserProfile> {
        nothing!()
    }
    async fn artist(&self, _: &str) -> Result<Artist> {
        nothing!()
    }
    async fn artist_profile(&self, _: &str) -> Result<ArtistProfile> {
        nothing!()
    }
    async fn artist_images(&self, _: Vec<String>) -> Result<HashMap<String, String>> {
        nothing!()
    }
    async fn saved_tracks(&self) -> Result<Vec<Track>> {
        nothing!()
    }
    async fn set_track_saved(&self, _: &str, _: bool) -> Result<()> {
        nothing!()
    }
    async fn track(&self, _: &str) -> Result<Track> {
        nothing!()
    }
    async fn track_playcount(&self, _: &str) -> Result<Option<u64>> {
        nothing!()
    }
    async fn playlists(&self) -> Result<Vec<Playlist>> {
        nothing!()
    }
    async fn create_playlist(&self, _: &str) -> Result<String> {
        nothing!()
    }
    async fn rename_playlist(&self, _: &str, _: &str) -> Result<()> {
        nothing!()
    }
    async fn delete_playlist(&self, _: &str) -> Result<()> {
        nothing!()
    }
    async fn remove_playlist_from_library(&self, _: &str) -> Result<()> {
        nothing!()
    }
    async fn add_playlist_to_library(&self, _: &str) -> Result<()> {
        nothing!()
    }
    async fn set_playlist_public(&self, _: &str, _: bool) -> Result<()> {
        nothing!()
    }
    async fn add_track_to_playlist(&self, _: &str, _: &str) -> Result<()> {
        nothing!()
    }
    async fn remove_track_from_playlist(&self, _: &str, _: &str) -> Result<()> {
        nothing!()
    }
    async fn saved_albums(&self) -> Result<Vec<Album>> {
        nothing!()
    }
    async fn set_album_saved(&self, _: &str, _: bool) -> Result<()> {
        nothing!()
    }
    async fn saved_artists(&self) -> Result<Vec<SavedArtist>> {
        nothing!()
    }
    async fn set_artist_saved(&self, _: &str, _: bool) -> Result<()> {
        nothing!()
    }
    async fn album(&self, _: &str) -> Result<AlbumDetail> {
        nothing!()
    }
    async fn album_tracks(&self, _: &str) -> Result<Vec<Track>> {
        nothing!()
    }
    async fn playlist(&self, _: &str) -> Result<PlaylistDetail> {
        nothing!()
    }
    async fn playlist_tracks(&self, _: &str) -> Result<Vec<Track>> {
        nothing!()
    }
    async fn playlist_covers(&self, _: &str, _: usize) -> Result<Vec<String>> {
        nothing!()
    }
    async fn track_radio(&self, _: &str, _: Option<&str>) -> Result<(Vec<Track>, Option<String>)> {
        nothing!()
    }
    async fn search(&self, _: &str) -> Result<Vec<Track>> {
        nothing!()
    }
}

/// An engine that plays nothing and says nothing.
impl PlaybackFactory for Sample {
    fn start(&self, _: PlaybackConfig) -> (Box<dyn Player>, Box<dyn PlaybackEvents>) {
        (Box::new(Sample), Box::new(Sample))
    }
}

impl Player for Sample {
    fn load(&self, _: &str, _: Duration, _: bool) -> Result<()> {
        nothing!()
    }
    fn load_paused_at(&self, _: &str, _: Duration) -> Result<()> {
        nothing!()
    }
    fn preload(&self, _: &str, _: bool) -> Result<()> {
        nothing!()
    }
    fn play(&self) {}
    fn pause(&self) {}
    fn seek(&self, _: Duration) {}
    fn set_gain(&self, _: f32) {}
}

#[async_trait]
impl PlaybackEvents for Sample {
    async fn next(&mut self) -> Option<PlaybackEvent> {
        None
    }
}
