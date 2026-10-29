//! A card: what a row of a provider's home page, feed or search holds, and
//! what the shell draws as a cover with lines of text under it.

use super::Section;

/// What activating a card does. A card without one is a label — a mix or a
/// module the API gave us no identifier for — and enter does nothing on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Playlist(String),
    Album(String),
    Track(String),
    /// An artist's own page. A profile card opened nothing before; it is
    /// the one kind of card the app drew with nowhere to go.
    Artist(String),
    /// Another page of rows, which is what an Explore link opens: a genre,
    /// a mood, a decade. They carry a path rather than an id.
    Page(String),
    /// One of this app's own views: what a `DEEP_LINK` shortcut such as
    /// "My Tracks" points at. The feed gives it a `tidal://` url for an
    /// id, and read as a mix it asked the API for a mix called that.
    Section(Section),
}

/// One card: a cover plus its lines of text.
#[derive(Debug, Clone, Default)]
pub struct Card {
    pub title: String,
    pub subtitle: String,
    /// A third line under the subtitle, as the playlist grid's track count.
    /// Empty means the card has only two lines.
    pub detail: String,
    pub cover_url: Option<String>,
    /// Artist avatars are round and centred, with no subtitle. Everything
    /// else is a square cover with left-aligned text.
    pub round: bool,
    /// What enter opens or plays. `None` on a card the API gave no id for.
    pub target: Option<Target>,
    /// How long a track card's track runs. Zero for anything that is not a
    /// track, and for a track the API did not say. Carried on the card
    /// because playing from one is all the player knows until the stream
    /// starts — without it the bar had no duration to divide by and drew
    /// itself full over a track that had just begun.
    pub duration: std::time::Duration,
    /// The day this happened, counted from 1970-01-01. Only the feed sets
    /// it, which is the one view that groups by when rather than by what.
    pub day: Option<i64>,
}

impl Card {
    pub fn new(title: impl Into<String>, subtitle: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            subtitle: subtitle.into(),
            ..Default::default()
        }
    }
}
