use super::*;

mod artist;
mod collections;
mod draw;
mod keys;
mod misc;
mod navigation;
mod playback;
mod search;
mod session;

use crossterm::event::KeyModifiers;

/// Draw `app` and check the pane stopped where the bar begins.
///
/// The bar is drawn last and would cover anything that overran, so the
/// pane is rendered on its own into a buffer the height of the whole
/// frame: whatever appears on the bar's rows got there from above.
#[track_caller]
fn assert_clear_of_the_bar(app: &mut App, height: u16, what: &str) {
    let width = 100u16;
    let regions = layout::split(ratatui::layout::Rect::new(0, 0, width, height));
    let bar_top = regions.now_playing.y;

    // Twice: once as the app draws it, once with the views emptied so
    // only the bar paints. Anything on the bar's rows in the first that
    // is not there in the second came from a view above it.
    let render = |app: &mut App| {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        terminal.backend().buffer().clone()
    };
    let full = render(app);

    let mut bare = App {
        session: app.session.clone(),
        now_playing: app.now_playing.clone(),
        palette: app.palette,
        ..App::default()
    };
    let bar_only = render(&mut bare);

    for y in bar_top..height {
        for x in 0..width {
            let a = &full[(x, y)];
            let b = &bar_only[(x, y)];
            assert!(
                a.symbol() == b.symbol(),
                "{what} at height {height}: ({x},{y}) is inside the \
                 now-playing bar and shows {:?}, where the bar alone \
                 draws {:?}\n{}",
                a.symbol(),
                b.symbol(),
                geometry::text(&full)
            );
        }
    }
}

fn signed_in(section: sidebar::Section) -> App {
    let mut app = App {
        session: Some(sample_session()),
        ..App::default()
    };
    while app.sidebar.section() != section {
        app.sidebar.next();
    }
    // A size the renderer would have measured; the movement keys need it.
    app.last_main_width = 120;
    app.last_main_height = 40;
    app
}

fn key(app: &mut App, code: KeyCode) -> Option<Action> {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn with_albums(n: u64) -> App {
    let mut app = signed_in(sidebar::Section::Albums);
    app.albums = (0..n)
        .map(|i| crate::library::Album {
            id: (i + 1).to_string(),
            title: format!("Album {i}"),
            artist: format!("Artist {i}"),
            year: Some("2026".into()),
            cover: None,
            track_count: 10,
            duration: None,
        })
        .collect();
    app
}

fn some_tracks(titles: &[&str]) -> Vec<crate::domain::Track> {
    titles
        .iter()
        .map(|t| crate::domain::Track::sample(t, "An Artist", std::time::Duration::from_secs(100)))
        .collect()
}

fn some_results() -> crate::search::Results {
    crate::search::Results {
        query: "daft punk".into(),
        tracks: (0..6)
            .map(|i| {
                crate::domain::Track::sample(
                    &format!("Track {i}"),
                    "Daft Punk",
                    std::time::Duration::from_secs(200),
                )
            })
            .collect(),
        // Enough of each kind to move within: a section holding one
        // card cannot move, so a shared selection would look correct.
        albums: (0..8)
            .map(|i| crate::library::Album {
                id: (i).to_string(),
                title: format!("Album {i}"),
                artist: "Daft Punk".into(),
                year: Some("2001".into()),
                cover: None,
                track_count: 10,
                duration: None,
            })
            .collect(),
        artists: (0..8)
            .map(|i| crate::library::Artist {
                id: (i).to_string(),
                name: format!("Artist {i}"),
                picture: None,
            })
            .collect(),
        playlists: (0..8)
            .map(|i| crate::library::Playlist {
                uuid: format!("u{i}"),
                title: format!("Playlist {i}"),
                track_count: 10,
                duration: None,
                creator: "TIDAL".into(),
                cover: None,
                owned: false,
            })
            .collect(),
    }
}

fn home_with_a_track_row() -> App {
    let mut app = signed_in(sidebar::Section::Music);
    app.home.rows = vec![home::Row {
        kind: crate::browse::RowKind::Compact,
        heading: "New Tracks".into(),
        cards: (0..6)
            .map(|i| carousel::Card {
                title: format!("Track {i}"),
                target: Some(carousel::Target::Track(i.to_string())),
                ..Default::default()
            })
            .collect(),
        state: carousel::CarouselState::default(),
    }];
    app
}

fn home_with_a_carousel_row() -> App {
    let mut app = signed_in(sidebar::Section::Music);
    app.home.rows = vec![home::Row {
        kind: crate::browse::RowKind::Carousel,
        heading: "New Albums".into(),
        cards: (0..3)
            .map(|i| carousel::Card {
                title: format!("Album {i}"),
                target: Some(carousel::Target::Album(i.to_string())),
                ..Default::default()
            })
            .collect(),
        state: carousel::CarouselState::default(),
    }];
    app
}

fn explore_rows() -> crate::browse::Home {
    crate::browse::Home {
        rows: vec![crate::browse::HomeRow {
            kind: crate::browse::RowKind::Carousel,
            heading: "Genres".into(),
            cards: vec![carousel::Card::new("Rock", "")],
            more: None,
        }],
    }
}

fn an_artist_page() -> crate::library::ArtistPage {
    crate::library::ArtistPage {
        name: "Daft Punk".into(),
        picture: None,
        top_tracks: some_tracks(&["One More Time", "Aerodynamic"]),
        albums: (0..3)
            .map(|i| crate::library::Album {
                id: (i).to_string(),
                title: format!("Album {i}"),
                artist: "Daft Punk".into(),
                year: Some("2001".into()),
                cover: None,
                track_count: 10,
                duration: None,
            })
            .collect(),
        similar: (0..2)
            .map(|i| crate::library::Artist {
                id: (100 + i).to_string(),
                name: format!("Similar {i}"),
                picture: None,
            })
            .collect(),
        ..Default::default()
    }
}

/// The request the open view waits on: what a reply names to be taken.
fn open_request(app: &App) -> RequestId {
    app.open.as_ref().expect("something is open").request
}

fn with_artist_open() -> App {
    let mut app = signed_in(sidebar::Section::Profiles);
    app.open = Some(OpenCollection {
        title: "Daft Punk".into(),
        subtitle: String::new(),
        detail: String::new(),
        cover: None,
        round_cover: false,
        came_from: sidebar::Section::Profiles,
        request: RequestId::default(),
        target: None,
    });
    app.update(Action::ArtistLoaded {
        request: open_request(&app),
        page: Box::new(an_artist_page()),
    });
    app.last_main_width = 100;
    app.last_main_height = 40;
    app
}

/// A signed-in TIDAL session on no token, asking nothing of the
/// network until a fetch is awaited.
fn sample_session() -> std::sync::Arc<crate::catalogue::Session> {
    crate::catalogue::Session::sample()
}

fn owned_playlist(uuid: &str, title: &str, owned: bool) -> crate::library::Playlist {
    let mut p = crate::library::Playlist::sample(title, 3);
    p.uuid = uuid.into();
    p.owned = owned;
    p
}

fn with_open_playlist(app: &mut App, uuid: &str) {
    app.open = Some(OpenCollection {
        title: "Road".into(),
        subtitle: String::new(),
        detail: String::new(),
        cover: None,
        round_cover: false,
        came_from: sidebar::Section::Playlists,
        request: RequestId::default(),
        target: Some(Collection::Playlist(uuid.into())),
    });
}
