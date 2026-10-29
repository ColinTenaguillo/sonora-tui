use super::*;

#[test]
fn typing_in_the_filter_box_is_text_not_commands() {
    // "q" must filter, not quit. This is the whole reason the filter has
    // a mode of its own.
    let mut app = signed_in(sidebar::Section::Tracks);
    assert!(app
        .on_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE))
        .is_none());
    assert!(app.filtering);

    for c in ['q', 'u', 'e'] {
        let action = app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        assert!(
            action.is_none(),
            "{c} must not fire a command while filtering"
        );
    }
    assert_eq!(app.filter_text(), "que");
    assert!(!app.should_quit, "typing q must not have quit");

    app.on_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
    assert_eq!(app.filter_text(), "qu");

    // Enter accepts what was typed; Esc would have cleared it.
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(!app.filtering);
    assert_eq!(app.filter_text(), "qu");
}

#[test]
fn filtering_the_track_list_keeps_the_selection_in_range() {
    use std::time::Duration;
    let mut app = signed_in(sidebar::Section::Tracks);
    app.tracks = (0..20)
        .map(|i| {
            crate::domain::Track::sample(
                &format!("Song {i}"),
                "An Artist",
                Duration::from_secs(100),
            )
        })
        .collect();
    app.tracklist.selected = 19;

    // "Song 1" matches 1 and 10..19: eleven tracks, so index 19 is gone.
    app.set_filter("Song 1".into());
    let visible = tracklist::filter(&app.tracks, &app.tracklist.filter).len();
    assert!(
        app.tracklist.selected < visible,
        "selection {} must stay inside {visible} filtered tracks",
        app.tracklist.selected
    );
}

#[test]
fn no_key_moves_the_selection_off_the_main_pane() {
    // Every key, on a page with something to move around in: none of
    // them may take the selection off the main pane. Tab in particular
    // used to, onto a nav where no key did anything — a mode with
    // nothing behind it, and one the user could not move back out of.
    // The nav is driven by J and K from wherever the user is; nothing
    // hands the selection over to it.
    let mut app = signed_in(sidebar::Section::Albums);
    app.albums = (0..9)
        .map(|i| crate::library::Album {
            id: (i).to_string(),
            title: format!("A{i}"),
            artist: "An Artist".into(),
            year: None,
            cover: None,
            track_count: 1,
            duration: None,
        })
        .collect();
    // The movement keys and Tab. J and K are left out on purpose: they
    // change section, and the section they land on has nothing loaded
    // in a test — that is the nav working, not the selection escaping.
    for code in [
        KeyCode::Tab,
        KeyCode::Char('j'),
        KeyCode::Char('k'),
        KeyCode::Char('h'),
        KeyCode::Char('l'),
    ] {
        if let Some(action) = key(&mut app, code) {
            app.update(action);
        }
        // Whatever the pane is showing, one of its cards is selected:
        // a selection that had moved to the nav would leave none.
        assert!(
            app.selected_card().is_some(),
            "after {code:?} the selection is still on a card in the pane"
        );
    }
}

#[test]
fn j_and_k_always_drive_the_content() {
    // j and k move through the list in front of you, always. Making
    // them depend on a focused pane meant remembering an invisible mode
    // before you could move at all — there is no such mode now.
    let mut app = signed_in(sidebar::Section::Tracks);
    assert!(matches!(
        key(&mut app, KeyCode::Char('j')),
        Some(Action::TrackNext)
    ));
    assert!(matches!(
        key(&mut app, KeyCode::Char('k')),
        Some(Action::TrackPrevious)
    ));
}

#[test]
fn h_and_l_stay_inside_the_content() {
    // They move along a carousel row; they no longer cross into the
    // sidebar, which has keys of its own.
    let mut app = signed_in(sidebar::Section::Playlists);
    app.playlists = (0..30)
        .map(|i| crate::library::Playlist::sample(&format!("P{i}"), i))
        .collect();
    assert!(matches!(
        key(&mut app, KeyCode::Char('l')),
        Some(Action::CarouselNext)
    ));
    assert!(matches!(
        key(&mut app, KeyCode::Char('h')),
        Some(Action::CarouselPrevious)
    ));
}

#[test]
fn shift_jk_still_reach_the_sidebar_from_the_main_pane() {
    // Kept alongside the focus-aware keys: it is a shortcut people learn.
    let mut app = signed_in(sidebar::Section::Tracks);
    assert!(matches!(
        key(&mut app, KeyCode::Char('J')),
        Some(Action::SidebarNext)
    ));
    assert!(matches!(
        key(&mut app, KeyCode::Char('K')),
        Some(Action::SidebarPrevious)
    ));
}

