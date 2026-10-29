use super::*;

#[test]
fn every_card_a_grid_counts_as_visible_is_drawn() {
    // The Feed showed six of fourteen releases: `rows` said how many
    // fit and the loop drew fewer, so the rest were reachable by the
    // keys and invisible on screen. Asked of the albums now, the Feed
    // having become a page of rows -- this is about the grid, and the
    // albums are one.
    let mut app = signed_in(sidebar::Section::Albums);
    app.update(Action::AlbumsLoaded(
        (0..14)
            .map(|i| crate::library::Album {
                id: (i).to_string(),
                title: format!("Release {i}"),
                artist: "An Artist".into(),
                year: None,
                cover: None,
                track_count: 0,
                duration: None,
            })
            .collect(),
    ));

    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 40)).unwrap();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let text = geometry::text(&terminal.backend().buffer().clone());

    // Counted by title, so this covers the rows drawn whole. The row
    // at the fold is cut off — its title is the first thing to go — so
    // what it holds is that no whole row is missing.
    let drawn = (0..14)
        .filter(|i| text.contains(&format!("Release {i}")))
        .count();
    let (cols, rows) = app.grid_geometry();
    let whole_rows = rows.saturating_sub(1);
    assert!(
        drawn >= (cols * whole_rows).min(14),
        "the grid drew {drawn} cards where {} rows fit whole\n{text}",
        whole_rows
    );
}

#[test]
fn a_selected_cards_shade_stays_out_of_the_sidebar() {
    // The shade is a column wider than the card, so the leftmost card's
    // ran over the rule and into the nav beside it.
    let palette = theme::Palette::detect();
    let mut app = signed_in(sidebar::Section::Playlists);
    app.playlists = (0..3)
        .map(|i| crate::library::Playlist {
            uuid: format!("u{i}"),
            title: format!("Playlist {i}"),
            track_count: 10,
            duration: None,
            creator: "TIDAL".into(),
            cover: None,
            owned: false,
        })
        .collect();

    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let buf = terminal.backend().buffer().clone();

    let regions = layout::split(ratatui::layout::Rect::new(0, 0, 100, 30));
    let card = geometry::find(&buf, "Playlist 0").expect("the selected card");

    // Nothing shaded at or before the rule, on the card's own rows.
    for x in 0..=regions.divider.x {
        assert_ne!(
            buf[(x, card.row)].bg,
            palette.selection,
            "column {x} is shaded, at or before the rule at {}\n{}",
            regions.divider.x,
            geometry::text(&buf)
        );
    }
}

#[test]
fn a_terminal_too_narrow_for_a_rule_gives_it_up() {
    // The nav and the content matter more than the line between them.
    let r = layout::split(ratatui::layout::Rect::new(0, 0, 20, 10));
    assert_eq!(r.divider.width, 0);
    assert_eq!(r.main.x, r.sidebar.right(), "the panes abut again");
}

#[test]
fn the_selection_stays_on_screen_inside_an_opened_album() {
    // Scrolling used the no-banner row count while the view drew with
    // one, so the selection could sit three rows below the last row
    // actually painted — the cursor simply vanished off the bottom.
    let mut app = with_albums(3);
    app.update(Action::ActivateSelection);
    app.last_main_height = 38;
    app.tracks = (0..60)
        .map(|i| {
            crate::domain::Track::sample(
                &format!("Track {i}"),
                "An Artist",
                std::time::Duration::from_secs(100),
            )
        })
        .collect();

    // Walk to the bottom of the list.
    for _ in 0..60 {
        app.update(Action::TrackNext);
    }

    let drawn = app.list_visible();
    let last_visible = app.tracklist.offset + drawn;
    assert!(
        app.tracklist.selected < last_visible,
        "selection {} is past the last drawn row {} (offset {}, {drawn} visible)",
        app.tracklist.selected,
        last_visible,
        app.tracklist.offset,
    );
}

