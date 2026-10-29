use super::*;

#[test]
fn a_section_emptied_by_an_open_view_is_fetched_again_on_the_way_in() {
    // The favourites, playlists and albums arrive together at sign-in,
    // and the reply is only taken when nothing is open over the pane --
    // `tracks` is shared with whatever collection is on screen. A reply
    // that lands while a view is open is dropped, and nothing asked for
    // it again: the section was empty for the rest of the session.
    let mut app = signed_in(sidebar::Section::Music);
    app.open = Some(OpenCollection {
        title: "An Album".into(),
        subtitle: String::new(),
        detail: String::new(),
        cover: None,
        round_cover: false,
        came_from: sidebar::Section::Music,
        request: RequestId::default(),
        target: None,
    });

    // The sign-in reply, arriving while that album is open.
    app.update(Action::TracksLoaded(some_tracks(&["Fav 1", "Fav 2"])));
    app.open = None;
    assert!(
        app.tracks.is_empty(),
        "the reply is dropped: it would have replaced the open album"
    );

    // Walking into Tracks has to ask for them again.
    let asked = app.update(Action::GoToSection(sidebar::Section::Tracks));
    assert!(
        matches!(asked, Some(Action::LoadFavourites)),
        "the empty section fetches its list rather than staying empty, got {asked:?}"
    );
}

