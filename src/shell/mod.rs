pub mod artistview;
pub mod artwork;
pub mod carousel;
pub mod cursor;
pub mod drmview;
pub mod fuzzy;
#[cfg(test)]
pub mod geometry;
pub mod grid;
pub mod help;
pub mod home;
pub mod icons;
pub mod inputbox;
pub mod keymap;
pub mod layout;
pub mod loadingview;
pub mod lyricsview;
pub mod mediakeys;
#[cfg(target_os = "linux")]
mod mediakeys_linux;
#[cfg(target_os = "macos")]
pub mod mediakeys_macos;
pub mod queueview;

/// Register with the desktop's media keys, where the platform has them.
///
/// Public so the integration test can drive it the way a desktop does:
/// registering on the bus is the whole feature, and calling the handlers
/// directly would prove nothing about that.
pub fn spawn_media_keys(
    actions: tokio::sync::mpsc::UnboundedSender<Action>,
    state: mediakeys::StateReceiver,
) {
    #[cfg(target_os = "linux")]
    mediakeys_linux::spawn(actions, state);
    #[cfg(target_os = "macos")]
    mediakeys_macos::spawn(actions, state);
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (actions, state);
    }
}
pub mod login;
pub mod nowplaying;
pub mod playlistedit;
pub mod scrollbar;
pub mod searchview;
pub mod settings;
pub mod sidebar;
pub mod theme;
pub mod trackgrid;
pub mod tracklist;
pub mod waiting;

use std::time::Duration;

use crossterm::event::{Event, EventStream, KeyCode, KeyEvent};
use futures::StreamExt as _;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::DefaultTerminal;

/// Everything that can change the app's state. Components emit these; only the
/// loop applies them.
#[derive(Debug, Clone)]
pub enum Action {
    Quit,
    Tick,
    Key(KeyEvent),
    Error(String),
    /// What the service wants on the way in: a code to confirm in the
    /// browser, a link to authorise at, cookies pasted, an account picked.
    LoginPrompt(music::SignInPrompt),
    /// What the user typed or picked for the prompt up: the cookies, or
    /// the id of the account.
    Answer(String),
    /// Use the service without an account, where it allows that.
    BeginGuest,
    /// A session is in hand, fresh or resumed: start its engine and
    /// fetch what every signed-in screen needs.
    SessionReady,
    /// The password typed for the Subsonic server the config names.
    Connect(login::Secret),
    /// Tab on the sign-in screen: the next service in the list, as the
    /// Settings row would pick it, without going through Settings -- which
    /// a screen stuck on "retry or quit" could not reach.
    NextService,
    /// Esc on the sign-in screen: the sign-in under way is dropped and
    /// the screen goes back to its start. Esc is a step back everywhere
    /// else in the app; quitting from it was the one place it was not.
    CancelLogin,
    /// A setting typed on the sign-in screen, to check and write to the
    /// config: the local folder, the Subsonic server or user.
    Typed(login::Field, String),
    /// A playlist edit the user confirmed in its prompt: the loop sends
    /// it to the service.
    Edit(playlistedit::Edit),
    /// `x` in a playlist of the account's own: the track out of it.
    RemoveFromPlaylist {
        uuid: String,
        track: crate::domain::TrackId,
    },
    /// What was typed into a text setting's box, on Enter.
    SettingTyped(settings::Setting, String),
    /// A setting changed, by key or by box, for whatever reads it live:
    /// the scrobblers link again, the Discord presence restarts, a folder
    /// of files is read again, the colours change.
    SettingChanged(settings::Setting),
    /// The service took an edit. The playlists are reloaded alongside;
    /// an open playlist that was edited is reloaded, a deleted one closed.
    PlaylistEdited {
        message: String,
        uuid: String,
        deleted: bool,
        /// What the edit did to the playlist's track count: a track in or
        /// out is applied here rather than by fetching every playlist
        /// again, which was the slow part.
        count_delta: i32,
    },
    /// Signed in, or a stored session resumed.
    SessionOpened(std::sync::Arc<crate::catalogue::Session>),
    BeginLogin,
    SessionExpired,
    SidebarNext,
    SidebarPrevious,
    TrackNext,
    TrackPrevious,
    ActivateSelection,
    CloseCollection,
    /// One step back through the views and sections the user has been in,
    /// and one step forward again.
    GoBack,
    GoForward,
    /// Straight back to the home page, from wherever the user is.
    GoHome,
    /// Straight to one nav entry, by its number.
    GoToSection(sidebar::Section),
    /// Put the selected track next, ahead of the rest of the queue.
    QueueTrackNext,
    /// Put it at the end of what the user has asked for.
    QueueTrackLast,
    /// Take the queued track at this position out of the queue.
    RemoveQueued(usize),
    /// Play the queued track at this position, skipping what is between.
    JumpInQueue(usize),
    /// Open the album of the selected track.
    OpenTrackAlbum,
    /// Open the artist of the selected track.
    OpenTrackArtist,
    /// The radio built around the track that is playing.
    PlayTrackRadio,
    /// The selected track's album or artist, when the row in hand names
    /// both and carries the id of neither -- a card from the home page.
    /// Fetch the track, which does carry them, then open what it names.
    FetchTrackAlbum(crate::domain::TrackId),
    FetchTrackArtist(crate::domain::TrackId),
    /// Open a collection the loop has already resolved: what the fetch
    /// above found. `title` is what the view is headed with, and the rest
    /// is its banner until the reply fills it in.
    OpenTarget {
        target: Collection,
        title: String,
        subtitle: String,
        cover: Option<String>,
    },
    /// The queue is nearly out and autoplay is on: fetch the radio that
    /// follows `seed`, the last track queued.
    Autoplay {
        seed: crate::domain::TrackId,
    },
    /// The radio came back: append it, and start it if the queue ran dry
    /// while waiting.
    QueueRadio(Vec<crate::domain::Track>),
    /// Nothing came back for this seed, so it is not asked about again.
    AutoplayFailed(crate::domain::TrackId),
    /// Turn autoplay on or off and save it, from the queue view.
    ToggleAutoplay,
    /// Half a screen down or up, as vim's ctrl-d and ctrl-u: the same
    /// step `j` or `k` takes, repeated for half of what is drawn.
    HalfPage {
        down: bool,
    },
    /// Favourite what is under the cursor, or take it out again: the
    /// artist whose page is open, the album, playlist, mix or artist card
    /// selected, or the collection whose view is up.
    ToggleFollow,
    /// The account says so now: mark it, and say it.
    FollowChanged {
        what: Favourable,
        name: String,
        following: bool,
    },
    /// The playlist a station is, learnt as it opens: what `F` saves there.
    RadioNamed {
        seed: crate::domain::TrackId,
        playlist: String,
    },
    /// The ids of everything favourited, from the startup fetch.
    FavouriteIdsLoaded(crate::library::FavouriteIds),
    /// The user's favourites, from the startup fetch.
    TracksLoaded(Vec<crate::domain::Track>),
    /// The contents of a playlist or album the user opened. Kept apart from
    /// the favourites so a slow startup fetch landing afterwards cannot
    /// overwrite what the user is looking at — they are two different lists
    /// that happened to share an action.
    CollectionLoaded {
        /// What was open when the request went out. A reply for something
        /// the user has since left is dropped rather than shown under the
        /// wrong heading.
        for_title: String,
        tracks: Vec<crate::domain::Track>,
    },
    PlaylistsLoaded(Vec<crate::library::Playlist>),
    AlbumsLoaded(Vec<crate::library::Album>),
    ArtistsLoaded(Vec<crate::library::Artist>),
    /// Boxed for the same reason a home page is: it is rows of cards.
    ExploreLoaded(Box<crate::browse::Home>),
    /// A genre, mood or decade page, opened from Explore.
    PageLoaded {
        title: String,
        home: Box<crate::browse::Home>,
    },
    /// An artist's page. Boxed: three sections of items.
    ArtistLoaded(Box<crate::library::ArtistPage>),
    ArtistLeft,
    ArtistRight,
    /// Fetch the Explore page. Its own request, since nothing else needs it.
    LoadExplore,
    /// Fetch the activity feed.
    LoadFeed,
    /// Fetch the favourite tracks, playlists and albums again.
    ///
    /// They are loaded once at sign-in, but the reply only lands in the
    /// pane when nothing is open over it -- `self.tracks` is shared with
    /// whatever collection is on screen, so an arriving list of favourites
    /// would otherwise replace an open album's contents. A section left
    /// empty by that is refetched when the user walks into it.
    LoadFavourites,
    FeedLoaded(Vec<carousel::Card>),
    /// The home page's rows. Boxed: it is by far the largest payload an
    /// Action carries, and every other variant would otherwise grow to
    /// match it.
    HomeLoaded(Box<crate::browse::Home>),
    CarouselNext,
    CarouselPrevious,
    RowNext,
    RowPrevious,
    LoadHome,
    /// Open the search box.
    BeginSearch,
    /// Run the query that has been typed.
    RunSearch(String),
    /// What a query returned. Carries the query, so a reply for one the user
    /// has since retyped is dropped rather than shown under the new text.
    SearchLoaded(Box<crate::search::Results>),
    /// Leave search and go back to what was showing.
    CloseSearch,
    /// Move to the next tab of the search results.
    NextSearchTab,
    SearchLeft,
    SearchRight,
    /// Add the selected track to the favourites, or take it out if it is
    /// already one.
    ToggleFavourite,
    /// Open the whole of the home row the selection is in.
    SeeAll,
    /// Skip forward or back in the queue.
    QueueNext,
    QueuePrevious,
    /// Play whatever the queue says comes next, or nothing if it is done.
    PlayQueued,
    /// The output level changed in Settings. Applied in the loop, which is
    /// what holds the channel to the player.
    SetVolume(f32),
    /// Fetch the track that plays next, ahead of time.
    Preload(crate::domain::TrackId),
    /// A setting the engine reads once at its start changed: start it
    /// again, on the same track and position.
    RestartEngine,
    /// The sleep timer's next step: 15, 30, 45, 60 minutes, the end of the
    /// track, off.
    CycleSleep,
    /// Leave the service for the one the Settings row names, and sign in
    /// to it.
    SwitchProvider,
    /// The lyrics pane, over the main pane and off again.
    ToggleLyrics,
    /// Ask the lyrics providers about this track.
    LoadLyrics(crate::domain::TrackId),
    /// What they said.
    LyricsLoaded {
        id: crate::domain::TrackId,
        found: lyricsview::Found,
    },
    /// The scrobbling services the config names, linked at start-up.
    /// The accounts linked, and what was refused, in words.
    ScrobblersLinked(Vec<crate::scrobble::Linked>, Vec<String>),
    /// The Widevine module: search the machine, what was found, download
    /// Google's with its terms, what the user said to each question, and
    /// how it ended.
    LookForCdm,
    CdmFound(Option<music::drm::Origin>),
    DownloadCdm,
    CdmOffered {
        version: String,
        license: String,
    },
    AnswerCdm(bool),
    CdmReady(music::drm::Origin),
    CdmFailed(String),
    ToggleShuffle,
    CycleRepeat,
    /// The reply: what the track's state is now, so the mark matches the
    /// account rather than what was guessed when the key was pressed.
    FavouriteChanged {
        id: crate::domain::TrackId,
        favourite: bool,
    },
    Playback(crate::playback::PlaybackEvent),
    TogglePause,
    /// Ten seconds along the track, or back. The bar moves at once and the
    /// player follows; the next position event settles any difference.
    SeekForward,
    SeekBack,
    /// A track already showing in the bar failed to start. Distinct from
    /// `Error`: the bar has to be undone as well as the message shown, and
    /// a generic error carries no way to tell which track it was about.
    PlaybackFailed {
        id: crate::domain::TrackId,
        message: String,
    },
}

/// The search box, and whatever the last query returned.
#[derive(Debug, Default)]
pub struct SearchState {
    /// What has been typed.
    pub query: String,
    /// True while the box has the keyboard.
    pub typing: bool,
    /// A query has been sent and its reply has not come back: the view
    /// says it is searching rather than nothing, or "Nothing found".
    pub pending: bool,
    /// The results of the last query that came back. Held separately from
    /// `query` so the previous results stay on screen while the next query
    /// is being typed, rather than blanking on every keystroke.
    pub results: crate::search::Results,
    /// Which tab is showing.
    pub tab: usize,
    /// The Tracks tab is the app's own track list, so it carries that
    /// view's state rather than a second kind of selection.
    pub tracks: tracklist::TrackListState,
    /// Albums, artists and playlists are the app's own card grid, one
    /// state each so moving through one does not disturb the others.
    pub albums: grid::GridState,
    pub artists: grid::GridState,
    pub playlists: grid::GridState,
    /// Which of Top results' stacked sections the selection is in. The tab
    /// shows three kinds at once, so moving down has to walk out of one
    /// section and into the next rather than stopping at the first.
    pub top: searchview::TopSection,
    /// Top results' own card rows. Carousels rather than the tabs' grids:
    /// a grid scrolls by whole rows, so a section of one row could never
    /// scroll, and it drew a scrollbar beside itself to say so.
    pub top_artists: carousel::CarouselState,
    pub top_albums: carousel::CarouselState,
    /// The first Top results section drawn, so the selected one is on
    /// screen. Scrolled by section, as the artist page is.
    pub top_scroll: usize,
}

impl SearchState {
    /// Whether the selection is in a track list: the Tracks tab, or the
    /// tracks section of Top results.
    fn in_tracks(&self, tab: searchview::Tab) -> bool {
        tab.is_tracks()
            || (matches!(tab, searchview::Tab::Top) && self.top == searchview::TopSection::Tracks)
    }
}

/// Which way a movement key goes.
///
/// The search tabs are three different views with three different rules, so
/// the key handler says only the direction and the view decides what a step
/// in it means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dir {
    Up,
    Down,
    Left,
    Right,
}

/// How far `.` and `,` move along the track.
const SEEK_STEP: std::time::Duration = std::time::Duration::from_secs(10);

/// How many of Top results' sections are on screen at once: it scrolls by
/// section, two at a time. An artist's page counts its own from its layout.
const SECTIONS_SHOWING: usize = 2;

/// A page of stacked sections as the keys see it, for [`App::move_in_stack`].
struct Stacked<'a> {
    /// How many sections are present.
    count: usize,
    /// Which of them holds the selection.
    at: usize,
    /// The track list among them: its index, state, length, and the rows
    /// the selection may land in.
    tracks: Option<(usize, &'a mut tracklist::TrackListState, usize, usize)>,
    /// The selected section's row of cards, when it is one: state, length,
    /// and the cards the selection may land on.
    row: Option<(&'a mut carousel::CarouselState, usize, usize)>,
    /// Sections scrolled past the top of the page, and how many from there
    /// are drawn whole: a move into one past those scrolls the page to it.
    scroll: &'a mut usize,
    visible: usize,
}

impl Dir {
    /// `Some(down)` for a vertical move, `None` for a horizontal one.
    fn vertical(self) -> Option<bool> {
        match self {
            Dir::Down => Some(true),
            Dir::Up => Some(false),
            _ => None,
        }
    }

    /// Down or right: the way a selection index grows.
    fn forward(self) -> bool {
        matches!(self, Dir::Down | Dir::Right)
    }
}

impl App {
    /// The track the highlighted row stands for, wherever the user is.
    ///
    /// The selection indexes whatever list is on screen. In the library that
    /// is the filtered tracks — reading `self.tracks` directly would play a
    /// different track than the highlighted row whenever a filter was in
    /// effect. In search it is the results, and reading the library there
    /// played whatever track sat at the same index in a list nothing was
    /// showing.
    fn selected_track(&self) -> Option<crate::domain::Track> {
        // Read from what is drawn, as `on_grid` is: the keys act on what
        // the user can see. This read the Tracks section's list whatever
        // was on screen, so on the home page `A` favourited and `c` opened
        // whichever favourite that hidden list had highlighted -- which is
        // what "the key does nothing" looked like from the outside.
        match self.showing() {
            Showing::Queue => self
                .queue
                .entries()
                .get(self.queue_list.selected)
                .map(|(track, _)| (*track).clone()),
            Showing::History => self
                .queue
                .history()
                .nth(self.history_list.selected)
                .cloned(),
            Showing::Artist => {
                let page = self.artist.as_ref()?;
                if artistview::Section::from_index(self.artist_section)
                    == artistview::Section::Tracks
                {
                    page.top_tracks.get(self.artist_tracks.selected).cloned()
                } else {
                    self.selected_card_track()
                }
            }
            Showing::Search(tab) => {
                let state = self.search.as_ref()?;
                // Top stacks card rows over a track list, and `top` says
                // which of them holds the selection.
                let in_tracks = state.in_tracks(tab);
                if in_tracks {
                    searchview::track_rows(&state.results, tab)
                        .get(state.tracks.selected)
                        .cloned()
                } else {
                    self.selected_card_track()
                }
            }
            Showing::Tracks | Showing::Section(sidebar::Section::Tracks) => {
                tracklist::filter(&self.tracks, &self.tracklist.filter)
                    .get(self.tracklist.selected)
                    .map(|t| (*t).clone())
            }
            // Everything else is cards, and a card is a track when it
            // points at one.
            Showing::Rows | Showing::Cards | Showing::Loading | Showing::Section(_) => {
                self.selected_card_track()
            }
        }
    }

    /// The track the selected card stands for, when it is a track at all.
    fn selected_card_track(&self) -> Option<crate::domain::Track> {
        self.selected_card().and_then(|card| track_from_card(&card))
    }

    /// The list the selection sits in, and where in it.
    ///
    /// Playing a track queues the list it came from, which is what makes an
    /// album play on. Same source as `selected_track`, so the two cannot
    /// disagree about which list is on screen.
    fn selected_list(&self) -> (Vec<crate::domain::Track>, usize, Option<String>) {
        // Read from what is drawn, as `selected_track` is, and for the same
        // reason: this knew three lists and nothing else, so a track played
        // from a row of cards on the home page went with no context at all
        // -- it played alone, the queue stayed empty, and the skip keys had
        // nowhere to go.
        match self.showing() {
            // Enter is the list's own key there, and never reaches this.
            Showing::Queue | Showing::History => (Vec::new(), 0, None),
            Showing::Artist => {
                let Some(page) = self.artist.as_ref() else {
                    return (Vec::new(), 0, None);
                };
                if artistview::Section::from_index(self.artist_section)
                    == artistview::Section::Tracks
                {
                    (
                        page.top_tracks.clone(),
                        self.artist_tracks.selected,
                        Some(page.name.clone()),
                    )
                } else {
                    self.selected_card_list()
                }
            }
            Showing::Search(tab) => {
                let Some(state) = self.search.as_ref() else {
                    return (Vec::new(), 0, None);
                };
                let in_tracks = state.in_tracks(tab);
                if in_tracks {
                    (
                        searchview::track_rows(&state.results, tab),
                        state.tracks.selected,
                        Some("Search".to_string()),
                    )
                } else {
                    self.selected_card_list()
                }
            }
            Showing::Tracks | Showing::Section(sidebar::Section::Tracks) => (
                tracklist::filter(&self.tracks, &self.tracklist.filter)
                    .into_iter()
                    .cloned()
                    .collect(),
                self.tracklist.selected,
                // Named after the view it came from, so the queue can say
                // where it is playing from rather than leaving the user to
                // work out which album this is.
                self.open
                    .as_ref()
                    .map(|o| o.title.clone())
                    .or_else(|| Some(self.sidebar.section().label().to_string())),
            ),
            Showing::Rows | Showing::Cards | Showing::Loading | Showing::Section(_) => {
                self.selected_card_list()
            }
        }
    }

    /// The tracks of the row the selected card sits in, and where the card
    /// is among them: a row of new tracks plays on like an album does, and
    /// the queue shows it under the row's own heading. A track card that is
    /// not in such a row is a list of itself; a card that is not a track is
    /// no list at all.
    fn selected_card_list(&self) -> (Vec<crate::domain::Track>, usize, Option<String>) {
        let Some(track) = self.selected_card_track() else {
            return (Vec::new(), 0, None);
        };
        if self.on_home() {
            if let Some(row) = self.rows_on_screen().current_row() {
                let is_track =
                    |c: &carousel::Card| matches!(c.target, Some(carousel::Target::Track(_)));
                let tracks: Vec<_> = row.cards.iter().filter_map(track_from_card).collect();
                // Its place among the tracks, not among the cards: a row
                // may mix in cards that are not tracks.
                let at = row
                    .cards
                    .iter()
                    .take(row.state.selected)
                    .filter(|c| is_track(c))
                    .count();
                if at < tracks.len() {
                    return (tracks, at, Some(row.heading.clone()));
                }
            }
        }
        (vec![track], 0, None)
    }

    /// Move the selection in whichever view the active search tab is.
    ///
    /// Each tab is one of the app's own views, so the movement rules are
    /// theirs too — a track list moves by lines, a grid by rows of cards.
    /// Keeping a separate selection here would have been a fifth set of the
    /// same arithmetic.
    fn search_move(&mut self, dir: Dir) {
        // The body the view actually draws into, not the whole pane: the
        // search header sits above it. Scrolling against the pane height let
        // the selection run six rows below the last drawn one, which is
        // under the now-playing bar.
        let height = self.search_body_height();
        let width = self.last_main_width;
        let (cols, grid_rows) = self.search_grid_geometry();
        let Some(state) = self.search.as_mut() else {
            return;
        };
        let tab = searchview::Tab::from_index(state.tab);

        match tab {
            searchview::Tab::Top => {
                Self::move_in_top(state, dir, width, height);
            }
            searchview::Tab::Tracks => {
                // A track list is one column, so left and right have nothing
                // to move along.
                let Some(down) = dir.vertical() else { return };
                let len = searchview::track_rows(&state.results, tab).len();
                let visible = tracklist::visible_rows_of(height, None, tracklist::Chrome::Bare);
                state.tracks.step(down, len, visible);
            }
            searchview::Tab::Albums | searchview::Tab::Artists | searchview::Tab::Playlists => {
                let len = searchview::cards(&state.results, tab).len();
                let grid = match tab {
                    searchview::Tab::Albums => &mut state.albums,
                    searchview::Tab::Artists => &mut state.artists,
                    _ => &mut state.playlists,
                };
                // A grid moves a whole row on j/k and one card on h/l, the
                // same as the collection grids it is borrowed from.
                grid.step(
                    dir.forward(),
                    dir.vertical().is_some(),
                    len,
                    cols,
                    grid_rows,
                );
            }
        }
    }

    /// Move within Top results, which stacks three sections.
    fn move_in_top(state: &mut SearchState, dir: Dir, width: u16, height: u16) {
        use searchview::TopSection;

        let present = TopSection::present(&state.results);
        if present.is_empty() {
            return;
        }
        // A section that is not drawn cannot hold the selection.
        if !present.contains(&state.top) {
            state.top = present[0];
        }
        let at = present.iter().position(|s| *s == state.top).unwrap_or(0);
        // Against the rows the list actually has under the covers, not the
        // whole body: measured against the body, the selection ran under
        // the now-playing bar.
        let tracks_visible = tracklist::visible_rows_of(
            searchview::tracks_height(&state.results, state.top_scroll, height),
            None,
            tracklist::Chrome::Bare,
        );
        let tracks_len = searchview::track_rows(&state.results, searchview::Tab::Top).len();
        let (artists, albums) = (state.results.artists.len(), state.results.albums.len());
        let row = match state.top {
            TopSection::Artists => Some((&mut state.top_artists, artists)),
            TopSection::Albums => Some((&mut state.top_albums, albums)),
            TopSection::Tracks => None,
        };
        let moved = Self::move_in_stack(
            Stacked {
                count: present.len(),
                at,
                tracks: present
                    .iter()
                    .position(|s| *s == TopSection::Tracks)
                    .map(|i| (i, &mut state.tracks, tracks_len, tracks_visible)),
                row: row.map(|(r, len)| (r, len, carousel::cards_to_land_in(width))),
                scroll: &mut state.top_scroll,
                visible: SECTIONS_SHOWING,
            },
            dir,
        );
        state.top = present[moved];
    }

    /// Move through a page of stacked sections: Top results, or an artist's
    /// page. Returns the index among the present sections that holds the
    /// selection afterwards.
    ///
    /// Each card section is a single row, so a vertical move leaves it for
    /// the next. Within the track list j and k move by rows as usual, and
    /// only running off either end steps out of it — without the step down
    /// off the last track, the sections under an album's list could not be
    /// reached at all.
    fn move_in_stack(stack: Stacked<'_>, dir: Dir) -> usize {
        let Stacked {
            count,
            at,
            tracks,
            row,
            scroll,
            visible,
        } = stack;
        let Some(down) = dir.vertical() else {
            // Along a card row. The track list has no second column.
            if let Some((row, len, visible)) = row {
                if len > 0 {
                    row.step(dir.forward(), len, visible);
                }
            }
            return at;
        };
        if let Some((tracks_at, tracks, len, visible)) = tracks {
            if tracks_at == at {
                let off_the_end = if down {
                    tracks.selected + 1 >= len
                } else {
                    tracks.selected == 0
                };
                if !off_the_end {
                    tracks.step(down, len, visible);
                    return at;
                }
            }
        }
        let next = if down {
            (at + 1).min(count.saturating_sub(1))
        } else {
            at.saturating_sub(1)
        };
        cursor::scroll_into_view(scroll, next, visible);
        next
    }

    /// Columns and rows of the grid a search tab draws into: the renderer's
    /// own count, at the lines of text this tab's cards carry. A fixed two
    /// lines put the keys a row out on the artists and playlists tabs.
    fn search_grid_geometry(&self) -> (usize, usize) {
        let tab = searchview::Tab::from_index(self.search.as_ref().map_or(0, |s| s.tab));
        grid::geometry_with(
            self.last_main_width,
            self.search_body_height(),
            searchview::card_lines(tab),
            grid::Chrome::Bare,
            &[],
        )
    }

    /// The rows the search view has left for results, under its own heading,
    /// box and tabs.
    fn search_body_height(&self) -> u16 {
        self.last_main_height
            .saturating_sub(searchview::HEADER_ROWS)
    }
}

pub use crate::catalogue::Favourable;

impl Favourable {
    /// The favourable behind a collection, when it is one: a page of rows
    /// or a home row has no list to be added to.
    pub fn of(collection: Collection) -> Option<Self> {
        match collection {
            Collection::Artist(id) => Some(Favourable::Artist(id)),
            Collection::Album(id) => Some(Favourable::Album(id)),
            Collection::Playlist(uuid) => Some(Favourable::Playlist(uuid)),
            Collection::Radio(_) | Collection::Page { .. } => None,
        }
    }
}

/// When the music stops on its own, if the user asked it to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sleep {
    /// At this moment; `minutes` is the step it was set with, so the key
    /// knows which comes next.
    At {
        when: std::time::Instant,
        minutes: u64,
    },
    EndOfTrack,
}