#[test]
fn each_grid_keeps_its_own_place() {
    // Moving around Albums must not disturb where the user was in
    // Playlists — they are separate views, not one list.
    let mut app = signed_in(sidebar::Section::Playlists);
    app.playlists = (0..30)
        .map(|i| crate::library::Playlist::sample(&format!("P{i}"), i))
        .collect();
    app.albums = (0..30)
        .map(|i| crate::library::Album {
            id: (i as u64).to_string(),
            title: format!("A{i}"),
            artist: "x".into(),
            year: None,
            cover: None,
            track_count: 10,
            duration: None,
        })
        .collect();

    app.update(Action::TrackNext);
    let playlist_row = app.playlist_grid.selected;
    assert!(playlist_row > 0, "the playlist grid moved");

    while app.sidebar.section() != sidebar::Section::Albums {
        app.sidebar.next();
    }
    app.update(Action::TrackNext);
    assert!(app.album_grid.selected > 0, "the album grid moved");
    assert_eq!(
        app.playlist_grid.selected, playlist_row,
        "the playlist grid must not have moved with it"
    );
}

#[test]
fn a_grid_moves_by_rows_down_and_by_cards_sideways() {
    let mut app = signed_in(sidebar::Section::Playlists);
    app.playlists = (0..40)
        .map(|i| crate::library::Playlist::sample(&format!("P{i}"), i))
        .collect();

    let (cols, _) = app.grid_geometry();
    assert!(cols > 1, "120 columns must fit several cards");

    app.update(Action::TrackNext);
    assert_eq!(app.playlist_grid.selected, cols, "j moves a whole row");

    app.update(Action::CarouselNext);
    assert_eq!(app.playlist_grid.selected, cols + 1, "l moves one card");

    app.update(Action::CarouselPrevious);
    assert_eq!(app.playlist_grid.selected, cols);
}

#[test]
fn a_filtered_grid_opens_the_card_that_is_highlighted() {
    // The selection indexes the filtered list. Using it against the full
    // list would open a different playlist than the one shown selected.
    let mut app = signed_in(sidebar::Section::Playlists);
    app.playlists = vec![
        crate::library::Playlist::sample("Alpha", 1),
        crate::library::Playlist::sample("Beta", 2),
        crate::library::Playlist::sample("Gamma", 3),
    ];
    for (i, p) in app.playlists.iter_mut().enumerate() {
        p.uuid = format!("uuid-{i}");
    }

    // Filter to the one playlist whose title contains "amm": Gamma, at
    // index 2 of the full list but index 0 of the filtered one.
    app.set_filter("amm".into());
    assert_eq!(app.playlist_grid.selected, 0);

    match app.selected_collection() {
        Some(Collection::Playlist(uuid)) => {
            assert_eq!(uuid, "uuid-2", "must open Gamma, not the first playlist");
        }
        other => panic!("expected a playlist, got {other:?}"),
    }
}

#[test]
fn opening_an_album_names_it_from_the_card_that_was_selected() {
    // The header has to say which album this is. Taking the name from
    // the card means it is right immediately, without waiting on the
    // request that fetches the tracks.
    let mut app = with_albums(5);
    app.album_grid.selected = 2;

    let open = app.selected_identity().expect("an album is selected");
    assert_eq!(open.title, "Album 2");
    assert_eq!(open.subtitle, "Artist 2");
    assert_eq!(open.detail, "2026");
    assert_eq!(open.came_from, sidebar::Section::Albums);
}

#[test]
fn an_opened_album_does_not_move_the_sidebar() {
    // It used to jump to Tracks, which highlighted the wrong nav entry
    // and made the album indistinguishable from the favourites view.
    let mut app = with_albums(3);
    app.update(Action::ActivateSelection);

    assert_eq!(
        app.sidebar.section(),
        sidebar::Section::Albums,
        "the sidebar stays on Albums while an album is open"
    );
    assert!(app.open.is_some());
}