#[test]
fn escape_clears_the_filter_and_leaves_the_box() {
    let mut app = signed_in(sidebar::Section::Playlists);
    app.on_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
    for c in ['a', 'b'] {
        app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    assert_eq!(app.filter_text(), "ab");

    app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(!app.filtering);
    assert_eq!(app.filter_text(), "", "escape abandons the filter");
    assert!(!app.should_quit, "escape must leave the box, not the app");
}

#[test]
fn the_home_page_has_no_filter_box() {
    // Only the collection views have one; on home, "/" is not a binding.
    let mut app = signed_in(sidebar::Section::Music);
    app.on_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
    assert!(!app.filtering);
}

#[test]
fn h_does_not_close_an_opened_album() {
    // It used to: the one place `h` meant something other than a
    // direction. Opening an album from the home page and reaching for
    // the left of a row put the user back on the page they came from,
    // so the key now means "left" everywhere and escape is what closes.
    let mut app = with_albums(3);
    app.update(Action::ActivateSelection);
    let acted = key(&mut app, KeyCode::Char('h'));
    assert!(
        !matches!(acted, Some(Action::CloseCollection)),
        "h is a direction, not a way out, got {acted:?}"
    );
    assert!(app.open.is_some(), "the album is still open");
}

#[test]
fn the_help_opens_on_question_mark_and_any_key_closes_it() {
    let mut app = signed_in(sidebar::Section::Tracks);
    assert!(key(&mut app, KeyCode::Char('?')).is_none());
    assert!(app.showing_help);

    // While it is up, keys dismiss rather than acting — including q,
    // which must not quit out from under the modal.
    assert!(key(&mut app, KeyCode::Char('q')).is_none());
    assert!(!app.showing_help);
    assert!(!app.should_quit, "q must have closed the help, not quit");

    // And the key that opened it closes it too.
    key(&mut app, KeyCode::Char('?'));
    assert!(app.showing_help);
    key(&mut app, KeyCode::Char('?'));
    assert!(!app.showing_help);
}

#[test]
fn enter_on_a_my_tracks_shortcut_goes_to_the_tracks_view() {
    // The feed's shortcut grid points at the app's own views: "My
    // Tracks" is the favourites, not a collection to fetch.
    let mut app = signed_in(sidebar::Section::Music);
    let mut card = carousel::Card::new("My Tracks", "");
    card.target = Some(carousel::Target::Section(sidebar::Section::Tracks));
    app.home.rows.push(home::Row {
        heading: "Shortcuts".into(),
        kind: crate::browse::RowKind::Shortcuts,
        cards: vec![card],
        state: carousel::CarouselState::default(),
    });
    assert!(app.selected_collection().is_none(), "nothing to fetch");

    let mut next = Some(Action::ActivateSelection);
    while let Some(action) = next {
        next = app.update(action);
    }
    assert_eq!(app.sidebar.section(), sidebar::Section::Tracks);
    assert!(app.open.is_none(), "the view itself, not something over it");
}

#[test]
fn enter_on_an_explore_link_opens_its_page() {
    // Explore did nothing at all: its cards carry a path rather than an
    // id of any kind, so none of them had anything to open.
    let mut app = signed_in(sidebar::Section::Explore);
    let mut card = carousel::Card::new("Hip-Hop", "");
    card.target = Some(carousel::Target::Page("pages/genre_hip_hop".into()));
    app.explore.rows.push(home::Row {
        heading: "Genres".into(),
        kind: crate::browse::RowKind::Carousel,
        cards: vec![card],
        state: carousel::CarouselState::default(),
    });

    assert!(
        matches!(
            app.selected_collection(),
            Some(Collection::Page { ref path, .. }) if path == "pages/genre_hip_hop"
        ),
        "enter has a page to open, got {:?}",
        app.selected_collection()
    );

    // Opened, and waited on; the reply takes the pane, and the history
    // gets back to Explore.
    app.update(Action::ActivateSelection);
    let mut home = crate::browse::Home::default();
    home.rows.push(crate::browse::HomeRow {
        heading: "Playlists".into(),
        kind: crate::browse::RowKind::Carousel,
        cards: vec![carousel::Card::new("A Playlist", "TIDAL")],
        more: None,
    });
    app.update(Action::PageLoaded {
        request: open_request(&app),
        title: "Hip-Hop".into(),
        home: Box::new(home),
    });
    // Drawn by the rows renderer under its own heading. It used to set
    // `open` instead, which hands the pane to the collection view --
    // and that view draws `open_cards`, which a page of rows never
    // fills, so the pane went empty and enter looked like it did
    // nothing.
    assert!(
        app.open.is_none(),
        "a page of rows is not an opened collection, got {:?}",
        app.open
    );
    assert_eq!(
        app.explore.heading.as_deref(),
        Some("Hip-Hop"),
        "headed by the genre the user pressed enter on"
    );
    assert_eq!(
        app.explore.rows.first().map(|r| r.heading.as_str()),
        Some("Playlists"),
        "with the page's own rows in it"
    );
    assert!(
        app.on_home(),
        "and the keys drive the rows, as they do on Explore itself"
    );
}

#[test]
fn the_numbers_reach_every_nav_entry() {
    // Nine entries and nine keys: 1 is Music and 9 is Settings, so the
    // far end is one press rather than eight of J.
    for (i, section) in sidebar::Section::ALL.iter().enumerate() {
        let mut app = signed_in(sidebar::Section::Music);
        let digit = char::from_digit(i as u32 + 1, 10).expect("1 to 9");
        let action = key(&mut app, KeyCode::Char(digit)).expect("bound");
        app.update(action);
        assert_eq!(
            app.sidebar.section(),
            *section,
            "{digit} reaches {:?}",
            section.label()
        );
    }
}

#[test]
fn a_number_leaves_an_opened_view_and_can_be_come_back_from() {
    // The same as J and K: what was on screen goes on the history.
    let mut app = with_albums(3);
    app.update(Action::ActivateSelection);
    let title = app.open.as_ref().expect("open").title.clone();

    let action = key(&mut app, KeyCode::Char('8')).expect("bound");
    app.update(action);
    assert_eq!(app.sidebar.section(), sidebar::Section::Settings);
    assert!(app.open.is_none(), "the album closed");

    app.update(Action::GoBack);
    assert_eq!(
        app.open.as_ref().map(|o| o.title.as_str()),
        Some(title.as_str()),
        "back returns to it"
    );
}

#[test]
fn pressing_the_number_of_the_section_already_showing_does_nothing() {
    // Otherwise the history fills with the same place and back walks
    // through a run of identical steps.
    let mut app = signed_in(sidebar::Section::Music);
    let before = app.back.len();
    let action = key(&mut app, KeyCode::Char('1'));
    if let Some(action) = action {
        app.update(action);
    }
    assert_eq!(app.back.len(), before, "nothing was recorded");
    assert_eq!(app.sidebar.section(), sidebar::Section::Music);
}

#[test]
fn escape_steps_back_like_the_bracket_and_never_quits() {
    // Escape and `[` are the same step. Escape is the key people reach
    // for to leave where they are, and leaving should mean one thing
    // whichever of the two is pressed. And it never quits: `q` alone
    // does that, since a key that usually means "leave this view"
    // should not sometimes close the app.
    let mut app = with_albums(3);
    app.update(Action::ActivateSelection);
    assert!(app.open.is_some(), "an album is open");

    let action = key(&mut app, KeyCode::Esc).expect("escape is bound");
    assert!(
        matches!(action, Action::GoBack),
        "the same as `[`, got {action:?}"
    );
    app.update(action);
    assert!(app.open.is_none(), "the album closed");
    assert!(!app.should_quit, "escape must not have quit");

    // Both keys ask for the same thing.
    let mut other = with_albums(3);
    other.update(Action::ActivateSelection);
    assert!(
        matches!(key(&mut other, KeyCode::Char('[')), Some(Action::GoBack)),
        "`[` steps back too"
    );

    // With nothing left to step back to it stays put rather than
    // quitting or emptying the pane.
    let action = key(&mut app, KeyCode::Esc).expect("still bound");
    app.update(action);
    assert!(!app.should_quit);

    // `q` is the way out of the app.
    assert!(matches!(
        key(&mut app, KeyCode::Char('q')),
        Some(Action::Quit)
    ));
}

#[test]
fn back_returns_to_where_escape_was_pressed() {
    // Escape is a step in the history like any other, so the arrow
    // returns to the view it left.
    let mut app = with_albums(3);
    app.update(Action::ActivateSelection);
    let title = app.open.as_ref().expect("open").title.clone();

    app.update(Action::GoHome);
    assert!(app.open.is_none());

    app.update(Action::GoBack);
    assert_eq!(
        app.open.as_ref().map(|o| o.title.as_str()),
        Some(title.as_str()),
        "back returns to the album escape left"
    );
}

#[test]
fn an_album_card_on_the_home_page_opens_too() {
    // Enter means the same thing wherever there are cards; it used to
    // work only in the Albums grid.
    let mut app = signed_in(sidebar::Section::Music);
    app.home.rows = vec![home::Row {
        kind: crate::browse::RowKind::Carousel,
        heading: "New Albums".into(),
        cards: vec![carousel::Card {
            title: "August 26".into(),
            subtitle: "Post Malone".into(),
            target: Some(carousel::Target::Album("42".into())),
            ..Default::default()
        }],
        state: carousel::CarouselState::default(),
    }];

    assert!(matches!(
        app.selected_collection(),
        Some(Collection::Album(id)) if id == "42"
    ));

    app.update(Action::ActivateSelection);
    assert_eq!(
        app.open.as_ref().map(|o| o.title.as_str()),
        Some("August 26"),
        "the home page opens an album like the grid does"
    );
}

#[test]
fn an_album_opened_from_the_home_page_is_not_still_the_home_page() {
    // Same failure through the other door: opening from Music left
    // `on_home` true, so j and k moved the carousel rows underneath.
    let mut app = signed_in(sidebar::Section::Music);
    app.home.rows = vec![home::Row {
        kind: crate::browse::RowKind::Carousel,
        heading: "New Albums".into(),
        cards: vec![carousel::Card {
            title: "August 26".into(),
            target: Some(carousel::Target::Album("42".into())),
            ..Default::default()
        }],
        state: carousel::CarouselState::default(),
    }];
    app.update(Action::ActivateSelection);
    app.tracks = vec![crate::domain::Track::sample(
        "t",
        "a",
        std::time::Duration::ZERO,
    )];

    assert!(app.open.is_some());
    assert!(!app.on_home(), "an opened album is not the home page");
    assert!(matches!(
        key(&mut app, KeyCode::Char('j')),
        Some(Action::TrackNext)
    ));
}

#[test]
fn the_home_rows_are_shown_when_they_land() {
    let mut app = signed_in(sidebar::Section::Music);
    let fresh = crate::browse::Home {
        rows: vec![crate::browse::HomeRow {
            heading: "Songs on repeat".into(),
            kind: crate::browse::RowKind::Compact,
            cards: vec![carousel::Card::new("A track", "An artist")],
            more: None,
        }],
    };
    app.update(Action::HomeLoaded(Box::new(fresh)));

    assert_eq!(
        app.home.rows.first().map(|r| r.heading.as_str()),
        Some("Songs on repeat")
    );
}

#[test]
fn the_selected_home_row_is_never_the_cut_one() {
    // Reported: arriving on a row cut off at the bottom did not scroll,
    // so the selection sat on a half-drawn row and the page looked
    // stuck. What must hold is that wherever the selection goes, the
    // row it lands on is drawn whole — checked by walking the page
    // from top to bottom at a height that does cut one.
    let mut app = signed_in(sidebar::Section::Music);
    let mut home = home::HomeState::default();
    for i in 0..8 {
        home.rows.push(home::Row {
            heading: format!("Row {i}"),
            kind: crate::browse::RowKind::Carousel,
            cards: (0..6)
                .map(|c| carousel::Card::new(format!("R{i} C{c}"), "artist"))
                .collect(),
            state: carousel::CarouselState::default(),
        });
    }
    app.home = home;
    app.last_main_width = 120;
    // A height that actually cuts a row, found rather than assumed:
    // the row heights have changed under this test before.
    let width = scrollbar::content_width(app.last_main_width);
    let height = (30..80u16)
        .find(|h| {
            home::rows_to_land_in(*h, width, &app.home)
                < home::visible_rows_of(*h, width, &app.home)
                && home::visible_rows_of(*h, width, &app.home) >= 3
        })
        .expect("some height cuts a row");
    app.last_main_height = height;

    let visible = home::visible_rows_of(height, width, &app.home);
    for step in 0..8 {
        app.update(Action::RowNext);
        // The rows drawn whole are `visible - 1` of them when the last
        // is cut; the selection must be inside that part.
        let whole = app.home.scroll + visible - 1;
        assert!(
            app.home.row < whole,
            "step {step}: selected row {} is the cut one (scroll {}, {visible} visible)",
            app.home.row,
            app.home.scroll
        );
    }
}

#[test]
fn j_and_k_leave_an_opened_view_for_the_next_section() {
    // They moved the nav and the opened album stayed on top of it, so
    // the keys looked like they did nothing at all.
    let mut app = with_albums(3);
    app.update(Action::ActivateSelection);
    assert!(app.open.is_some(), "an album is open");
    let was = app.sidebar.section();

    app.update(Action::SidebarNext);
    assert!(app.open.is_none(), "the album closed");
    assert_ne!(app.sidebar.section(), was, "and the section moved");
}

#[test]
fn back_returns_to_an_opened_view_left_by_the_nav() {
    // The view J and K closed is on the history, so back is how the
    // user gets to it again.
    let mut app = with_albums(3);
    app.update(Action::ActivateSelection);
    let title = app.open.as_ref().expect("open").title.clone();

    app.update(Action::SidebarNext);
    assert!(app.open.is_none());

    app.update(Action::GoBack);
    assert_eq!(
        app.open.as_ref().map(|o| o.title.as_str()),
        Some(title.as_str()),
        "the album the nav closed comes back"
    );
}

#[test]
fn o_opens_the_whole_of_a_home_row() {
    // The page hands back six items per row out of hundreds; the web
    // client has a "See all" for the rest and this is it.
    let mut app = home_with_a_track_row();
    assert!(matches!(
        key(&mut app, KeyCode::Char('o')),
        Some(Action::SeeAll)
    ));

    app.update(Action::SeeAll);
    let open = app.open.as_ref().expect("the row is open");
    assert_eq!(open.title, "New Tracks", "under its own heading");
    assert!(app.tracks.is_empty(), "waiting for the reply");
    assert_eq!(app.tracklist.selected, 0, "from the top");
}

#[test]
fn explore_is_its_own_page_rather_than_the_favourites_list() {
    // It fell through to the arm that draws the track list, so three
    // sidebar entries showed the same thing and none said what it was.
    let mut app = signed_in(sidebar::Section::Explore);
    app.update(Action::ExploreLoaded(Box::new(explore_rows())));

    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let text = geometry::text(&terminal.backend().buffer().clone());

    assert!(text.contains("Genres"), "explore's own rows:\n{text}");
    assert!(!text.contains("TITLE"), "not the track list:\n{text}");
}

#[test]
fn reaching_explore_asks_for_its_page() {
    // Nothing else fetches it, so without this the section drew an
    // empty page until something unrelated happened to load it.
    let mut app = signed_in(sidebar::Section::Music);
    let mut asked = false;
    for _ in 0..sidebar::Section::ALL.len() {
        if matches!(app.update(Action::SidebarNext), Some(Action::LoadExplore)) {
            asked = true;
            break;
        }
    }
    assert!(asked, "moving onto Explore asks for its page");
}

#[test]
fn explore_is_not_asked_for_twice() {
    // Its rows do not change under us, and a fetch on every pass of the
    // sidebar would be a request per keystroke.
    let mut app = signed_in(sidebar::Section::Explore);
    app.update(Action::ExploreLoaded(Box::new(explore_rows())));

    app.update(Action::SidebarNext);
    let mut again = false;
    for _ in 0..sidebar::Section::ALL.len() {
        if matches!(
            app.update(Action::SidebarPrevious),
            Some(Action::LoadExplore)
        ) {
            again = true;
        }
    }
    assert!(!again, "the page it already has is not fetched again");
}

#[test]
fn the_movement_keys_drive_explores_own_rows() {
    // `on_home` read the section rather than what is drawn, which is
    // the fault that has come back four times: j and k would have moved
    // the home page behind Explore.
    let mut app = signed_in(sidebar::Section::Explore);
    let mut home = explore_rows();
    home.rows.push(crate::browse::HomeRow {
        kind: crate::browse::RowKind::Carousel,
        heading: "Decades".into(),
        cards: vec![carousel::Card::new("80s", "")],
        more: None,
    });
    app.update(Action::ExploreLoaded(Box::new(home)));
    app.last_main_height = 40;

    assert!(app.on_home(), "explore moves like a page of rows");
    app.update(Action::RowNext);
    assert_eq!(app.explore.row, 1, "explore's own selection moved");
    assert_eq!(app.home.row, 0, "and the home page's did not");
}

#[test]
fn escape_goes_back_one_view_at_a_time() {
    // Escape used to drop straight to the sidebar's section from any
    // depth: a single `came_from` is all one level can remember. Opening
    // from inside an opened view is the ordinary case — a profile, an
    // artist, then one of their albums — and each of those is a step
    // back, not a jump out.
    let mut app = signed_in(sidebar::Section::Profiles);
    app.artists = vec![crate::library::Artist {
        id: (1).to_string(),
        name: "A Profile".into(),
        picture: None,
    }];

    // Into the profile.
    app.update(Action::ActivateSelection);
    let first = app.open.as_ref().expect("the profile opened").title.clone();
    assert_eq!(first, "A Profile");

    // From inside it, into something else — the cards an opened view
    // holds are what enter opens next.
    app.open_cards = vec![{
        let mut c = carousel::Card::new("An Album", "An Artist");
        c.target = Some(carousel::Target::Album("7".into()));
        c
    }];
    app.update(Action::ActivateSelection);
    assert_eq!(
        app.open.as_ref().map(|o| o.title.as_str()),
        Some("An Album"),
        "the second view is what is open now"
    );

    // Back out one: the profile again, not the sidebar.
    app.update(Action::CloseCollection);
    assert_eq!(
        app.open.as_ref().map(|o| o.title.as_str()),
        Some("A Profile"),
        "escape goes back one view, not all the way out"
    );

    // And once more: out to the section.
    app.update(Action::CloseCollection);
    assert!(app.open.is_none(), "the last escape leaves the stack");
    assert_eq!(app.sidebar.section(), sidebar::Section::Profiles);
}

#[test]
fn moving_down_the_page_walks_out_of_one_section_into_the_next() {
    // Every section is drawn, so every section has to take the keys —
    // the fault that has come back five times now.
    let mut app = with_artist_open();
    assert_eq!(app.artist.section, artistview::Section::Tracks.index());

    // Two tracks: one press moves within them, the next leaves.
    app.update(Action::TrackNext);
    assert_eq!(
        app.artist.section,
        artistview::Section::Tracks.index(),
        "still in the tracks"
    );
    assert_eq!(app.artist.tracks.selected, 1, "on the second of them");
    app.update(Action::TrackNext);
    assert_eq!(
        app.artist.section,
        artistview::Section::Albums.index(),
        "off the end of the tracks and into the albums"
    );
    app.update(Action::TrackNext);
    assert_eq!(
        app.artist.section,
        artistview::Section::Similar.index(),
        "and on to the similar artists"
    );

    // And back up.
    app.update(Action::TrackPrevious);
    assert_eq!(app.artist.section, artistview::Section::Albums.index());
}

#[test]
fn h_and_l_run_along_the_pages_card_rows() {
    let mut app = with_artist_open();
    app.artist.section = artistview::Section::Albums.index();

    assert!(matches!(
        key(&mut app, KeyCode::Char('l')),
        Some(Action::ArtistRight)
    ));
    app.update(Action::ArtistRight);
    assert_eq!(
        app.artist.rows[artistview::Section::Albums.index()].selected,
        1,
        "along the album row"
    );
    app.update(Action::ArtistLeft);
    assert_eq!(
        app.artist.rows[artistview::Section::Albums.index()].selected,
        0
    );
}

#[test]
fn every_overlay_asks_for_a_repaint_when_it_closes() {
    // An overlay writes over cells the image protocols marked `Skip`.
    // The next frame marks them `Skip` again and writes nothing, so
    // whatever the modal put there stays on the artwork until a full
    // repaint. The help remembered; the queue did not, and left a
    // rectangle of itself over the covers.
    // The help closes on any key; `z` is used by nothing else.
    let mut app = signed_in(sidebar::Section::Music);
    app.showing_help = true;
    let _ = app.take_repaint();
    app.on_key(KeyEvent::from(KeyCode::Char('z')));
    assert!(
        app.take_repaint(),
        "closing the help leaves it on the artwork without one"
    );

    // The queue is a view now, and escape is a step back out of it.
    let mut app = signed_in(sidebar::Section::Music);
    key(&mut app, KeyCode::Char('Q'));
    let _ = app.take_repaint();
    let back = key(&mut app, KeyCode::Esc).expect("escape steps back");
    app.update(back);
    assert!(app.list != Some(List::Queue), "escape leaves the queue");
    assert!(
        app.take_repaint(),
        "leaving the queue leaves it on the artwork without one"
    );
}

#[test]
fn the_feed_is_cut_into_four_dated_sections() {
    // A flat grid of releases said nothing about when any of them
    // landed. The windows are the last seven days, the seven before
    // that, the rest of the month and everything behind it.
    let today = 20_000;
    let card = |title: &str, age: i64| carousel::Card {
        day: Some(today - age),
        ..carousel::Card::new(title, "An Artist")
    };
    let rows = feed_rows(
        &[
            card("Today", 0),
            card("Six days", 6),
            card("Eight days", 8),
            card("Twenty days", 20),
            card("A year", 365),
        ],
        today,
    );

    let headings: Vec<&str> = rows.iter().map(|r| r.heading.as_str()).collect();
    assert_eq!(
        headings,
        ["This week", "Last week", "Last month", "Older"],
        "the four windows, newest first"
    );
    assert_eq!(
        rows[0].cards.len(),
        2,
        "today and six days ago are this week"
    );
    assert_eq!(rows[1].cards[0].title, "Eight days");
    assert_eq!(rows[2].cards[0].title, "Twenty days");
    assert_eq!(rows[3].cards[0].title, "A year");
}

#[test]
fn a_section_with_nothing_in_it_is_not_drawn_at_all() {
    // A heading over no cards reads as a fault. The sections are built
    // from what is there rather than drawn and then filled.
    let today = 20_000;
    let rows = feed_rows(
        &[carousel::Card {
            day: Some(today),
            ..carousel::Card::new("Today", "An Artist")
        }],
        today,
    );
    assert_eq!(
        rows.len(),
        1,
        "only the week that has something in it: {:?}",
        rows.iter().map(|r| &r.heading).collect::<Vec<_>>()
    );
}

#[test]
fn a_release_with_no_date_is_shown_under_the_oldest_heading() {
    // The card is still a release by someone the user follows, so it
    // belongs on the page rather than being dropped for want of a
    // stamp.
    let rows = feed_rows(&[carousel::Card::new("Undated", "An Artist")], 20_000);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].heading, "Older");
    assert_eq!(rows[0].cards[0].title, "Undated");
}