#[test]
fn closing_the_help_repaints_what_it_covered() {
    // Reported: covers kept a patch of the help modal on them after it
    // closed. The image protocols paint straight to the terminal and
    // mark their cells `Skip` so ratatui leaves them alone — but the
    // modal wrote over those cells, and on the next frame they went
    // back to `Skip`, so nothing was written to undo it.
    //
    // What has to happen is a full repaint on the frame the modal
    // leaves. This checks the flag that asks for one.
    let mut app = signed_in(sidebar::Section::Music);
    app.showing_help = true;
    assert!(!app.needs_repaint, "nothing to undo yet");

    key(&mut app, KeyCode::Esc);
    assert!(!app.showing_help, "the help closed");
    assert!(
        app.needs_repaint,
        "and asked for the frame under it to be drawn again"
    );

    // Taken by the draw, so it does not repaint on every frame after.
    assert!(app.take_repaint());
    assert!(!app.take_repaint(), "once, not for ever");
}

#[test]
fn the_keys_and_the_pane_agree_about_what_is_on_screen() {
    // The bug this closes: a genre page set `open`, the renderer read
    // that as an opened collection and drew an empty track list, and
    // the keys went on driving a page of rows. Both now ask the same
    // question, so a state that draws rows is a state the keys treat as
    // rows.
    let mut app = signed_in(sidebar::Section::Explore);
    app.explore.heading = Some("Hip-Hop".into());
    app.explore.rows.push(home::Row {
        heading: "Playlists".into(),
        kind: crate::browse::RowKind::Carousel,
        cards: vec![carousel::Card::new("A Playlist", "TIDAL")],
        state: carousel::CarouselState::default(),
    });

    assert_eq!(
        app.showing(),
        Showing::Rows,
        "a genre page is a page of rows"
    );
    assert!(app.on_home(), "so the keys drive it as one");
    assert!(!app.on_grid(), "and not as a grid of covers");

    // An opened collection is the other case, and it must not be
    // mistaken for a page of rows. Opening it waits first: what comes
    // back is what decides the shape.
    let mut app = with_albums(3);
    app.update(Action::ActivateSelection);
    assert_eq!(
        app.showing(),
        Showing::Loading,
        "nothing has arrived yet, so neither shape is drawn"
    );
    assert!(!app.on_home(), "and the keys drive neither");

    app.update(Action::TracksLoaded(Vec::new()));
    assert_eq!(app.showing(), Showing::Tracks, "the reply makes it a list");
    assert!(!app.on_home(), "which the keys do not drive as rows");

    // And a section with nothing over it is itself.
    let app = signed_in(sidebar::Section::Albums);
    assert_eq!(app.showing(), Showing::Section(sidebar::Section::Albums));
    assert!(app.on_grid(), "Albums is a grid of covers");
}

#[test]
fn the_keys_act_on_what_is_drawn_not_on_the_hidden_track_list() {
    // Reported: `c` and `a` did nothing, or opened the wrong thing, on
    // every page. `selected_track` read the Tracks section's favourites
    // whatever was on screen, so on the home page `A` favourited and
    // `c` opened whichever favourite that hidden list had highlighted.
    let mut app = signed_in(sidebar::Section::Music);
    app.last_main_width = 120;
    app.tracks = vec![crate::domain::Track {
        id: crate::domain::TrackId::from(1),
        ..crate::domain::Track::sample("Hidden", "Favourite", std::time::Duration::from_secs(1))
    }];
    let mut card = carousel::Card::new("Song", "Someone");
    card.target = Some(carousel::Target::Track("42".into()));
    app.home.rows = vec![home::Row {
        heading: "Songs".into(),
        kind: crate::browse::RowKind::Compact,
        cards: vec![card],
        state: Default::default(),
    }];
    app.home.row = 0;
    assert_eq!(
        app.selected_track().map(|t| t.id.clone()),
        Some(crate::domain::TrackId::from(42)),
        "the card under the cursor, not the hidden favourite"
    );

    // A card that is not a track is nothing to act on -- not the
    // favourite either.
    app.home.rows[0].cards[0].target = Some(carousel::Target::Album("9".into()));
    assert!(app.selected_track().is_none());
    assert!(
        key(&mut app, KeyCode::Char('c')).is_none(),
        "nothing to open"
    );
}

