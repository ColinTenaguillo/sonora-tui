//! A page of titled rows of cards, as the home page, a genre's page and
//! Explore all are: what the shell draws, whichever service answered.

use crate::domain::Card;

/// How a row's items should be laid out.
///
/// The API says which in each module's `type`, so this is read rather than
/// guessed: a TRACK_LIST is a grid of small rows in the web client, not a
/// carousel of covers, and drawing every module the same way made two of
/// the home page's five rows wrong.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    /// Covers in a scrolling strip: albums, playlists, mixes.
    #[default]
    Carousel,
    /// The web client's compact grid: wide cells of thumbnail beside two
    /// lines of text, three across and three deep. `COMPACT_GRID_CARD` on
    /// the v2 feed whatever it holds -- Recently played is one, of albums
    /// and mixes -- and `TRACK_LIST` on a v1 page.
    Compact,
    /// The web client's shortcut grid: the same cells, two deep.
    /// `SHORTCUT_LIST` on the v2 feed, `HIGHLIGHT_MODULE` on a v1 page.
    Shortcuts,
    /// Links to other pages -- the genres, moods and decades on Explore.
    /// These carry no artwork of any kind: their `imageId` is a name like
    /// "hiphop" rather than a uuid, and there is no image behind it. Drawn
    /// as covers they were a row of empty grey squares, so they get the
    /// web client's own shape instead: a rounded pill holding the title.
    Links,
}

impl RowKind {
    /// How many lines of cells a grid of this kind draws, or `None` for a
    /// kind that is not a grid. The web's track grid is three deep, its
    /// shortcut grid two.
    pub fn grid_rows(&self) -> Option<usize> {
        match self {
            RowKind::Compact => Some(GRID_ROWS),
            RowKind::Shortcuts => Some(2),
            RowKind::Carousel | RowKind::Links => None,
        }
    }
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

/// Columns of cells in a track grid, as the web client's three.
pub const GRID_COLUMNS: usize = 3;
/// Rows of cells in a track grid, as the web client's three.
pub const GRID_ROWS: usize = 3;