/// The sleep timer's steps, in minutes. After the last comes the end of
/// the track, then off.
const SLEEP_STEPS: [u64; 4] = [15, 30, 45, 60];

/// What opening a card in a grid loads.
#[derive(Debug, Clone)]
pub enum Collection {
    Playlist(String),
    Album(String),
    /// An artist's own page: their top tracks, albums and similar artists.
    Artist(String),
    /// A station built on a track: what `R` opens, and what autoplay
    /// appends when the queue runs out.
    Radio(crate::domain::TrackId),
    /// Another page of rows: a genre, a mood, a decade. What an Explore
    /// link opens.
    Page {
        title: String,
        path: String,
    },
}

/// A playlist or album the user opened, shown in place of the section the
/// sidebar points at.
///
/// Its title and byline are carried here rather than looked up again: the
/// card the user pressed enter on already had them, and a second lookup
/// could disagree with what they just saw.
#[derive(Debug, Clone)]
pub struct OpenCollection {
    pub title: String,
    pub subtitle: String,
    pub detail: String,
    pub cover: Option<String>,
    /// Whether that cover is an avatar rather than a record sleeve. Carried
    /// from the card the user opened: an artist opens into a moment of the
    /// collection view before their page arrives, and drawing their photo
    /// square there made it flash from round to square and back.
    pub round_cover: bool,
    /// The section to go back to when this is closed.
    pub came_from: sidebar::Section,
    /// What was opened, when the view knows: an album, a playlist, a mix
    /// or an artist. What `F` favourites while the view is up.
    pub target: Option<Collection>,
}

/// A view escape goes back to.
///
/// Opening a collection replaces what the main pane holds, and an artist
/// page can be opened from inside one — a profile, then an artist, then one
/// of their albums. Escape used to drop straight back to the sidebar's
/// section from any depth, because a single `came_from` is all one level
/// remembers. This is the level itself, pushed on the way in.
#[derive(Debug)]
pub struct Level {
    /// Which nav entry the main pane was showing. A change of section is a
    /// step in the history like any other — going back from Albums to
    /// Playlists is what the arrows are for.
    section: sidebar::Section,
    /// The search this view was opened from, if it was. Put back on the way
    /// out, so escape returns to the results rather than to the section
    /// behind them.
    search: Option<SearchState>,
    open: Option<OpenCollection>,
    artist: Option<crate::library::ArtistPage>,
    tracks: Vec<crate::domain::Track>,
    tracklist: tracklist::TrackListState,
    open_cards: Vec<carousel::Card>,
    open_grid: grid::GridState,
    /// The rows the Explore pane held, when a genre page replaced them.
    /// Without keeping them there was no way back to Explore to choose
    /// another — going back left the genre's own rows on screen.
    explore: Option<home::HomeState>,
    /// Which list was up, when one was. The queue and the history are
    /// views of their own: back returns to them, and opening something
    /// from one is a step forward from it.
    showing_queue: bool,
    showing_history: bool,
}

/// What the main pane is drawing, asked once and read by everything that
/// needs to know: the movement keys, the card they act on, and the
/// renderer. See [`App::showing`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Showing {
    /// An artist's page, which stacks card sections over a track list.
    Artist,
    /// The search results, on whichever tab.
    Search(searchview::Tab),
    /// A page of card rows: the home page, Explore, or a genre opened from
    /// it. Told apart from an opened collection by carrying its own heading.
    Rows,
    /// An opened collection that is a list of tracks.
    Tracks,
    /// An opened collection that carries covers, drawn as a grid.
    Cards,
    /// A view opened but not yet filled: the request is still in flight,
    /// and what comes back decides which view this becomes.
    Loading,
    /// What is queued, over whatever view it was opened from.
    Queue,
    /// What has been played, the same way.
    History,
    /// No view over it: whatever the sidebar points at.
    Section(sidebar::Section),
}

#[derive(Debug, Default)]
pub struct App {
    pub should_quit: bool,
    pub status: Option<String>,
    /// When the status was put up, so it can be taken down again.
    ///
    /// A notice that never leaves becomes part of the furniture: it stood
    /// in the corner for the rest of the session, over a tab strip, saying
    /// something that stopped being true minutes ago.
    pub status_since: Option<std::time::Instant>,
    pub login: login::LoginState,
    /// Where an answer to the login prompt goes while a sign-in is running:
    /// the provider waits on the other end for the cookies or the account.
    pub login_input: Option<tokio::sync::mpsc::UnboundedSender<String>>,
    /// The sign-in task under way, to stop when the service is left: a
    /// session it opened afterwards would be the wrong service's.
    pub login_task: Option<tokio::task::AbortHandle>,
    /// The equaliser the engine reads: a preset's gains, or flat and off.
    /// Shared with the engine, so a change reaches the output at once.
    pub equalizer: music::Equalizer,
    /// The sleep timer, when one is set.
    pub sleep: Option<Sleep>,
    /// The service the config names, signed in or not. `None` in a test,
    /// where nothing may sign in or out for real.
    pub provider: Option<std::sync::Arc<dyn crate::catalogue::Provider>>,
    /// The signed-in service, behind sonora's `MusicApi`.
    pub session: Option<std::sync::Arc<crate::catalogue::Session>>,
    pub sidebar: sidebar::SidebarState,
    pub tracklist: tracklist::TrackListState,
    pub tracks: Vec<crate::domain::Track>,
    /// What plays after this track. Playing from a list queues the whole
    /// list, so an album goes on rather than stopping after one track.
    pub queue: crate::playback::Queue,
    /// Whether a radio for the end of the queue is on its way. Asked once:
    /// every position tick sees the same short queue until it lands.
    pub autoplay_fetching: bool,
    /// The last track autoplay could find nothing for, so the same
    /// question is not asked on every tick until the queue changes.
    pub autoplay_dead_end: Option<crate::domain::TrackId>,
    /// The queue ran out before autoplay landed: what it brings has to be
    /// started, since nothing is playing that would move on to it.
    pub ran_dry: bool,
    /// The cards of an opened row that holds albums or playlists rather than
    /// tracks. A row of covers cannot be shown as a track list, so it gets
    /// the card grid instead — the same one the collection views use.
    pub open_cards: Vec<carousel::Card>,
    pub open_grid: grid::GridState,
    /// Whether the opened view is still waiting on the request that fills
    /// it. What is drawn while it waits is a line saying so, rather than
    /// the empty shape of the view that is coming.
    pub awaiting: bool,
    /// The views escape backs out through, outermost first. Empty when the
    /// main pane is showing a sidebar section rather than something opened
    /// from inside one.
    pub back: Vec<Level>,
    /// What going back stepped out of, so it can be stepped into again —
    /// the other half of a browser's two arrows. Cleared when the user
    /// opens something new, since that is a different branch.
    pub forward: Vec<Level>,
    /// The settings, as loaded from the config file and as the Settings
    /// view edits them.
    pub config: crate::config::Config,
    pub settings: settings::SettingsState,
    /// Where the settings are written. `None` in tests, which must not
    /// touch the real file — one that did left this machine on LOW.
    pub config_path: Option<std::path::PathBuf>,
    /// The ids of the user's favourites, kept apart from `tracks` because
    /// opening an album replaces that list — so it cannot say what is a
    /// favourite once the user has gone anywhere else.
    pub favourites: std::collections::HashSet<crate::domain::TrackId>,
    /// The ids of the artists the user follows, so `F` knows which way to
    /// go. Filled from the favourites' id list, and from the Artists
    /// section's reply.
    pub followed: std::collections::HashSet<String>,
    /// The same for albums and playlists.
    pub favourite_albums: std::collections::HashSet<String>,
    pub favourite_playlists: std::collections::HashSet<String>,
    /// The playlist each station opened is, by seed. A radio's view is
    /// keyed by its seed, and the service only names the playlist once
    /// asked.
    pub radio_playlists: std::collections::HashMap<crate::domain::TrackId, String>,
    pub now_playing: nowplaying::NowPlaying,
    pub home: home::HomeState,
    /// Explore's own rows. The same shape as the home page's, drawn by the
    /// same renderer — it is a page of card rows like any other.
    pub explore: home::HomeState,
    /// The service answered its home page with nothing -- Deezer does --
    /// so the Music page is built from the collection as it loads.
    pub home_from_collection: bool,
    /// The service has no Explore page: said on it, rather than blank.
    pub explore_empty: bool,
    /// Whether the home and Explore replies have arrived at all: until
    /// then an empty page is loading, after it the service had nothing.
    pub home_loaded: bool,
    pub explore_loaded: bool,
    /// The activity feed as it came back: releases from the artists the
    /// user follows, newest first and ungrouped. Kept beside the rows so a
    /// filter can be applied without asking for the feed again.
    pub feed_cards: Vec<carousel::Card>,
    /// The same releases in four dated sections, drawn as the home page's
    /// rows are.
    pub feed: home::HomeState,
    /// The artist whose page is open, if one is.
    pub artist: Option<crate::library::ArtistPage>,
    /// Which of that page's sections has the selection, and where in it.
    pub artist_section: usize,
    /// The first section of the artist page that is drawn. Five sections
    /// and a portrait are taller than a terminal, so it scrolls.
    pub artist_scroll: usize,
    /// Whether the artist's blurb is open. TIDAL's run to a page of prose,
    /// so the header shows the first paragraph and `b` opens the rest.
    pub artist_bio_open: bool,
    /// Lines of the open bio scrolled off the top; see `artist_move`.
    pub artist_bio_scroll: u16,
    /// Where the history is kept between runs. Set by `run` alone, so a
    /// test that plays a track writes nothing to the user's cache.
    pub history_file: Option<std::path::PathBuf>,
    pub artist_tracks: tracklist::TrackListState,
    /// One carousel state per card section of an artist's page, indexed by
    /// `Section::index` — a field each meant a new one to remember every
    /// time a section was added, and two of the five went without.
    pub artist_rows: [carousel::CarouselState; artistview::Section::ALL.len()],
    pub playlists: Vec<crate::library::Playlist>,
    pub albums: Vec<crate::library::Album>,
    pub artists: Vec<crate::library::Artist>,
    /// One grid state per grid view, so moving around Albums does not disturb
    /// where the user was in Playlists.
    pub playlist_grid: grid::GridState,
    pub album_grid: grid::GridState,
    pub artist_grid: grid::GridState,
    pub palette: theme::Palette,
    /// `None` in tests and until the terminal has been probed.
    pub artwork: Option<artwork::Artwork>,
    /// Size of the main pane at the last draw. Scrolling needs to know how
    /// many cards or rows fit, which only the renderer can measure.
    pub last_main_width: u16,
    pub last_main_height: u16,
    /// While typing in a view's filter box, keys are text rather than
    /// commands — otherwise "q" quits instead of filtering.
    pub filtering: bool,
    /// The key list, on "?".
    pub showing_help: bool,
    /// How far the key list is scrolled, on a terminal too short for it.
    pub help_scroll: u16,
    /// A playlist edit's question, while it is up. It takes the keys.
    pub prompt: Option<playlistedit::Prompt>,
    /// A text setting being typed into, while its box is up.
    pub setting_edit: Option<settings::Edit>,
    /// The lyrics, on "L".
    pub lyrics: lyricsview::Pane,
    /// Where listens are reported, once the config's services are linked.
    pub scrobblers: Vec<crate::scrobble::Linked>,
    /// The listen in progress: the track playing, as a service files it.
    pub listen: Option<music::scrobble::Play>,
    /// What the loop sends on: the track that started, the listens that
    /// earned a submission.
    pub scrobble_outbox: crate::scrobble::Outbox,
    /// The engine's spectrum, once it has started.
    pub spectrum: Option<crate::playback::Bands>,
    /// The Discord presence thread, when the config names an application,
    /// and what it was last told: the track and whether it plays.
    pub presence: Option<std::sync::mpsc::Sender<crate::presence::Presence>>,
    pub presence_shown: Option<(crate::domain::TrackId, bool)>,
    /// The Widevine module, for a service whose streams are encrypted.
    pub cdm: drmview::Cdm,
    /// The download waiting on the user's word about Google's terms.
    pub cdm_answer: Option<tokio::sync::oneshot::Sender<bool>>,
    /// Whether the queue is open over the pane.
    pub showing_queue: bool,
    /// Where in it the user is. The selection is set to what is playing
    /// each time the view opens, since that is where the eye goes.
    pub queue_list: tracklist::TrackListState,
    /// Whether the history is open over the pane.
    pub showing_history: bool,
    /// Where in it the user is, as for the queue.
    pub history_list: tracklist::TrackListState,
    /// Set when something drew over the artwork and the next frame has to
    /// be painted in full rather than diffed. See `take_repaint`.
    pub needs_repaint: bool,
    /// The search box and its results, on "s".
    ///
    /// Kept apart from `filtering`, which narrows what is already on screen.
    /// Search fetches new content and replaces the pane, so sharing the one
    /// mode would mean a keystroke that sometimes filters and sometimes
    /// fires a request.
    pub search: Option<SearchState>,
    /// The playlist or album being viewed, if the user opened one. While this
    /// is set the main pane shows it rather than the sidebar's section.
    pub open: Option<OpenCollection>,
    /// The user's rebound keys, applied before anything else reads a
    /// keystroke. Empty unless the config names some.
    pub keymap: keymap::Keymap,
}

impl App {
    /// The cards a grid section shows. Built on demand rather than cached:
    /// the lists are small, and a cache would be one more thing to keep in
    /// step with the data behind it.
    pub fn grid_cards(&self, section: sidebar::Section) -> Vec<carousel::Card> {
        if self.open.is_some() && !self.open_cards.is_empty() {
            return self.open_cards.clone();
        }
        match section {
            sidebar::Section::Playlists => {
                self.playlists.iter().map(carousel::playlist_card).collect()
            }
            sidebar::Section::Albums => self.albums.iter().map(carousel::album_card).collect(),
            sidebar::Section::Profiles => self.artists.iter().map(carousel::artist_card).collect(),
            _ => Vec::new(),
        }
    }

    /// The filter text of whichever view is showing.
    pub fn filter_text(&self) -> &str {
        // The Feed keeps its own: it is a page of rows rather than a grid,
        // so neither of the two below is where its filter lives.
        if self.on_feed() {
            &self.feed.filter
        } else if self.on_grid() {
            &self.grid_state(self.sidebar.section()).filter
        } else {
            &self.tracklist.filter
        }
    }

    /// Whether the Feed is what the pane is drawing.
    fn on_feed(&self) -> bool {
        matches!(self.showing(), Showing::Section(sidebar::Section::Feed))
    }

    /// Set it, and pull the selection back into what is left — a filter that
    /// shrinks the list under a stale index selects nothing at all.
    pub fn set_filter(&mut self, text: String) {
        // Rebuilt rather than clamped: the Feed's sections are built from
        // what survives the filter, so the rows themselves change.
        if self.on_feed() {
            self.feed.filter = text;
            self.rebuild_feed();
        } else if self.on_grid() {
            let section = self.sidebar.section();
            let len = grid::filter(&self.grid_cards(section), &text).len();
            let state = self.grid_state_mut(section);
            state.filter = text;
            state.clamp(len);
        } else {
            let len = tracklist::filter(&self.tracks, &text).len();
            self.tracklist.filter = text;
            self.tracklist.clamp(len);
        }
    }

    pub fn grid_state(&self, section: sidebar::Section) -> &grid::GridState {
        // An opened row has its own selection: routing by section would move
        // the cursor in the collection view behind it instead.
        if self.open.is_some() && !self.open_cards.is_empty() {
            return &self.open_grid;
        }
        match section {
            sidebar::Section::Albums => &self.album_grid,
            sidebar::Section::Profiles => &self.artist_grid,
            _ => &self.playlist_grid,
        }
    }

    pub fn grid_state_mut(&mut self, section: sidebar::Section) -> &mut grid::GridState {
        if self.open.is_some() && !self.open_cards.is_empty() {
            return &mut self.open_grid;
        }
        match section {
            sidebar::Section::Albums => &mut self.album_grid,
            sidebar::Section::Profiles => &mut self.artist_grid,
            _ => &mut self.playlist_grid,
        }
    }

    /// What the selected card opens, if it opens anything: a track plays
    /// instead, and a card pointing at nothing opens nothing.
    pub fn selected_collection(&self) -> Option<Collection> {
        match self.selected_card()?.target? {
            carousel::Target::Playlist(uuid) => Some(Collection::Playlist(uuid)),
            carousel::Target::Album(id) => Some(Collection::Album(id)),
            carousel::Target::Artist(id) => Some(Collection::Artist(id)),
            carousel::Target::Page(path) => Some(Collection::Page {
                title: self.selected_card()?.title,
                path,
            }),
            // A track plays rather than opening, and a shortcut into one
            // of the app's own views has nothing to fetch.
            carousel::Target::Track(_) | carousel::Target::Section(_) => None,
        }
    }

    /// The track under the selection: a list's row, or a card that plays a
    /// track rather than opening a collection.
    ///
    /// The row first. Read from the cards alone, the keys that fetch what a
    /// track does not carry -- its radio, its album, its artist -- were dead
    /// on every list: the queue, an album's tracks, a search's.
    pub fn selected_track_id(&self) -> Option<crate::domain::TrackId> {
        if let Some(track) = self.selected_track() {
            return Some(track.id);
        }
        match self.selected_card()?.target? {
            carousel::Target::Track(id) => Some(crate::domain::TrackId(id)),
            _ => None,
        }
    }

    /// The card the selection is on, in whichever card view is showing.
    ///
    /// The home page's carousels and the grids are both rows of cards, so
    /// enter means the same thing in both: open what the card points at.
    pub fn selected_card(&self) -> Option<carousel::Card> {
        // Whatever is drawn is what enter acts on: reading the cards under
        // the queue opened whatever the hidden page had highlighted.
        match self.showing() {
            Showing::Queue | Showing::History => return None,
            Showing::Artist => {
                let page = self.artist.as_ref()?;
                let section = artistview::Section::from_index(self.artist_section);
                if section == artistview::Section::Tracks {
                    return None;
                }
                let cards = artistview::cards(page, section);
                let row = &self.artist_rows[section.index()];
                return cards.get(row.selected).cloned();
            }
            // A search tab that draws cards has one too. It reuses the app's
            // own grid to draw them, so enter has to reach the same card the
            // grid has highlighted — without this it read the collection view
            // behind search and opened nothing.
            Showing::Search(tab) => {
                let state = self.search.as_ref()?;
                // Top stacks an artist row, an album row and a track list, and
                // `top` says which of them holds the selection. Refusing the
                // whole tab left the artists and albums drawn there impossible
                // to open — enter did nothing at all.
                if tab == searchview::Tab::Top {
                    use searchview::TopSection;
                    let (cards, row) = match state.top {
                        TopSection::Artists => (
                            searchview::cards(&state.results, searchview::Tab::Artists),
                            &state.top_artists,
                        ),
                        TopSection::Albums => (
                            searchview::cards(&state.results, searchview::Tab::Albums),
                            &state.top_albums,
                        ),
                        // A track plays rather than opening, and the track list
                        // is read by `selected_track` instead.
                        TopSection::Tracks => return None,
                    };
                    return cards.get(row.selected).cloned();
                }
                if tab.is_tracks() {
                    return None;
                }
                let cards = searchview::cards(&state.results, tab);
                let grid = match tab {
                    searchview::Tab::Albums => &state.albums,
                    searchview::Tab::Artists => &state.artists,
                    _ => &state.playlists,
                };
                return cards.get(grid.selected).cloned();
            }
            _ => {}
        }
        if self.on_home() {
            let row = self.rows_on_screen().current_row()?;
            return row.cards.get(row.state.selected).cloned();
        }
        if self.on_grid() {
            let section = self.sidebar.section();
            let state = self.grid_state(section);
            let cards = self.grid_cards(section);
            let index = grid::filter_indices(&cards, &state.filter)
                .get(state.selected)
                .copied()?;
            return cards.get(index).cloned();
        }
        None
    }

    /// What the selected card is called, for the header of the view it opens.
    ///
    /// Taken from the card the user is looking at rather than fetched again:
    /// they pressed enter on a title, and the view they land on should be
    /// headed by that same title, immediately, without waiting on a request.
    pub fn selected_identity(&self) -> Option<OpenCollection> {
        let target = self.selected_collection();
        let card = self.selected_card()?;
        Some(OpenCollection {
            title: card.title,
            subtitle: card.subtitle,
            detail: card.detail,
            cover: card.cover_url,
            round_cover: card.round,
            came_from: self.sidebar.section(),
            target,
        })
    }

    /// Whether the main pane is the Settings view.
    ///
    /// Read from what is drawn, not from the sidebar alone: an opened
    /// collection or a search covers the section, and the keys belong to
    /// whatever is actually on screen.
    pub fn on_settings(&self) -> bool {
        matches!(self.showing(), Showing::Section(sidebar::Section::Settings))
    }

    /// Whether the search results are what the pane is drawing. Every key
    /// and renderer asks this rather than whether a search exists: one
    /// opened over a page is on top of it, and a page opened from a result
    /// has the search under it on the stack.
    fn on_search(&self) -> bool {
        matches!(self.showing(), Showing::Search(_))
    }

    /// Whether an artist's or album's page is what the pane is drawing.
    fn on_artist(&self) -> bool {
        self.showing() == Showing::Artist
    }

    /// Where a seek lands: a step along the track, short of its end, or
    /// nowhere when nothing is playing. Asked by the bar, which moves at
    /// once, and for the command to the player, so the two cannot disagree.
    pub fn seek_target(&self, forward: bool) -> Option<std::time::Duration> {
        let track = self.now_playing.track.as_ref()?;
        let at = self.now_playing.position;
        Some(if forward {
            // A track of unknown length has no end to stop short of.
            let end = if track.duration.is_zero() {
                std::time::Duration::MAX
            } else {
                track.duration.saturating_sub(std::time::Duration::from_secs(1))
            };
            (at + SEEK_STEP).min(end)
        } else {
            at.saturating_sub(SEEK_STEP)
        })
    }

    /// The Music page from the collection, for a service whose home page
    /// is empty. Rebuilt as each list arrives; a row keeps its place when
    /// it was already there.
    fn refresh_collection_home(&mut self) {
        if !self.home_from_collection {
            return;
        }
        let rows = [
            ("Playlists", self.playlists.iter().map(carousel::playlist_card).collect::<Vec<_>>()),
            ("Favourite albums", self.albums.iter().map(carousel::album_card).collect()),
            ("Artists", self.artists.iter().map(carousel::artist_card).collect()),
        ];
        let mut old = std::mem::take(&mut self.home.rows);
        self.home.rows = rows
            .into_iter()
            .filter(|(_, cards)| !cards.is_empty())
            .map(|(heading, cards)| home::Row {
                heading: heading.into(),
                kind: crate::browse::RowKind::Carousel,
                cards,
                state: old
                    .iter_mut()
                    .find(|r| r.heading == heading)
                    .map(|r| std::mem::take(&mut r.state))
                    .unwrap_or_default(),
            })
            .collect();
    }

    /// The playlist the edit keys act on: the one open, else the one under
    /// the selection in a grid of them.
    fn playlist_in_hand(&self) -> Option<&crate::library::Playlist> {
        let uuid = match self.open.as_ref().and_then(|o| o.target.clone()) {
            Some(Collection::Playlist(uuid)) => uuid,
            Some(_) => return None,
            None => match self.selected_collection()? {
                Collection::Playlist(uuid) => uuid,
                _ => return None,
            },
        };
        self.playlists.iter().find(|p| p.uuid == uuid)
    }

    /// The open playlist's id, when it is the account's own.
    fn open_owned_playlist(&self) -> Option<String> {
        match self.open.as_ref()?.target.as_ref()? {
            Collection::Playlist(uuid)
                if self.playlists.iter().any(|p| &p.uuid == uuid && p.owned) =>
            {
                Some(uuid.clone())
            }
            _ => None,
        }
    }

    /// Write the config where it was read from. Nowhere in a test, which
    /// has no file.
    fn save_config(&mut self) {
        if let Some(path) = self.config_path.as_ref() {
            if let Err(e) = self.config.save_to(path) {
                tracing::warn!("the settings could not be saved: {e}");
                self.say(format!("could not save settings: {e}"));
            }
        }
    }

    /// Step the highlighted setting and write the file.
    ///
    /// Saved on the keystroke rather than on leaving the view: there is no
    /// "apply", so a setting that only lived in memory would be silently
    /// lost on quit. A write that fails says so in the status line instead
    /// of leaving the user to find out next launch.
    fn change_setting(&mut self, forward: bool) -> Option<Action> {
        let which = self.settings.current();
        settings::cycle(&mut self.config, which, forward);
        self.save_config();
        // The volume is heard now rather than on the next track: a level
        // you cannot hear yourself setting is one you set by guessing.
        match which {
            settings::Setting::Service => {
                self.say(format!("Enter to switch to {}", self.config.provider.kind.label()));
                None
            }
            settings::Setting::Volume => Some(Action::SetVolume(self.config.audio.volume)),
            // Heard now: the engine keeps reading the shared equaliser.
            settings::Setting::Equalizer => {
                self.apply_equalizer();
                None
            }
            // Read once when the engine starts, so it starts again.
            settings::Setting::Normalisation | settings::Setting::Gapless => {
                Some(Action::RestartEngine)
            }
            // The icons are already swapped -- `cycle` does that, so the row
            // shows the new set as it is chosen -- and the rest of the
            // screen picks them up on the next frame.
            settings::Setting::Quality
            | settings::Setting::Autoplay
            | settings::Setting::Explicit
            | settings::Setting::Ai
            | settings::Setting::NerdFont => None,
            settings::Setting::Theme | settings::Setting::Librefm => {
                Some(Action::SettingChanged(which))
            }
            // Typed into or acted on with Enter: h and l did nothing.
            settings::Setting::LastfmKey
            | settings::Setting::LastfmSecret
            | settings::Setting::LastfmLink
            | settings::Setting::ListenbrainzToken
            | settings::Setting::MalojaUrl
            | settings::Setting::MalojaKey
            | settings::Setting::Discord
            | settings::Setting::SubsonicServer
            | settings::Setting::SubsonicUser
            | settings::Setting::LocalFolders => None,
        }
    }

    /// Drop everything that belonged to the service -- the session, the
    /// lists, the queue, the view -- and stand ready to sign in to the one
    /// the config now names. What is the app's rather than the service's
    /// stays: the config, the keys, the palette, the artwork cache, the
    /// window size, the history, the equaliser, Discord and the scrobblers.
    fn leave_service(&mut self) {
        if let Some(task) = self.login_task.take() {
            task.abort();
        }
        let kept = std::mem::take(self);
        let history: Vec<crate::domain::Track> = kept.queue.history().cloned().collect();
        *self = App {
            config: kept.config,
            config_path: kept.config_path,
            keymap: kept.keymap,
            palette: kept.palette,
            artwork: kept.artwork,
            last_main_width: kept.last_main_width,
            last_main_height: kept.last_main_height,
            history_file: kept.history_file,
            equalizer: kept.equalizer,
            presence: kept.presence,
            scrobblers: kept.scrobblers,
            settings: kept.settings,
            sidebar: kept.sidebar,
            needs_repaint: true,
            ..App::default()
        };
        self.queue.restore_history(history);
        match crate::catalogue::provider_for(&self.config) {
            Ok(provider) => self.provider = Some(provider),
            Err(error) => {
                tracing::error!("could not set up the provider: {error:#}");
                self.say(format!("could not set up the provider: {error:#}"));
            }
        }
        self.login = self.login_prompt();
    }

