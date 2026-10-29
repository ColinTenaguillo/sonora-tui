use super::*;

#[test]
fn enter_opens_the_artist_and_album_rows_of_top_results() {
    // Reported: the profiles and albums drawn in Top Results could not
    // be entered. `selected_card` refused the whole tab, so enter had
    // nothing to open — even though Top draws the same cards the
    // Artists and Albums tabs do, and moving through them worked.
    let mut app = signed_in(sidebar::Section::Music);
    app.update(Action::BeginSearch);
    // The results are only taken when they answer the query that is
    // typed, so the query has to be there.
    for c in "daft punk".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    app.update(Action::SearchLoaded(Box::new(some_results())));
    app.search.as_mut().unwrap().typing = false;
    assert_eq!(
        searchview::Tab::from_index(app.search.as_ref().unwrap().tab),
        searchview::Tab::Top,
        "search opens on Top"
    );

    // The artist row is drawn first, so that is where the selection is.
    app.search.as_mut().unwrap().top = searchview::TopSection::Artists;
    assert!(
        matches!(app.selected_collection(), Some(Collection::Artist(_))),
        "the highlighted artist is what enter opens, got {:?}",
        app.selected_collection()
    );
    app.update(Action::ActivateSelection);
    assert!(app.open.is_some(), "and it opened");

    // The album row, the other half of the report.
    let mut app = signed_in(sidebar::Section::Music);
    app.update(Action::BeginSearch);
    for c in "daft punk".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    app.update(Action::SearchLoaded(Box::new(some_results())));
    app.search.as_mut().unwrap().typing = false;
    app.search.as_mut().unwrap().top = searchview::TopSection::Albums;
    assert!(
        matches!(app.selected_collection(), Some(Collection::Album(_))),
        "the highlighted album, got {:?}",
        app.selected_collection()
    );
    app.update(Action::ActivateSelection);
    assert!(app.open.is_some(), "and it opened too");
}

#[test]
fn a_track_in_top_results_still_plays_rather_than_opening() {
    // The track list is read by `selected_track`; a track has nothing
    // to open, and saying otherwise would have enter open a view for
    // something that should simply play.
    let mut app = signed_in(sidebar::Section::Music);
    app.update(Action::BeginSearch);
    for c in "daft punk".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    app.update(Action::SearchLoaded(Box::new(some_results())));
    app.search.as_mut().unwrap().typing = false;
    app.search.as_mut().unwrap().top = searchview::TopSection::Tracks;

    assert!(app.selected_collection().is_none(), "nothing to open");
    assert!(!app.on_grid(), "and the keys move it as a list");
    assert!(
        app.selected_track().is_some(),
        "the highlighted track is what enter plays"
    );
}

#[test]
fn the_search_box_opens_over_a_page_and_closes_back_onto_it() {
    // `s` on an album left the album drawn over the box, with the keys
    // still moving through its tracks.
    let mut app = signed_in(sidebar::Section::Albums);
    app.artist.page = Some(crate::library::ArtistPage {
        name: "Section.80".into(),
        ..Default::default()
    });
    app.last_main_width = 100;
    app.last_main_height = 40;

    app.update(Action::BeginSearch);
    assert!(
        matches!(app.showing(), Showing::Search(_)),
        "the box is on top"
    );
    let text = geometry::text(&geometry::draw(120, 40, |f, _a, _p| draw(f, &mut app)));
    assert!(text.contains("Search"), "the box is drawn:\n{text}");
    assert!(
        !text.contains("Section.80"),
        "and the page is not drawn over it:\n{text}"
    );

    app.update(Action::CloseSearch);
    assert_eq!(
        app.showing(),
        Showing::Artist,
        "closing the box gives the page back"
    );
}