#[test]
fn see_all_on_a_feed_section_opens_what_is_already_held() {
    // The four sections are cut from one reply, so there is no path to
    // fetch: `o` opens the cards in hand. Without this the key did
    // nothing at all, the rows having no `more`.
    let mut app = signed_in(sidebar::Section::Feed);
    let today = today();
    app.update(Action::FeedLoaded(
        (0..12)
            .map(|i| carousel::Card {
                day: Some(today),
                ..carousel::Card::new(format!("Release {i}"), "An Artist")
            })
            .collect(),
    ));
    app.last_main_width = 100;
    app.last_main_height = 40;

    app.update(Action::SeeAll);
    let open = app.open.as_ref().expect("the section opened");
    assert_eq!(open.title, "This week", "under its own heading");
    assert_eq!(
        app.open_cards.len(),
        12,
        "with the whole of what the section holds"
    );
}

#[test]
fn the_feed_is_its_own_view_rather_than_the_favourites_list() {
    // It fell through to the arm that draws the track list, so the
    // section showed the same thing as Tracks and said nothing about
    // what the followed artists had released.
    let mut app = signed_in(sidebar::Section::Feed);
    app.update(Action::FeedLoaded(vec![
        carousel::Card::new("Discovery", "Daft Punk"),
        carousel::Card::new("Homework", "Daft Punk"),
    ]));

    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let text = geometry::text(&terminal.backend().buffer().clone());

    assert!(text.contains("Discovery"), "the feed's own cards:\n{text}");
    assert!(!text.contains("TITLE"), "not the track list:\n{text}");
}