#[test]
fn closing_returns_to_the_grid_it_was_opened_from() {
    let mut app = with_albums(9);
    app.album_grid.selected = 4;
    app.update(Action::ActivateSelection);
    // Pretend the fetch landed.
    app.tracks = vec![crate::domain::Track::sample(
        "t",
        "a",
        std::time::Duration::ZERO,
    )];
    // And that the user moved around inside it.
    app.tracklist.selected = 0;

    app.update(Action::CloseCollection);

    assert!(app.open.is_none());
    assert_eq!(app.sidebar.section(), sidebar::Section::Albums);
    assert_eq!(
        app.album_grid.selected, 4,
        "the grid selection is where it was left"
    );
    assert!(
        app.tracks.is_empty(),
        "the album's tracks are not left behind as favourites"
    );
}

#[test]
fn h_in_an_open_album_does_not_reach_the_sidebar() {
    // It no longer closes the album either -- `h` is a direction
    // everywhere -- but the thing this has always guarded is that it
    // does not fall through to the nav behind the view.
    let mut app = with_albums(3);
    app.update(Action::ActivateSelection);

    let action = key(&mut app, KeyCode::Char('h'));
    assert!(
        !matches!(
            action,
            Some(Action::SidebarPrevious | Action::GoToSection(_))
        ),
        "the keyboard stays in the album, got {action:?}"
    );
    assert!(app.open.is_some(), "which is still open");
}

#[test]
fn opening_an_album_gives_it_the_keyboard() {
    // Without this the opened view could not be moved around in: focus
    // stayed on the grid behind it, so j and k drove a list nobody could
    // see any more.
    let mut app = with_albums(3);

    app.update(Action::ActivateSelection);

    assert!(app.open.is_some());
    assert!(
        matches!(key(&mut app, KeyCode::Char('j')), Some(Action::TrackNext)),
        "and j moves through its tracks"
    );
}

#[test]
fn a_track_card_is_not_a_collection() {
    // A track plays; it has no track list to open.
    let mut app = signed_in(sidebar::Section::Music);
    app.home.rows = vec![home::Row {
        kind: crate::browse::RowKind::Carousel,
        heading: "New Tracks".into(),
        cards: vec![carousel::Card {
            title: "Bass Persuades".into(),
            target: Some(carousel::Target::Track("9".into())),
            ..Default::default()
        }],
        state: carousel::CarouselState::default(),
    }];

    assert!(app.selected_collection().is_none());
    assert_eq!(
        app.selected_track_id(),
        Some(crate::domain::TrackId::from(9))
    );

    app.update(Action::ActivateSelection);
    assert!(app.open.is_none(), "a track does not open a view");
}

#[test]
fn a_card_with_no_identifier_does_nothing() {
    // Mixes and some modules come back without an id; enter on one must
    // not open an empty view.
    let mut app = signed_in(sidebar::Section::Music);
    app.home.rows = vec![home::Row {
        kind: crate::browse::RowKind::Carousel,
        heading: "My Mixes".into(),
        cards: vec![carousel::Card {
            title: "My Mix 1".into(),
            ..Default::default()
        }],
        state: carousel::CarouselState::default(),
    }];

    assert!(app.selected_collection().is_none());
    app.update(Action::ActivateSelection);
    assert!(app.open.is_none());
}