#[test]
fn typing_in_the_search_box_is_text_not_commands() {
    // Same trap as the filter box: "q" has to be a letter while a query
    // is being typed, or the app quits mid-word.
    let mut app = signed_in(sidebar::Section::Music);
    assert!(matches!(
        key(&mut app, KeyCode::Char('s')),
        Some(Action::BeginSearch)
    ));
    app.update(Action::BeginSearch);

    for c in ['q', 'u', 'e'] {
        assert!(
            key(&mut app, KeyCode::Char(c)).is_none(),
            "{c} must not act"
        );
    }
    assert_eq!(app.search.as_ref().map(|s| s.query.as_str()), Some("que"));
    assert!(!app.should_quit, "typing q must not have quit");

    key(&mut app, KeyCode::Backspace);
    assert_eq!(app.search.as_ref().map(|s| s.query.as_str()), Some("qu"));
}

#[test]
fn enter_runs_the_query_and_hands_the_keys_back() {
    let mut app = signed_in(sidebar::Section::Music);
    app.update(Action::BeginSearch);
    for c in "daft".chars() {
        key(&mut app, KeyCode::Char(c));
    }

    match key(&mut app, KeyCode::Enter) {
        Some(Action::RunSearch(q)) => assert_eq!(q, "daft"),
        other => panic!("expected a search for \"daft\", got {other:?}"),
    }
    assert!(
        !app.search.as_ref().unwrap().typing,
        "the box gives the keyboard back, so j and k drive the results"
    );
}

#[test]
fn escape_leaves_search_rather_than_only_the_box() {
    // A box with no way back to the app would be a trap.
    let mut app = signed_in(sidebar::Section::Music);
    app.update(Action::BeginSearch);
    assert!(matches!(
        key(&mut app, KeyCode::Esc),
        Some(Action::CloseSearch)
    ));
    app.update(Action::CloseSearch);
    assert!(app.search.is_none());
    assert!(!app.should_quit, "and it did not quit the app");
}

#[test]
fn results_for_a_query_the_user_has_retyped_are_dropped() {
    // The request is slow enough that a reply can land after the query
    // has changed; it must not appear under text that did not produce it.
    let mut app = signed_in(sidebar::Section::Music);
    app.update(Action::BeginSearch);
    for c in "kendrick".chars() {
        key(&mut app, KeyCode::Char(c));
    }

    app.update(Action::SearchLoaded(Box::new(some_results())));
    assert!(
        app.search.as_ref().unwrap().results.is_empty(),
        "results for \"daft punk\" do not belong to \"kendrick\""
    );
}

#[test]
fn results_for_the_query_on_screen_are_shown() {
    let mut app = signed_in(sidebar::Section::Music);
    app.update(Action::BeginSearch);
    for c in "daft punk".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    app.update(Action::SearchLoaded(Box::new(some_results())));
    assert_eq!(app.search.as_ref().unwrap().results.tracks.len(), 6);
}

#[test]
fn j_and_k_reach_the_search_list_through_the_keys() {
    // The bug this is here for: the earlier tests called `update`
    // directly, so they never went through `on_key` — where "j" asked
    // `on_home()`, got true because the sidebar was still on Music, and
    // emitted RowNext. That moved the home carousels behind the search
    // view while the results sat still.
    let mut app = signed_in(sidebar::Section::Music);
    app.update(Action::BeginSearch);
    for c in "daft punk".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    key(&mut app, KeyCode::Enter);
    app.update(Action::SearchLoaded(Box::new(some_results())));
    app.last_main_height = 30;

    assert!(
        matches!(key(&mut app, KeyCode::Char('j')), Some(Action::TrackNext)),
        "j must drive the search list, not the home page behind it"
    );
    assert!(matches!(
        key(&mut app, KeyCode::Char('k')),
        Some(Action::TrackPrevious)
    ));

    // And the action actually moves the search selection, which now
    // lives in the track list's own state.
    app.update(Action::TrackNext);
    assert_eq!(app.search.as_ref().unwrap().tracks.selected, 1);
}