#[test]
fn reaching_the_feed_asks_for_it_once() {
    let mut app = signed_in(sidebar::Section::Music);
    let mut asked = 0;
    for _ in 0..sidebar::Section::ALL.len() * 2 {
        if matches!(app.update(Action::SidebarNext), Some(Action::LoadFeed)) {
            asked += 1;
            // The reply it would have got.
            app.update(Action::FeedLoaded(vec![carousel::Card::new("A", "B")]));
        }
    }
    assert_eq!(asked, 1, "asked for once, not on every pass");
}

#[test]
fn the_movement_keys_drive_the_feeds_own_rows() {
    // `rows_on_screen_mut` routes by section, so without the feed in it
    // the keys would move the home page's rows behind — the fault that
    // has come back five times, in the grid's version of this routing.
    let mut app = signed_in(sidebar::Section::Feed);
    let today = today();
    app.update(Action::FeedLoaded(
        (0..6)
            .map(|i| carousel::Card {
                day: Some(today - i64::from(i)),
                ..carousel::Card::new(format!("Release {i}"), "An Artist")
            })
            .collect(),
    ));
    app.last_main_width = 100;
    app.last_main_height = 30;

    assert!(app.on_home(), "the feed moves like a page of rows");
    app.update(Action::CarouselNext);
    assert_eq!(
        app.feed.rows[0].state.selected, 1,
        "the feed's own selection moved"
    );
    assert!(
        app.home.rows.is_empty(),
        "and the home page's rows were left alone"
    );
}