    /// What the engine is started with, from the settings.
    fn playback_options(&self) -> crate::playback::Options {
        crate::playback::Options {
            normalisation: self.config.audio.normalisation,
            gapless: self.config.audio.gapless,
            equalizer: self.equalizer.clone(),
        }
    }

    /// Put the config's preset into the equaliser the engine reads. `off`
    /// turns it off and leaves the gains flat, so turning it back on is
    /// not a surprise.
    pub fn apply_equalizer(&self) {
        match settings::equalizer_preset(&self.config) {
            Some(preset) => {
                self.equalizer.set_gains(&preset.gains());
                self.equalizer.set_enabled(true);
            }
            None => {
                self.equalizer.set_enabled(false);
                self.equalizer.set_gains(&music::equalizer::Preset::Flat.gains());
            }
        }
    }

    /// The sleep timer's next step, said out loud.
    fn cycle_sleep(&mut self) {
        let now = std::time::Instant::now();
        self.sleep = match self.sleep {
            None => Some(Sleep::At {
                when: now + std::time::Duration::from_secs(SLEEP_STEPS[0] * 60),
                minutes: SLEEP_STEPS[0],
            }),
            Some(Sleep::At { minutes, .. }) => match SLEEP_STEPS.iter().find(|m| **m > minutes) {
                Some(next) => Some(Sleep::At {
                    when: now + std::time::Duration::from_secs(next * 60),
                    minutes: *next,
                }),
                None => Some(Sleep::EndOfTrack),
            },
            Some(Sleep::EndOfTrack) => None,
        };
        let said = match self.sleep {
            Some(Sleep::At { minutes, .. }) => format!("sleep in {minutes} minutes"),
            Some(Sleep::EndOfTrack) => "sleep at the end of the track".to_string(),
            None => "sleep timer off".to_string(),
        };
        self.say(said);
    }

    /// What the bar says about the sleep timer.
    fn sleep_mark(&self) -> Option<nowplaying::SleepMark> {
        match self.sleep? {
            Sleep::At { when, .. } => {
                let left = when.saturating_duration_since(std::time::Instant::now());
                Some(nowplaying::SleepMark::Minutes(left.as_secs().div_ceil(60)))
            }
            Sleep::EndOfTrack => Some(nowplaying::SleepMark::EndOfTrack),
        }
    }

    /// The search the lyrics pane is waiting on, if it is open on a track
    /// nothing has been found for yet. Marked as searching here, so the
    /// same track is not asked about on every event.
    fn lyrics_wanted(&mut self) -> Option<Action> {
        if !self.lyrics.open {
            return None;
        }
        let id = self.now_playing.track.as_ref()?.id.clone();
        if !self.lyrics.wants(&id) {
            return None;
        }
        self.lyrics.found.insert(id.clone(), lyricsview::Found::Searching);
        Some(Action::LoadLyrics(id))
    }

    /// Tell Discord what changed, if anything did: the track, or whether
    /// it plays. Positions are not watched -- the clock runs on its own
    /// over there -- except that a seek starts it again from where it
    /// landed, which `presence_shown` is cleared for.
    fn tell_presence(&mut self) {
        let Some(sender) = &self.presence else {
            return;
        };
        let key = self
            .now_playing
            .track
            .as_ref()
            .map(|t| (t.id.clone(), self.now_playing.playing));
        if key == self.presence_shown {
            return;
        }
        self.presence_shown = key;
        let shown = match &self.now_playing.track {
            None => crate::presence::Presence::Off,
            Some(t) if self.now_playing.playing => crate::presence::Presence::Playing {
                title: t.title.clone(),
                artist: t.artist.clone(),
                album: t.album.clone(),
                cover: t.cover.clone(),
                position: self.now_playing.position,
                duration: t.duration,
            },
            Some(t) => crate::presence::Presence::Paused {
                title: t.title.clone(),
                artist: t.artist.clone(),
            },
        };
        let _ = sender.send(shown);
    }

    /// The listen that just ended goes to the outbox if `played` earned it
    /// a submission -- the services' rule, not ours.
    fn settle_listen(&mut self, played: Duration) {
        if let Some(play) = self.listen.take() {
            if play.earned(played) {
                self.scrobble_outbox.finished.push(play);
            }
        }
    }

    /// The track that just started is the listen in progress, and what
    /// the services show as playing now.
    fn begin_listen(&mut self) {
        self.listen = self
            .now_playing
            .track
            .as_ref()
            .and_then(|t| crate::scrobble::play_of(t, std::time::SystemTime::now()));
        self.scrobble_outbox.now_playing = self.listen.clone();
    }

    /// The sleep timer going off: pause, once, and forget it.
    fn sleep_due(&mut self) -> Option<Action> {
        match self.sleep {
            Some(Sleep::At { when, .. }) if std::time::Instant::now() >= when => {
                self.sleep = None;
                self.now_playing.playing.then_some(Action::TogglePause)
            }
            _ => None,
        }
    }

    /// Put a message in the corner, and start its clock.
    ///
    /// Set through here rather than by assignment so nothing can put up a
    /// notice that has no way of coming down.
    pub fn say(&mut self, message: impl Into<String>) {
        self.status = Some(message.into());
        self.status_since = Some(std::time::Instant::now());
    }

    /// Take the message down once it has been up long enough.
    ///
    /// Long enough to read twice at a glance, and short enough that it is
    /// gone before it is furniture. Nothing depends on the message staying:
    /// what it reports has already happened, and the log keeps it.
    fn expire_status(&mut self) {
        const SHOWN_FOR: std::time::Duration = std::time::Duration::from_secs(6);
        if self
            .status_since
            .is_some_and(|at| at.elapsed() >= SHOWN_FOR)
        {
            self.status = None;
            self.status_since = None;
        }
    }

    /// The radio to fetch now, so the queue is never caught empty.
    ///
    /// Only with autoplay on, only when the queue is nearly out, only once
    /// per fetch, and never when the queue repeats -- it has no end to
    /// follow on from. Seeded from the last track queued rather than the
    /// one playing: what follows the end of the queue is what the end of
    /// the queue sounds like. TIDAL's own "continue with similar content"
    /// does the same.
    fn autoplay_seed(&mut self) -> Option<Action> {
        /// How few tracks left is nearly out. Two: the fetch is one or two
        /// requests and a track is minutes long, and a one-track radio
        /// reply still leaves time to ask for the next.
        const LOOK_AHEAD: usize = 2;

        if !self.config.playback.autoplay
            || self.autoplay_fetching
            || self.queue.repeat == crate::playback::Repeat::All
            || self.queue.remaining() > LOOK_AHEAD
        {
            return None;
        }
        let last = self.queue.last_track()?;
        if self.autoplay_dead_end.as_ref() == Some(&last.id) {
            return None;
        }
        self.autoplay_fetching = true;
        Some(Action::Autoplay {
            seed: last.id.clone(),
        })
    }