#[test]
fn search_is_neither_the_home_page_nor_a_grid() {
    // Both predicates read the sidebar's section, which does not move
    // when search opens.
    let mut app = signed_in(sidebar::Section::Music);
    app.update(Action::BeginSearch);
    assert!(!app.on_home(), "search is not the home page");

    let mut app = signed_in(sidebar::Section::Albums);
    app.update(Action::BeginSearch);
    assert!(!app.on_grid(), "search is not the album grid");
}

#[test]
fn moving_runs_down_the_chosen_tabs_list() {
    // Each tab is one of the app's own views, so the selection lives in
    // that view's state and follows its movement rules.
    let mut app = signed_in(sidebar::Section::Music);
    app.update(Action::BeginSearch);
    for c in "daft punk".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    app.update(Action::SearchLoaded(Box::new(some_results())));
    app.search.as_mut().unwrap().typing = false;
    app.last_main_height = 30;
    app.last_main_width = 100;

    app.update(Action::NextSearchTab); // Tracks
    assert_eq!(app.search.as_ref().unwrap().tab, 1);

    for _ in 0..3 {
        app.update(Action::TrackNext);
    }
    assert_eq!(app.search.as_ref().unwrap().tracks.selected, 3);

    app.update(Action::TrackPrevious);
    assert_eq!(app.search.as_ref().unwrap().tracks.selected, 2);
}

#[test]
fn the_selection_stops_at_both_ends_of_a_tab() {
    let mut app = signed_in(sidebar::Section::Music);
    app.update(Action::BeginSearch);
    for c in "daft punk".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    app.update(Action::SearchLoaded(Box::new(some_results())));
    app.search.as_mut().unwrap().typing = false;
    app.last_main_height = 30;
    app.last_main_width = 100;
    app.update(Action::NextSearchTab); // Tracks

    for _ in 0..20 {
        app.update(Action::TrackPrevious);
    }
    assert_eq!(
        app.search.as_ref().unwrap().tracks.selected,
        0,
        "stops at the top"
    );

    for _ in 0..40 {
        app.update(Action::TrackNext);
    }
    let len = searchview::track_rows(
        &app.search.as_ref().unwrap().results,
        searchview::Tab::Tracks,
    )
    .len();
    assert_eq!(
        app.search.as_ref().unwrap().tracks.selected,
        len - 1,
        "and at the last row rather than past it"
    );
}

#[test]
fn each_tab_keeps_its_own_place() {
    // Every tab is a different view with its own state, so coming back
    // to one finds it where it was left rather than at the top.
    let mut app = signed_in(sidebar::Section::Music);
    app.update(Action::BeginSearch);
    for c in "daft punk".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    app.update(Action::SearchLoaded(Box::new(some_results())));
    app.search.as_mut().unwrap().typing = false;
    app.last_main_height = 30;
    app.last_main_width = 100;

    app.update(Action::NextSearchTab); // Tracks
    for _ in 0..3 {
        app.update(Action::TrackNext);
    }
    assert_eq!(app.search.as_ref().unwrap().tracks.selected, 3);

    // Round every tab and back to Tracks.
    for _ in 0..searchview::Tab::ALL.len() {
        app.update(Action::NextSearchTab);
    }
    assert_eq!(app.search.as_ref().unwrap().tab, 1, "back on Tracks");
    assert_eq!(
        app.search.as_ref().unwrap().tracks.selected,
        3,
        "and where it was left"
    );
}

#[test]
fn enter_in_search_plays_the_result_not_a_library_track() {
    // Both lists are indexed by the same selection, so reading the
    // library in search played whatever track happened to sit at that
    // index in a list nothing was showing.
    let mut app = signed_in(sidebar::Section::Music);
    app.tracks = some_tracks(&["A library track", "Another", "A third"]);
    app.update(Action::BeginSearch);
    for c in "daft punk".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    app.update(Action::SearchLoaded(Box::new(some_results())));
    app.search.as_mut().unwrap().typing = false;
    app.update(Action::NextSearchTab); // Tracks
    app.update(Action::TrackNext);

    let picked = app.selected_track().expect("a track");
    assert_eq!(
        picked.title, "Track 1",
        "the highlighted search result, not a library track"
    );
}

