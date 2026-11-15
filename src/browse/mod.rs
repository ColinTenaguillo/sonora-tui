//! A page of titled rows of cards, as the home page, a genre's page and
//! Explore all are: what the shell draws, whichever service answered.

use crate::domain::Card;

/// How a row's items should be laid out.
///
/// A row made only of links to other pages is drawn as pills; anything
/// with artwork is a strip of covers.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    /// Covers in a scrolling strip: albums, playlists, mixes.
    #[default]
    Carousel,
    /// Links to other pages -- the genres, moods and decades on Explore.
    /// These carry no artwork of any kind: their `imageId` is a name like
    /// "hiphop" rather than a uuid, and there is no image behind it. Drawn
    /// as covers they were a row of empty grey squares, so they get the
    /// web client's own shape instead: a rounded pill holding the title.
    Links,
}

/// One titled row of the home page.
#[derive(Debug, Clone)]
pub struct HomeRow {
    pub heading: String,
    pub kind: RowKind,
    pub cards: Vec<Card>,
    /// Where to ask for more of this row's items.
    ///
    /// A TRACK_LIST module returns five whatever `limit` the page is asked
    /// for — the count is fixed on the server — but it carries a path that
    /// takes one. Without it a grid of six always has a hole in it.
    pub more: Option<String>,
}

/// What the home page turned out to contain.
#[derive(Debug, Default, Clone)]
pub struct Home {
    pub rows: Vec<HomeRow>,
}