    /// Why this track will not play, or `None` when it will.
    ///
    /// Read rather than acted on, so a test can ask the question the player
    /// asks without a session or a network.
    pub fn why_blocked(&self, track: &crate::domain::Track) -> Option<&'static str> {
        if track.allowed(self.config.playback.explicit, self.config.playback.ai) {
            return None;
        }
        Some(if track.explicit && !self.config.playback.explicit {
            "explicit content is turned off in Settings"
        } else {
            "AI-generated content is turned off in Settings"
        })
    }

    /// Move the queue on past tracks the user does not allow, saying why
    /// each was passed over. True when it rests on a track that may play.
    ///
    /// A refused start never finishes, so nothing moved the queue on from
    /// it: one AI-generated track in a radio stopped the whole evening.
    /// Every path that lands the cursor somewhere steps through here. The
    /// walk is bounded by the queue's length, since with repeat on and
    /// nothing allowed `next` never runs out.
    fn skip_blocked(&mut self, forward: bool) -> bool {
        for _ in 0..=self.queue.len() {
            let Some(track) = self.queue.current() else {
                return false;
            };
            let Some(why) = self.why_blocked(track) else {
                return true;
            };
            let said = format!("{} — {why}", track.title);
            tracing::info!("skipping {:?}: {why}", track.title);
            self.say(said);
            let moved = if forward {
                self.queue.next()
            } else {
                self.queue.previous()
            };
            if moved.is_none() {
                return false;
            }
        }
        false
    }

    /// Whether the next frame must be drawn in full, clearing the flag.
    ///
    /// The image protocols paint straight to the terminal and mark their
    /// cells `Skip` so ratatui leaves them alone. Anything drawn over them
    /// — the help modal — is therefore not undone by the next frame, which
    /// marks the same cells `Skip` again and writes nothing. A full repaint
    /// is the only thing that puts the artwork back.
    pub fn take_repaint(&mut self) -> bool {
        std::mem::take(&mut self.needs_repaint)
    }

    /// How many rows the queue or the history shows, for the keys to
    /// scroll by. The same count the renderer lays out.
    fn over_list_visible(&self) -> usize {
        queueview::visible_rows(self.last_main_height)
    }

    /// How many `j` presses make half a screen in whatever is showing.
    ///
    /// Each view counts its own rows: a grid moves a line of cards per
    /// press, the home page a whole row of covers, a list a row. Never
    /// less than one, so the key always does something.
    fn half_page(&self) -> usize {
        let rows = if matches!(self.showing(), Showing::Queue | Showing::History) {
            self.over_list_visible()
        } else if self.on_home() {
            self.home_rows_to_land_in()
        } else if self.on_grid() {
            self.grid_geometry().1
        } else {
            self.list_visible()
        };
        (rows / 2).max(1)
    }

    /// The track `R` builds a station on: the selected one, or else what
    /// is playing.
    ///
    /// The fall-back is what keeps the obvious case working: on the home
    /// page or in Settings nothing is selected, and continuing from what is
    /// in your ears is then the only thing the key could mean.
    pub fn track_radio(&self) -> Option<crate::domain::TrackId> {
        self.selected_track_id()
            .or_else(|| self.now_playing.track.as_ref().map(|t| t.id.clone()))
    }

    /// Whether the main pane is already the station on this seed.
    ///
    /// A radio lists its seed first, so with the playing track selected R
    /// over it opened the same radio over itself, reloading the list and
    /// losing the place in it.
    fn radio_is_open(&self, seed: &crate::domain::TrackId) -> bool {
        matches!(
            self.open.as_ref().and_then(|o| o.target.as_ref()),
            Some(Collection::Radio(open)) if open == seed
        )
    }

    /// What the radio key would be named after, for the heading.
    fn track_radio_title(&self) -> String {
        self.selected_track()
            .or_else(|| self.now_playing.track.clone())
            .map(|t| t.title)
            .unwrap_or_default()
    }

    /// The artist `F` acts on: the page that is open, whatever section is
    /// selected in it, or else the artist card under the selection. The
    /// page wins because the selection inside it is a track or a record,
    /// never the artist themself.
    pub fn selected_favourable(&self) -> Option<(Favourable, String)> {
        if let Some(page) = self.artist.as_ref() {
            let what = match page.kind {
                crate::library::PageKind::Artist => Favourable::Artist(page.id.clone()),
                crate::library::PageKind::Album => Favourable::Album(page.id.clone()),
            };
            return Some((what, page.name.clone()));
        }
        if let Some(card) = self.selected_card() {
            if let Some(what) = self.selected_collection().and_then(Favourable::of) {
                return Some((what, card.title));
            }
        }
        // The view itself: an album opened from a card, its tracks on
        // screen, is what someone pressing F there means.
        let open = self.open.as_ref()?;
        Some((self.open_favourable()?, open.title.clone()))
    }

    /// What the open view is as a favourite. A radio is the playlist its
    /// station is, once the service has named it.
    fn open_favourable(&self) -> Option<Favourable> {
        match self.open.as_ref()?.target.clone()? {
            Collection::Radio(seed) => {
                Some(Favourable::Playlist(self.radio_playlists.get(&seed)?.clone()))
            }
            target => Favourable::of(target),
        }
    }

    /// Whether the account has this already, as far as the app knows.
    pub fn is_favourite(&self, what: &Favourable) -> bool {
        match what {
            Favourable::Artist(id) => self.followed.contains(id),
            Favourable::Album(id) => self.favourite_albums.contains(id),
            Favourable::Playlist(uuid) => self.favourite_playlists.contains(uuid),
        }
    }

    /// What pressing enter would go and fetch, and under what heading.
    ///
    /// The loop asked this in pieces — a guard, then the selected card,
    /// then the collection — and a test that filled the reply in by hand
    /// could pass while the fetch never started. It is one question now, so
    /// a test can ask exactly what the loop asks.
    pub fn what_enter_opens(&self) -> Option<(Collection, String)> {
        if !(self.on_grid() || self.on_home()) {
            return None;
        }
        // The heading comes from the selected card rather than `open`: the
        // loop asks before `update` runs, so `open` is still whatever was
        // open before — the previous album, or nothing.
        let for_title = self.selected_card().map(|c| c.title).unwrap_or_default();
        Some((self.selected_collection()?, for_title))
    }

    /// The album of the selected track, and what to head it with.
    ///
    /// The track list is where this matters: a grid of covers is already
    /// showing albums, but a list of tracks names one it cannot open.
    pub fn selected_track_album(&self) -> Option<(Collection, String)> {
        self.selected_track().and_then(|t| album_of(&t))
    }

    /// The artist of the selected track, the same way.
    pub fn selected_track_artist(&self) -> Option<(Collection, String)> {
        self.selected_track().and_then(|t| artist_of(&t))
    }

    /// How many home rows the selection may land in without scrolling.
    ///
    /// The count the renderer draws, less the last one when it is cut off
    /// by the pane's edge: landing there leaves the selection half drawn
    /// and the page looking stuck, since the row it moved to is the row
    /// already at the bottom. Scrolling it up into the whole part is what
    /// the web client does, and what the card grids already did.
    fn home_rows_to_land_in(&self) -> usize {
        // The rows themselves, so the count matches what the renderer
        // draws: a track grid is taller than a carousel.
        home::rows_to_land_in(
            self.last_main_height,
            scrollbar::content_width(self.last_main_width),
            self.rows_on_screen(),
        )
    }

    /// The rows the track list's selection may land in, under whatever
    /// heads it. With a banner the header is nine rows taller, so the
    /// no-banner count let the selection run three rows below the last
    /// drawn one — off the bottom, cursor gone.
    fn list_visible(&self) -> usize {
        tracklist::visible_rows_of(
            self.last_main_height,
            self.open
                .as_ref()
                .map(|o| o.cover.is_some() || o.round_cover),
            tracklist::Chrome::Full,
        )
    }

    /// The tracks the list shows once the filter box has had its say. The
    /// keys moved through the unfiltered list, so with a filter typed the
    /// selection could walk onto tracks that were not on screen.
    fn list_len(&self) -> usize {
        tracklist::filter(&self.tracks, &self.tracklist.filter).len()
    }

    /// The cards a home row's selection may land on: the renderer draws the
    /// rows beside the scrollbar's column, so the keys count there too.
    fn home_cards_to_land_in(&self) -> usize {
        carousel::cards_to_land_in(scrollbar::content_width(self.last_main_width))
    }

    /// The columns a home page's track grid draws, counted the same way.
    fn home_grid_columns(&self) -> usize {
        trackgrid::columns(scrollbar::content_width(self.last_main_width))
    }

    /// As [`list_len`], for the grid of the section showing.
    fn grid_len(&self) -> usize {
        let section = self.sidebar.section();
        grid::filter(&self.grid_cards(section), &self.grid_state(section).filter).len()
    }

    /// Move the selection of the grid showing: a whole row on j/k, one card
    /// on h/l.
    fn grid_move(&mut self, dir: Dir) {
        let (cols, rows) = self.grid_geometry();
        let len = self.grid_len();
        self.grid_state_mut(self.sidebar.section()).step(
            dir.forward(),
            dir.vertical().is_some(),
            len,
            cols,
            rows,
        );
    }

    /// Move the selection of the track list showing.
    fn list_move(&mut self, down: bool) {
        let (len, visible) = (self.list_len(), self.list_visible());
        self.tracklist.step(down, len, visible);
    }

    /// `j` and `k`: whichever view is drawn gets the move, asked of
    /// [`Self::showing`] so a view under another never takes the keys.
    fn move_vertical(&mut self, dir: Dir) {
        let down = dir == Dir::Down;
        match self.showing() {
            Showing::Section(sidebar::Section::Settings) => {
                if down {
                    self.settings.next();
                } else {
                    self.settings.previous();
                }
            }
            Showing::Search(_) => self.search_move(dir),
            Showing::Artist => self.artist_move(dir),
            _ if self.on_grid() => self.grid_move(dir),
            _ => self.list_move(down),
        }
    }

    /// `h` and `l`, the same way: a setting's value, a grid's card, or a
    /// home row's card.
    fn move_horizontal(&mut self, dir: Dir) -> Option<Action> {
        let right = dir == Dir::Right;
        if self.on_settings() {
            return self.change_setting(right);
        }
        if self.on_grid() {
            self.grid_move(dir);
            return None;
        }
        let visible = self.home_cards_to_land_in();
        if let Some(row) = self.rows_on_screen_mut().current_row_mut() {
            let len = row.cards.len();
            row.state.step(right, len, visible);
        }
        None
    }

    /// How far back the arrows go.
    ///
    /// Every move through the nav is a step, so a session spent walking the
    /// sidebar would otherwise grow this without end. Fifty is further back
    /// than anyone retraces by hand, and the oldest is what is dropped.
    const HISTORY: usize = 50;

    /// Put a level on the back stack, dropping the oldest past `HISTORY`.
    fn remember(&mut self, level: Level) {
        self.back.push(level);
        if self.back.len() > Self::HISTORY {
            self.back.remove(0);
        }
    }

    /// What the main pane holds right now, taken out of it.
    fn take_level(&mut self) -> Level {
        // A list that was up goes onto the level, so back brings it back.
        // Its rows drew covers into cells the image protocols mark `Skip`,
        // which the next frame writes nothing over: a full paint.
        if self.showing_queue || self.showing_history {
            self.needs_repaint = true;
        }
        // Read before anything is taken: the fields below are moved out as
        // the struct is built, so asking afterwards always says "nothing
        // was open".
        let over_a_section = self.open.is_some() || self.artist.is_some();
        Level {
            section: self.sidebar.section(),
            showing_queue: std::mem::take(&mut self.showing_queue),
            showing_history: std::mem::take(&mut self.showing_history),
            // Taken, not cloned: a search left open would draw over the
            // view being opened, which is what made the Albums and Profils
            // tabs look as though enter did nothing at all.
            search: self.search.take(),
            open: self.open.take(),
            artist: self.artist.take(),
            // An opened collection's tracks go with it, but the Tracks
            // section's own favourites do not: they are the section's
            // contents, fetched once at startup, and taking them left the
            // view empty with nothing to fetch them again -- the same
            // reason Explore's own rows stay behind.
            tracks: if over_a_section {
                std::mem::take(&mut self.tracks)
            } else {
                Vec::new()
            },
            tracklist: std::mem::take(&mut self.tracklist),
            open_cards: std::mem::take(&mut self.open_cards),
            open_grid: std::mem::take(&mut self.open_grid),
            // A genre page IS the Explore pane's contents rather than
            // something drawn over it, so leaving one has to carry it or
            // there is nothing for forward to come back to. Explore's own
            // list is not taken: it is the section's contents, and emptying
            // it here made arriving back on Explore fetch the page it
            // already had.
            explore: self
                .explore
                .heading
                .is_some()
                .then(|| std::mem::take(&mut self.explore)),
        }
    }

    /// Put a level back into the main pane.
    fn restore_level(&mut self, level: Level) {
        self.sidebar.select(level.section);
        self.search = level.search;
        self.open = level.open;
        self.artist = level.artist;
        self.tracks = level.tracks;
        self.tracklist = level.tracklist;
        self.open_cards = level.open_cards;
        self.open_grid = level.open_grid;
        self.showing_queue = level.showing_queue;
        self.showing_history = level.showing_history;
        // Back onto a list: its rows go over covers the view being left
        // drew, in cells the diff never writes.
        if self.showing_queue || self.showing_history {
            self.needs_repaint = true;
        }
        if let Some(explore) = level.explore {
            self.explore = explore;
        }
    }

    /// Step back one view, returning whether there was one to step to.
    ///
    /// What escape does, and the back arrow: the view stepped out of goes
    /// on the forward stack so it can be stepped into again.
    fn go_back(&mut self) -> bool {
        let Some(level) = self.back.pop() else {
            return false;
        };
        let leaving = self.take_level();
        self.forward.push(leaving);
        self.restore_level(level);
        true
    }

    /// Step into the view that going back stepped out of.
    fn go_forward(&mut self) -> bool {
        let Some(level) = self.forward.pop() else {
            return false;
        };
        let leaving = self.take_level();
        self.remember(leaving);
        self.restore_level(level);
        true
    }

    /// Move to the next or previous nav entry, closing whatever is open
    /// over it.
    ///
    /// Without the closing, J and K moved the nav and the opened view
    /// stayed on top — the keys looked like they did nothing at all. The
    /// view goes on the history, so back returns to it.
    fn go_to_section(&mut self, down: bool) {
        self.leave_for_a_new_place();
        if down {
            self.sidebar.next();
        } else {
            self.sidebar.previous();
        }
    }

    /// Record where the user is and clear the pane, ready for somewhere
    /// else.
    ///
    /// The clearing is what taking the level does: an opened album left on
    /// top of a change of section made J and K look like they did nothing.
    /// Going somewhere new is a branch, so what was ahead is dropped.
    fn leave_for_a_new_place(&mut self) {
        let level = self.take_level();
        self.remember(level);
        self.forward.clear();
    }

    /// Take the pane for a view opened from inside another, and remember
    /// what it replaced.
    ///
    /// Every "open something" did this by hand, and the four copies had
    /// drifted: one forgot to push a level at all, so back and escape had
    /// nothing to pop and the view it opened could not be left; one cleared
    /// the artist page it covered and two did not; one reset the track list
    /// by its selection alone, leaving the filter and the offset behind.
    /// One place now, so a view opened from anywhere leaves the same state
    /// behind it.
    ///
    /// `cards` is the covers an opened row carries, empty for a list of
    /// tracks -- which is also what says whether the pane draws a grid.
    fn open_view(&mut self, identity: Option<OpenCollection>, cards: Vec<carousel::Card>) {
        // Read before the level is put away: `push_level` takes the cards
        // with it, and an identity read off the card underneath is gone by
        // then. Callers hand it in for the same reason.
        self.push_level();
        self.open = identity;
        // Nothing has arrived yet. Without this the pane drew whatever the
        // opened view would eventually be -- an empty track list, columns
        // and filter box and all -- for as long as the request took, which
        // read as a broken view rather than as one still loading. Cards
        // handed in here are already the content, so those are not waiting.
        self.awaiting = cards.is_empty();
        self.open_cards = cards;
        self.open_grid = grid::GridState::default();

        // A page from the last artist would show under the new one's name
        // until its own reply arrived.
        self.artist = None;
        self.artist_section = 0;
        self.artist_scroll = 0;
        self.artist_bio_open = false;
        self.artist_bio_scroll = 0;
        self.artist_tracks = tracklist::TrackListState::default();
        self.artist_rows = Default::default();

        // The opened view is what the user is now looking at, so it gets
        // the keys. Leaving focus on the view behind it made the new one
        // impossible to move around in.
        self.tracks.clear();
        self.tracklist = tracklist::TrackListState::default();
    }

    /// The heading-only identity most opens carry: a title and nothing else.
    /// Take the pane for a collection a key asked for, under its name.
    ///
    /// An artist opens into a moment of the collection view before their
    /// page arrives, so their cover -- when there is one -- is round there.
    fn open_for(&mut self, round: bool, title: String, subtitle: String, cover: Option<String>) {
        let mut identity = self.heading_only(title);
        if let Some(open) = identity.as_mut() {
            open.subtitle = subtitle;
            open.cover = cover;
            open.round_cover = round;
        }
        self.open_view(identity, Vec::new());
    }

    /// Whether `title` is the view already on screen, filled.
    ///
    /// `c` on a track inside its own album opened the album again, and
    /// each press stacked a level: seven presses on a single were eight
    /// steps back out. A key that would open what is showing does nothing.
    fn already_showing(&self, title: &str) -> bool {
        (!self.awaiting && self.open.as_ref().is_some_and(|o| o.title == title))
            || self.artist.as_ref().is_some_and(|p| p.name == title)
    }

    fn heading_only(&self, title: String) -> Option<OpenCollection> {
        Some(OpenCollection {
            title,
            subtitle: String::new(),
            detail: String::new(),
            cover: None,
            round_cover: false,
            came_from: self.sidebar.section(),
            target: None,
        })
    }

    /// As [`heading_only`], with the artwork of whatever it was opened from.
    ///
    /// A radio is a mix, and a mix has a cover -- checked against a real
    /// response. Opened by a key rather than by enter, it had nowhere to
    /// take one from and the page came up with a blank square where every
    /// other opened view has a picture.
    ///
    /// The seed goes in as the target, which is what the loop fetches.
    fn heading_with_cover(
        &self,
        seed: crate::domain::TrackId,
        title: String,
        cover: Option<String>,
        round: bool,
    ) -> Option<OpenCollection> {
        Some(OpenCollection {
            title,
            subtitle: String::new(),
            detail: String::new(),
            cover,
            round_cover: round,
            came_from: self.sidebar.section(),
            target: Some(Collection::Radio(seed)),
        })
    }

    /// Remember what the main pane holds, so escape can put it back.
    ///
    /// The opened collection stays behind as well as going onto the stack:
    /// what is being opened is read off the card of the view above it, and
    /// that view's own identity is what heads it until its reply lands.
    fn push_level(&mut self) {
        let open = self.open.clone();
        let level = self.take_level();
        self.remember(level);
        self.open = open;
        // A new branch: what was ahead is no longer reachable, the same as
        // following a link after going back in a browser.
        self.forward.clear();
    }

    /// Back to the sidebar's section: nothing opened, nothing to go back to.
    fn clear_open(&mut self) {
        if let Some(open) = self.open.take() {
            self.sidebar.select(open.came_from);
        }
        self.tracks.clear();
        self.tracklist = tracklist::TrackListState::default();
        // An opened row's cards go with it, or the next view drawn would
        // show the last row's contents.
        self.open_cards.clear();
        self.open_grid = grid::GridState::default();
        self.artist = None;
    }

    /// Columns and visible rows of the current grid, measured from the pane
    /// the renderer last drew into.
    ///
    /// The same count, and the same rows of chrome above the cards, that the
    /// renderer draws with. Keeping a second copy of this arithmetic let the
    /// two disagree, and the keys reached cards that were never drawn.
    fn grid_geometry(&self) -> (usize, usize) {
        grid::geometry_with(
            self.last_main_width,
            self.last_main_height,
            self.grid_lines(self.sidebar.section()),
            grid::Chrome::Full,
            &[],
        )
    }

    /// Lines of text under a section's covers, which decides how tall a
    /// card is. One place, so the renderer and the keys cannot disagree.
    fn grid_lines(&self, section: sidebar::Section) -> u16 {
        // The rail under an opened album is records too.
        if self.open.is_some() && !self.open_cards.is_empty() {
            return 3;
        }
        match section {
            // Three: the artist or the creator, then the year or the count.
            sidebar::Section::Playlists | sidebar::Section::Albums => 3,
            sidebar::Section::Feed => 2,
            _ => 1,
        }
    }

    /// What the main pane is actually drawing.
    ///
    /// Three places used to work this out for themselves -- the movement
    /// keys, the card the keys act on, and the renderer -- each with its own
    /// chain of `if`s in the same order. They agreed until they did not: a
    /// genre page set `open`, which the renderer read as an opened
    /// collection and drew as an empty track list, while the keys still
    /// thought they were on a page of rows. Asked once here, the three
    /// cannot drift apart again.
    ///
    /// The order is what is drawn over what: an artist page and a search
    /// cover whatever is behind them, an opened collection covers the
    /// section, and the section is what is left.
    pub(super) fn showing(&self) -> Showing {
        // The box opened over a page is on top of it: `s` on an album put
        // the box under the album, which stayed drawn and kept the keys.
        // A page opened from a result is the other way round, and the
        // stack keeps them apart then: the search goes onto the level.
        if let Some(state) = self.search.as_ref() {
            return Showing::Search(searchview::Tab::from_index(state.tab));
        }
        if self.artist.is_some() {
            return Showing::Artist;
        }
        // A list is a level of its own: opening anything from it puts it on
        // the stack and takes it off the screen, so while it shows it is
        // the top -- only the search box, which is not a level, opens over
        // it, and closes back onto it.
        if self.showing_queue {
            return Showing::Queue;
        }
        if self.showing_history {
            return Showing::History;
        }
        // A page of rows opened from Explore: drawn by the rows renderer
        // under its own heading, and driven by the keys as the page of rows
        // it is. It carries no `open` -- setting one handed the pane to the
        // collection view, which drew an empty track list.
        if self.explore.heading.is_some() && self.sidebar.section() == sidebar::Section::Explore {
            return Showing::Rows;
        }
        if self.open.is_some() {
            // Still waiting on the request that fills it. What arrives
            // decides which view this becomes -- a genre is a page of rows,
            // an album a list of tracks -- so until it does, neither shape
            // is drawn. The empty one used to be, columns and filter box
            // and all, which read as a broken view rather than a loading
            // one.
            // Cards already in hand are the content, whatever the flag
            // says: a view that has something to draw is not waiting.
            if self.awaiting && self.open_cards.is_empty() && self.tracks.is_empty() {
                return Showing::Loading;
            }
            // An opened collection is a list of tracks, or a grid when it
            // carries covers.
            return if self.open_cards.is_empty() {
                Showing::Tracks
            } else {
                Showing::Cards
            };
        }
        Showing::Section(self.sidebar.section())
    }

    /// Whether the main pane is currently a card grid, which decides what the
    /// movement keys mean: a grid moves by rows, a table by lines.
    pub fn on_grid(&self) -> bool {
        // Whatever the sidebar still points at, what is drawn wins. Reading
        // the section alone left j and k driving the view behind the one on
        // screen — a selection nobody could see. Twice: once for an opened
        // album, once for search.
        match self.showing() {
            // An artist page's card sections are grids; its track list is not.
            Showing::Artist => {
                artistview::Section::from_index(self.artist_section) != artistview::Section::Tracks
            }
            // Top is a grid while the selection is on one of its card rows
            // and a list while it is in the tracks — the keys follow what is
            // highlighted, not what the tab is called.
            Showing::Search(searchview::Tab::Top) => self
                .search
                .as_ref()
                .is_some_and(|s| s.top != searchview::TopSection::Tracks),
            Showing::Search(tab) => !tab.is_tracks(),
            Showing::Cards => true,
            // Nothing to move around in until the reply lands. The queue
            // is a list, and drives its own cursor.
            Showing::Tracks
            | Showing::Rows
            | Showing::Loading
            | Showing::Queue
            | Showing::History => false,
            Showing::Section(section) => matches!(
                section,
                sidebar::Section::Playlists
                    | sidebar::Section::Albums
                    | sidebar::Section::Profiles
            ),
        }
    }

    /// Apply one action. Returns a follow-up action when one is implied.
    pub fn update(&mut self, action: Action) -> Option<Action> {
        match action {
            Action::Quit => {
                self.should_quit = true;
                None
            }
            Action::Key(key) => self.on_key(key),
            Action::Error(message) => {
                // A failed request is still an answer: the pane stops
                // waiting, or a view whose fetch died says "loading" for
                // the rest of the session.
                self.awaiting = false;
                if let Some(state) = self.search.as_mut() {
                    state.pending = false;
                }
                // The login modal paints over `status`, so while it is
                // showing (no session yet) an error must go where it is
                // actually visible.
                if self.session.is_none() {
                    self.login = login::LoginState::Failed(message);
                } else {
                    self.say(message);
                }
                None
            }
            Action::Tick => {
                self.expire_status();
                self.sleep_due()
            }
            Action::BeginLogin => {
                // A Subsonic server asks for its password first; any other
                // service is contacted now, and says what it wants next.
                self.login = match self.login_prompt() {
                    login::LoginState::Idle => {
                        login::LoginState::Busy(format!("Contacting {}…", self.service_name()))
                    }
                    other => other,
                };
                None
            }
            Action::BeginGuest => {
                self.login = login::LoginState::Busy("Continuing as a guest…".into());
                None
            }
            // Sent on by the loop; the prompt is taken down while the
            // service checks the answer.
            Action::Answer(_) => {
                self.login = login::LoginState::Busy("Signing in…".into());
                None
            }
            // The connection is made by the loop; the prompt stays up,
            // emptied, until it answers.
            Action::Connect(_) => None,
            // Not `SwitchProvider`: that signs in at once, and a sign-in
            // for every service passed on the way to the wanted one would
            // open seven browser tabs. The new service waits for Enter --
            // except a folder of files, which has nothing to ask.
            Action::CancelLogin => {
                if let Some(task) = self.login_task.take() {
                    task.abort();
                }
                if let Some(provider) = &self.provider {
                    provider.abandon();
                }
                self.login_input = None;
                self.login = login::LoginState::Idle;
                None
            }
            Action::NextService => {
                settings::cycle(&mut self.config, settings::Setting::Service, true);
                self.save_config();
                self.leave_service();
                let local = self.config.provider.kind == crate::config::ProviderKind::Local;
                (local && !self.config.local.folders().is_empty()).then_some(Action::BeginLogin)
            }
            Action::Typed(field, typed) => {
                match field {
                    login::Field::Folder => {
                        let probe = crate::config::LocalConfig {
                            folders: vec![typed.clone()],
                        };
                        if !probe.folders().first().is_some_and(|p| p.is_dir()) {
                            self.login =
                                login::LoginState::Failed(format!("{typed} is not a folder"));
                            return None;
                        }
                        self.config.local.folders.push(typed);
                    }
                    login::Field::Server => self.config.subsonic.server = typed,
                    login::Field::Username => self.config.subsonic.username = typed,
                }
                self.save_config();
                // On to what the service wants next: the user after the
                // server, the password after that; a folder is read now.
                Some(Action::BeginLogin)
            }
            Action::SessionOpened(session) => {
                self.session = Some(session.clone());
                self.login_input = None;
                self.login_task = None;
                self.status = None;
                self.status_since = None;
                Some(Action::SessionReady)
            }
            Action::SessionExpired => {
                // Log the sign-out, not just a failure. Silence here made a
                // spurious one impossible to trace: the token was gone with
                // nothing in the log to say who removed it.
                tracing::warn!("session rejected — signing out");
                self.session = None;
                if let Some(provider) = &self.provider {
                    provider.sign_out();
                }
                self.login = login::LoginState::Failed(
                    "your session expired — press Enter to sign in again".into(),
                );
                self.tracks.clear();
                None
            }
            Action::LoginPrompt(prompt) => {
                self.login = match prompt {
                    music::SignInPrompt::Code { code, url } => {
                        login::LoginState::Waiting { code, url }
                    }
                    music::SignInPrompt::Url(url) => login::LoginState::Url(url),
                    music::SignInPrompt::Secret => login::LoginState::Secret {
                        typed: login::Secret::default(),
                    },
                    music::SignInPrompt::Accounts(choices) => login::LoginState::Accounts {
                        choices,
                        selected: 0,
                    },
                };
                None
            }
            // A service whose streams are encrypted needs the module, and
            // the machine is searched once an account is in -- never for
            // a service that does not need it.
            Action::SessionReady => {
                let protected = self.provider.as_ref().is_some_and(|p| p.protected());
                if protected && self.cdm == drmview::Cdm::Unneeded {
                    self.cdm = drmview::Cdm::Looking;
                    return Some(Action::LookForCdm);
                }
                None
            }
            // Both are the loop's.
            Action::LookForCdm | Action::DownloadCdm => None,
            Action::CdmFound(found) => {
                self.cdm = match found {
                    Some(origin) => {
                        self.say(drmview::describe(origin));
                        drmview::Cdm::Ready(origin)
                    }
                    None => drmview::Cdm::Wanted,
                };
                self.needs_repaint = true;
                None
            }
            Action::AnswerCdm(yes) => {
                self.needs_repaint = true;
                match (&self.cdm, yes) {
                    (drmview::Cdm::Wanted, true) => {
                        self.cdm = drmview::Cdm::Offering;
                        return Some(Action::DownloadCdm);
                    }
                    (drmview::Cdm::Offered { .. }, true) => {
                        if let Some(answer) = self.cdm_answer.take() {
                            let _ = answer.send(true);
                        }
                        self.cdm = drmview::Cdm::Installing;
                    }
                    _ => {
                        if let Some(answer) = self.cdm_answer.take() {
                            let _ = answer.send(false);
                        }
                        self.cdm = drmview::Cdm::Declined;
                        self.say("without the Widevine module, this service will not play");
                    }
                }
                None
            }
            Action::CdmOffered { version, license } => {
                self.cdm = drmview::Cdm::Offered {
                    version,
                    license,
                    scroll: 0,
                };
                None
            }
            Action::CdmReady(origin) => {
                self.cdm = drmview::Cdm::Ready(origin);
                self.needs_repaint = true;
                self.say(drmview::describe(origin));
                None
            }
            Action::CdmFailed(message) => {
                self.cdm = drmview::Cdm::Missing;
                self.needs_repaint = true;
                self.say(message);
                None
            }
            Action::TrackNext => {
                self.move_vertical(Dir::Down);
                None
            }
            Action::TrackPrevious => {
                self.move_vertical(Dir::Up);
                None
            }
            Action::SidebarNext => {
                self.go_to_section(true);
                self.load_if_needed()
            }
            Action::SidebarPrevious => {
                self.go_to_section(false);
                self.load_if_needed()
            }
            Action::TracksLoaded(tracks) => {
                // The reply is here, so the pane stops waiting.
                self.awaiting = false;
                // Favourites only fill the Tracks view. Writing them in
                // while an album is open would replace its contents with
                // something else entirely.
                // The favourites are the only list that says what is one,
                // so they are remembered whether or not they are on screen.
                self.favourites = tracks.iter().map(|t| t.id.clone()).collect();
                if self.open.is_none() {
                    self.tracks = tracks;
                    self.tracklist.selected = 0;
                }
                None
            }
            Action::CollectionLoaded { for_title, tracks } => {
                // The reply is here, so the pane stops waiting. Outside the
                // match: a reply for a view already closed still ends the
                // wait, or the next thing opened inherits it.
                self.awaiting = false;
                match self.open.as_ref() {
                    Some(open) if open.title == for_title => {
                        // The banner's own line, from the tracks that just
                        // arrived: they carry their durations, so it needs
                        // no second request and works for a playlist as
                        // well as an album.
                        let total: std::time::Duration = tracks.iter().map(|t| t.duration).sum();
                        let detail = carousel::collection_detail(tracks.len() as u32, Some(total));
                        self.tracks = tracks;
                        self.tracklist.selected = 0;
                        if let Some(open) = self.open.as_mut() {
                            open.detail = detail;
                        }
                    }
                    // Closed, or already moved on to another album: this
                    // reply is for a view that is no longer on screen.
                    _ => tracing::info!("dropping tracks for {for_title:?}, no longer open"),
                }
                None
            }
            Action::ArtistLoaded(page) => {
                // The reply is here, so the pane stops waiting.
                self.awaiting = false;
                // Same rule as any other reply: one for a view the user has
                // since left would arrive under the wrong heading.
                match self.open.as_ref() {
                    Some(open) if open.title == page.name => {
                        // The server's word on whether this account
                        // follows them, which is what `F` reads.
                        if page.following {
                            self.followed.insert(page.id.clone());
                        } else {
                            self.followed.remove(&page.id);
                        }
                        self.artist = Some(*page);
                    }
                    _ => tracing::info!("dropping the page for {:?}", page.name),
                }
                None
            }
            Action::PageLoaded { title, home } => {
                // The reply is here, so the pane stops waiting.
                self.awaiting = false;
                // A genre is a page of rows, so it is drawn by the rows
                // renderer under its own heading -- not by `open`, which
                // hands the pane to the collection view. It used to set
                // `open` as well, and that view draws `open_cards`, which
                // a page of rows never fills: the pane went empty and
                // opening a genre looked like it did nothing at all.
                //
                // The level was pushed when the card was activated, and
                // Explore's own rows went onto it then, so back puts the
                // genre list back.
                self.open = None;
                self.explore.heading = Some(title);
                self.explore.rows = home
                    .rows
                    .into_iter()
                    .map(|row| home::Row {
                        heading: row.heading,
                        kind: row.kind,
                        cards: row.cards,
                        state: carousel::CarouselState::default(),
                    })
                    .collect();
                self.explore.row = 0;
                self.explore.scroll = 0;
                None
            }
            Action::ExploreLoaded(home) => {
                let home = *home;
                // Explore's own list is named by the nav entry, not by a
                // heading of its own -- a genre's name left over here would
                // sit above the list of genres.
                self.explore.heading = None;
                self.explore.rows = home
                    .rows
                    .into_iter()
                    .map(|row| home::Row {
                        heading: row.heading,
                        kind: row.kind,
                        cards: row.cards,
                        state: carousel::CarouselState::default(),
                    })
                    .collect();
                self.explore_empty = self.explore.rows.is_empty();
                self.explore_loaded = true;
                None
            }
            Action::PlaylistsLoaded(playlists) => {
                self.playlists = playlists;
                self.playlist_grid.clamp(self.playlists.len());
                self.refresh_collection_home();
                None
            }
            Action::AlbumsLoaded(albums) => {
                self.albums = albums;
                self.album_grid.clamp(self.albums.len());
                self.refresh_collection_home();
                None
            }
            Action::ArtistsLoaded(artists) => {
                self.followed = artists.iter().map(|a| a.id.clone()).collect();
                self.artists = artists;
                self.artist_grid.clamp(self.artists.len());
                self.refresh_collection_home();
                None
            }
            Action::HomeLoaded(home) => {
                let home = *home;
                self.home_loaded = true;
                self.home_from_collection = home.rows.is_empty();
                self.home.rows = home
                    .rows
                    .into_iter()
                    .map(|row| home::Row {
                        heading: row.heading,
                        kind: row.kind,
                        cards: row.cards,
                        state: carousel::CarouselState::default(),
                    })
                    .collect();
                self.refresh_collection_home();
                None
            }
            Action::CarouselNext => self.move_horizontal(Dir::Right),
            Action::CarouselPrevious => self.move_horizontal(Dir::Left),
            Action::RowNext => {
                // A track row is a grid, so down has somewhere to go inside
                // it before leaving for the next row.
                let (visible, cols) = (self.home_rows_to_land_in(), self.home_grid_columns());
                let width = scrollbar::content_width(self.last_main_width);
                self.rows_on_screen_mut().down(visible, cols, width);
                None
            }
            Action::RowPrevious => {
                let (visible, cols) = (self.home_rows_to_land_in(), self.home_grid_columns());
                let width = scrollbar::content_width(self.last_main_width);
                self.rows_on_screen_mut().up(visible, cols, width);
                None
            }
            Action::LoadHome => None,
            // The request is a side effect in run(), like LoadHome's.
            Action::LoadExplore => None,
            Action::LoadFeed => None,
            Action::LoadFavourites => None,
            Action::FeedLoaded(cards) => {
                self.feed_cards = cards;
                self.rebuild_feed();
                None
            }
            Action::BeginSearch => {
                // Opening the box does not clear what a previous search
                // found: reopening it to refine a query should not blank
                // the results being refined.
                let state = self.search.get_or_insert_with(SearchState::default);
                state.typing = true;
                None
            }
            // The request is a side effect in run(); the view only notes
            // that it is out.
            Action::RunSearch(_) => {
                if let Some(state) = self.search.as_mut() {
                    state.pending = true;
                }
                None
            }
            Action::SearchLoaded(results) => {
                if let Some(state) = self.search.as_mut() {
                    // A reply for a query the user has since retyped would
                    // show results under text that no longer produced them.
                    if state.query.trim() == results.query {
                        state.pending = false;
                        state.results = *results;
                        state.tab = 0;
                        // New results: every tab's position means nothing.
                        state.tracks = tracklist::TrackListState::default();
                        state.albums = grid::GridState::default();
                        state.artists = grid::GridState::default();
                        state.playlists = grid::GridState::default();
                        state.top_artists = carousel::CarouselState::default();
                        state.top_albums = carousel::CarouselState::default();
                        state.top_scroll = 0;
                    } else {
                        tracing::info!(
                            "dropping results for {:?}, the query is now {:?}",
                            results.query,
                            state.query
                        );
                    }
                }
                None
            }
            Action::CloseSearch => {
                self.search = None;
                None
            }
            // Moving through the queue is decided here; the loop turns the
            // resulting track into a stream. Split that way so the rules can
            // be tested without a sound card.
            Action::QueueNext => {
                if self.queue.next().is_some() && self.skip_blocked(true) {
                    return Some(Action::PlayQueued);
                }
                self.now_playing.playing = false;
                None
            }
            Action::QueuePrevious => {
                if self.queue.previous().is_some() && self.skip_blocked(false) {
                    return Some(Action::PlayQueued);
                }
                None
            }
            // Sent on to the player by the loop; nothing to change here.
            Action::SetVolume(_) => None,
            // Both are the loop's: it has the engine.
            Action::Preload(_) | Action::RestartEngine => None,
            Action::CycleSleep => {
                self.cycle_sleep();
                None
            }
            Action::ToggleLyrics => {
                self.lyrics.toggle();
                // Like the help: the pane wrote over cells the image
                // protocols skip, and only a full repaint clears that.
                self.needs_repaint = true;
                self.lyrics_wanted()
            }
            // The search is the loop's.
            Action::LoadLyrics(_) => None,
            Action::LyricsLoaded { id, found } => {
                self.lyrics.found.insert(id, found);
                None
            }
            Action::ScrobblersLinked(linked, refused) => {
                let mut words = Vec::new();
                if !linked.is_empty() {
                    let names: Vec<String> = linked.iter().map(|l| l.label()).collect();
                    words.push(format!("scrobbling to {}", names.join(", ")));
                } else if !self.scrobblers.is_empty() {
                    // Only worth a word when something was linked before:
                    // at start, no scrobbler is the ordinary case.
                    words.push("scrobbling off".to_string());
                }
                // A refusal is said whatever else happened: a token typed
                // wrong in Settings read "set" and nothing else.
                words.extend(refused);
                if !words.is_empty() {
                    self.say(words.join("; "));
                }
                self.scrobblers = linked;
                None
            }
            Action::ToggleShuffle => {
                let on = !self.queue.shuffled();
                self.queue
                    .set_shuffled(on, &mut crate::playback::clock_rng());
                // No message: the transport row lights the button, which
                // says it better than a notice that covers the view.
                None
            }
            Action::CycleRepeat => {
                self.queue.repeat = self.queue.repeat.cycle();
                None
            }
            // Handled in the loop, which has the client to fetch a stream
            // with; this arm keeps `update` alone from silently doing
            // nothing.
            Action::PlayQueued => None,
            // The fetch belongs to the loop, which has the client; these
            // arms are here so the match stays exhaustive.
            Action::Autoplay { .. } => None,
            Action::AutoplayFailed(seed) => {
                self.autoplay_fetching = false;
                self.autoplay_dead_end = Some(seed);
                None
            }
            Action::HalfPage { down } => {
                // The key `j` or `k` stands for in this config, pressed as
                // many times as half the screen holds: every view's own
                // idea of down applies, and so does the scrolling.
                let default = KeyCode::Char(if down { 'j' } else { 'k' });
                let key = KeyEvent::new(
                    self.keymap.pressed_for(default),
                    crossterm::event::KeyModifiers::NONE,
                );
                for _ in 0..self.half_page() {
                    let mut next = self.on_key(key);
                    while let Some(action) = next {
                        next = self.update(action);
                    }
                }
                None
            }
            Action::ToggleAutoplay => {
                self.config.playback.autoplay = !self.config.playback.autoplay;
                if let Some(path) = self.config_path.as_ref() {
                    if let Err(e) = self.config.save_to(path) {
                        tracing::warn!("the settings could not be saved: {e}");
                        self.say(format!("could not save settings: {e}"));
                        return None;
                    }
                }
                self.say(if self.config.playback.autoplay {
                    "autoplay on"
                } else {
                    "autoplay off"
                });
                None
            }
            // The pane is taken now, under the name the row knows, rather
            // than when the track comes back: the fetch is a round trip
            // before the one that brings the album, and for that first
            // trip nothing changed on screen -- the key looked dead.
            Action::FetchTrackAlbum(_) => {
                let track = self.selected_track()?;
                let title = if track.album.is_empty() {
                    track.title.clone()
                } else {
                    track.album.clone()
                };
                self.open_for(false, title, track.artist.clone(), track.cover.clone());
                None
            }
            Action::FetchTrackArtist(_) => {
                let track = self.selected_track()?;
                self.open_for(true, first_artist(&track.artist), String::new(), None);
                None
            }
            // What the fetch found. The pane is already waiting under the
            // name the row knew; the reply is named by the API, and that
            // is the name the tracks or the page will arrive under, so the
            // wait is renamed rather than opened again -- a second open
            // was a second level to step back out of.
            Action::OpenTarget {
                target,
                title,
                subtitle,
                cover,
            } => {
                let round = matches!(target, Collection::Artist(_));
                let awaiting = self.awaiting;
                match self.open.as_mut() {
                    Some(open) if awaiting => {
                        open.title = title;
                        open.subtitle = subtitle;
                        open.cover = cover;
                        open.round_cover = round;
                    }
                    // The wait was left before the track came back: the
                    // same rule as any other late reply.
                    _ => tracing::info!("dropping {title:?}, no longer waited for"),
                }
                None
            }
            Action::QueueRadio(tracks) => {
                self.autoplay_fetching = false;
                // Marked as autoplay, which is what it is: what TIDAL
                // suggested following the end of the queue. That marking
                // is what lets the next album the user starts throw it away
                // rather than play a radio chosen for a track from twenty
                // minutes ago.
                //
                // Appended, never started: whatever is playing moves on to
                // it by itself. The one exception is a queue that ran out
                // before this landed -- then nothing is playing, and the
                // first of these has to be started by hand.
                self.queue.append_autoplay(tracks);
                if self.ran_dry && self.queue.next().is_some() && self.skip_blocked(true) {
                    self.ran_dry = false;
                    return Some(Action::PlayQueued);
                }
                None
            }
            Action::ToggleFavourite | Action::ToggleFollow => None,
            Action::FollowChanged {
                what,
                name,
                following,
            } => {
                let said = match (&what, following) {
                    (Favourable::Artist(_), true) => format!("Following {name}"),
                    (Favourable::Artist(_), false) => format!("Unfollowed {name}"),
                    (_, true) => format!("Added {name} to favourites"),
                    (_, false) => format!("Removed {name} from favourites"),
                };
                match &what {
                    Favourable::Artist(id) => {
                        if following {
                            self.followed.insert(id.clone());
                        } else {
                            self.followed.remove(id);
                        }
                        // The page on screen says so too, count included,
                        // rather than waiting to be opened again.
                        if let Some(page) = self.artist.as_mut().filter(|p| p.id == *id) {
                            page.following = following;
                            page.fans = page.fans.map(|n| {
                                if following {
                                    n + 1
                                } else {
                                    n.saturating_sub(1)
                                }
                            });
                        }
                    }
                    Favourable::Album(id) => {
                        if following {
                            self.favourite_albums.insert(id.clone());
                        } else {
                            self.favourite_albums.remove(id);
                        }
                        // The album's page, when it is up, shows the heart
                        // at once rather than on the next open.
                        if let Some(page) = self
                            .artist
                            .as_mut()
                            .filter(|p| p.kind == crate::library::PageKind::Album && p.id == *id)
                        {
                            page.following = following;
                        }
                    }
                    Favourable::Playlist(uuid) => {
                        if following {
                            self.favourite_playlists.insert(uuid.clone());
                        } else {
                            self.favourite_playlists.remove(uuid);
                        }
                    }
                }
                self.say(said);
                None
            }
            Action::RadioNamed { seed, playlist } => {
                self.radio_playlists.insert(seed, playlist);
                None
            }
            Action::FavouriteIdsLoaded(ids) => {
                self.followed.extend(ids.artists);
                self.favourite_albums = ids.albums.into_iter().collect();
                self.favourite_playlists = ids.playlists.into_iter().collect();
                None
            }
            // Applied here rather than in the loop, for the same reason
            // opening is: the loop's copy could only be exercised by running
            // the whole app.
            Action::SeeAll if self.on_artist() => {
                // An artist's sections are already in hand — the page came
                // back with forty-odd albums and fifty singles — so this
                // opens what is held rather than asking for it again.
                let page = self.artist.as_ref()?;
                // The section the selection is actually in: an artist with
                // no top tracks starts on a section they do not have, and
                // asking for its cards gave an empty list.
                let present = artistview::Section::present(page);
                let wanted = artistview::Section::from_index(self.artist_section);
                let section = if present.contains(&wanted) {
                    wanted
                } else {
                    *present.first()?
                };
                let heading = format!("{} — {}", page.name, section.heading(page.kind));
                if section == artistview::Section::Tracks {
                    if page.top_tracks.is_empty() {
                        return None;
                    }
                    let identity = self.heading_only(heading);
                    self.open_view(identity, Vec::new());
                    return None;
                }
                let cards = artistview::cards(page, section);
                if cards.is_empty() {
                    return None;
                }
                let identity = self.heading_only(heading);
                self.open_view(identity, cards);
                None
            }
            // A Feed section is whole in hand -- the four are cut from one
            // reply -- so this opens what is held rather than asking for a
            // page that was never fetched.
            Action::SeeAll if self.on_feed() => {
                let row = self.rows_on_screen().current_row()?;
                let (heading, cards) = (row.heading.clone(), row.cards.clone());
                let identity = self.heading_only(heading);
                self.open_view(identity, cards);
                None
            }
            // A Top results row offers "See all" when its cards run past the
            // edge; the whole of what was found is on the kind's own tab.
            Action::SeeAll if self.on_search() => {
                let state = self.search.as_mut()?;
                if searchview::Tab::from_index(state.tab) != searchview::Tab::Top {
                    return None;
                }
                let tab = match state.top {
                    searchview::TopSection::Artists => searchview::Tab::Artists,
                    searchview::TopSection::Albums => searchview::Tab::Albums,
                    searchview::TopSection::Tracks => return None,
                };
                state.tab = searchview::Tab::ALL.iter().position(|t| *t == tab)?;
                None
            }
            // A home row, or one of a page opened from Explore: whole in
            // hand, like the Feed's, so it opens from what is held.
            Action::SeeAll if self.on_home() => {
                let row = self.rows_on_screen().current_row()?;
                let (heading, cards) = (row.heading.clone(), row.cards.clone());
                let identity = self.heading_only(heading);
                self.open_view(identity, cards);
                None
            }
            Action::SeeAll => None,
            // The edits are requests the loop makes; the answer comes
            // back as `PlaylistEdited` or `Error`. Said now, so the wait
            // for the service -- several round trips on TIDAL -- is not
            // silence.
            Action::Edit(edit) => {
                use playlistedit::Edit;
                self.say(match edit {
                    Edit::AddTo { title, .. } => format!("adding to {title}…"),
                    Edit::Create { name, .. } => format!("creating {name}…"),
                    Edit::Rename { name, .. } => format!("renaming to {name}…"),
                    Edit::Delete { .. } => "deleting the playlist…".to_string(),
                });
                None
            }
            Action::RemoveFromPlaylist { .. } => {
                self.say("taking the track out…");
                None
            }
            Action::PlaylistEdited {
                message,
                uuid,
                deleted,
                count_delta,
            } => {
                self.say(message);
                if let Some(p) = self.playlists.iter_mut().find(|p| p.uuid == uuid) {
                    p.track_count = p.track_count.saturating_add_signed(count_delta);
                }
                let open_is_it = matches!(
                    self.open.as_ref().and_then(|o| o.target.as_ref()),
                    Some(Collection::Playlist(open)) if *open == uuid
                );
                // The playlists arrived just before this; the open view's
                // heading takes the name they now carry, so a rename shows
                // without reopening.
                if open_is_it && !deleted {
                    if let Some(name) = self.playlists.iter().find(|p| p.uuid == uuid).map(|p| p.title.clone()) {
                        if let Some(open) = self.open.as_mut() {
                            open.title = name;
                        }
                    }
                }
                (deleted && open_is_it).then_some(Action::CloseCollection)
            }
            Action::FavouriteChanged { id, favourite } => {
                if favourite {
                    self.favourites.insert(id);
                } else {
                    self.favourites.remove(&id);
                }
                None
            }
            Action::ArtistRight => {
                self.artist_move(Dir::Right);
                None
            }
            Action::ArtistLeft => {
                self.artist_move(Dir::Left);
                None
            }
            Action::SearchRight => {
                self.search_move(Dir::Right);
                None
            }
            Action::SearchLeft => {
                self.search_move(Dir::Left);
                None
            }
            Action::NextSearchTab => {
                if let Some(state) = self.search.as_mut() {
                    // Each tab keeps its own position now that each is one
                    // of the app's own views, so coming back to a tab finds
                    // it where it was left.
                    state.tab = (state.tab + 1) % searchview::Tab::ALL.len();
                }
                None
            } // the fetch is a side effect in run()
            Action::PlaybackFailed { id, message } => {
                // The bar is filled the moment a track is chosen, so the
                // keypress has a visible effect before the request
                // returns. When that request fails the guess has to be
                // taken back: otherwise the cover and title of a track
                // that never started sit there looking exactly like one
                // that is playing. Only if it is still the same track —
                // a later choice has already replaced it.
                if self.now_playing.track.as_ref().is_some_and(|t| t.id == id) {
                    self.now_playing = nowplaying::NowPlaying::default();
                }
                self.say(message);
                None
            }
            Action::TogglePause => {
                self.now_playing.playing = !self.now_playing.playing;
                None
            }
            Action::SeekForward => {
                if let Some(to) = self.seek_target(true) {
                    self.now_playing.position = to;
                }
                None
            }
            Action::SeekBack => {
                if let Some(to) = self.seek_target(false) {
                    self.now_playing.position = to;
                }
                None
            }
            Action::Playback(event) => {
                match event {
                    crate::playback::PlaybackEvent::Position(p) => {
                        self.now_playing.position = p;
                        // Filled while the track is still playing rather
                        // than at the silence after the last one: the web
                        // client's "up next" is never empty, and fetching
                        // at the end means the user waits for the request
                        // with nothing in their ears. The reply appends, so
                        // this does not interrupt anything.
                        if let Some(fetch) = self.autoplay_seed() {
                            return Some(fetch);
                        }
                    }
                    crate::playback::PlaybackEvent::Length(length) => {
                        // A track that came without a duration -- YouTube
                        // Music's quick picks -- drew a clock over an
                        // empty bar. The stream knows how long it is.
                        if let Some(track) = self.now_playing.track.as_mut() {
                            if track.duration.is_zero() {
                                track.duration = length;
                            }
                        }
                    }
                    crate::playback::PlaybackEvent::Started {
                        bit_depth,
                        sample_rate,
                        delivered,
                    } => {
                        // The track before this one is over, however far it
                        // got: the position is still its own here.
                        self.settle_listen(self.now_playing.position);
                        // Report what was DELIVERED.
                        let bits = bit_depth.map(|b| format!("{b}-bit ")).unwrap_or_default();
                        // Two decimals, then one trailing zero dropped:
                        // 44.1, 48.0, 22.05. A fixed ".1" rounded HE-AAC's
                        // 22050 Hz to "22.1".
                        let khz = format!("{:.2}", sample_rate as f64 / 1000.0);
                        let khz = khz.strip_suffix('0').unwrap_or(&khz);
                        // A file of your own reports no rate: no badge,
                        // rather than "0.0kHz".
                        self.now_playing.quality =
                            (sample_rate > 0).then(|| format!("{bits}{khz}kHz"));
                        self.now_playing.tier =
                            nowplaying::Tier::of_quality(delivered, bit_depth, sample_rate);
                        // A new stream starts at the beginning. Without this
                        // the previous track's position survived into the new
                        // one, and if that track was the longer of the two
                        // the ratio saturated and drew a full bar over a
                        // track that had barely started.
                        self.now_playing.position = std::time::Duration::ZERO;
                        self.now_playing.playing = true;
                        self.ran_dry = false;
                        self.begin_listen();
                        // A new track: the lyrics pane follows it from the
                        // top, and asks for its words if it is open.
                        self.lyrics.restart();
                        if let Some(ask) = self.lyrics_wanted() {
                            return Some(ask);
                        }
                    }
                    // The whole track is here: fetch the next one, so it
                    // starts at once and a gapless engine joins the two.
                    // Not with the sleep timer set to the end of the
                    // track: the next is not to be lined up.
                    crate::playback::PlaybackEvent::Downloaded(id) => {
                        if self.now_playing.track.as_ref().is_some_and(|t| t.id == id)
                            && self.sleep != Some(Sleep::EndOfTrack)
                        {
                            if let Some(next) = self.queue.upcoming() {
                                if self.why_blocked(next).is_none() {
                                    return Some(Action::Preload(next.id.clone()));
                                }
                            }
                        }
                    }
                    crate::playback::PlaybackEvent::Finished => {
                        // Played to the end: the whole of it counts.
                        let whole = self.listen.as_ref().map(|p| p.duration);
                        self.settle_listen(whole.unwrap_or(self.now_playing.position));
                        self.now_playing.position = std::time::Duration::ZERO;
                        // The sleep timer set to the end of the track: this
                        // is it. Paused rather than left, since a gapless
                        // engine may already have the next lined up.
                        if self.sleep == Some(Sleep::EndOfTrack) {
                            self.sleep = None;
                            return self.now_playing.playing.then_some(Action::TogglePause);
                        }
                        // A track ending on its own moves the queue on, which
                        // is what makes an album play through. `advance`
                        // rather than `next`: repeat-one applies to a track
                        // that ends, not to the skip button.
                        if self.queue.advance().is_some() && self.skip_blocked(true) {
                            return Some(Action::PlayQueued);
                        }
                        self.now_playing.playing = false;
                        // The queue is out. The radio was asked for while
                        // the last track played; if it has not landed, or
                        // was never asked for, this is the last resort.
                        self.ran_dry = true;
                        if let Some(fetch) = self.autoplay_seed() {
                            tracing::info!("queue empty, following the last track's radio");
                            return Some(fetch);
                        }
                    }
                    crate::playback::PlaybackEvent::Error(e) => {
                        self.say(e);
                        self.now_playing.playing = false;
                    }
                    crate::playback::PlaybackEvent::Spectrum(bands) => {
                        self.spectrum = Some(bands);
                    }
                    // The engine's verdicts on a track it could not play,
                    // handled where the shell already handles them.
                    crate::playback::PlaybackEvent::Failed { id, message } => {
                        // Never played, so never a listen.
                        self.listen = None;
                        return Some(Action::PlaybackFailed { id, message });
                    }
                    crate::playback::PlaybackEvent::SessionExpired => {
                        return Some(Action::SessionExpired);
                    }
                }
                None
            }
            // Opening is applied here rather than in the loop so it can be
            // tested: the loop's copy could only be exercised by running the
            // whole app, and a test that called the helpers directly passed
            // just as well with the bug in place.
            // Enter on the Service row is the switch itself; on a text or
            // folder row it opens the box; on the link row it links; on a
            // toggle it flips, as l would.
            Action::ActivateSelection if self.on_settings() => {
                let which = self.settings.current();
                match (which, which.kind()) {
                    (settings::Setting::Service, _) => Some(Action::SwitchProvider),
                    (_, settings::Kind::Text { .. } | settings::Kind::Folders) => {
                        self.setting_edit = Some(settings::Edit {
                            setting: which,
                            typed: settings::text_of(&self.config, which),
                        });
                        None
                    }
                    (_, settings::Kind::Link) => Some(Action::SettingChanged(which)),
                    (_, settings::Kind::Toggle) => self.change_setting(true),
                    (_, settings::Kind::Cycle) => None,
                }
            }
            Action::SettingTyped(setting, text) => {
                settings::set_text(&mut self.config, setting, &text);
                self.save_config();
                Some(Action::SettingChanged(setting))
            }
            // What the shell itself reads live; the loop does the rest.
            Action::SettingChanged(setting) => match setting {
                settings::Setting::Theme => {
                    self.palette = theme::Palette::from_config(&self.config.ui).0;
                    self.needs_repaint = true;
                    None
                }
                // The folders are the library: it is read again, the way
                // a service switch reads it, when that is the service on.
                settings::Setting::LocalFolders
                    if self.config.provider.kind == crate::config::ProviderKind::Local =>
                {
                    Some(Action::SwitchProvider)
                }
                _ => None,
            },
            Action::SwitchProvider => {
                self.leave_service();
                Some(Action::BeginLogin)
            }
            Action::ActivateSelection if self.on_grid() || self.on_home() => {
                // A shortcut into one of the app's own views is a change
                // of section, the same as pressing its number.
                if let Some(carousel::Target::Section(section)) =
                    self.selected_card().and_then(|c| c.target)
                {
                    return Some(Action::GoToSection(section));
                }
                if self.selected_collection().is_some() {
                    // Read before the level is put away: `push_level` takes
                    // the cards with it, and the identity is read off the
                    // card the user pressed enter on.
                    let identity = self.selected_identity();
                    // The sidebar stays where it is. Moving it to Tracks made
                    // an opened album look like the favourites view and
                    // highlighted the wrong nav entry.
                    self.open_view(identity, Vec::new());
                    // A page opened from Explore replaces its rows, and the
                    // reply takes a moment to arrive. Left in place, the
                    // pane went on drawing the genres under the new
                    // heading, so pressing enter looked like nothing at
                    // all. They go on the level, which is what back
                    // restores.
                    if self.sidebar.section() == sidebar::Section::Explore {
                        let replaced = std::mem::take(&mut self.explore);
                        if let Some(level) = self.back.last_mut() {
                            level.explore = Some(replaced);
                        }
                    }
                }
                None
            }
            Action::ActivateSelection => None, // playback is a side effect in run()
            // The fetch belongs to the loop, which has the client. These
            // arms keep the match exhaustive.
            // The pane is taken here, so the reply has a view to land in:
            // a reply is only accepted for the title that is open, and
            // nothing opened one -- every album and artist a key asked for
            // came back and was dropped as "no longer open". The loop reads
            // the target before this runs, so it sees the same selection.
            Action::OpenTrackAlbum => {
                let track = self.selected_track()?;
                let (_, title) = album_of(&track)?;
                self.open_for(false, title, track.artist.clone(), track.cover.clone());
                None
            }
            Action::OpenTrackArtist => {
                let track = self.selected_track()?;
                let (_, name) = artist_of(&track)?;
                self.open_for(true, name, String::new(), None);
                None
            }
            // Both change what is playing, which only the loop can do: it
            // owns the channel to the audio thread.
            Action::RemoveQueued(_) | Action::JumpInQueue(_) => None,
            // These only add to the queue, so nothing has to reach the
            // audio thread: what is playing carries on.
            Action::QueueTrackNext | Action::QueueTrackLast => {
                let track = self.selected_track()?;
                let title = track.title.clone();
                if matches!(action, Action::QueueTrackNext) {
                    self.queue.play_next(track);
                    self.say(format!("{title} — next"));
                } else {
                    self.queue.play_last(track);
                    self.say(format!("{title} — queued"));
                }
                None
            }
            Action::PlayTrackRadio => {
                // The same step opening anything else takes: what is on
                // screen goes on the history, and the radio takes the pane.
                let seed = self.track_radio()?;
                let title = self.track_radio_title();
                // The track's own artwork: a radio built from it is about
                // that record, and the mix's own cover is not in hand until
                // its items arrive.
                let cover = self
                    .selected_track()
                    .or_else(|| self.now_playing.track.clone())
                    .and_then(|t| t.cover);
                let identity = self.heading_with_cover(seed, format!("{title} Radio"), cover, false);
                self.open_view(identity, Vec::new());
                None
            }
            Action::GoToSection(section) => {
                if self.sidebar.section() == section && self.open.is_none() {
                    // Already there and nothing over it: pressing the number
                    // again should not fill the history with the same place.
                    return None;
                }
                self.leave_for_a_new_place();
                self.sidebar.select(section);
                self.load_if_needed()
            }
            Action::GoHome => {
                // A step in the history like any other, so `[` returns to
                // whatever the user pressed escape from.
                self.leave_for_a_new_place();
                self.sidebar.select(sidebar::Section::Music);
                self.load_if_needed()
            }
            Action::GoBack => {
                self.go_back();
                self.load_if_needed()
            }
            Action::GoForward => {
                self.go_forward();
                self.load_if_needed()
            }
            Action::CloseCollection => {
                // One step back, to whatever was on screen when this view
                // was opened — the view below it when they are nested, and
                // the sidebar's section at the bottom of the stack. Its own
                // selection comes back with it.
                if !self.go_back() {
                    self.clear_open();
                }
                None
            }
        }
    }

    /// The login screen for the service the config names: TIDAL waits for
    /// Enter to show its code, a Subsonic server asks for its password.
    fn login_prompt(&self) -> login::LoginState {
        match self.config.provider.kind {
            crate::config::ProviderKind::Subsonic => self.subsonic_prompt(),
            // Nothing to sign in to: the folders are read as soon as the
            // loop starts, and this is what the screen says meanwhile. With
            // none named yet, the first is asked for here.
            crate::config::ProviderKind::Local if self.config.local.folders().is_empty() => {
                login::LoginState::Text {
                    field: login::Field::Folder,
                    typed: "~/Music".into(),
                }
            }
            crate::config::ProviderKind::Local => {
                login::LoginState::Busy("Reading your music folders…".into())
            }
            _ => login::LoginState::Idle,
        }
    }

    /// What the login screen calls the service.
    fn service_name(&self) -> &'static str {
        self.provider.as_ref().map(|p| p.name()).unwrap_or("TIDAL")
    }

    /// The login screen's key for going in without an account. Named
    /// rather than written where it is matched: the help modal lists the
    /// signed-in keys, and its test reads this file for every `Char('x')`.
    const GUEST_KEY: char = 'g';

    /// Whether the service can be used without an account -- YouTube
    /// Music can -- so the login screen offers it.
    fn guest_allowed(&self) -> bool {
        self.provider
            .as_ref()
            .is_some_and(|p| p.sign_in_options().contains(&music::SignIn::Anonymous))
    }

    /// The login screen for the Subsonic server the config names: the
    /// server and the user are asked for first when it names none, and
    /// the password every time.
    fn subsonic_prompt(&self) -> login::LoginState {
        let subsonic = &self.config.subsonic;
        if subsonic.server.trim().is_empty() {
            return login::LoginState::Text {
                field: login::Field::Server,
                typed: "https://".into(),
            };
        }
        if subsonic.username.trim().is_empty() {
            return login::LoginState::Text {
                field: login::Field::Username,
                typed: String::new(),
            };
        }
        login::LoginState::Password {
            server: subsonic.server.trim().to_owned(),
            username: subsonic.username.trim().to_owned(),
            typed: login::Secret::default(),
        }
    }

    /// True when the main pane is showing the home page rather than a list.
    fn on_home(&self) -> bool {
        // Explore and the genre pages it opens are drawn by the same
        // renderer as the home page, so they move the same way.
        match self.showing() {
            Showing::Rows => true,
            Showing::Section(section) => matches!(
                section,
                sidebar::Section::Music
                    | sidebar::Section::Explore
                    // Four dated sections of covers, which is the same
                    // shape as a page of rows and moves the same way.
                    | sidebar::Section::Feed
            ),
            _ => false,
        }
    }

    /// Move within an artist's page, which stacks its sections the way Top
    /// results does.
    fn artist_move(&mut self, dir: Dir) {
        let (width, height) = (self.last_main_width, self.last_main_height);
        let Some(page) = self.artist.as_ref() else {
            return;
        };
        // With the bio open, the bio is the page: up and down scroll it,
        // and the sections under it wait until it closes. Without this the
        // first screen of a long blurb was all of it that could be read.
        if self.artist_bio_open {
            let most = artistview::bio_overflow(page, width, height);
            self.artist_bio_scroll = match dir {
                Dir::Down => (self.artist_bio_scroll + 1).min(most),
                Dir::Up => self.artist_bio_scroll.saturating_sub(1),
                Dir::Left | Dir::Right => self.artist_bio_scroll,
            };
            return;
        }
        let present = artistview::Section::present(page);
        if present.is_empty() {
            return;
        }
        let current = artistview::Section::from_index(self.artist_section);
        let current = if present.contains(&current) {
            current
        } else {
            present[0]
        };
        let at = present.iter().position(|s| *s == current).unwrap_or(0);
        // The rows the renderer gives the list at this size, so the keys
        // scroll it where it is drawn.
        let tracks_visible = tracklist::visible_rows_of(
            artistview::tracks_height(
                page,
                self.artist_scroll,
                self.artist_bio_open,
                width,
                height,
            ),
            None,
            tracklist::Chrome::Bare,
        );
        // Asked of the same function the renderer draws from, so a section
        // added to one is not missed by the other.
        let cards = artistview::cards(page, current).len();
        let in_view = artistview::sections_in_view(
            page,
            self.artist_scroll,
            self.artist_bio_open,
            width,
            height,
        );
        let moved = Self::move_in_stack(
            Stacked {
                count: present.len(),
                at,
                tracks: present
                    .iter()
                    .position(|s| *s == artistview::Section::Tracks)
                    .map(|i| {
                        (
                            i,
                            &mut self.artist_tracks,
                            page.top_tracks.len(),
                            tracks_visible,
                        )
                    }),
                row: (current != artistview::Section::Tracks).then(|| {
                    (
                        &mut self.artist_rows[current.index()],
                        cards,
                        carousel::cards_to_land_in(scrollbar::content_width(width)),
                    )
                }),
                scroll: &mut self.artist_scroll,
                visible: in_view,
            },
            dir,
        );
        self.artist_section = present[moved].index();
    }

    /// What the section just moved to still needs fetching.
    ///
    /// Explore is a page of its own, and nothing else asks for it: without
    /// this the section drew an empty page until something else happened
    /// to trigger a load.
    fn load_if_needed(&mut self) -> Option<Action> {
        match self.sidebar.section() {
            sidebar::Section::Explore if self.explore.rows.is_empty() => Some(Action::LoadExplore),
            sidebar::Section::Feed
                if self.feed_cards.is_empty()
                    && self.session.as_ref().is_some_and(|s| s.has_feed()) =>
            {
                Some(Action::LoadFeed)
            }
            // The three that arrive together at sign-in. Any of them can be
            // dropped on the way in -- the reply is only taken when nothing
            // is open over the pane -- and nothing asked for them a second
            // time, so the section stayed empty for the rest of the session.
            sidebar::Section::Tracks if self.tracks.is_empty() => Some(Action::LoadFavourites),
            sidebar::Section::Playlists if self.playlists.is_empty() => {
                Some(Action::LoadFavourites)
            }
            sidebar::Section::Albums if self.albums.is_empty() => Some(Action::LoadFavourites),
            _ => None,
        }
    }

    /// Group the feed into its four dated sections.
    ///
    /// Filtered first and grouped after, so a filter that empties a bucket
    /// leaves no heading behind rather than a heading over nothing: a
    /// section that has no cards is never built.
    fn rebuild_feed(&mut self) {
        let filter = std::mem::take(&mut self.feed.filter);
        let cards: Vec<carousel::Card> = grid::filter(&self.feed_cards, &filter)
            .into_iter()
            .cloned()
            .collect();
        self.feed = home::HomeState {
            rows: feed_rows(&cards, today()),
            filter,
            ..Default::default()
        };
    }

    /// The page of rows on screen, whichever it is.
    fn rows_on_screen(&self) -> &home::HomeState {
        match self.sidebar.section() {
            sidebar::Section::Explore => &self.explore,
            sidebar::Section::Feed => &self.feed,
            _ => &self.home,
        }
    }

    fn rows_on_screen_mut(&mut self) -> &mut home::HomeState {
        match self.sidebar.section() {
            sidebar::Section::Explore => &mut self.explore,
            sidebar::Section::Feed => &mut self.feed,
            _ => &mut self.home,
        }
    }

    /// Public so a test can drive the app the way a person does: keys in,
    /// buffer out. Reaching past this and calling the handlers directly is
    /// how a hint has twice been drawn for a key that did nothing.
    pub fn on_key(&mut self, key: KeyEvent) -> Option<Action> {
        // A playlist prompt owns the keyboard while it is up: what is
        // typed into it is a name, `q` included, and the keymap does not
        // apply to text.
        if let Some(prompt) = self.prompt.as_mut() {
            let owned: Vec<crate::library::Playlist> =
                self.playlists.iter().filter(|p| p.owned).cloned().collect();
            let outcome = playlistedit::on_key(prompt, key.code, &owned);
            return match outcome {
                playlistedit::Outcome::Kept => None,
                playlistedit::Outcome::Closed => {
                    self.prompt = None;
                    self.needs_repaint = true;
                    None
                }
                playlistedit::Outcome::Done(edit) => {
                    self.prompt = None;
                    self.needs_repaint = true;
                    Some(Action::Edit(edit))
                }
            };
        }
        if let Some(edit) = self.setting_edit.as_mut() {
            match key.code {
                KeyCode::Char(c) => edit.typed.push(c),
                KeyCode::Backspace => {
                    edit.typed.pop();
                }
                KeyCode::Enter => {
                    let edit = self.setting_edit.take().expect("checked");
                    self.needs_repaint = true;
                    return Some(Action::SettingTyped(edit.setting, edit.typed));
                }
                KeyCode::Esc => {
                    self.setting_edit = None;
                    self.needs_repaint = true;
                }
                _ => {}
            }
            return None;
        }
        // Read before the keymap: the map knows keys, not modifiers, and
        // would translate the `d` of ctrl-d as a bare `d`. Not while a box
        // has the keyboard, where nothing is a command.
        if key
            .modifiers
            .contains(crossterm::event::KeyModifiers::CONTROL)
            && !self.filtering
            && !self.search.as_ref().is_some_and(|s| s.typing)
        {
            match key.code {
                KeyCode::Char('d') => return Some(Action::HalfPage { down: true }),
                KeyCode::Char('u') => return Some(Action::HalfPage { down: false }),
                _ => {}
            }
        }
        // What the user asked this key to mean, before anything reads it.
        // Rebinding is a translation rather than a table of its own: every
        // key below is guarded by what is on screen, and a table would have
        // to carry all of that or lose it.
        //
        // Not applied to the text boxes below: while one has the keyboard
        // `q` is a letter, and a config that could change that would be a
        // config that breaks typing.
        let key = KeyEvent {
            code: self.keymap.resolve(key.code),
            ..key
        };

        // The search box takes the keyboard before anything else. Same
        // reason as the filter box below: while text is being typed, "q" is
        // a letter, not a command.
        if self.search.as_ref().is_some_and(|s| s.typing) {
            let mut query = self
                .search
                .as_ref()
                .map(|s| s.query.clone())
                .unwrap_or_default();
            match key.code {
                KeyCode::Esc => {
                    // Escape backs out of search entirely rather than only
                    // leaving the box: a box with no way back to the app
                    // would be a trap.
                    return Some(Action::CloseSearch);
                }
                KeyCode::Enter => {
                    if let Some(state) = self.search.as_mut() {
                        state.typing = false;
                    }
                    return Some(Action::RunSearch(query));
                }
                KeyCode::Backspace => {
                    query.pop();
                }
                KeyCode::Char(c) => query.push(c),
                _ => return None,
            }
            if let Some(state) = self.search.as_mut() {
                state.query = query;
            }
            return None;
        }

        // While the filter box has the keyboard, printable keys are text.
        // Without this "q" would quit rather than filter.
        if self.filtering {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => {
                    self.filtering = false;
                    if key.code == KeyCode::Esc {
                        self.set_filter(String::new());
                    }
                }
                KeyCode::Backspace => {
                    let mut f = self.filter_text().to_string();
                    f.pop();
                    self.set_filter(f);
                }
                KeyCode::Char(c) => {
                    let mut f = self.filter_text().to_string();
                    f.push(c);
                    self.set_filter(f);
                }
                _ => {}
            }
            return None;
        }

        // Any key closes the help, including the one that opened it. A modal
        // that takes a specific key to dismiss is one more thing to know.
        // The queue is a view rather than a notice: it moves and it edits,
        // so only the keys it does not use close it. Nothing is passed on
        // to the app behind -- pressing `q` to shut it must not also quit.
        // The queue's own keys. Only these: it used to take every key and
        // close on any it did not know, which made each command a way out
        // and none of them a command -- `A` over the track that was playing
        // shut the queue instead of liking it. The rest fall through to what
        // they always do, and act on the row under the cursor, which
        // `selected_track` reads from the queue while it is showing.
        if self.showing() == Showing::Queue {
            let len = self.queue.len();
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.queue_list.next(len, self.over_list_visible());
                    return None;
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.queue_list.previous(self.over_list_visible());
                    return None;
                }
                // Take it out. Removing what is playing has to stop or move
                // the player, which the loop does -- `update` cannot reach
                // the audio thread.
                KeyCode::Char('x') | KeyCode::Delete | KeyCode::Backspace => {
                    if len > 0 {
                        // Taking a track out shortens the list and uncovers
                        // a row of whatever was drawn there before.
                        self.needs_repaint = true;
                        return Some(Action::RemoveQueued(self.queue_list.selected));
                    }
                    return None;
                }
                // Jump to it, which is what enter means in every list here.
                // Taken even when there is nothing to jump to: falling
                // through would open whatever the hidden page had selected.
                KeyCode::Enter => {
                    return (len > 0).then_some(Action::JumpInQueue(self.queue_list.selected));
                }
                // Whether the queue has an end. Apple Music puts the same
                // switch at the top of its Playing Next list: the place
                // you look when wondering what comes after the last row.
                KeyCode::Char('i') => return Some(Action::ToggleAutoplay),
                // The key that opened it closes it, which is a step back:
                // the queue is a view of its own. Escape and `[` reach the
                // same step below, as they do from any view.
                KeyCode::Char('Q') => return Some(Action::GoBack),
                _ => {}
            }
        }
        // The history's keys, as the queue's: only its own, the rest fall
        // through and act on the row under the cursor.
        if self.showing() == Showing::History {
            let len = self.queue.history().count();
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.history_list.next(len, self.over_list_visible());
                    return None;
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.history_list.previous(self.over_list_visible());
                    return None;
                }
                // Bringing a played track back means queuing it next; `n`
                // then reaches it. Taken even with nothing to queue, or it
                // would open whatever the hidden page had selected.
                KeyCode::Enter => {
                    return (len > 0).then_some(Action::QueueTrackNext);
                }
                KeyCode::Char('H') => return Some(Action::GoBack),
                _ => {}
            }
        }
        // A question about the Widevine module takes every key: y or n
        // answer it, j and k scroll the terms, nothing else gets through.
        if self.cdm.asking() {
            // Matched on the letter rather than the key, so the help's
            // own scan does not count these among the app's bindings: they
            // are the modal's, and it says them itself.
            let typed = match key.code {
                KeyCode::Char(c) => Some(c),
                _ => None,
            };
            return match (typed, key.code) {
                (Some('y'), _) => Some(Action::AnswerCdm(true)),
                (Some('n'), _) | (_, KeyCode::Esc) => Some(Action::AnswerCdm(false)),
                (Some('j'), _) | (_, KeyCode::Down) => {
                    self.cdm.scroll_by(true);
                    None
                }
                (Some('k'), _) | (_, KeyCode::Up) => {
                    self.cdm.scroll_by(false);
                    None
                }
                _ => None,
            };
        }
        // The lyrics pane takes the keys that move it and lets the rest
        // through: pausing and skipping are still wanted while reading.
        if self.lyrics.open {
            match key.code {
                KeyCode::Esc | KeyCode::Char('L') => return Some(Action::ToggleLyrics),
                KeyCode::Char('j') | KeyCode::Down => {
                    self.lyrics.scroll_by(true);
                    return None;
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.lyrics.scroll_by(false);
                    return None;
                }
                _ => {}
            }
        }
        if self.showing_help {
            // Too short a terminal cut the end of the list off, with no
            // way to reach it: j and k scroll, anything else closes.
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.help_scroll = self.help_scroll.saturating_add(1);
                    return None;
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.help_scroll = self.help_scroll.saturating_sub(1);
                    return None;
                }
                _ => {}
            }
            self.showing_help = false;
            self.help_scroll = 0;
            // The modal wrote over cells the image protocols had marked
            // `Skip`, and on the next frame they go back to `Skip` — so
            // nothing is written to undo it and a patch of the help stays
            // on the artwork. Only a full repaint clears that.
            self.needs_repaint = true;
            return None;
        }

        if self.session.is_none() {
            // The login screen's own prompts take every key: what is typed
            // into them is a password or a cookie, `q` included.
            match &mut self.login {
                login::LoginState::Password { typed, .. } => {
                    match key.code {
                        KeyCode::Char(c) => typed.0.push(c),
                        KeyCode::Backspace => {
                            typed.0.pop();
                        }
                        KeyCode::Enter if !typed.0.is_empty() => {
                            return Some(Action::Connect(std::mem::take(typed)));
                        }
                        KeyCode::Tab => return Some(Action::NextService),
                        KeyCode::Esc => return Some(Action::CancelLogin),
                        _ => {}
                    }
                    return None;
                }
                login::LoginState::Text { field, typed } => {
                    match key.code {
                        KeyCode::Char(c) => typed.push(c),
                        KeyCode::Backspace => {
                            typed.pop();
                        }
                        KeyCode::Enter if !typed.trim().is_empty() => {
                            return Some(Action::Typed(*field, typed.trim().to_owned()));
                        }
                        KeyCode::Tab => return Some(Action::NextService),
                        KeyCode::Esc => return Some(Action::CancelLogin),
                        _ => {}
                    }
                    return None;
                }
                login::LoginState::Secret { typed } => {
                    match key.code {
                        KeyCode::Char(c) => typed.0.push(c),
                        KeyCode::Backspace => {
                            typed.0.pop();
                        }
                        KeyCode::Enter if !typed.0.trim().is_empty() => {
                            return Some(Action::Answer(std::mem::take(typed).0));
                        }
                        KeyCode::Esc => return Some(Action::CancelLogin),
                        _ => {}
                    }
                    return None;
                }
                login::LoginState::Accounts { choices, selected } => {
                    match key.code {
                        KeyCode::Char('j') | KeyCode::Down => {
                            *selected = (*selected + 1).min(choices.len().saturating_sub(1));
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            *selected = selected.saturating_sub(1);
                        }
                        KeyCode::Enter => {
                            return choices.get(*selected).map(|c| Action::Answer(c.id.clone()));
                        }
                        KeyCode::Esc => return Some(Action::CancelLogin),
                        _ => {}
                    }
                    return None;
                }
                // Nothing to answer while the service works, or waits on
                // the browser: Enter must not start a second sign-in.
                login::LoginState::Busy(_)
                | login::LoginState::Url(_)
                | login::LoginState::Waiting { .. } => {
                    return match key.code {
                        KeyCode::Esc => Some(Action::CancelLogin),
                        // The sign-in under way is abandoned: a service
                        // waiting on a browser that will never confirm
                        // was a screen with no way out but quitting.
                        KeyCode::Tab => Some(Action::NextService),
                        _ => None,
                    };
                }
                login::LoginState::Idle | login::LoginState::Failed(_) => {
                    if key.code == KeyCode::Char(Self::GUEST_KEY) && self.guest_allowed() {
                        return Some(Action::BeginGuest);
                    }
                    if key.code == KeyCode::Tab {
                        return Some(Action::NextService);
                    }
                }
            }
        }

        match key.code {
            // "s" puts the keyboard back in the box, whether search is open
            // or not: with the box already up it did nothing at all, so
            // there was no way back to the query except closing search and
            // starting over.
            KeyCode::Char('s') if self.session.is_some() => Some(Action::BeginSearch),
            // Only when the results are what is on screen: an album opened
            // from a result stands on top of them, and escape there closed
            // the search out from under it -- taking away both views at
            // once when the user meant to leave the one they were in.
            KeyCode::Esc if self.on_search() => Some(Action::CloseSearch),
            KeyCode::Char('/') if self.on_search() => Some(Action::BeginSearch),
            // Shift-/ on most layouts, so this is the one key to remember.
            KeyCode::Char('?') if self.session.is_some() => {
                self.showing_help = true;
                None
            }
            KeyCode::Char('L') if self.session.is_some() => Some(Action::ToggleLyrics),
            // What is playing next, and where each of it came from. Capital,
            // because `q` quits and the two would be a bad pair to confuse.
            KeyCode::Char('Q') if self.session.is_some() => {
                // A view of its own: what was showing goes onto the stack
                // and back returns to it -- the history included, so the
                // two lists are never up at once.
                self.push_level();
                self.open = None;
                self.showing_queue = true;
                self.queue_list.selected = self.queue.position();
                let visible = self.over_list_visible();
                self.queue_list.scroll_into_view(visible);
                tracing::debug!(
                    queued = self.queue.len(),
                    entries = self.queue.entries().len(),
                    at = self.queue.position(),
                    selected = self.queue_list.selected,
                    offset = self.queue_list.offset,
                    visible,
                    context = ?self.queue.context_name(),
                    "queue opened"
                );
                None
            }
            // What has been played. Beside `Q` on the keyboard, and the
            // other half of the same question.
            KeyCode::Char('H') if self.session.is_some() => {
                // As the queue: a level of its own.
                self.push_level();
                self.open = None;
                self.showing_history = true;
                // From the top, which is the newest: what was just played
                // is what the eye is after.
                self.history_list = Default::default();
                None
            }
            // The bio is drawn over the artist's page rather than pushed as
            // a view, so back closes it first. Otherwise it popped the page
            // from under the reader, who landed on whatever the artist had
            // been opened from -- two views away from where they were.
            KeyCode::Esc | KeyCode::Char('[') if self.on_artist() && self.artist_bio_open => {
                self.artist_bio_open = false;
                None
            }
            // Escape steps back one view, the same as `[`: it is the key
            // people reach for to leave where they are, and leaving should
            // mean the same thing whichever of the two is pressed. It never
            // quits — `q` alone does that, since a key that usually means
            // "leave this view" should not sometimes close the app instead.
            KeyCode::Esc if self.session.is_some() => Some(Action::GoBack),
            KeyCode::Esc => None,
            // Only on an artist's page, where there is a blurb to open.
            KeyCode::Char('b') if self.on_artist() => {
                self.artist_bio_open = !self.artist_bio_open;
                // From the top each time it opens, as a page would.
                self.artist_bio_scroll = 0;
                None
            }
            // Queue the highlighted track by hand: `e` next, `E` last.
            // Both survive a change of album, which is the whole point of
            // asking for a track by name rather than playing its list.
            KeyCode::Char('e') if self.selected_track().is_some() => Some(Action::QueueTrackNext),
            KeyCode::Char('E') if self.selected_track().is_some() => Some(Action::QueueTrackLast),
            // The album and the artist of the highlighted track. A list of
            // tracks names both and could open neither, so the only way to
            // an album from your favourites was to search for it.
            //
            // `c` for the cover: `a` is the artist and every letter that
            // says "album" was already spoken for, so this names the thing
            // on screen rather than a word nobody would guess.
            KeyCode::Char('c') if self.selected_track_album().is_some() => {
                let (_, title) = self.selected_track_album()?;
                (!self.already_showing(&title)).then_some(Action::OpenTrackAlbum)
            }
            KeyCode::Char('a') if self.selected_track_artist().is_some() => {
                let (_, name) = self.selected_track_artist()?;
                (!self.already_showing(&name)).then_some(Action::OpenTrackArtist)
            }
            // The row names an album and an artist but carries the id of
            // neither -- a card from the home page, a favourite from before
            // the ids were kept. `R` has the same gap and fills it the same
            // way: fetch the track, which does carry them, then open.
            // Without this the key did nothing, in silence, on the one page
            // most people start from.
            KeyCode::Char('c') if self.selected_track().is_some() => {
                self.selected_track().map(|t| Action::FetchTrackAlbum(t.id))
            }
            KeyCode::Char('a') if self.selected_track().is_some() => self
                .selected_track()
                .map(|t| Action::FetchTrackArtist(t.id)),
            // The radio around the selected track, or whatever is playing.
            // `R` reaches this from anywhere, which is where the user is
            // when the thought occurs.
            // Already in that radio: nothing to open.
            KeyCode::Char('R') if self.track_radio().is_some_and(|s| self.radio_is_open(&s)) => None,
            KeyCode::Char('R') if self.track_radio().is_some() => Some(Action::PlayTrackRadio),
            // The nav by number, 1 through 8: there are exactly eight
            // entries, Music first and Settings last, and reaching the far
            // end with J took seven presses.
            KeyCode::Char(c @ '1'..='8') if self.session.is_some() => {
                let at = c as usize - '1' as usize;
                sidebar::Section::ALL
                    .get(at)
                    .copied()
                    .map(Action::GoToSection)
            }
            // The two arrows, as a browser has them: back through a run of
            // sections as well as of opened views, and forward again, which
            // escape alone cannot do.
            KeyCode::Char('[') if self.session.is_some() => Some(Action::GoBack),
            KeyCode::Char(']') if self.session.is_some() => Some(Action::GoForward),
            KeyCode::Char('q') => Some(Action::Quit),
            // Every view but the home page has a filter box.
            KeyCode::Char('/') if self.session.is_some() && !self.on_home() => {
                self.filtering = true;
                None
            }
            KeyCode::Enter if self.session.is_none() => Some(Action::BeginLogin),
            // j and k always drive the content. The sidebar has J and K to
            // itself, so moving through a list never depends on remembering
            // which pane last took focus.
            KeyCode::Char('j') | KeyCode::Down if self.session.is_some() => {
                if self.on_home() {
                    Some(Action::RowNext)
                } else {
                    Some(Action::TrackNext)
                }
            }
            // Within an artist's page, which is drawn over everything else.
            KeyCode::Char('l') | KeyCode::Right if self.on_artist() => Some(Action::ArtistRight),
            KeyCode::Char('h') | KeyCode::Left if self.on_artist() => Some(Action::ArtistLeft),
            // h and l move within the search results. They used to fall
            // straight through to the carousel, which in a search grid moved
            // a row of the home page nothing was drawing.
            KeyCode::Char('l') | KeyCode::Right if self.on_search() => Some(Action::SearchRight),
            KeyCode::Char('h') | KeyCode::Left if self.on_search() => Some(Action::SearchLeft),
            KeyCode::Char('k') | KeyCode::Up if self.session.is_some() => {
                if self.on_home() {
                    Some(Action::RowPrevious)
                } else {
                    Some(Action::TrackPrevious)
                }
            }
            KeyCode::Char('l') | KeyCode::Right if self.session.is_some() => {
                Some(Action::CarouselNext)
            }
            // Left, everywhere. It used to back out of an opened album --
            // the one place `h` meant something other than a direction --
            // which put the user back on the page they came from whenever
            // they reached for the left-hand end of a row. Escape and `[`
            // are what close a view; in a track list, where there is
            // nothing to the left, this simply does nothing.
            KeyCode::Char('h') | KeyCode::Left if self.session.is_some() => {
                Some(Action::CarouselPrevious)
            }
            // Favouriting the highlighted track, both ways: pressing it on
            // one that is already a favourite takes it out again.
            // Playlist edits. Each opens its question; `x` asks none.
            KeyCode::Char('P') if self.session.is_some() => {
                if let Some(track) = self.selected_track() {
                    self.prompt = Some(playlistedit::Prompt::add_to(track.id, track.title));
                }
                None
            }
            KeyCode::Char('N') if self.session.is_some() => {
                self.prompt = Some(playlistedit::Prompt::NewPlaylist {
                    name: String::new(),
                });
                None
            }
            KeyCode::Char('m') | KeyCode::Char('D') if self.session.is_some() => {
                match self.playlist_in_hand() {
                    Some(p) if p.owned => {
                        let (uuid, title) = (p.uuid.clone(), p.title.clone());
                        self.prompt = Some(match key.code {
                            KeyCode::Char('m') => playlistedit::Prompt::Rename { uuid, name: title },
                            _ => playlistedit::Prompt::Delete { uuid, title },
                        });
                    }
                    Some(_) => self.say("not one of your playlists"),
                    None => {}
                }
                None
            }
            KeyCode::Char('x')
                if self.on_settings()
                    && self.settings.current() == settings::Setting::LocalFolders =>
            {
                settings::remove_last_folder(&mut self.config)?;
                self.save_config();
                Some(Action::SettingChanged(settings::Setting::LocalFolders))
            }
            KeyCode::Char('x') if self.open_owned_playlist().is_some() => {
                let uuid = self.open_owned_playlist()?;
                let track = self.selected_track()?.id;
                Some(Action::RemoveFromPlaylist { uuid, track })
            }
            KeyCode::Char('A') if self.session.is_some() => Some(Action::ToggleFavourite),
            // The collections' own `A`: the artist, album, playlist or mix
            // under the cursor. Not `A` itself: on an artist's page that
            // key still means the top track selected.
            KeyCode::Char('F') if self.selected_favourable().is_some() => {
                Some(Action::ToggleFollow)
            }
            // The web client's "See all": the page hands back six items per
            // row, and this opens the rest of them.
            KeyCode::Char('o') if self.session.is_some() => Some(Action::SeeAll),
            // The transport, which the player bar has always drawn but
            // nothing was behind.
            KeyCode::Char('n') if self.session.is_some() => Some(Action::QueueNext),
            KeyCode::Char('p') if self.session.is_some() => Some(Action::QueuePrevious),
            KeyCode::Char('z') if self.session.is_some() => Some(Action::ToggleShuffle),
            KeyCode::Char('r') if self.session.is_some() => Some(Action::CycleRepeat),
            KeyCode::Char('S') if self.session.is_some() => Some(Action::CycleSleep),
            KeyCode::Char('t') if self.on_search() => Some(Action::NextSearchTab),
            // Tab keeps working in search too, since it is what the web
            // client's own tab strip responds to.
            KeyCode::Tab if self.on_search() => Some(Action::NextSearchTab),
            // Shift-J/K reach the sidebar without moving focus first. Kept
            // alongside the focus-aware j/k rather than replaced by them:
            // it is a shortcut people learn and then rely on.
            KeyCode::Char('J') if self.session.is_some() => Some(Action::SidebarNext),
            KeyCode::Char('K') if self.session.is_some() => Some(Action::SidebarPrevious),
            KeyCode::Enter if self.session.is_some() => Some(Action::ActivateSelection),
            KeyCode::Char(' ') if self.session.is_some() => Some(Action::TogglePause),
            // Along the track, in steps the ear can place. Only with a track
            // up: a seek with nothing playing has nowhere to go.
            KeyCode::Char('.') if self.now_playing.track.is_some() => Some(Action::SeekForward),
            KeyCode::Char(',') if self.now_playing.track.is_some() => Some(Action::SeekBack),
            _ => None,
        }
    }
}

