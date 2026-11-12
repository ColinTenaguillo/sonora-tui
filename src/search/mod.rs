//! Everything one search turned up, in the shell's types.

use crate::domain::Track;
use crate::library::{Album, Artist, Playlist};

/// Everything one query turned up.
#[derive(Debug, Default, Clone)]
pub struct Results {
    pub query: String,
    pub tracks: Vec<Track>,
    pub albums: Vec<Album>,
    pub artists: Vec<Artist>,
    pub playlists: Vec<Playlist>,
}

impl Results {
    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
            && self.albums.is_empty()
            && self.artists.is_empty()
            && self.playlists.is_empty()
    }

    pub fn total(&self) -> usize {
        self.tracks.len() + self.albums.len() + self.artists.len() + self.playlists.len()
    }
}
