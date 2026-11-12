//! The library's records as the shell draws them: playlists, albums,
//! artists, and the page an artist or an album opens.

use crate::domain::Track;

#[derive(Debug, Clone)]
pub struct Playlist {
    pub uuid: String,
    pub title: String,
    pub track_count: u32,
    /// Running time, as the web client shows under the count.
    pub duration: Option<std::time::Duration>,
    /// Who made it. TIDAL's editorial playlists have no user behind them, so
    /// the web client shows "TIDAL" there and so do we.
    pub creator: String,
    pub cover: Option<String>,
    /// The account's own, so it can be edited: the service says.
    pub owned: bool,
}

impl Playlist {
    /// A playlist with only the fields a caller cares about set. Tests and
    /// previews want a title and a count, not five fields of ceremony.
    pub fn sample(title: &str, track_count: u32) -> Self {
        Self {
            uuid: String::new(),
            title: title.into(),
            track_count,
            duration: None,
            creator: "Coco".into(),
            cover: None,
            owned: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Album {
    pub id: String,
    pub title: String,
    pub artist: String,
    /// Release year. Absent rather than guessed when the date is missing or
    /// malformed — a wrong year is worse than none.
    pub year: Option<String>,
    pub cover: Option<String>,
    pub track_count: u32,
    /// Running time, which the web client shows beside the track count.
    pub duration: Option<std::time::Duration>,
}

#[derive(Debug, Clone)]
pub struct Artist {
    pub id: String,
    pub name: String,
    pub picture: Option<String>,
}

/// The ids of everything the user has favourited, in one reply: albums,
/// playlists and artists. What the key that toggles a favourite reads to
/// know which way to go, without walking the three collections.
#[derive(Debug, Clone, Default)]
pub struct FavouriteIds {
    pub albums: Vec<String>,
    pub playlists: Vec<String>,
    pub artists: Vec<String>,
}

/// Which page this is. An album's page is the artist's shape -- a cover,
/// a name, a blurb, a list of tracks, rows of related records -- so it is
/// the same struct, and this says what the header draws and what `F`
/// favourites.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum PageKind {
    #[default]
    Artist,
    Album,
}

/// Everything an artist's page shows -- or an album's, see [`PageKind`].
#[derive(Debug, Default, Clone)]
pub struct ArtistPage {
    pub kind: PageKind,
    pub id: String,
    pub name: String,
    /// The year an album came out, on an album's page. An artist has none.
    pub year: Option<String>,
    /// How many follow them: the "23.8K fans" the web puts under the name.
    /// Only v2 has it, and that request is best-effort.
    pub fans: Option<u64>,
    /// Whether this account follows them, as the server says.
    pub following: bool,
    pub picture: Option<String>,
    /// The artist's own blurb, when TIDAL has one. Editorial rather than
    /// generated, so plenty of artists have none — Prince has six thousand
    /// words and Kaaris nothing at all.
    pub bio: Option<String>,
    pub top_tracks: Vec<Track>,
    pub albums: Vec<Album>,
    /// EPs and singles, which the web client shows as its own row under the
    /// albums.
    pub singles: Vec<Album>,
    /// Records the artist appears on rather than made: compilations, other
    /// people's albums.
    pub appears_on: Vec<Album>,
    pub similar: Vec<Artist>,
    /// The artist's radio, which the web client offers in its header.
    pub radio: Option<String>,
    /// Where the whole of Top Tracks lives. The page returns four of a
    /// hundred; the provider fetches the rest for `artist_catalogue`.
    pub top_tracks_path: Option<String>,
}