/// Fetch whatever a collection holds and send it back as tracks.
///
/// Shared by opening a card and by "See all", so an album reached either way
/// arrives through the same path — and a reply for a view the user has since
/// left is dropped by `for_title` rather than shown under the wrong heading.
/// The album a track names, as something a key can open.
fn album_of(track: &crate::domain::Track) -> Option<(Collection, String)> {
    Some((Collection::Album(track.album_id.clone()?), track.album.clone()))
}

/// The artist a track names, the same way.
///
/// The first credited one: TIDAL lists several on a collaboration and the
/// web opens the first, which is also the one the row shows.
fn artist_of(track: &crate::domain::Track) -> Option<(Collection, String)> {
    Some((
        Collection::Artist(track.artist_id.clone()?),
        first_artist(&track.artist),
    ))
}

/// The first of a track's credited artists: TIDAL lists several on a
/// collaboration, and the first is the one the row shows and the web opens.
fn first_artist(credited: &str) -> String {
    credited
        .split(',')
        .next()
        .unwrap_or(credited)
        .trim()
        .to_string()
}

/// The tail of every playlist edit: the word for the status line, and
/// the playlists again when the list itself changed -- made, renamed,
/// deleted. A track in or out only moves a count, applied in place: the
/// fetch of every playlist was most of the wait.
async fn finish_edit(
    client: &std::sync::Arc<crate::catalogue::Session>,
    tx: &tokio::sync::mpsc::UnboundedSender<Action>,
    done: crate::catalogue::Result<(String, String, bool, i32)>,
) {
    match done {
        Ok((message, uuid, deleted, count_delta)) => {
            if count_delta == 0 {
                if let Ok(p) = client.playlists().await {
                    let _ = tx.send(Action::PlaylistsLoaded(p));
                }
                // The hearts too: a new playlist of the account's own is
                // among its favourites on TIDAL, and the grid draws it
                // from this list, which was still the one from sign-in.
                if let Ok(ids) = client.favourite_ids().await {
                    let _ = tx.send(Action::FavouriteIdsLoaded(ids));
                }
            }
            let _ = tx.send(Action::PlaylistEdited {
                message,
                uuid,
                deleted,
                count_delta,
            });
        }
        Err(e) => {
            let _ = tx.send(Action::Error(e.to_string()));
        }
    }
}

