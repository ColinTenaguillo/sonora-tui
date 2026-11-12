//! What the shell asks a signed-in service for, in this crate's own types.
//!
//! Every service comes through sonora's `music` crate: a `MusicProvider`
//! signs in and hands back a `ProviderSession`, whose `MusicApi` the
//! `Session` here reads into the shell's types. The shell holds a `Session`
//! and does not know which service answers.
//!
//! The error is the catalogue's own: the one thing the shell tells apart
//! is a session the service no longer accepts, and everything else is
//! words for the status line.

mod carried;
mod sample;
pub(crate) mod session;
mod tidal_extras;

use std::sync::Arc;

use music::MusicProvider;

use crate::config::{Config, ProviderKind};
pub use carried::tidal_sign_in as carry_tidal_sign_in_over;
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

/// A service the shell can sign in to: sonora's own trait, under a name
/// of ours so the shell's state, which derives `Debug`, can print one.
pub trait Provider: MusicProvider {}

impl<P: MusicProvider> Provider for P {}

/// What a service has that sonora's `MusicApi` has no word for yet. Each
/// answers nothing by default, and the session then makes do with what
/// sonora gives: an adapter says only what its service adds, and goes the
/// day sonora says it itself.
#[async_trait::async_trait]
pub trait Extras: Send + Sync {
    /// The service's own Explore page, where it has more on it than its
    /// genres.
    async fn explore(&self) -> anyhow::Result<Option<crate::browse::Home>> {
        Ok(None)
    }

    /// The page of rows a card of such a page opens, when `path` is one
    /// of the service's own rather than a genre of sonora's.
    async fn page(&self, _path: &str) -> anyhow::Result<Option<crate::browse::Home>> {
        Ok(None)
    }

    /// The picture an artist is drawn with: their own, or what the service
    /// puts in its place for one it keeps no portrait of.
    async fn artist_picture(&self, _artist_id: &str) -> anyhow::Result<Option<String>> {
        Ok(None)
    }

    /// The playlist a track's radio is, where the service keeps stations
    /// as playlists: what `F` saves on an open radio.
    async fn radio_playlist(&self, _track_id: &str) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
}

/// A service that adds nothing.
struct NoExtras;

impl Extras for NoExtras {}

/// What the service with this slug adds, over its signed-in `api`.
fn extras_for(slug: &str, api: &Arc<dyn music::MusicApi>) -> Arc<dyn Extras> {
    match slug {
        "tidal" => Arc::new(tidal_extras::TidalExtras::new(Arc::clone(api))),
        _ => Arc::new(NoExtras),
    }
}

impl dyn Provider {
    /// What the screen calls the service: its own name, but for TIDAL,
    /// which writes itself in capitals and sonora does not.
    pub fn label(&self) -> &'static str {
        match self.slug() {
            "tidal" => "TIDAL",
            _ => self.name(),
        }
    }
}

/// The shell's state derives `Debug`; a provider prints as its name.
impl std::fmt::Debug for dyn Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// The service the config names, signed out.
pub fn provider_for(config: &Config) -> anyhow::Result<Arc<dyn Provider>> {
    Ok(match config.provider.kind {
        ProviderKind::Tidal => Arc::new(music::tidal::TidalProvider::new()),
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