#[test]
fn enter_outside_search_still_plays_the_filtered_library_row() {
    // On the Tracks section, where the library is what is drawn. It
    // was asserted on the home page, which is a page of cards: reading
    // the library there was the bug that made `A` and `c` act on a
    // favourite nobody could see.
    let mut app = signed_in(sidebar::Section::Tracks);
    app.tracks = some_tracks(&["A library track", "Another", "A third"]);
    app.update(Action::TrackNext);
    assert_eq!(
        app.selected_track().expect("a track").title,
        "Another",
        "the library list is untouched by the search path"
    );
}

#[test]
fn the_search_selection_stays_inside_the_rows_that_are_drawn() {
    // The view draws into the pane minus its own header, but scrolling
    // was measured against the whole pane — so the selection ran below
    // the last drawn row, which is where the now-playing bar is.
    let mut app = signed_in(sidebar::Section::Music);
    app.update(Action::BeginSearch);
    for c in "daft punk".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    let mut results = some_results();
    results.tracks = some_tracks(&["T0", "T1", "T2", "T3", "T4", "T5", "T6", "T7", "T8", "T9"]);
    app.update(Action::SearchLoaded(Box::new(results)));
    app.search.as_mut().unwrap().typing = false;
    app.last_main_width = 100;
    app.last_main_height = 24;
    app.update(Action::NextSearchTab); // Tracks

    // Walk to the bottom of the list.
    for _ in 0..20 {
        app.update(Action::TrackNext);
    }

    let state = app.search.as_ref().unwrap();
    let body = app.last_main_height - searchview::HEADER_ROWS;
    let drawn = tracklist::visible_rows_of(body, None, tracklist::Chrome::Bare);
    assert!(drawn > 0, "the pane draws some rows");
    assert!(
        state.tracks.selected < state.tracks.offset + drawn,
        "selected row {} is past the last drawn row ({} + {}), so it sits \
         under the now-playing bar",
        state.tracks.selected,
        state.tracks.offset,
        drawn
    );
}

#[test]
fn moving_down_top_results_walks_out_of_one_section_into_the_next() {
    // Every section was drawn but only the tracks took the keys, so the
    // cursor could never reach the artists or albums above them.
    use searchview::TopSection;
    let mut app = signed_in(sidebar::Section::Music);
    app.update(Action::BeginSearch);
    for c in "daft punk".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    app.update(Action::SearchLoaded(Box::new(some_results())));
    app.search.as_mut().unwrap().typing = false;
    app.last_main_width = 100;
    app.last_main_height = 40;
    // Top results is tab 0, where the selection starts.
    assert_eq!(app.search.as_ref().unwrap().tab, 0);

    // Starts in the artists, the first section drawn.
    app.search.as_mut().unwrap().top = TopSection::Artists;
    app.update(Action::TrackNext);
    assert_eq!(
        app.search.as_ref().unwrap().top,
        TopSection::Albums,
        "down out of the artists lands in the albums"
    );
    app.update(Action::TrackNext);
    assert_eq!(
        app.search.as_ref().unwrap().top,
        TopSection::Tracks,
        "and again into the tracks"
    );

    // Back up through them.
    app.update(Action::TrackPrevious);
    assert_eq!(
        app.search.as_ref().unwrap().top,
        TopSection::Albums,
        "up off the first track returns to the albums"
    );
    app.update(Action::TrackPrevious);
    assert_eq!(app.search.as_ref().unwrap().top, TopSection::Artists);
}