#[test]
fn an_empty_home_is_built_from_the_collection_as_it_loads() {
    // Deezer answers its home page with no rows at all, so the Music
    // page was blank while Playlists and Albums had plenty.
    let mut app = signed_in(sidebar::Section::Music);
    app.update(Action::HomeLoaded(Box::new(crate::browse::Home {
        rows: Vec::new(),
    })));
    assert!(app.home.rows.is_empty());
    app.update(Action::PlaylistsLoaded(vec![crate::library::Playlist {
        uuid: "p1".into(),
        title: "Mine".into(),
        track_count: 3,
        duration: None,
        creator: "me".into(),
        cover: None,
        owned: false,
    }]));
    app.update(Action::AlbumsLoaded(vec![crate::library::Album {
        id: "a1".into(),
        title: "Blue".into(),
        artist: "Miles".into(),
        year: Some("1959".into()),
        cover: None,
        track_count: 5,
        duration: None,
    }]));
    let headings: Vec<&str> = app.home.rows.iter().map(|r| r.heading.as_str()).collect();
    assert_eq!(headings, ["Playlists", "Favourite albums"]);
    assert_eq!(app.home.rows[1].cards[0].title, "Blue");

    // A home page with rows of its own is left alone.
    app.update(Action::HomeLoaded(Box::new(crate::browse::Home {
        rows: vec![crate::browse::HomeRow {
            heading: "Picks".into(),
            kind: Default::default(),
            cards: vec![carousel::Card::new("x", "y")],
            more: None,
        }],
    })));
    app.update(Action::AlbumsLoaded(Vec::new()));
    assert_eq!(app.home.rows.len(), 1);
    assert_eq!(app.home.rows[0].heading, "Picks");
}

