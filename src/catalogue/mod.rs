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
pub trait Provider: MusicProvider {
    /// The lyrics the service keeps itself, asked beside the sources any
    /// track is. Built from this provider so that Spotify's shares its
    /// account: two would each refresh the one stored grant, and Spotify
    /// may then refuse the other's.
    fn own_lyrics(&self) -> Option<Arc<dyn music::LyricsProvider>> {
        None
    }
}

impl Provider for spotify::SpotifyProvider {
    fn own_lyrics(&self) -> Option<Arc<dyn music::LyricsProvider>> {
        Some(Arc::new(spotify::SpotifyProvider::lyrics(self)))
    }
}

/// Subsonic's and Jellyfin's read the sign-in sonora stored, and answer
/// only for the server's own tracks.
impl Provider for subsonic::SubsonicProvider {
    fn own_lyrics(&self) -> Option<Arc<dyn music::LyricsProvider>> {
        Some(Arc::new(subsonic::SubsonicLyrics::new()))
    }
}

impl Provider for jellyfin::JellyfinProvider {
    fn own_lyrics(&self) -> Option<Arc<dyn music::LyricsProvider>> {
        Some(Arc::new(jellyfin::JellyfinLyrics::new()))
    }
}

/// sonora has no lyrics of TIDAL's, Apple Music's or Deezer's own. A
/// folder's and YouTube Music's, which need no sign-in, are among the
/// sources every track is asked.
impl Provider for tidal::TidalProvider {}
impl Provider for apple::AppleProvider {}
impl Provider for deezer::DeezerProvider {}
impl Provider for youtube::YouTubeProvider {}
impl Provider for local::LocalProvider {}

/// What a service has that sonora's `MusicApi` has no word for yet. Each
/// answers nothing by default, and the session then makes do with what
/// sonora gives: an adapter says only what its service adds, and goes the
/// day sonora says it itself.
#[async_trait::async_trait]
pub trait Extras: Send + Sync {
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
        ProviderKind::Tidal => Arc::new(tidal::TidalProvider::new()),
        ProviderKind::Subsonic => Arc::new(subsonic::SubsonicProvider::new()),
        ProviderKind::Jellyfin => Arc::new(jellyfin::JellyfinProvider::new()),
        ProviderKind::Spotify => Arc::new(spotify::SpotifyProvider::from_env()),
        ProviderKind::Apple => Arc::new(apple::AppleProvider::new()),
        ProviderKind::Deezer => Arc::new(deezer::DeezerProvider::new()),
        ProviderKind::Youtube => Arc::new(youtube::YouTubeProvider::new()),
        // Its index and artwork go under this app's own cache rather than
        // sonora's, so the two never read each other's files.
        ProviderKind::Local => {
            let dir = crate::config::paths::cache_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join("local");
            Arc::new(local::LocalProvider::new(
                dir.clone(),
                storage::Database::at(dir.join("state.sqlite")),
                storage::Cache::at(dir.join("cache.sqlite")),
            ))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Spotify provider whose sign-in would be kept in a folder of the
    /// test's: `from_env`, and YouTube Music's `new`, name sonora's own
    /// folder, which moves the sign-ins an older Sonora left in the cache.
    pub(crate) fn spotify_away_from_home() -> spotify::SpotifyProvider {
        spotify::SpotifyProvider::new(spotify::AuthConfig {
            client_id: "test".into(),
            redirect_uri: "http://127.0.0.1:1/".into(),
            cache_dir: std::env::temp_dir().join("sonora-tui-test-spotify"),
        })
    }

    #[test]
    fn the_services_with_lyrics_of_their_own_hand_them_over_and_the_others_none() {
        let own = |provider: &dyn Provider| provider.own_lyrics().map(|l| l.name());
        assert_eq!(own(&spotify_away_from_home()), Some("Spotify"));
        assert_eq!(own(&subsonic::SubsonicProvider::new()), Some("Subsonic"));
        assert_eq!(own(&jellyfin::JellyfinProvider::new()), Some("Jellyfin"));
        for none in [
            own(&tidal::TidalProvider::new()),
            own(&apple::AppleProvider::new()),
            own(&deezer::DeezerProvider::new()),
        ] {
            assert_eq!(none, None);
        }
    }
}