#[test]
fn moving_down_the_top_results_tracks_keeps_the_selection_on_screen() {
    // Reported: the selection ran below the pane instead of the list
    // scrolling to keep it in view. The list was scrolled against the
    // whole body, but it starts under two rows of covers.
    use searchview::TopSection;
    let mut app = signed_in(sidebar::Section::Music);
    app.update(Action::BeginSearch);
    for c in "daft punk".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    app.update(Action::SearchLoaded(Box::new(some_results())));
    app.search.as_mut().unwrap().typing = false;
    app.last_main_width = 100;
    app.last_main_height = 30;
    app.search.as_mut().unwrap().top = TopSection::Albums;

    // Into the tracks: the page scrolls so they are on screen.
    app.update(Action::TrackNext);
    let state = app.search.as_ref().unwrap();
    assert_eq!(state.top, TopSection::Tracks);
    assert_eq!(state.top_scroll, 1, "the artists scroll off to make room");

    let body = app.search_body_height();
    let visible = tracklist::visible_rows_of(
        searchview::tracks_height(&state.results, state.top_scroll, body),
        None,
        tracklist::Chrome::Bare,
    );
    assert!(
        visible < some_results().tracks.len(),
        "the list has to scroll"
    );
    for _ in 0..some_results().tracks.len() {
        app.update(Action::TrackNext);
        let t = &app.search.as_ref().unwrap().tracks;
        assert!(
            t.selected >= t.offset && t.selected < t.offset + visible,
            "track {} is drawn: offset {} with {visible} visible",
            t.selected,
            t.offset
        );
    }
}

#[test]
fn see_all_on_a_top_results_row_opens_that_kinds_tab() {
    // The row's heading offers it when the cards run past the edge, and
    // the whole of what was found is on the kind's own tab.
    use searchview::TopSection;
    let mut app = signed_in(sidebar::Section::Music);
    app.update(Action::BeginSearch);
    for c in "daft punk".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    app.update(Action::SearchLoaded(Box::new(some_results())));
    app.search.as_mut().unwrap().typing = false;
    app.search.as_mut().unwrap().top = TopSection::Albums;
    app.update(Action::SeeAll);
    assert_eq!(
        searchview::Tab::from_index(app.search.as_ref().unwrap().tab),
        searchview::Tab::Albums
    );
}

#[test]
fn h_and_l_run_along_a_top_results_card_row() {
    use searchview::TopSection;
    let mut app = signed_in(sidebar::Section::Music);
    app.update(Action::BeginSearch);
    for c in "daft punk".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    app.update(Action::SearchLoaded(Box::new(some_results())));
    app.search.as_mut().unwrap().typing = false;
    app.last_main_width = 100;
    app.last_main_height = 40;
    app.search.as_mut().unwrap().top = TopSection::Artists;

    app.update(Action::SearchRight);
    assert_eq!(
        app.search.as_ref().unwrap().top_artists.selected,
        1,
        "l moves along the artist row"
    );
    app.update(Action::SearchLeft);
    assert_eq!(app.search.as_ref().unwrap().top_artists.selected, 0);
}

#[test]
fn a_top_section_with_no_results_is_skipped_rather_than_entered() {
    // Moving into a section that is not drawn would park the cursor
    // somewhere invisible.
    use searchview::TopSection;
    let mut app = signed_in(sidebar::Section::Music);
    app.update(Action::BeginSearch);
    for c in "daft punk".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    let mut results = some_results();
    results.albums.clear();
    app.update(Action::SearchLoaded(Box::new(results)));
    app.search.as_mut().unwrap().typing = false;
    app.last_main_width = 100;
    app.last_main_height = 40;
    app.search.as_mut().unwrap().top = TopSection::Artists;

    app.update(Action::TrackNext);
    assert_eq!(
        app.search.as_ref().unwrap().top,
        TopSection::Tracks,
        "with no albums, down from the artists reaches the tracks"
    );
}