#[test]
fn ctrl_d_and_ctrl_u_move_half_a_screen_at_a_time() {
    use crossterm::event::KeyModifiers;
    let mut app = signed_in(sidebar::Section::Tracks);
    app.tracks = (0..80)
        .map(|i| crate::domain::Track::sample(&format!("T{i}"), "A", Duration::from_secs(1)))
        .collect();
    app.last_main_height = 40;
    let press = |app: &mut App, c: char| {
        let mut next = app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL));
        while let Some(action) = next {
            next = app.update(action);
        }
    };
    press(&mut app, 'd');
    let half = app.tracklist.selected;
    assert!(half > 1, "further than one row, got {half}");
    assert_eq!(half, app.half_page(), "half of what the list draws");
    press(&mut app, 'd');
    assert_eq!(app.tracklist.selected, 2 * half);
    press(&mut app, 'u');
    assert_eq!(app.tracklist.selected, half, "and back up by the same");

    // In the queue view, where the list keeps its own cursor.
    app.tracklist.selected = 0;
    app.queue = crate::playback::Queue::new(app.tracks.clone(), 0);
    key(&mut app, KeyCode::Char('Q'));
    assert!(app.list == Some(List::Queue));
    press(&mut app, 'd');
    assert!(
        app.queue_list.selected > 1,
        "the queue moved too: {}",
        app.queue_list.selected
    );
}

#[test]
fn an_account_prompt_is_walked_with_j_and_k_and_picked_with_enter() {
    // YouTube cookies that carry several accounts: the service asks
    // which, and the answer is the id of the one chosen.
    let choice = |id: &str, name: &str| music::AccountChoice {
        id: id.into(),
        name: name.into(),
        detail: None,
    };
    let mut app = App::default();
    app.config.provider.kind = crate::config::ProviderKind::Youtube;
    app.update(Action::LoginPrompt(music::SignInPrompt::Accounts(vec![
        choice("1", "Me"),
        choice("2", "Label"),
    ])));
    assert!(key(&mut app, KeyCode::Char('j')).is_none());
    assert!(
        key(&mut app, KeyCode::Char('j')).is_none(),
        "stops at the last"
    );
    assert!(key(&mut app, KeyCode::Char('k')).is_none());
    assert!(key(&mut app, KeyCode::Char('j')).is_none());
    match key(&mut app, KeyCode::Enter) {
        Some(Action::Answer(id)) => assert_eq!(id, "2"),
        other => panic!("Enter picks the account, got {other:?}"),
    }
}

#[test]
fn o_sorts_the_track_list_by_a_column_and_i_reverses_it_over_the_filter() {
    let mut app = signed_in(sidebar::Section::Tracks);
    app.tracks = ["Beta", "Alpha", "Gamma", "Other"]
        .iter()
        .map(|t| crate::domain::Track::sample(t, "Zed", std::time::Duration::from_secs(1)))
        .collect();
    let first = |app: &App| app.selected_track().expect("a track").title;
    assert_eq!(first(&app), "Beta", "until O, the list is in its own order");

    key(&mut app, KeyCode::Char('O'));
    assert_eq!(first(&app), "Alpha", "O sorts by title");
    key(&mut app, KeyCode::Char('I'));
    assert_eq!(first(&app), "Other", "I reverses it");

    // The filter narrows the list; the sort orders what is left.
    app.set_filter("a".into());
    assert_eq!(app.list_len(), 3, "Other has no a");
    assert_eq!(first(&app), "Gamma");

    // Every key that moves through the list moves through the sorted one.
    app.tracklist.selected = 2;
    assert_eq!(first(&app), "Alpha");
}

#[test]
fn o_sorts_an_opened_album_too() {
    let mut app = with_albums(3);
    app.update(Action::ActivateSelection);
    let request = app.open.as_ref().expect("an album is opening").request;
    app.update(Action::CollectionLoaded {
        request,
        tracks: some_tracks(&["Beta", "Alpha", "Gamma"]),
    });
    assert_eq!(app.showing(), Showing::Tracks);
    let first = |app: &App| app.selected_track().expect("a track").title;
    assert_eq!(
        first(&app),
        "Beta",
        "an album keeps its track order until O"
    );
    key(&mut app, KeyCode::Char('O'));
    assert_eq!(first(&app), "Alpha", "O sorts the album by title");
}