#[test]
fn an_empty_home_says_loading_until_the_reply_and_nothing_after() {
    let text_of = |app: &mut App| {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        geometry::text(terminal.backend().buffer())
    };
    let mut app = signed_in(sidebar::Section::Music);
    assert!(text_of(&mut app).contains("Loading"), "before the reply");
    app.update(Action::HomeLoaded(Box::new(crate::browse::Home {
        rows: Vec::new(),
    })));
    let text = text_of(&mut app);
    assert!(
        !text.contains("Loading") && text.contains("nothing to show"),
        "{text}"
    );
}

#[test]
fn an_empty_explore_is_remembered_as_such() {
    let mut app = signed_in(sidebar::Section::Explore);
    app.update(Action::ExploreLoaded(Box::new(crate::browse::Home {
        rows: Vec::new(),
    })));
    assert!(app.explore_empty);
}

#[test]
fn local_files_without_a_folder_ask_for_one_and_write_it() {
    let mut app = App::default();
    app.config.provider.kind = crate::config::ProviderKind::Local;
    app.update(Action::BeginLogin);
    assert!(matches!(
        &app.login,
        login::LoginState::Text { field: login::Field::Folder, typed } if typed == "~/Music"
    ));

    for _ in 0..7 {
        key(&mut app, KeyCode::Backspace);
    }
    for c in "/nowhere".chars() {
        assert!(
            key(&mut app, KeyCode::Char(c)).is_none(),
            "{c} is typed, not bound"
        );
    }
    let sent = match key(&mut app, KeyCode::Enter) {
        Some(Action::Typed(login::Field::Folder, path)) => path,
        other => panic!("Enter sends the folder, got {other:?}"),
    };
    assert_eq!(sent, "/nowhere");
    assert!(app
        .update(Action::Typed(login::Field::Folder, sent))
        .is_none());
    assert!(
        matches!(app.login, login::LoginState::Failed(_)),
        "{:?}",
        app.login
    );
    assert!(
        app.config.local.folders.is_empty(),
        "nothing is written for a path that is not there"
    );

    let here = std::env::temp_dir().display().to_string();
    assert!(matches!(
        app.update(Action::Typed(login::Field::Folder, here.clone())),
        Some(Action::BeginLogin)
    ));
    assert_eq!(app.config.local.folders, vec![here]);
}