#[test]
fn the_transport_keys_are_letters_while_a_query_is_being_typed() {
    // n, p, z and r are all letters someone will type into the search
    // box; bound unconditionally they would skip tracks mid-word.
    let mut app = signed_in(sidebar::Section::Music);
    app.update(Action::BeginSearch);
    for c in ['n', 'p', 'z', 'r'] {
        let action = key(&mut app, KeyCode::Char(c));
        assert!(
            !matches!(
                action,
                Some(Action::QueueNext)
                    | Some(Action::QueuePrevious)
                    | Some(Action::ToggleShuffle)
                    | Some(Action::CycleRepeat)
            ),
            "{c:?} types rather than driving the transport"
        );
    }
    assert_eq!(
        app.search.as_ref().unwrap().query,
        "npzr",
        "they all reached the box"
    );
}

#[test]
fn s_puts_the_keyboard_back_in_the_search_box() {
    // With search already open it did nothing, so the only way back to
    // the query was to close search and start over.
    let mut app = signed_in(sidebar::Section::Music);
    app.update(Action::BeginSearch);
    for c in "daft".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    // Enter runs the search and hands the keys to the results.
    key(&mut app, KeyCode::Enter);
    app.search.as_mut().unwrap().typing = false;

    assert!(
        matches!(key(&mut app, KeyCode::Char('s')), Some(Action::BeginSearch)),
        "s reaches the box from the results"
    );
    app.update(Action::BeginSearch);
    assert!(
        app.search.as_ref().unwrap().typing,
        "and it has the keyboard"
    );
    assert_eq!(
        app.search.as_ref().unwrap().query,
        "daft",
        "with the query still there to refine"
    );
}

#[test]
fn enter_opens_an_album_from_the_search_results() {
    // Search draws the app's own card grid, but the selection path did
    // not know it: `on_grid` was guarded on search being closed, so
    // enter fell through and opened nothing.
    let mut app = signed_in(sidebar::Section::Music);
    app.update(Action::BeginSearch);
    for c in "daft punk".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    app.update(Action::SearchLoaded(Box::new(some_results())));
    app.search.as_mut().unwrap().typing = false;
    app.last_main_width = 100;
    app.last_main_height = 30;
    for _ in 0..2 {
        app.update(Action::NextSearchTab); // Albums
    }

    assert!(app.on_grid(), "a card tab of search is a grid");
    let card = app.selected_card().expect("the highlighted card");
    assert!(
        card.title.starts_with("Album"),
        "and it is the one search is showing, found {:?}",
        card.title
    );
    assert!(
        matches!(app.selected_collection(), Some(Collection::Album(_))),
        "which enter opens"
    );

    // And it must actually open: the checks above stopped at the
    // selection, so enter opening nothing went unnoticed.
    app.update(Action::ActivateSelection);
    assert!(app.open.is_some(), "the album opened");
    assert!(
        app.search.is_none(),
        "and search stepped aside, or it draws over the view it opened"
    );

    // Escape comes back to the results, not to the section behind them:
    // the search was what the user was looking at.
    app.update(Action::CloseCollection);
    assert!(app.open.is_none(), "the album closed");
    let search = app.search.as_ref().expect("back in the results");
    assert_eq!(search.query, "daft punk", "with the query still typed");
    assert_eq!(
        searchview::Tab::from_index(search.tab),
        searchview::Tab::Albums,
        "and on the tab it was opened from"
    );
}

#[test]
fn enter_opens_a_playlist_and_an_artist_from_search_too() {
    // Albums were wired and checked; the other two card tabs go through
    // the same path, so they are held here rather than assumed.
    let mut app = signed_in(sidebar::Section::Music);
    app.update(Action::BeginSearch);
    for c in "daft punk".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    app.update(Action::SearchLoaded(Box::new(some_results())));
    app.search.as_mut().unwrap().typing = false;
    app.last_main_width = 100;
    app.last_main_height = 30;

    // Artists is tab 3, Playlists tab 4.
    for _ in 0..3 {
        app.update(Action::NextSearchTab);
    }
    assert!(
        matches!(app.selected_collection(), Some(Collection::Artist(_))),
        "an artist card opens their page"
    );

    app.update(Action::NextSearchTab);
    assert!(
        matches!(app.selected_collection(), Some(Collection::Playlist(_))),
        "and a playlist card opens the playlist"
    );
}