#[test]
fn j_and_k_move_through_an_opened_album_not_the_grid_behind_it() {
    // The bug this is here for: the sidebar deliberately stays on Albums
    // while an album is open, and `on_grid` read only the sidebar — so
    // j moved the grid's selection, which was no longer on screen, and
    // the track list never budged.
    let mut app = with_albums(9);
    app.album_grid.selected = 3;
    app.update(Action::ActivateSelection);

    // Stand in for the fetch that fills the view.
    app.tracks = (0..6)
        .map(|i| {
            crate::domain::Track::sample(
                &format!("Track {i}"),
                "An Artist",
                std::time::Duration::from_secs(100),
            )
        })
        .collect();

    assert!(!app.on_grid(), "an opened album is a list, not a grid");

    app.update(Action::TrackNext);
    assert_eq!(app.tracklist.selected, 1, "j moves down the album's tracks");
    app.update(Action::TrackNext);
    assert_eq!(app.tracklist.selected, 2);
    app.update(Action::TrackPrevious);
    assert_eq!(app.tracklist.selected, 1);

    assert_eq!(
        app.album_grid.selected, 3,
        "and the grid behind it has not moved"
    );
}

#[test]
fn favourites_landing_late_do_not_replace_an_open_album() {
    // Both fetches used to report through one action, so the startup
    // favourites request finishing after an album was opened would
    // silently swap the album's tracks for the user's whole library.
    let mut app = with_albums(3);
    app.update(Action::ActivateSelection);

    app.update(Action::CollectionLoaded {
        request: open_request(&app),
        tracks: some_tracks(&["Album track"]),
    });
    assert_eq!(app.tracks.len(), 1);

    // The startup fetch lands now.
    app.update(Action::TracksLoaded(some_tracks(&[
        "Fav 1", "Fav 2", "Fav 3",
    ])));

    assert_eq!(
        app.tracks.len(),
        1,
        "the album's tracks are still what is shown"
    );
    assert_eq!(app.tracks[0].title, "Album track");
}

#[test]
fn a_reply_for_an_album_that_was_left_is_dropped() {
    // Open one album, leave it before the response arrives: the tracks
    // must not appear under whatever is on screen now.
    let mut app = with_albums(5);
    app.update(Action::ActivateSelection);
    let first = open_request(&app);
    app.update(Action::CloseCollection);

    app.update(Action::CollectionLoaded {
        request: first,
        tracks: some_tracks(&["Late", "Reply"]),
    });

    assert!(
        app.tracks.is_empty(),
        "a reply for a closed view is dropped"
    );
}

#[test]
fn a_view_left_while_loading_is_asked_for_again_on_the_way_back() {
    // Its reply was dropped as a late one for a view that had been left,
    // so coming back to it found an empty list and nothing on its way.
    let mut app = with_albums(3);
    app.update(Action::ActivateSelection);
    let first = open_request(&app);
    assert!(app.awaiting, "nothing has arrived");
    app.update(Action::GoBack);
    assert!(app.open.is_none());
    app.update(Action::GoForward);
    let open = app.open.as_ref().expect("the album is back");
    assert!(app.awaiting, "and still waiting");
    assert!(
        open.request > first,
        "stamped afresh, so the loop fetches it again"
    );
    assert!(open.target.is_some(), "with something to fetch");
}

#[test]
fn a_reply_for_the_previous_album_does_not_fill_the_current_one() {
    let mut app = with_albums(5);
    app.update(Action::ActivateSelection);
    let first = open_request(&app);

    // Straight on to another album.
    app.update(Action::CloseCollection);
    app.album_grid.selected = 2;
    app.update(Action::ActivateSelection);
    let second = open_request(&app);
    assert_ne!(first, second);

    // The first album's reply arrives now.
    app.update(Action::CollectionLoaded {
        request: first,
        tracks: some_tracks(&["Wrong album"]),
    });
    assert!(app.tracks.is_empty(), "the stale reply is dropped");

    // And the right one still fills it.
    app.update(Action::CollectionLoaded {
        request: second,
        tracks: some_tracks(&["Right album"]),
    });
    assert_eq!(app.tracks[0].title, "Right album");
}

#[test]
fn favourites_still_fill_the_tracks_view_when_nothing_is_open() {
    let mut app = signed_in(sidebar::Section::Tracks);
    app.update(Action::TracksLoaded(some_tracks(&["Fav 1", "Fav 2"])));
    assert_eq!(app.tracks.len(), 2);
}

