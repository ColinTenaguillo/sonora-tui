//! What the shell asks a signed-in service for, in this crate's own types.
//!
//! Every service comes through sonora's `music` crate: a `MusicProvider`
//! signs in and hands back a `ProviderSession`, whose `MusicApi` the
//! `Session` here reads into the shell's types. TIDAL is one such provider
//! (`crate::provider`), with a `Fetch` of its own behind the engine. The
//! shell holds a `Session` and does not know which service answers.
//!
//! The error is the catalogue's own: the one thing the shell tells apart
//! is a session the service no longer accepts, and everything else is
//! words for the status line.

pub(crate) mod session;

use std::sync::Arc;

use async_trait::async_trait;
use music::MusicProvider;

use crate::config::{Config, ProviderKind};
use crate::playback::{NoMeta, StreamMeta};
pub use session::Session;

/// What a request to the service can fail with, as the shell tells them
/// apart: a session the service no longer accepts, and everything else
/// as words.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not authorized — the session may have expired")]
    Unauthorized,
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// What `F` can favourite: the three kinds the account keeps a list of.
/// A TIDAL mix is a playlist here, under its prefixed id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Favourable {
    Artist(String),
    Album(String),
    Playlist(String),
}

/// A service the shell can sign in to: sonora's trait, plus the two things
/// the shell shows that `MusicApi` has no word for. Both default to
/// nothing, so a service answers only what it has.
#[async_trait]
pub trait Provider: MusicProvider {
    /// What the engine's events leave out about a stream, where the
    /// service's own fetch records it. The quality badge reads that.
    fn stream_meta(&self) -> Arc<dyn StreamMeta> {
        Arc::new(NoMeta)
    }

    /// The Feed: what is new for this account, dated. Releases from the
    /// artists followed, on TIDAL.
    async fn feed(&self) -> anyhow::Result<Vec<crate::shell::carousel::Card>> {
        Ok(Vec::new())
    }

    /// Whether `feed` ever has anything: the page says so rather than
    /// staying blank.
    fn has_feed(&self) -> bool {
        false
    }

    /// Artists matching a query. `MusicApi` has no artist search: a
    /// service with one answers here, and for the rest the session
    /// reads the artists off the tracks and albums it found.
    async fn search_artists(&self, _query: &str) -> anyhow::Result<Vec<crate::library::Artist>> {
        Ok(Vec::new())
    }
}

impl Provider for music::subsonic::SubsonicProvider {}
impl Provider for music::spotify::SpotifyProvider {}
impl Provider for music::apple::AppleProvider {}
impl Provider for music::deezer::DeezerProvider {}
impl Provider for music::youtube::YouTubeProvider {}
impl Provider for music::local::LocalProvider {}

/// The shell's state derives `Debug`; a provider prints as its name.
impl std::fmt::Debug for dyn Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// The service the config names, signed out.
pub fn provider_for(config: &Config) -> anyhow::Result<Arc<dyn Provider>> {
    Ok(match config.provider.kind {
        ProviderKind::Tidal => Arc::new(crate::provider::TidalProvider::new(
            config.auth.clone(),
            config.audio.quality(),
        )?),
        ProviderKind::Subsonic => Arc::new(music::subsonic::SubsonicProvider::new()),
        ProviderKind::Spotify => Arc::new(music::spotify::SpotifyProvider::from_env()),
        ProviderKind::Apple => Arc::new(music::apple::AppleProvider::new()),
        ProviderKind::Deezer => Arc::new(music::deezer::DeezerProvider::new()),
        ProviderKind::Youtube => Arc::new(music::youtube::YouTubeProvider::new()),
        // Its index and artwork go under this app's own cache rather than
        // sonora's, so the two never read each other's files.
        ProviderKind::Local => {
            let dir = crate::config::paths::cache_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join("local");
            Arc::new(music::local::LocalProvider::new(
                dir.clone(),
                storage::Database::at(dir.join("state.sqlite")),
                storage::Cache::at(dir.join("cache.sqlite")),
            ))
        }
    })
}