#[test]
fn enter_on_a_track_tab_of_search_is_not_a_card() {
    // Tracks and Top draw a list, so there is no card to open — enter
    // plays the row instead.
    let mut app = signed_in(sidebar::Section::Music);
    app.update(Action::BeginSearch);
    for c in "daft punk".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    app.update(Action::SearchLoaded(Box::new(some_results())));
    app.search.as_mut().unwrap().typing = false;
    app.update(Action::NextSearchTab); // Tracks

    assert!(!app.on_grid(), "a track tab is not a grid");
    assert!(app.selected_card().is_none(), "and has no card");
}

#[test]
fn a_is_a_letter_while_a_query_is_being_typed() {
    // Same trap as q: a binding that works everywhere would put an "a"
    // out of reach of the search box.
    let mut app = signed_in(sidebar::Section::Music);
    app.update(Action::BeginSearch);
    assert!(
        !matches!(
            key(&mut app, KeyCode::Char('A')),
            Some(Action::ToggleFavourite)
        ),
        "A types rather than toggling while the box has the keyboard"
    );
}

#[test]
fn h_and_l_move_within_a_search_grid() {
    // They used to fall through to CarouselNext, which moved a row of
    // the home page that search was not drawing — so the keys did
    // nothing visible at all.
    let mut app = signed_in(sidebar::Section::Music);
    app.update(Action::BeginSearch);
    for c in "daft punk".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    app.update(Action::SearchLoaded(Box::new(some_results())));
    app.search.as_mut().unwrap().typing = false;
    app.last_main_height = 30;
    app.last_main_width = 100;
    for _ in 0..2 {
        app.update(Action::NextSearchTab); // Albums
    }

    // Through on_key, so the routing is what is under test and not just
    // the handler behind it.
    assert!(
        matches!(key(&mut app, KeyCode::Char('l')), Some(Action::SearchRight)),
        "l moves right in search, not along a home carousel"
    );
    app.update(Action::SearchRight);
    assert_eq!(
        app.search.as_ref().unwrap().albums.selected,
        1,
        "and lands on the next card"
    );

    assert!(matches!(
        key(&mut app, KeyCode::Char('h')),
        Some(Action::SearchLeft)
    ));
    app.update(Action::SearchLeft);
    assert_eq!(app.search.as_ref().unwrap().albums.selected, 0, "and back");
}

#[test]
fn an_album_opened_from_search_can_still_be_left() {
    // A result opened by mistake must not be a dead end. `h` is a
    // direction rather than a way out, so escape is what does it --
    // and it leaves the album rather than the search underneath, which
    // would take away both views at once.
    let mut app = signed_in(sidebar::Section::Music);
    app.update(Action::BeginSearch);
    for c in "daft punk".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    app.update(Action::SearchLoaded(Box::new(some_results())));
    app.search.as_mut().unwrap().typing = false;
    app.last_main_width = 100;
    app.last_main_height = 30;
    for _ in 0..2 {
        app.update(Action::NextSearchTab); // Albums
    }
    app.update(Action::ActivateSelection);
    assert!(app.open.is_some(), "the album is open");
    assert!(
        app.search.is_none(),
        "with the results on the stack under it"
    );
    assert!(matches!(key(&mut app, KeyCode::Esc), Some(Action::GoBack)));
    app.update(Action::GoBack);
    // And the search is still there, gone back to.
    assert!(app.search.is_some(), "the results were not closed as well");
}