#[test]
fn every_way_of_opening_a_view_leaves_the_same_state_behind() {
    // The four of these were written out by hand and had drifted: one
    // pushed no level at all, so the view it opened could not be left;
    // one kept the artist page it covered; one reset the track list by
    // its selection alone. Whatever opens a view, the state behind it
    // has to be the same, or the keys act on the view underneath.
    let opens = [
        ("enter on a card", Action::ActivateSelection),
        ("see all on a row", Action::SeeAll),
    ];

    for (what, action) in opens {
        let mut app = with_albums(3);
        app.home.rows.push(home::Row {
            heading: "A row".into(),
            kind: crate::browse::RowKind::Carousel,
            cards: vec![carousel::Card::new("A card", "artist")],
            state: carousel::CarouselState::default(),
        });
        // State the view being left had, which must not survive it.
        app.artist.scroll = 4;
        app.artist.bio_open = true;
        app.tracklist.offset = 9;
        app.tracklist.filter = "leftover".into();

        let before = app.back.len();
        app.update(action.clone());
        if app.open.is_none() {
            continue; // this one had nothing to open here
        }

        assert_eq!(
            app.back.len(),
            before + 1,
            "{what}: nothing went on the history, so back has nothing to pop"
        );
        assert!(app.artist.page.is_none(), "{what}: an artist page survived");
        assert_eq!(app.artist.scroll, 0, "{what}: the artist scroll survived");
        assert!(!app.artist.bio_open, "{what}: an open bio survived");
        // The track list and its tracks are not asserted here: the level
        // takes them on its way out, so they are already empty whatever
        // `open_view` does with them. The artist fields are the ones it
        // is really answerable for -- nothing else clears those.

        // And it can be left again, which is what the history is for.
        assert!(app.go_back(), "{what}: back found nothing to return to");
        assert!(app.open.is_none(), "{what}: back did not close it");
    }
}

#[test]
fn a_row_that_fits_offers_nothing_but_still_opens() {
    // A row of one card: nothing is behind it, so the hint stays off.
    // The key still opens it as a grid of what is held, which is the
    // same cards drawn larger -- harmless, and cheaper than a key that
    // works on some rows and not others.
    let mut app = signed_in(sidebar::Section::Music);
    app.home.rows.push(home::Row {
        heading: "A short row".into(),
        kind: crate::browse::RowKind::Carousel,
        cards: vec![carousel::Card::new("The only card", "artist")],
        state: carousel::CarouselState::default(),
    });

    let buf = geometry::draw(120, 30, |f, _area, _p| draw(f, &mut app));
    let text = geometry::text(&buf);
    assert!(
        !text.contains("See all"),
        "nothing is behind it, so nothing offers to show it:\n{text}"
    );
    assert!(
        matches!(key(&mut app, KeyCode::Char('o')), Some(Action::SeeAll)),
        "and o still opens it"
    );
    app.update(Action::SeeAll);
    assert_eq!(app.open_cards.len(), 1, "as a grid of what is held");
}

#[test]
fn opening_something_new_drops_what_was_ahead() {
    // A browser does the same: following a link after going back means
    // the forward arrow has nowhere to go.
    let mut app = with_albums(3);
    app.update(Action::SidebarNext);
    app.update(Action::GoBack);
    assert!(!app.forward.is_empty(), "there is something ahead");

    app.update(Action::ActivateSelection);
    assert!(
        app.forward.is_empty(),
        "opening something is a new branch, so what was ahead is gone"
    );
}

#[test]
fn o_opens_a_carousel_row_too() {
    // Every module on the page carries a paging path, not just the
    // track grids: New Albums shows ten of a hundred and sixty-six.
    let mut app = home_with_a_carousel_row();
    assert!(matches!(
        key(&mut app, KeyCode::Char('o')),
        Some(Action::SeeAll)
    ));
    app.update(Action::SeeAll);
    assert_eq!(
        app.open.as_ref().expect("opened").title,
        "New Albums",
        "under its own heading"
    );
}