fn open_collection(
    app: &App,
    target: Option<Collection>,
    for_title: String,
    action_tx: &tokio::sync::mpsc::UnboundedSender<Action>,
) {
    // Both halves used to fall out of here without a word, and a session
    // that expired while the app was running made every open do exactly
    // nothing: no request, no error, no line in the log.
    let (Some(session), Some(target)) = (&app.session, target) else {
        if app.session.is_none() {
            tracing::warn!("cannot open a collection: there is no session");
            let _ = action_tx.send(Action::Error(
                "the session has expired — restart to sign in again".into(),
            ));
        }
        return;
    };
    let (client, tx) = (std::sync::Arc::clone(session), action_tx.clone());
    tokio::spawn(async move {
        let loaded = match &target {
            Collection::Playlist(uuid) => client.playlist_tracks(uuid).await,
            // An album is a page of its own, the artist's shape: its tracks
            // and, under them, the rows the web draws -- more by the
            // artist, other versions, related records.
            Collection::Album(id) => {
                match client.album_page(id, &for_title).await {
                    Ok(page) => {
                        let _ = tx.send(Action::ArtistLoaded(Box::new(page)));
                        return;
                    }
                    Err(e) => Err(e),
                }
            }
            // Named alongside, best-effort: a radio that cannot be saved
            // still plays.
            Collection::Radio(seed) => {
                let (tracks, playlist) =
                    tokio::join!(client.track_radio(seed), client.radio_playlist(seed));
                if let Ok(Some(playlist)) = playlist {
                    let seed = seed.clone();
                    let _ = tx.send(Action::RadioNamed { seed, playlist });
                }
                tracks
            }
            // A genre, a mood or a decade is a page of rows, not a list of
            // tracks — the same shape Explore itself is, so it comes back
            // as its own reply and takes the pane.
            Collection::Page { title, path } => {
                match client.page_of_rows(path).await {
                    Ok(home) => {
                        let _ = tx.send(Action::PageLoaded {
                            title: title.clone(),
                            home: Box::new(home),
                        });
                        return;
                    }
                    Err(e) => Err(e),
                }
            }
            // An artist is a page of its own — three sections rather than
            // one list — so it comes back as its own reply.
            Collection::Artist(id) => match client.artist_page(id).await {
                Ok(page) => {
                    let _ = tx.send(Action::ArtistLoaded(Box::new(page)));
                    return;
                }
                Err(e) => Err(e),
            },
        };
        match loaded {
            Ok(tracks) => {
                let _ = tx.send(Action::CollectionLoaded { for_title, tracks });
            }
            Err(crate::catalogue::Error::Unauthorized) => {
                let _ = tx.send(Action::SessionExpired);
            }
            Err(e) => {
                let _ = tx.send(Action::Error(e.to_string()));
            }
        }
    });
}

/// Fetch a track's stream and hand it to the player.
///
/// Shared by starting a track by hand and by the queue moving on, so the two
/// cannot drift: the bar is filled in from what is known now, and the
/// response says what was actually delivered.
fn start_track(
    app: &mut App,
    track: Option<crate::domain::Track>,
    cmd_tx: &std::sync::mpsc::Sender<crate::playback::Cmd>,
) {
    let (Some(session), Some(track)) = (&app.session, track) else {
        return;
    };
    // What the user allows. Both paths into playback come through here, so a
    // blocked track cannot be reached by starting one by hand and then
    // letting the queue run on past it.
    if let Some(why) = app.why_blocked(&track) {
        tracing::info!("not playing {:?}: {why}", track.title);
        app.say(format!("{} — {why}", track.title));
        return;
    }
    // What the user asked for, not what this build prefers. The response
    // still says what was actually delivered, and the badge reads that.
    let wanted = app.config.audio.quality();
    // Remembered as it starts rather than as it ends: a track skipped half
    // way through is still one the user heard and may want to name later.
    app.queue.remember(&track);
    if let Some(path) = &app.history_file {
        crate::playback::save_history(path, app.queue.history());
    }
    // The engine fetches the stream itself and reports back as events: a
    // rejected session as `SessionExpired`, anything else as `Failed`.
    session.set_quality(wanted);
    let _ = cmd_tx.send(crate::playback::Cmd::Play(track.id.clone()));
    app.now_playing.track = Some(track);
    app.now_playing.playing = true;
}

/// The track a card stands for, when it stands for one.
///
/// A card carries what the page said about the item — enough for a row, and
/// the same fields the player fills in when a track is started from one.
fn track_from_card(card: &carousel::Card) -> Option<crate::domain::Track> {
    let Some(carousel::Target::Track(id)) = card.target.as_ref() else {
        return None;
    };
    Some(crate::domain::Track {
        id: crate::domain::TrackId(id.clone()),
        title: card.title.clone(),
        artist: card.subtitle.clone(),
        album: card.detail.clone(),
        cover: card.cover_url.clone(),
        ..crate::domain::Track::sample("", "", card.duration)
    })
}