#[test]
fn the_three_grid_tabs_each_hold_their_own_selection() {
    // Albums, Artists and Playlists are three separate grids. Pointing
    // them at one state looks right until you move in two of them.
    let mut app = signed_in(sidebar::Section::Music);
    app.update(Action::BeginSearch);
    for c in "daft punk".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    app.update(Action::SearchLoaded(Box::new(some_results())));
    app.search.as_mut().unwrap().typing = false;
    app.last_main_height = 30;
    app.last_main_width = 100;

    // Albums (tab 2): move down one row.
    for _ in 0..2 {
        app.update(Action::NextSearchTab);
    }
    assert_eq!(app.search.as_ref().unwrap().tab, 2, "on Albums");
    app.update(Action::TrackNext);
    let albums_at = app.search.as_ref().unwrap().albums.selected;
    assert!(albums_at > 0, "the album grid moved");

    // Artists (tab 3) starts at the top regardless of what Albums did.
    app.update(Action::NextSearchTab);
    assert_eq!(app.search.as_ref().unwrap().tab, 3, "on Artists");
    assert_eq!(
        app.search.as_ref().unwrap().artists.selected,
        0,
        "the artist grid has its own selection, untouched by Albums"
    );

    // Moving here must not disturb the album grid.
    app.update(Action::TrackNext);
    assert!(app.search.as_ref().unwrap().artists.selected > 0);
    assert_eq!(
        app.search.as_ref().unwrap().albums.selected,
        albums_at,
        "and Albums stayed where it was"
    );

    // Playlists (tab 4) is a third, independent grid.
    app.update(Action::NextSearchTab);
    assert_eq!(app.search.as_ref().unwrap().tab, 4, "on Playlists");
    assert_eq!(
        app.search.as_ref().unwrap().playlists.selected,
        0,
        "untouched by either of the other two"
    );
}

#[test]
fn new_results_put_every_tab_back_to_the_top() {
    // A different query is a different list; a position kept from the
    // last one would point at something unrelated.
    let mut app = signed_in(sidebar::Section::Music);
    app.update(Action::BeginSearch);
    for c in "daft punk".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    app.update(Action::SearchLoaded(Box::new(some_results())));
    app.search.as_mut().unwrap().typing = false;
    app.last_main_height = 30;
    app.last_main_width = 100;
    app.update(Action::NextSearchTab);
    for _ in 0..3 {
        app.update(Action::TrackNext);
    }
    assert!(app.search.as_ref().unwrap().tracks.selected > 0);

    // The same query again, with fresh results.
    app.update(Action::SearchLoaded(Box::new(some_results())));
    assert_eq!(app.search.as_ref().unwrap().tracks.selected, 0);
    assert_eq!(app.search.as_ref().unwrap().tab, 0);
}

#[test]
fn t_is_a_letter_while_a_query_is_being_typed() {
    // Searching for "the strokes" must not change tab three times.
    let mut app = signed_in(sidebar::Section::Music);
    app.update(Action::BeginSearch);
    for c in "the".chars() {
        assert!(key(&mut app, KeyCode::Char(c)).is_none(), "{c} is text");
    }
    assert_eq!(app.search.as_ref().map(|s| s.query.as_str()), Some("the"));
    assert_eq!(app.search.as_ref().unwrap().tab, 0, "and no tab changed");
}

#[test]
fn tab_cycles_round_the_result_tabs() {
    let mut app = signed_in(sidebar::Section::Music);
    app.update(Action::BeginSearch);
    app.search.as_mut().unwrap().typing = false;

    for expected in 1..searchview::Tab::ALL.len() {
        assert!(matches!(
            key(&mut app, KeyCode::Tab),
            Some(Action::NextSearchTab)
        ));
        app.update(Action::NextSearchTab);
        assert_eq!(app.search.as_ref().unwrap().tab, expected);
    }
    app.update(Action::NextSearchTab);
    assert_eq!(app.search.as_ref().unwrap().tab, 0, "round to the first");
}