#[test]
fn a_row_of_covers_opens_as_a_grid_not_a_track_list() {
    // A cover has no track to show in a list, so the row keeps its
    // cards and gets the grid the collection views use.
    let mut app = home_with_a_carousel_row();
    app.update(Action::SeeAll);

    assert_eq!(app.open_cards.len(), 3, "the cards are held");
    assert!(app.on_grid(), "and the movement keys drive a grid");
    assert_eq!(
        app.grid_cards(app.sidebar.section()).len(),
        3,
        "which reads the opened row rather than the section behind it"
    );
}

#[test]
fn closing_a_row_takes_its_cards_with_it() {
    // Left behind, they would show under whatever view was drawn next.
    let mut app = home_with_a_carousel_row();
    app.update(Action::SeeAll);
    assert!(!app.open_cards.is_empty());

    app.update(Action::CloseCollection);
    assert!(app.open.is_none());
    assert!(app.open_cards.is_empty(), "and its cards are gone");
}

#[test]
fn the_rows_tracks_arrive_under_its_heading() {
    let mut app = home_with_a_track_row();
    app.update(Action::SeeAll);
    app.update(Action::CollectionLoaded {
        request: open_request(&app),
        tracks: some_tracks(&["One", "Two", "Three"]),
    });
    assert_eq!(app.tracks.len(), 3, "the row's tracks are shown");
}

#[test]
fn a_reply_for_a_row_the_user_has_left_is_dropped() {
    // Same race as an album: the reply names what it was for.
    let mut app = home_with_a_track_row();
    app.update(Action::SeeAll);
    let other_row = RequestId(open_request(&app).0 + 1);
    app.update(Action::CollectionLoaded {
        request: other_row,
        tracks: some_tracks(&["One", "Two"]),
    });
    assert!(app.tracks.is_empty(), "not shown under the wrong heading");
}

#[test]
fn a_key_that_would_open_what_is_showing_does_nothing() {
    // Seven presses of `c` on a one-track single stacked the same album
    // seven times, and it took eight steps back to leave it.
    let mut app = signed_in(sidebar::Section::Tracks);
    let track = crate::domain::Track {
        id: crate::domain::TrackId::from(1),
        album: "Single".into(),
        album_id: Some("7".into()),
        ..crate::domain::Track::sample("Song", "Someone", std::time::Duration::from_secs(1))
    };
    app.tracks = vec![track.clone()];
    let open = key(&mut app, KeyCode::Char('c')).expect("opens the album");
    app.update(open);
    // Still loading: a repeat of the key must not open it again. Held
    // down, it opened the album once per repeat, each a level and a
    // request of its own.
    let depth = app.back.len();
    let request = open_request(&app);
    assert!(
        key(&mut app, KeyCode::Char('c')).is_none(),
        "already on its way"
    );
    assert_eq!(app.back.len(), depth, "nothing stacked while it loads");
    assert_eq!(open_request(&app), request, "and nothing asked for again");
    app.update(Action::CollectionLoaded {
        request,
        tracks: vec![track],
    });
    assert_eq!(app.showing(), Showing::Tracks);

    assert!(
        key(&mut app, KeyCode::Char('c')).is_none(),
        "already looking at it"
    );
    assert_eq!(app.back.len(), depth, "and nothing was stacked");
}

#[test]
fn a_reaches_the_favourite_toggle() {
    let mut app = signed_in(sidebar::Section::Tracks);
    app.tracks = some_tracks(&["One", "Two"]);
    assert!(
        matches!(
            key(&mut app, KeyCode::Char('A')),
            Some(Action::ToggleFavourite)
        ),
        "A toggles the favourite"
    );
}