pub async fn run(
    terminal: &mut DefaultTerminal,
    picker: ratatui_image::picker::Picker,
) -> anyhow::Result<()> {
    let config = crate::config::Config::load()?;
    // Before the first frame: the icons are read as the screen is drawn, so
    // setting this afterwards would open the app on the wrong set.
    icons::set_nerd_font(config.ui.nerd_font);

    // What the user rebound, and what could not be read. A typo names an
    // action that does nothing, which is worth saying rather than leaving
    // the user to wonder why their key is dead.
    let (keys, mut problems) = keymap::Keymap::from_config(&config.keys);
    for problem in &problems {
        tracing::warn!("keys: {problem}");
    }

    // The colours, the same way: a theme that cannot be read costs the
    // colours it names and is said out loud, rather than stopping the app.
    let (palette, colour_problems) = theme::Palette::from_config(&config.ui);
    for problem in &colour_problems {
        tracing::warn!("theme: {problem}");
    }
    problems.extend(colour_problems);

    let mut app = App {
        config: config.clone(),
        config_path: crate::config::paths::config_file(),
        keymap: keys,
        palette,
        status: problems.first().map(|p| format!("config: {p}")),
        // Stamped like any other message, so a config warning does not sit
        // in the corner for the rest of the session. The log keeps it.
        status_since: problems.first().map(|_| std::time::Instant::now()),
        ..App::default()
    };
    // What the last run played, so the history is not blank on every
    // start: a track heard yesterday is one the user may want to name.
    // The engine reads the shared equaliser from its first frame.
    app.apply_equalizer();
    app.history_file = crate::config::paths::cache_dir().map(|d| d.join("history.json"));
    if let Some(path) = &app.history_file {
        app.queue
            .restore_history(crate::playback::load_history(path));
    }
    // Resume an existing session rather than making the user log in again.
    //
    // Every branch below logs what it decided. Without that, a spurious
    // sign-out is indistinguishable from a missing file, an unreadable one,
    // or a token judged expired — which cost several rounds of guessing.
    match crate::catalogue::provider_for(&config) {
        Ok(provider) => {
            match provider.restore().await {
                Ok(Some(session)) => {
                    tracing::info!("resuming the stored {} session", provider.name());
                    let session = crate::catalogue::Session::open(std::sync::Arc::clone(&provider), session);
                    app.session = Some(std::sync::Arc::new(session));
                }
                Ok(None) => {
                    tracing::info!(
                        "no stored {} session; showing the login screen",
                        provider.name()
                    );
                    app.login = app.login_prompt();
                }
                // Not forgotten: a launch on a bad network lands here with
                // a perfectly good token on disk, and Enter tries it again.
                Err(e) => {
                    tracing::warn!(
                        "the stored {} session could not be resumed: {e:#}",
                        provider.name()
                    );
                    app.login = login::LoginState::Failed(format!("{e:#}"));
                }
            }
            app.provider = Some(provider);
        }
        Err(e) => {
            tracing::error!("could not set up the provider: {e:#}");
            app.login = login::LoginState::Failed(format!("{e:#}"));
        }
    }

    let mut events = EventStream::new();
    // ~30fps ceiling: a burst of scroll events coalesces into one frame.
    let mut ticker = tokio::time::interval(Duration::from_millis(33));
    let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel::<Action>();
    // Discord, when the config names an application to show the track as.
    let application_id = config.discord.application_id.trim();
    if !application_id.is_empty() {
        app.presence = Some(crate::presence::spawn(application_id.to_owned()));
    }
    // A folder of files has nothing stored to resume and nothing to ask:
    // it is read now, without waiting for Enter.
    if config.provider.kind == crate::config::ProviderKind::Local && app.session.is_none() {
        let _ = action_tx.send(Action::BeginLogin);
    }
    // The scrobbling services, linked off the main thread: Last.fm and
    // Libre.fm wait on the browser the first time.
    if crate::scrobble::configured(&config.scrobble) {
        let (scrobble, tx) = (config.scrobble.clone(), action_tx.clone());
        tokio::spawn(async move {
            let file = crate::config::paths::scrobble_file();
            let (linked, refused) = crate::scrobble::link(&scrobble, file.as_deref()).await;
            let _ = tx.send(Action::ScrobblersLinked(linked, refused));
        });
    }
    let (cmd_tx, mut playback_rx) = crate::playback::spawn();

    // The OS media keys. They reach the desktop, never the terminal, so the
    // app registers itself as a media player and the commands come back as
    // the same actions the keyboard sends.
    let (media_tx, media_rx) = mediakeys::channel();
    spawn_media_keys(action_tx.clone(), media_rx);

    // A resumed session needs the same engine and fetches a fresh login
    // gets. Without this the app opens already signed in and shows nothing.
    if app.session.is_some() {
        let _ = action_tx.send(Action::SessionReady);
    }

    // Cover art. Detection ran before the terminal was put into raw mode —
    // it queries stdout and reads stdin, so it cannot happen from in here.
    let (art_tx, mut art_rx) = tokio::sync::mpsc::unbounded_channel();
    // Cards are sized from the terminal's own cell, which the picker
    // measured while probing for an image protocol. Without this they
    // assume a cell exactly twice as tall as it is wide and a cover fills
    // only part of its card on everything else.
    let cell = picker.font_size();
    carousel::set_cell_size(cell.width, cell.height);
    tracing::info!(
        "terminal cell is {}x{}px, cards are {} columns",
        cell.width,
        cell.height,
        carousel::card_width()
    );
    app.artwork = Some(artwork::Artwork::with_picker(picker, art_tx));

    // The level from the config file, so a session starts where the last
    // one left off rather than at full whatever the setting says.
    let _ = cmd_tx.send(crate::playback::Cmd::Volume(app.config.audio.volume));

    while !app.should_quit {
        // A modal that covered the artwork leaves its text there: the image
        // cells are marked `Skip`, so the next frame writes nothing over
        // them. Clearing first forces the whole screen to be sent again,
        // which is what puts the covers back.
        // Tell the desktop what is playing. Sent every frame, filtered at
        // the other end: only a real change is announced, since the position
        // moves on every tick and a metadata signal that often makes the
        // desktop redraw its player for nothing.
        let _ = media_tx.send(mediakeys::State {
            track: app.now_playing.track.clone(),
            playing: app.now_playing.playing,
            position: app.now_playing.position,
            // More than one track in the queue means there is something
            // either side of this one to reach.
            can_next: app.queue.len() > 1,
            can_previous: app.queue.len() > 1,
        });

        if app.take_repaint() {
            // Not `Terminal::clear`: since ratatui 0.30 it reads the cursor
            // position back from the terminal first, and with the event
            // stream holding stdin that read times out after two seconds
            // and the error ends the run. The screen is wiped and the
            // buffer the next draw diffs against is emptied, which is the
            // rest of what `clear` does.
            use ratatui::backend::Backend as _;
            terminal
                .backend_mut()
                .clear_region(ratatui::backend::ClearType::All)?;
            terminal.swap_buffers();
            terminal.current_buffer_mut().reset();
            terminal.swap_buffers();
        }
        terminal.draw(|frame| draw(frame, &mut app))?;

        let action = tokio::select! {
            _ = ticker.tick() => Some(Action::Tick),
            Some(action) = action_rx.recv() => Some(action),
            Some(event) = playback_rx.recv() => Some(Action::Playback(event)),
            Some(loaded) = art_rx.recv() => {
                // Not an Action: nothing about app state changes, the cache
                // just gains a decoded cover for the next frame.
                if let Some(art) = app.artwork.as_mut() {
                    art.insert(loaded);
                }
                None
            }
            maybe_event = events.next() => match maybe_event {
                // Windows emits both press and release; without this filter
                // every keystroke counts twice.
                Some(Ok(Event::Key(key))) if key.is_press() => Some(Action::Key(key)),
                Some(Ok(_)) => None,
                Some(Err(e)) => Some(Action::Error(e.to_string())),
                None => Some(Action::Quit),
            },
        };

        // Actions may cascade; apply until the chain settles.
        let mut next = action;
        while let Some(action) = next {
            // What happened, for a run that has to be replayed by reading
            // its log: the key, what it became, and what was under it.
            // Debug only -- `RUST_LOG=sonora_tui=debug` -- so the default log
            // stays as it is, and the tick is left out or it would be
            // thirty lines a second.
            if tracing::enabled!(tracing::Level::DEBUG) && !matches!(action, Action::Tick) {
                let shown: String = format!("{action:?}").chars().take(120).collect();
                tracing::debug!(
                    action = %shown,
                    showing = ?app.showing(),
                    section = ?app.sidebar.section(),
                    selected = ?app.selected_track().map(|t| t.title),
                    queued = app.queue.len(),
                    "action"
                );
            }
            // Side effects that must not block the render loop are spawned
            // here and report back through action_tx.
            match &action {
                // The service may have been switched since the start, so
                // the config read here is the app's, not the one loaded.
                Action::Connect(secret) if app.session.is_none() => {
                    let method = music::SignIn::Credentials {
                        server: app.config.subsonic.server.trim().to_owned(),
                        username: app.config.subsonic.username.trim().to_owned(),
                        password: secret.0.clone(),
                    };
                    start_sign_in(&mut app, method, &action_tx);
                }
                // The old engine falls silent; the new one attaches at
                // `SessionReady`.
                Action::SwitchProvider => {
                    let _ = cmd_tx.send(crate::playback::Cmd::Stop);
                }
                // The way in the service offers, apart from going without an
                // account: a code for TIDAL, a link for Spotify, cookies for
                // the rest. A Subsonic server went through `Connect` above.
                Action::BeginLogin
                    if app.session.is_none()
                        && app.config.provider.kind != crate::config::ProviderKind::Subsonic =>
                {
                    let method = match app.config.provider.kind {
                        // Nothing to read without a folder named; the
                        // screen says what to set.
                        crate::config::ProviderKind::Local => {
                            let folders = app.config.local.folders();
                            (!folders.is_empty()).then_some(music::SignIn::Path(folders))
                        }
                        _ => Some(
                            app.provider
                                .as_ref()
                                .map(|p| p.sign_in_options())
                                .unwrap_or_default()
                                .into_iter()
                                .find(|m| *m != music::SignIn::Anonymous)
                                .unwrap_or(music::SignIn::Default),
                        ),
                    };
                    if let Some(method) = method {
                        start_sign_in(&mut app, method, &action_tx);
                    }
                }
                Action::BeginGuest if app.session.is_none() => {
                    start_sign_in(&mut app, music::SignIn::Anonymous, &action_tx);
                }
                Action::Answer(text) => {
                    if let Some(input) = &app.login_input {
                        let _ = input.send(text.clone());
                    }
                }
                // The engine on the session's own factory, then the fetches
                // every signed-in screen needs. Both after a fresh login and
                // when a stored session is resumed: the second case is easy
                // to forget, and forgetting it means the app opens already
                // signed in and shows nothing at all.
                Action::SessionReady => {
                    if let Some(session) = &app.session {
                        let _ = cmd_tx.send(crate::playback::Cmd::Attach {
                            factory: session.playback(),
                            meta: session.stream_meta(),
                            options: app.playback_options(),
                        });
                        load_collection(std::sync::Arc::clone(session), action_tx.clone());
                    }
                }
                // Read before `update` clears the page: the same reason an
                // opened card's title is.
                Action::PlayTrackRadio => {
                    if let Some(id) = app.track_radio() {
                        let title = app.track_radio_title();
                        open_collection(
                            &app,
                            Some(Collection::Radio(id)),
                            format!("{title} Radio"),
                            &action_tx,
                        );
                    }
                }
                // The card is a track whose radio the page did not send.
                // Fetch the track for it, then open the radio the same way
                // the key would have.
                Action::FetchTrackAlbum(id) | Action::FetchTrackArtist(id) => {
                    if let Some(session) = &app.session {
                        let (client, tx) =
                            (std::sync::Arc::clone(session), action_tx.clone());
                        let id = id.clone();
                        let album = matches!(action, Action::FetchTrackAlbum(_));
                        tokio::spawn(async move {
                            match client.track(&id).await {
                                Ok(track) => {
                                    let found = if album {
                                        album_of(&track)
                                    } else {
                                        artist_of(&track)
                                    };
                                    match found {
                                        Some((target, title)) => {
                                            // The track's own artist and sleeve
                                            // head an album; an artist's page
                                            // brings its own picture.
                                            let (subtitle, cover) = if album {
                                                (track.artist.clone(), track.cover.clone())
                                            } else {
                                                (String::new(), None)
                                            };
                                            let _ = tx.send(Action::OpenTarget {
                                                target,
                                                title,
                                                subtitle,
                                                cover,
                                            });
                                        }
                                        None => tracing::info!(
                                            "{:?} names no {}",
                                            track.title,
                                            if album { "album" } else { "artist" }
                                        ),
                                    }
                                }
                                Err(e) => tracing::warn!("could not fetch the track: {e}"),
                            }
                        });
                    }
                }
                Action::OpenTarget { target, title, .. } => {
                    open_collection(&app, Some(target.clone()), title.clone(), &action_tx);
                }
                // A card that opens something: a playlist, an album, an
                // artist, a mix, a genre page. Anything else falls through
                // to the arm below, which plays a track.
                Action::ActivateSelection if app.what_enter_opens().is_some() => {
                    if let Some((target, for_title)) = app.what_enter_opens() {
                        open_collection(&app, Some(target), for_title, &action_tx);
                    }
                }
                // Taking a track out of the queue. Only the loop can do
                // it: dropping what is playing has to stop the audio
                // thread, and `update` has no channel to it.
                Action::RemoveQueued(at) => {
                    let was_playing = app.queue.remove(*at);
                    // The cursor follows: the row that was highlighted is
                    // gone, so the one that took its place is highlighted
                    // instead, and the last row steps back.
                    app.queue_list.selected = app
                        .queue_list
                        .selected
                        .min(app.queue.len().saturating_sub(1));
                    if was_playing {
                        match app.queue.current().cloned() {
                            Some(next) => start_track(&mut app, Some(next), &cmd_tx),
                            None => {
                                let _ = cmd_tx.send(crate::playback::Cmd::Stop);
                                app.now_playing.playing = false;
                                app.now_playing.track = None;
                            }
                        }
                    }
                }
                // Jump straight to a queued track, skipping what is between.
                Action::JumpInQueue(at) => {
                    if let Some(track) = app.queue.jump_to(*at).cloned() {
                        app.queue_list.selected = *at;
                        start_track(&mut app, Some(track), &cmd_tx);
                    }
                }
                // The album or the artist behind the highlighted track,
                // opened the same way enter opens a card.
                Action::OpenTrackAlbum => {
                    if let Some((target, title)) = app.selected_track_album() {
                        open_collection(&app, Some(target), title, &action_tx);
                    }
                }
                Action::OpenTrackArtist => {
                    if let Some((target, title)) = app.selected_track_artist() {
                        open_collection(&app, Some(target), title, &action_tx);
                    }
                }
                Action::ToggleFollow => {
                    if let (Some(session), Some((what, name))) =
                        (&app.session, app.selected_favourable())
                    {
                        let client = std::sync::Arc::clone(session);
                        let tx = action_tx.clone();
                        let wanted = !app.is_favourite(&what);
                        tokio::spawn(async move {
                            if let Err(e) = client.set_favourite(&what, wanted).await {
                                let _ = tx.send(Action::Error(e.to_string()));
                                return;
                            }
                            let _ = tx.send(Action::FollowChanged {
                                what: what.clone(),
                                name,
                                following: wanted,
                            });
                            // The section that lists these is read again,
                            // so it shows the change rather than the last
                            // launch.
                            match what {
                                Favourable::Artist(_) => {
                                    if let Ok(a) = client.artists().await {
                                        let _ = tx.send(Action::ArtistsLoaded(a));
                                    }
                                }
                                Favourable::Album(_) => {
                                    if let Ok(a) = client.albums().await {
                                        let _ = tx.send(Action::AlbumsLoaded(a));
                                    }
                                }
                                Favourable::Playlist(_) => {
                                    if let Ok(p) = client.playlists().await {
                                        let _ = tx.send(Action::PlaylistsLoaded(p));
                                    }
                                }
                            }
                        });
                    }
                }
                // The settings that something off the loop reads: the
                // scrobblers are linked again from the config as it is now
                // -- Last.fm and Libre.fm through the browser if they have
                // not been -- and the Discord presence starts over on the
                // new application, or stops on none.
                Action::SettingChanged(setting) if setting.is_scrobbling() => {
                    let (scrobble, tx) = (app.config.scrobble.clone(), action_tx.clone());
                    app.say("linking the scrobblers…");
                    tokio::spawn(async move {
                        let (linked, refused) = match crate::scrobble::configured(&scrobble) {
                            true => {
                                let file = crate::config::paths::scrobble_file();
                                crate::scrobble::link(&scrobble, file.as_deref()).await
                            }
                            false => (Vec::new(), Vec::new()),
                        };
                        let _ = tx.send(Action::ScrobblersLinked(linked, refused));
                    });
                }
                Action::SettingChanged(settings::Setting::Discord) => {
                    if let Some(presence) = app.presence.take() {
                        let _ = presence.send(crate::presence::Presence::Off);
                    }
                    let id = app.config.discord.application_id.trim();
                    if !id.is_empty() {
                        app.presence = Some(crate::presence::spawn(id.to_owned()));
                    }
                }
                Action::Edit(edit) => {
                    if let Some(session) = &app.session {
                        let (client, tx) = (std::sync::Arc::clone(session), action_tx.clone());
                        let edit = edit.clone();
                        tokio::spawn(async move {
                            use playlistedit::Edit;
                            let done = match edit {
                                Edit::AddTo { uuid, title, track } => client
                                    .add_track_to_playlist(&uuid, &track)
                                    .await
                                    .map(|()| (format!("added to {title}"), uuid, false, 1)),
                                Edit::Create { name, then_add } => {
                                    match client.create_playlist(&name).await {
                                        Ok(uuid) => match then_add {
                                            Some(track) => client
                                                .add_track_to_playlist(&uuid, &track)
                                                .await
                                                .map(|()| (format!("added to {name}"), uuid, false, 0)),
                                            None => Ok((format!("created {name}"), uuid, false, 0)),
                                        },
                                        Err(e) => Err(e),
                                    }
                                }
                                Edit::Rename { uuid, name } => client
                                    .rename_playlist(&uuid, &name)
                                    .await
                                    .map(|()| (format!("renamed to {name}"), uuid, false, 0)),
                                Edit::Delete { uuid } => client
                                    .delete_playlist(&uuid)
                                    .await
                                    .map(|()| ("playlist deleted".to_string(), uuid, true, 0)),
                            };
                            finish_edit(&client, &tx, done).await;
                        });
                    }
                }
                Action::RemoveFromPlaylist { uuid, track } => {
                    if let Some(session) = &app.session {
                        let (client, tx) = (std::sync::Arc::clone(session), action_tx.clone());
                        let (uuid, track) = (uuid.clone(), track.clone());
                        tokio::spawn(async move {
                            let done = client
                                .remove_track_from_playlist(&uuid, &track)
                                .await
                                .map(|()| ("taken out of the playlist".to_string(), uuid, false, -1));
                            finish_edit(&client, &tx, done).await;
                        });
                    }
                }
                // The open playlist shows the edit: its tracks again.
                Action::PlaylistEdited {
                    uuid,
                    deleted: false,
                    ..
                } => {
                    if let Some(open) = app.open.as_ref() {
                        if matches!(&open.target, Some(Collection::Playlist(o)) if o == uuid) {
                            open_collection(
                                &app,
                                Some(Collection::Playlist(uuid.clone())),
                                open.title.clone(),
                                &action_tx,
                            );
                        }
                    }
                }
                Action::ToggleFavourite => {
                    let selected = app.selected_track();
                    if let (Some(session), Some(track)) = (&app.session, selected.as_ref()) {
                        let client = std::sync::Arc::clone(session);
                        let (id, tx) = (track.id.clone(), action_tx.clone());
                        // What it should become, decided here so the request
                        // matches what the user saw when they pressed it.
                        let wanted = !app.favourites.contains(&id);
                        tokio::spawn(async move {
                            match client.set_track_favourite(&id, wanted).await {
                                // The reply carries the id, so a slow one
                                // cannot mark whatever is selected by the
                                // time it lands.
                                Ok(()) => {
                                    let _ = tx.send(Action::FavouriteChanged {
                                        id,
                                        favourite: wanted,
                                    });
                                }
                                Err(e) => {
                                    let _ = tx.send(Action::Error(e.to_string()));
                                }
                            }
                        });
                    }
                }
                Action::ActivateSelection => {
                    // Playing from a list queues the whole list, so what
                    // follows this track is already decided.
                    let (list, at, named) = app.selected_list();
                    tracing::debug!(
                        list = list.len(),
                        at,
                        named = ?named,
                        showing = ?app.showing(),
                        "activate: the list under the selection"
                    );
                    if !list.is_empty() {
                        let shuffled = app.queue.shuffled();
                        // A new context rather than a new queue: what the
                        // user put next by hand outlives the album it was
                        // queued over, and the radio trailing the old one
                        // goes. Repeat is the player's mode, not the
                        // queue's contents, so it is left alone.
                        app.queue.start_context(list, at, named.clone());
                        if shuffled {
                            app.queue
                                .set_shuffled(true, &mut crate::playback::clock_rng());
                        }
                    }
                    let selected = app.selected_track();
                    start_track(&mut app, selected, &cmd_tx);
                }
                // The queue decided which track; this fetches it. Same path
                // as starting one by hand, so a queued track and a chosen
                // one cannot drift apart.
                Action::PlayQueued => {
                    let next = app.queue.current().cloned();
                    start_track(&mut app, next, &cmd_tx);
                }
                // Autoplay: the queue is nearly out. The station built on
                // its last track is fetched and appended -- so what follows
                // is a queue like any other and the skip keys work on it.
                Action::Autoplay { seed } => {
                    if let Some(session) = &app.session {
                        let (client, tx) =
                            (std::sync::Arc::clone(session), action_tx.clone());
                        let seed = seed.clone();
                        tokio::spawn(async move {
                            match client.track_radio(&seed).await {
                                Ok(tracks) if !tracks.is_empty() => {
                                    tracing::info!("autoplay: {} tracks", tracks.len());
                                    let _ = tx.send(Action::QueueRadio(tracks));
                                }
                                Ok(_) => {
                                    tracing::info!("autoplay: nothing follows {seed:?}");
                                    let _ = tx.send(Action::AutoplayFailed(seed.clone()));
                                }
                                Err(e) => {
                                    tracing::warn!("autoplay failed: {e}");
                                    let _ = tx.send(Action::AutoplayFailed(seed.clone()));
                                }
                            }
                        });
                    }
                }
                Action::RunSearch(query) => {
                    let query = query.clone();
                    if let Some(session) = &app.session {
                        let (client, t) =
                            (std::sync::Arc::clone(session), action_tx.clone());
                        tokio::spawn(async move {
                            match client.search(&query).await {
                                Ok(results) => {
                                    tracing::info!("search {query:?}: {} results", results.total());
                                    let _ = t.send(Action::SearchLoaded(Box::new(results)));
                                }
                                Err(crate::catalogue::Error::Unauthorized) => {
                                    let _ = t.send(Action::SessionExpired);
                                }
                                Err(e) => {
                                    tracing::warn!("search {query:?} failed: {e}");
                                    let _ = t.send(Action::Error(e.to_string()));
                                }
                            }
                        });
                    }
                }
                Action::LoadHome => {
                    if let Some(session) = &app.session {
                        let (client, t) =
                            (std::sync::Arc::clone(session), action_tx.clone());
                        tokio::spawn(async move {
                            match client.home().await {
                                Ok(home) => {
                                    tracing::info!("loaded home: {} rows", home.rows.len());
                                    let _ = t.send(Action::HomeLoaded(Box::new(home)));
                                }
                                Err(e) => {
                                    tracing::warn!("could not load the home page: {e}");
                                    let _ = t.send(Action::Error(e.to_string()));
                                }
                            }
                        });
                    }
                }
                // The same three fetches sign-in makes: whichever of them
                // was dropped fills in, and the ones already held are
                // written back unchanged.
                Action::LoadFavourites => {
                    if let Some(session) = &app.session {
                        load_collection(std::sync::Arc::clone(session), action_tx.clone());
                    }
                }
                Action::LoadFeed => {
                    if let Some(session) = &app.session {
                        let (client, t) =
                            (std::sync::Arc::clone(session), action_tx.clone());
                        tokio::spawn(async move {
                            match client.feed().await {
                                Ok(cards) => {
                                    tracing::info!("loaded {} feed cards", cards.len());
                                    let _ = t.send(Action::FeedLoaded(cards));
                                }
                                Err(e) => {
                                    tracing::warn!("could not load the feed: {e}");
                                    let _ = t.send(Action::Error(e.to_string()));
                                }
                            }
                        });
                    }
                }
                Action::LoadExplore => {
                    if let Some(session) = &app.session {
                        let (client, t) =
                            (std::sync::Arc::clone(session), action_tx.clone());
                        tokio::spawn(async move {
                            match client.explore().await {
                                Ok(home) => {
                                    tracing::info!("loaded explore: {} rows", home.rows.len());
                                    let _ = t.send(Action::ExploreLoaded(Box::new(home)));
                                }
                                Err(e) => {
                                    tracing::warn!("could not load explore: {e}");
                                    let _ = t.send(Action::Error(e.to_string()));
                                }
                            }
                        });
                    }
                }
                Action::TogglePause => {
                    let cmd = if app.now_playing.playing {
                        crate::playback::Cmd::Pause
                    } else {
                        crate::playback::Cmd::Resume
                    };
                    let _ = cmd_tx.send(cmd);
                }
                Action::SetVolume(v) => {
                    let _ = cmd_tx.send(crate::playback::Cmd::Volume(*v));
                }
                Action::LookForCdm => {
                    let tx = action_tx.clone();
                    tokio::spawn(async move {
                        // Reads browser folders: off the loop's thread.
                        let found = tokio::task::spawn_blocking(music::drm::find)
                            .await
                            .ok()
                            .flatten();
                        let _ = tx.send(Action::CdmFound(found.map(|f| f.origin)));
                    });
                }
                // Download, show the terms, wait for the word, install. The
                // archive stays in this task: the screen only sees the terms.
                Action::DownloadCdm => {
                    let (answer, asked) = tokio::sync::oneshot::channel();
                    app.cdm_answer = Some(answer);
                    let tx = action_tx.clone();
                    tokio::spawn(async move {
                        let offer = match music::drm::offer().await {
                            Ok(offer) => offer,
                            Err(e) => {
                                tracing::warn!("widevine: cannot download the module: {e:#}");
                                let _ = tx.send(Action::CdmFailed(format!(
                                    "could not download the Widevine module: {e:#}"
                                )));
                                return;
                            }
                        };
                        let _ = tx.send(Action::CdmOffered {
                            version: offer.version().to_owned(),
                            license: offer.license().to_owned(),
                        });
                        if asked.await != Ok(true) {
                            return;
                        }
                        match tokio::task::spawn_blocking(move || offer.install()).await {
                            Ok(Ok(found)) => {
                                tracing::info!("widevine: installed into the store");
                                let _ = tx.send(Action::CdmReady(found.origin));
                            }
                            Ok(Err(e)) => {
                                tracing::warn!("widevine: cannot install the module: {e:#}");
                                let _ = tx.send(Action::CdmFailed(format!(
                                    "could not install the Widevine module: {e:#}"
                                )));
                            }
                            Err(_) => {
                                let _ = tx.send(Action::CdmFailed(
                                    "the Widevine install task went away".into(),
                                ));
                            }
                        }
                    });
                }
                Action::Preload(id) => {
                    let _ = cmd_tx.send(crate::playback::Cmd::Preload(id.clone()));
                }
                Action::LoadLyrics(id) => {
                    if let (Some(session), Some(track)) =
                        (&app.session, app.now_playing.track.clone())
                    {
                        if track.id == *id {
                            let query = lyricsview::query(&track, session.slug());
                            let (id, tx) = (id.clone(), action_tx.clone());
                            tokio::spawn(async move {
                                let found = lyricsview::search(query).await;
                                let _ = tx.send(Action::LyricsLoaded { id, found });
                            });
                        }
                    }
                }
                // A fresh engine on the same session, picking the track
                // back up where it was, paused if it was paused.
                Action::RestartEngine => {
                    if let Some(session) = &app.session {
                        let _ = cmd_tx.send(crate::playback::Cmd::Attach {
                            factory: session.playback(),
                            meta: session.stream_meta(),
                            options: app.playback_options(),
                        });
                        if let Some(track) = &app.now_playing.track {
                            let _ = cmd_tx.send(crate::playback::Cmd::PlayAt(
                                track.id.clone(),
                                app.now_playing.position,
                            ));
                            if !app.now_playing.playing {
                                let _ = cmd_tx.send(crate::playback::Cmd::Pause);
                            }
                        }
                    }
                }
                Action::SeekForward | Action::SeekBack => {
                    if let Some(to) = app.seek_target(matches!(action, Action::SeekForward)) {
                        let _ = cmd_tx.send(crate::playback::Cmd::Seek(to));
                        // Discord's clock starts again from where this lands.
                        app.presence_shown = None;
                    }
                }
                _ => {}
            }
            next = app.update(action);
        }
        // What the actions above put in the outbox goes out now, off the
        // loop: a service that is slow must not hold the keys.
        for sending in crate::scrobble::drain(&app.scrobblers, &mut app.scrobble_outbox) {
            tokio::spawn(sending);
        }
        app.tell_presence();
    }
    if let Some(presence) = &app.presence {
        let _ = presence.send(crate::presence::Presence::Off);
    }
    // On the way out: the listen in progress, if it earned it, and whatever
    // is waiting. Awaited, briefly, since nothing runs after this.
    app.settle_listen(app.now_playing.position);
    let sending = crate::scrobble::drain(&app.scrobblers, &mut app.scrobble_outbox);
    if !sending.is_empty() {
        let _ = tokio::time::timeout(
            Duration::from_secs(3),
            futures::future::join_all(sending),
        )
        .await;
    }
    Ok(())
}

/// Today, counted in days from 1970-01-01.
///
/// The feed's stamps are date-only UTC, so the comparison is between whole
/// days and the time of day never enters into it.
fn today() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| (d.as_secs() / 86_400) as i64)
        .unwrap_or_default()
}

/// The four sections the Feed is drawn in, newest first.
///
/// Fixed windows rather than calendar weeks: "this week" is the last seven
/// days, which is what a feed of releases means by it. A card whose date
/// the API left out falls under the oldest heading rather than being
/// dropped -- it is still a release the user follows.
///
/// `more` is left empty: the whole bucket is already in hand, so `o` opens
/// what is held rather than asking for a page that does not exist.
fn feed_rows(cards: &[carousel::Card], today: i64) -> Vec<home::Row> {
    const SECTIONS: [(&str, i64); 4] = [
        ("This week", 7),
        ("Last week", 14),
        ("Last month", 30),
        ("Older", i64::MAX),
    ];
    let mut rows = Vec::new();
    let mut from = 0;
    for (heading, until) in SECTIONS {
        let mine: Vec<carousel::Card> = cards
            .iter()
            .filter(|c| {
                // No date is as old as it gets: the card is still a release
                // the user follows, so it is shown rather than dropped.
                let age = c.day.map_or(i64::MAX, |d| today - d);
                // Inclusive at the top so the oldest section, whose bound is
                // the largest number there is, holds those as well.
                age >= from && (age < until || until == i64::MAX)
            })
            .cloned()
            .collect();
        // No cards, no heading: an empty section is not drawn at all.
        if !mine.is_empty() {
            rows.push(home::Row {
                heading: heading.to_string(),
                kind: crate::browse::RowKind::Carousel,
                cards: mine,
                state: carousel::CarouselState::default(),
            });
        }
        from = until;
    }
    rows
}

/// Fetch everything a signed-in session needs.
///
/// Called both after a fresh login and when a stored session is resumed at
/// startup — the second case is easy to forget, and forgetting it means the
/// app opens already signed in and shows nothing at all.
///
/// Each fetch is its own task so a slow one does not hold up the others; the
/// home page is by far the largest response.
fn load_collection(
    catalogue: std::sync::Arc<crate::catalogue::Session>,
    tx: tokio::sync::mpsc::UnboundedSender<Action>,
) {
    let (client, t) = (std::sync::Arc::clone(&catalogue), tx.clone());
    tokio::spawn(async move {
        match client.favourite_tracks().await {
            Ok(tracks) => {
                tracing::info!("loaded {} favourite tracks", tracks.len());
                let _ = t.send(Action::TracksLoaded(tracks));
            }
            Err(crate::catalogue::Error::Unauthorized) => {
                let _ = t.send(Action::SessionExpired);
            }
            Err(e) => {
                tracing::warn!("could not load favourites: {e}");
                let _ = t.send(Action::Error(e.to_string()));
            }
        }
    });

    let (client, t) = (std::sync::Arc::clone(&catalogue), tx.clone());
    tokio::spawn(async move {
        match client.playlists().await {
            Ok(playlists) => {
                tracing::info!("loaded {} playlists", playlists.len());
                let _ = t.send(Action::PlaylistsLoaded(playlists));
            }
            // A failure here costs the sidebar list, not the session — log it
            // rather than throwing the user back to the login screen.
            Err(e) => tracing::warn!("could not load playlists: {e}"),
        }
    });

    let (client, t) = (std::sync::Arc::clone(&catalogue), tx.clone());
    tokio::spawn(async move {
        match client.albums().await {
            Ok(albums) => {
                tracing::info!("loaded {} favourite albums", albums.len());
                let _ = t.send(Action::AlbumsLoaded(albums));
            }
            Err(e) => tracing::warn!("could not load albums: {e}"),
        }
    });

    let (client, t) = (std::sync::Arc::clone(&catalogue), tx.clone());
    tokio::spawn(async move {
        match client.artists().await {
            Ok(artists) => {
                tracing::info!("loaded {} favourite artists", artists.len());
                let _ = t.send(Action::ArtistsLoaded(artists));
            }
            Err(e) => tracing::warn!("could not load artists: {e}"),
        }
    });

    // One reply with every favourited id, for the key that toggles them:
    // the sections above are pages of covers, this is the list.
    let (client, t) = (std::sync::Arc::clone(&catalogue), tx.clone());
    tokio::spawn(async move {
        match client.favourite_ids().await {
            Ok(ids) => {
                tracing::info!(
                    "loaded favourite ids: {} albums, {} playlists, {} artists",
                    ids.albums.len(),
                    ids.playlists.len(),
                    ids.artists.len()
                );
                let _ = t.send(Action::FavouriteIdsLoaded(ids));
            }
            Err(e) => tracing::warn!("could not load the favourite ids: {e}"),
        }
    });

    let (client, t) = (catalogue, tx);
    tokio::spawn(async move {
        // The same call `LoadHome` makes: one page loaded two ways is two
        // ways for it to differ, and it did.
        match client.home().await {
            Ok(home) => {
                tracing::info!("loaded home: {} rows", home.rows.len());
                let _ = t.send(Action::HomeLoaded(Box::new(home)));
            }
            Err(e) => tracing::warn!("could not load the home page: {e}"),
        }
    });
}

/// Signs in to `provider`, in its own task so the UI stays responsive. A
/// stored session is tried first: a launch on a bad network lands here with
/// a perfectly good token on disk. What the service shows on the way -- the
/// code to confirm in the browser -- reaches the login screen as actions;
/// so does the outcome.
fn start_sign_in(
    app: &mut App,
    method: music::SignIn,
    action_tx: &tokio::sync::mpsc::UnboundedSender<Action>,
) {
    if let Some(provider) = app.provider.clone() {
        let (typed, input) = tokio::sync::mpsc::unbounded_channel();
        app.login_input = Some(typed);
        let task = tokio::spawn(sign_in(provider, method, input, action_tx.clone()));
        app.login_task = Some(task.abort_handle());
    }
}

/// The sign-in itself, in its own task so the UI stays responsive. The
/// provider's questions arrive on `input`, answered through `Answer`.
async fn sign_in(
    provider: std::sync::Arc<dyn crate::catalogue::Provider>,
    method: music::SignIn,
    input: music::InputSource,
    tx: tokio::sync::mpsc::UnboundedSender<Action>,
) {
    let name = provider.name();
    if provider.stored() {
        if let Ok(Some(session)) = provider.restore().await {
            tracing::info!("resumed the stored {name} session");
            let session = crate::catalogue::Session::open(std::sync::Arc::clone(&provider), session);
            let _ = tx.send(Action::SessionOpened(std::sync::Arc::new(session)));
            return;
        }
    }
    let prompt: music::PromptSink = {
        let tx = tx.clone();
        std::sync::Arc::new(move |prompt| {
            // Save the user copying a URL out of a TUI. Best effort: the
            // link stays on screen, so a failure here costs nothing. A
            // `Url` prompt is not opened here: the provider opens it
            // itself, and listens for the browser to come back.
            if let music::SignInPrompt::Code { url, .. } = &prompt {
                login::open_in_browser(url);
            }
            let _ = tx.send(Action::LoginPrompt(prompt));
        })
    };
    match provider.sign_in(method, prompt, input).await {
        Ok(session) => {
            tracing::info!("signed in to {name}");
            let session = crate::catalogue::Session::open(std::sync::Arc::clone(&provider), session);
            let _ = tx.send(Action::SessionOpened(std::sync::Arc::new(session)));
        }
        Err(e) => {
            tracing::warn!("could not sign in to {name}: {e:#}");
            let _ = tx.send(Action::Error(format!("{e:#}")));
        }
    }
}

/// Public so `examples/screenshot.rs` renders what the app actually renders.
/// A preview that assembles the layout itself drifts from the real one, and
/// then it is checking its own copy rather than the UI.
/// Whether the account has what a card opens, for the heart on it.
fn liked_in(
    target: &carousel::Target,
    followed: &std::collections::HashSet<String>,
    albums: &std::collections::HashSet<String>,
    playlists: &std::collections::HashSet<String>,
) -> bool {
    match target {
        carousel::Target::Artist(id) => followed.contains(id),
        carousel::Target::Album(id) => albums.contains(id),
        carousel::Target::Playlist(uuid) => playlists.contains(uuid),
        carousel::Target::Track(_) | carousel::Target::Page(_) | carousel::Target::Section(_) => {
            false
        }
    }
}

/// The closure every renderer draws covers through: the artwork store when
/// there is one, nothing drawn when the terminal has none.
fn cover_drawer(
    art: &mut Option<artwork::Artwork>,
) -> impl FnMut(&mut ratatui::Frame, ratatui::layout::Rect, &str, artwork::Shape) -> bool + '_ {
    move |frame, area, url, shape| {
        art.as_mut()
            .is_some_and(|a| a.render_shaped(frame, area, url, shape))
    }
}

/// A line or two in the middle of an otherwise empty pane: loading,
/// nothing here, no such page on this service.
fn notice(frame: &mut ratatui::Frame, area: ratatui::layout::Rect, palette: &theme::Palette, lines: Vec<ratatui::text::Line<'static>>) {
    let height = (lines.len() as u16).min(area.height);
    frame.render_widget(
        ratatui::widgets::Paragraph::new(lines)
            .style(palette.subtitle())
            .alignment(ratatui::layout::Alignment::Center),
        ratatui::layout::Rect {
            y: area.y + area.height / 3,
            height,
            ..area
        },
    );
}

/// What the Feed page says on a service that has none.
fn feed_notice(service: &str) -> Vec<ratatui::text::Line<'static>> {
    vec![
        ratatui::text::Line::from(format!("{service} has no feed.")),
        ratatui::text::Line::from("TIDAL's lists new releases from the artists you follow."),
    ]
}

pub fn draw(frame: &mut ratatui::Frame, app: &mut App) {
    // Borrowed by field, so the closure lives beside the artwork being
    // taken out and put back below.
    let (followed, fav_albums, fav_playlists) =
        (&app.followed, &app.favourite_albums, &app.favourite_playlists);
    let liked = |t: &carousel::Target| liked_in(t, followed, fav_albums, fav_playlists);
    let regions = layout::split(frame.area());
    let palette = app.palette;
    app.last_main_width = regions.main.width;
    app.last_main_height = regions.main.height;

    // The rule between the panes, drawn before either so neither has to
    // know it is there.
    if regions.divider.width > 0 {
        frame.render_widget(
            ratatui::widgets::Block::default()
                .borders(ratatui::widgets::Borders::LEFT)
                .border_style(palette.rule()),
            regions.divider,
        );
    }

    sidebar::render(
        frame,
        regions.sidebar,
        &palette,
        &app.sidebar,
        // The nav is never the focused pane: J and K move it from
        // wherever the user is, and nothing else drives it, so a highlight
        // that says otherwise points at a mode that does not exist.
        false,
    );

    // The main pane always has the keys. Tab used to move a highlight onto
    // the nav, where no key did anything — a mode with nothing behind it.
    let main_focused = true;
    // Search takes the pane while it is open, ahead of both an opened album
    // and the sidebar's section — it is where the user is looking. Drawn
    // here rather than with an early return, so the now-playing bar and the
    // status line below still get their turn.
    // Asked once, and every branch below reads it: exactly one view is
    // drawn, the one `showing` names, whatever else is set behind it.
    let showing = app.showing();
    if let (Showing::Search(_), Some(search)) = (showing, app.search.as_ref()) {
        // `artwork` needs &mut while the renderer borrows `app`, so take it
        // out for the duration and put it back — as the other views do.
        let mut art = app.artwork.take();
        searchview::render(
            frame,
            regions.main,
            &palette,
            searchview::View {
                query: &search.query,
                typing: search.typing,
                pending: search.pending,
                results: &search.results,
                tab: search.tab,
                tracks: &search.tracks,
                // Whichever grid this tab reuses; the others keep their own
                // place for when the user comes back to them.
                grid: match searchview::Tab::from_index(search.tab) {
                    searchview::Tab::Albums => &search.albums,
                    searchview::Tab::Artists => &search.artists,
                    _ => &search.playlists,
                },
                liked: &liked,
                artists: &search.top_artists,
                albums: &search.top_albums,
                top: search.top,
                scroll: search.top_scroll,
                favourites: &app.favourites,
                playing: app.now_playing.track.as_ref().map(|t| t.id.clone()),
                tier: app.now_playing.tier,
            },
            cover_drawer(&mut art),
        );
        app.artwork = art;
    }

    if let (Showing::Artist, Some(page)) = (showing, app.artist.as_ref()) {
        let mut art = app.artwork.take();
        artistview::render(
            frame,
            regions.main,
            &palette,
            artistview::View {
                page,
                section: artistview::Section::from_index(app.artist_section),
                tracks: &app.artist_tracks,
                rows: &app.artist_rows,
                scroll: app.artist_scroll,
                bio_open: app.artist_bio_open,
                bio_scroll: app.artist_bio_scroll,
                liked: &liked,
                favourites: &app.favourites,
                playing: app.now_playing.track.as_ref().map(|t| t.id.clone()),
                tier: app.now_playing.tier,
            },
            cover_drawer(&mut art),
        );
        app.artwork = art;
    }

    // A view opened but not yet filled: its name and a line saying it is
    // on its way. Drawn here rather than by a section's renderer because
    // what it becomes is not known until the reply lands.
    if showing == Showing::Loading {
        let title = app
            .open
            .as_ref()
            .map(|o| o.title.as_str())
            .unwrap_or_default();
        loadingview::render(frame, regions.main, &palette, title);
    }
    let section = match showing {
        // Already drawn above, or drawn below in the queue's case; this
        // keeps the match from drawing a section under them.
        Showing::Artist
        | Showing::Search(_)
        | Showing::Loading
        | Showing::Queue
        | Showing::History => None,
        // An opened row of covers is a grid, not a track list — Albums is
        // the section whose renderer draws two-line cards.
        Showing::Tracks => Some(sidebar::Section::Tracks),
        Showing::Cards => Some(sidebar::Section::Albums),
        // A page of rows is drawn by the home renderer, on whichever state
        // holds it.
        Showing::Rows => Some(app.sidebar.section()),
        Showing::Section(section) => Some(section),
    };

    if let Some(showing) = section {
        match showing {
            // Explore is a page of card rows like the home one, so it is drawn
            // by the same renderer on its own state.
            section @ (sidebar::Section::Music | sidebar::Section::Explore) => {
                // `artwork` needs &mut to cache what it decodes, and the closure
                // is handed to a renderer that also borrows `app` — so take it out
                // for the duration and put it back.
                let explore = section == sidebar::Section::Explore;
                let (state, loaded) = match explore {
                    true => (&app.explore, app.explore_loaded),
                    false => (&app.home, app.home_loaded),
                };
                // An empty page says why rather than staying blank: no such
                // page on this service, the reply still on its way, or
                // nothing in it. No early return: the bar below is drawn
                // after this match.
                if explore && app.explore_empty {
                    notice(
                        frame,
                        regions.main,
                        &palette,
                        vec![
                            ratatui::text::Line::from(format!(
                                "{} has no Explore page.",
                                app.service_name()
                            )),
                            ratatui::text::Line::from("Search (s) still covers its catalogue."),
                        ],
                    );
                } else if state.rows.is_empty() {
                    let line = match loaded {
                        false => waiting::animated("Loading…").into_owned(),
                        true => format!("{} has nothing to show here yet.", app.service_name()),
                    };
                    notice(frame, regions.main, &palette, vec![ratatui::text::Line::from(line)]);
                } else {
                    let mut art = app.artwork.take();
                    home::render(
                        frame,
                        regions.main,
                        &palette,
                        state,
                        main_focused,
                        trackgrid::Marks {
                            favourites: &app.favourites,
                            playing: app.now_playing.track.as_ref().map(|t| &t.id),
                            tier: app.now_playing.tier,
                            liked: &liked,
                        },
                        cover_drawer(&mut art),
                    );
                    app.artwork = art;
                }
            }
            sidebar::Section::Feed => {
                // The Feed is a page of rows like the home page, but headed and
                // filtered like a grid: its chrome is drawn here and what is
                // left of the pane goes to the row renderer. The heading rows
                // come from the grid's own count so the two cannot drift.
                let header = inputbox::render_headed(
                    frame,
                    scrollbar::reserve(regions.main),
                    &palette,
                    "Feed",
                    None,
                    "Filter releases",
                    &app.feed.filter,
                    app.filtering,
                );
                let body = ratatui::layout::Rect {
                    y: regions.main.y + header,
                    height: regions.main.height.saturating_sub(header),
                    ..regions.main
                };
                // Only TIDAL has one. Blank, the page read as broken; it
                // says what it would show and where it is shown.
                if !app.session.as_ref().is_some_and(|s| s.has_feed()) {
                    notice(frame, body, &palette, feed_notice(app.service_name()));
                } else {
                    let mut art = app.artwork.take();
                    home::render(
                        frame,
                        body,
                        &palette,
                        &app.feed,
                        main_focused,
                        trackgrid::Marks {
                            favourites: &app.favourites,
                            playing: app.now_playing.track.as_ref().map(|t| &t.id),
                            tier: app.now_playing.tier,
                            liked: &liked,
                        },
                        cover_drawer(&mut art),
                    );
                    app.artwork = art;
                }
            }
            sidebar::Section::Settings => {
                let linked: Vec<(String, String)> = app
                    .scrobblers
                    .iter()
                    .map(|l| (l.service.id().to_string(), l.label()))
                    .collect();
                settings::render(
                    frame,
                    regions.main,
                    &palette,
                    &app.config,
                    &app.settings,
                    main_focused,
                    &linked,
                );
            }
            section @ (sidebar::Section::Playlists
            | sidebar::Section::Albums
            | sidebar::Section::Profiles) => {
                // An opened row brings its own cards and its own heading; the
                // sidebar's section only says which renderer draws them.
                let opened = app.open.as_ref().filter(|_| !app.open_cards.is_empty());
                let cards = match opened {
                    Some(_) => app.open_cards.clone(),
                    None => app.grid_cards(section),
                };
                // Cloned rather than borrowed: `artwork` is taken out below and
                // a live borrow of `app` here would hold across it.
                let state = match opened {
                    Some(_) => app.open_grid.clone(),
                    None => app.grid_state(section).clone(),
                };
                let visible = grid::filter(&cards, &state.filter);
                // `lines` comes from the same place the movement keys read it,
                // so the two cannot drift: they disagreed by a whole row of
                // cards, and the keys reached ones that were never drawn.
                let lines = app.grid_lines(section);
                let (heading, hint) = match opened {
                    Some(open) => (open.title.as_str(), "Filter"),
                    None => match section {
                        sidebar::Section::Playlists => ("Playlists", "Filter playlists"),
                        sidebar::Section::Albums => ("Albums", "Filter albums"),
                        _ => ("Profiles", "Filter profiles"),
                    },
                };
                let mut art = app.artwork.take();
                grid::render(
                    frame,
                    regions.main,
                    &palette,
                    grid::Grid {
                        filtering: app.filtering,
                        chrome: grid::Chrome::Full,
                        heading,
                        filter_hint: hint,
                        cards: &visible,
                        state: &state,
                        focused: main_focused,
                        lines,
                        tabs: (&[], 0),
                        liked: &liked,
                    },
                    cover_drawer(&mut art),
                );
                app.artwork = art;
            }
            _ => {
                let visible = tracklist::filter(&app.tracks, &app.tracklist.filter);
                let mut art = app.artwork.take();
                tracklist::render(
                    frame,
                    regions.main,
                    &palette,
                    tracklist::TrackList {
                        filtering: app.filtering,
                        favourites: &app.favourites,
                        chrome: tracklist::Chrome::Full,
                        tracks: &visible,
                        state: &app.tracklist,
                        focused: main_focused,
                        playing: app.now_playing.track.as_ref().map(|t| t.id.clone()),
                        tier: app.now_playing.tier,
                        banner: app.open.as_ref().map(|o| tracklist::Banner {
                            title: &o.title,
                            subtitle: &o.subtitle,
                            detail: &o.detail,
                            cover: o.cover.as_deref(),
                            round: o.round_cover,
                            liked: app.open_favourable().is_some_and(|w| app.is_favourite(&w)),
                        }),
                    },
                    cover_drawer(&mut art),
                );
                app.artwork = art;
            }
        }
    }

    // The queue and the history take the pane, as the artist page does,
    // and draw the list the Tracks section draws: a row here is a row like
    // any other, and the keys act on it. They were modals with a list of
    // their own, and a modal that closed on any key was one no key could
    // act in.
    if matches!(showing, Showing::Queue | Showing::History) {
        let marks = trackgrid::Marks {
            favourites: &app.favourites,
            playing: app.now_playing.track.as_ref().map(|t| &t.id),
            tier: app.now_playing.tier,
            liked: &liked,
        };
        let entries = app.queue.entries();
        // The heading says whether the queue has an end: what follows the
        // last row is either silence or more of the same, and the list
        // itself cannot show which.
        let queue_heading = if app.config.playback.autoplay {
            "Queue · autoplay on"
        } else {
            "Queue · autoplay off"
        };
        let (heading, tracks, state): (&str, Vec<&crate::domain::Track>, _) =
            if showing == Showing::Queue {
                (
                    queue_heading,
                    entries.iter().map(|(t, _)| *t).collect(),
                    &app.queue_list,
                )
            } else {
                ("History", app.queue.history().collect(), &app.history_list)
            };
        let mut art = app.artwork.take();
        queueview::render(
            frame,
            regions.main,
            &palette,
            heading,
            &tracks,
            state,
            marks,
            cover_drawer(&mut art),
        );
        app.artwork = art;
    }

    let mut art = app.artwork.take();
    nowplaying::render_with_cover(
        frame,
        regions.now_playing,
        &palette,
        &app.now_playing,
        // Straight from the queue, so the bar cannot disagree with what
        // actually plays next.
        nowplaying::Modes {
            shuffled: app.queue.shuffled(),
            repeat: app.queue.repeat,
            sleep: app.sleep_mark(),
            // Only while something plays: a paused track's last frame
            // frozen under the badge read as the music still going.
            spectrum: match (&app.spectrum, app.now_playing.playing) {
                (Some(bands), true) => bands.levels(),
                _ => Vec::new(),
            },
        },
        cover_drawer(&mut art),
    );
    app.artwork = art;

    if let Some(status) = &app.status {
        // Tucked into the top right rather than laid across the whole line.
        // Taking the line meant covering the tab strip that lives there:
        // drawn over it the two read as one garbled line, and clearing it
        // first hid the tabs for as long as the message stood.
        let width = (status.chars().count() as u16 + 2).min(regions.main.width);
        let notice = ratatui::layout::Rect {
            x: regions.main.x + regions.main.width - width,
            y: regions.main.y,
            width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(Line::styled(
                format!(" {} ", waiting::animated(status)),
                palette.notice(),
            )),
            notice,
        );
    }

    if app.session.is_none() {
        login::render(
            frame,
            frame.area(),
            &app.login,
            app.service_name(),
            app.guest_allowed(),
        );
    }

    if app.cdm.shown() {
        drmview::render(frame, regions.main, &palette, &app.cdm);
    }

    if app.lyrics.open {
        lyricsview::render(
            frame,
            regions.main,
            &palette,
            &app.lyrics,
            app.now_playing.track.as_ref(),
            app.now_playing.position,
        );
    }

    // Last, so it covers whatever is underneath.
    if let Some(prompt) = &app.prompt {
        let owned: Vec<crate::library::Playlist> =
            app.playlists.iter().filter(|p| p.owned).cloned().collect();
        let mut art = app.artwork.take();
        playlistedit::render(frame, frame.area(), &palette, prompt, &owned, art.as_mut());
        app.artwork = art;
    }
    if let Some(edit) = &app.setting_edit {
        settings::render_edit(frame, frame.area(), &palette, edit);
    }
    if app.showing_help {
        // Clamped here, where the height is known: the key handler only
        // counts.
        app.help_scroll = app.help_scroll.min(help::overflow(frame.area().height));
        help::render(frame, frame.area(), &palette, app.help_scroll);
    }
}

#[cfg(test)]
mod tests;
