use super::*;

#[test]
fn an_artist_card_opens_that_artists_page() {
    // It was the one kind of card the app drew with nowhere to go.
    let mut app = signed_in(sidebar::Section::Profiles);
    app.artists = vec![crate::library::Artist {
        id: (1).to_string(),
        name: "2Pac".into(),
        picture: None,
    }];
    assert!(matches!(
        app.selected_collection(),
        Some(Collection::Artist(id)) if id == "1"
    ));
}

#[test]
fn opening_an_artist_waits_rather_than_drawing_a_stand_in_photo() {
    // The flash in the report: opening an artist drew a moment of the
    // collection view -- their photo in a banner over an empty track
    // list -- before their page arrived. There is nothing to show yet,
    // so the pane says so instead of standing something in.
    let mut app = signed_in(sidebar::Section::Profiles);
    app.artists = vec![crate::library::Artist {
        id: (1).to_string(),
        name: "13 Block".into(),
        picture: Some("https://example.invalid/a.jpg".into()),
    }];
    app.update(Action::ActivateSelection);

    assert_eq!(
        app.showing(),
        Showing::Loading,
        "the page has not arrived, so neither shape is drawn"
    );

    // And when it does, the artist page takes the pane.
    app.update(Action::ArtistLoaded {
        request: open_request(&app),
        page: Box::new(crate::library::ArtistPage {
            name: "13 Block".into(),
            ..Default::default()
        }),
    });
    assert_eq!(app.showing(), Showing::Artist, "the page took the pane");
}

#[test]
fn opening_an_artist_never_asks_for_a_square_photo() {
    // Reported as a flash from round to square and back. Opening an
    // artist shows a moment of the collection view before their page
    // arrives, and that view drew every banner cover square -- so the
    // avatar that was round in the grid turned square, then round again
    // when the page landed. The shape follows the card that was opened.
    let mut app = signed_in(sidebar::Section::Profiles);
    app.artists = vec![crate::library::Artist {
        id: (1).to_string(),
        name: "13 Block".into(),
        picture: Some("https://example.invalid/a.jpg".into()),
    }];
    app.update(Action::ActivateSelection);

    let open = app.open.as_ref().expect("the artist opened");
    assert!(
        open.round_cover,
        "the card was a round avatar, so what it opened into is one too"
    );

    // The shape the banner asks for, which is what the terminal draws.
    let asked = std::cell::RefCell::new(Vec::new());
    let banner = tracklist::Banner {
        title: &open.title,
        subtitle: &open.subtitle,
        detail: &open.detail,
        cover: open.cover.as_deref(),
        round: open.round_cover,
        liked: false,
    };
    let state = tracklist::TrackListState::default();
    let favourites = std::collections::HashSet::new();
    geometry::draw(120, 30, |f, area, palette| {
        tracklist::render(
            f,
            area,
            palette,
            tracklist::TrackList {
                tracks: &[],
                state: &state,
                focused: true,
                playing: None,
                tier: nowplaying::Tier::Low,
                banner: Some(banner),
                chrome: tracklist::Chrome::Full,
                filtering: false,
                favourites: &favourites,
            },
            |_f, _a, url, shape| {
                asked.borrow_mut().push((url.to_string(), shape));
                false
            },
        )
    });

    let shapes = asked.borrow();
    assert!(
        !shapes.is_empty(),
        "the banner asked for no cover at all, so nothing was checked"
    );
    assert!(
        shapes.iter().all(|(_, s)| *s == artwork::Shape::Round),
        "an artist's photo is round wherever it is drawn, got {shapes:?}"
    );
}

#[test]
fn j_and_k_scroll_an_open_bio_that_runs_past_the_pane() {
    // A blurb TIDAL writes can run to a page and a half, and the pane
    // drew the first screen of it with no way to the rest.
    let mut app = with_artist_open();
    // j and k come back as actions, as from the event loop.
    let press = |app: &mut App, code: KeyCode| {
        if let Some(action) = key(app, code) {
            app.update(action);
        }
    };
    {
        let page = app.artist.page.as_mut().unwrap();
        page.picture = Some("pic".into());
        page.bio = Some(
            (0..600)
                .map(|i| format!("w{i}"))
                .collect::<Vec<_>>()
                .join(" "),
        );
    }
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(130, 40)).unwrap();
    let text = |t: &ratatui::Terminal<ratatui::backend::TestBackend>| {
        t.backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect::<String>()
    };
    key(&mut app, KeyCode::Char('b'));
    assert!(app.artist.bio_open);
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    assert!(
        text(&terminal).contains("w0 "),
        "the bio starts at its first line"
    );
    let most = artistview::bio_overflow(
        app.artist.page.as_ref().unwrap(),
        app.last_main_width,
        app.last_main_height,
    );
    assert!(most > 0, "the bio runs past the pane");

    press(&mut app, KeyCode::Char('j'));
    assert_eq!(app.artist.bio_scroll, 1);
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    assert!(
        !text(&terminal).contains("w0 "),
        "the first line scrolled off"
    );
    assert!(app.artist.page.is_some(), "the page is still up");

    for _ in 0..1000 {
        press(&mut app, KeyCode::Char('j'));
    }
    assert_eq!(app.artist.bio_scroll, most, "no further than the last line");
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    assert!(
        text(&terminal).contains("w599"),
        "the last line is on screen"
    );
    press(&mut app, KeyCode::Char('k'));
    assert_eq!(app.artist.bio_scroll, most - 1);

    // Closed and opened again, it starts from the top.
    key(&mut app, KeyCode::Char('b'));
    key(&mut app, KeyCode::Char('b'));
    assert_eq!(app.artist.bio_scroll, 0);
}

#[test]
fn esc_closes_an_open_bio_before_leaving_the_artist() {
    // The blurb is drawn over the page rather than pushed as a view, so
    // escape used to pop the page from under it: a reader who pressed
    // the key everything else answers with "back" landed two views
    // away, on whatever the artist was opened from.
    for code in [KeyCode::Esc, KeyCode::Char('[')] {
        let mut app = with_artist_open();
        let depth = app.back.len();
        key(&mut app, KeyCode::Char('b'));
        assert!(app.artist.bio_open, "b opens the bio");

        assert!(
            key(&mut app, code).is_none(),
            "{code:?} is spent on the bio"
        );
        assert!(!app.artist.bio_open, "{code:?} closed the bio");
        assert!(
            app.artist.page.is_some(),
            "{code:?} left the artist's page up"
        );
        assert_eq!(app.back.len(), depth, "{code:?} popped nothing");

        // With the bio closed, the same key leaves the page as before.
        assert!(matches!(key(&mut app, code), Some(Action::GoBack)));
    }
}

#[test]
fn an_artists_page_is_drawn_over_the_view_behind_it() {
    let mut app = with_artist_open();
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 40)).unwrap();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let text = geometry::text(&terminal.backend().buffer().clone());

    assert!(text.contains("Daft Punk"), "the artist's name:\n{text}");
    assert!(text.contains("One More Time"), "their top tracks:\n{text}");
    assert!(text.contains("Album 0"), "their albums:\n{text}");
}

#[test]
fn a_similar_artist_opens_their_own_page() {
    let mut app = with_artist_open();
    app.artist.section = artistview::Section::Similar.index();
    assert!(matches!(
        app.selected_collection(),
        Some(Collection::Artist(id)) if id == "100"
    ));
}

#[test]
fn opening_another_artist_clears_the_last_ones_page() {
    // Left behind, it would show under the new artist's name until
    // their own reply arrived.
    let mut app = with_artist_open();
    app.artist.section = artistview::Section::Albums.index();
    app.update(Action::ActivateSelection);
    assert!(app.artist.page.is_none(), "the old page is gone");
    assert_eq!(app.artist.section, 0, "and its selection with it");
}

#[test]
fn a_page_for_an_artist_the_user_has_left_is_dropped() {
    let mut app = with_artist_open();
    let mut other = an_artist_page();
    other.name = "Someone Else".into();
    // A request the view on screen is not waiting on.
    let left = RequestId(open_request(&app).0 + 1);
    app.update(Action::ArtistLoaded {
        request: left,
        page: Box::new(other),
    });
    assert_eq!(
        app.artist.page.as_ref().map(|p| p.name.as_str()),
        Some("Daft Punk"),
        "not shown under the wrong heading"
    );
}

#[test]
fn opening_an_album_or_artist_takes_the_pane_so_the_reply_lands() {
    // Reported: `c` and `a` did nothing, from every list. The log said
    // why -- "dropping tracks for ..., no longer open": the reply is
    // only accepted for the title that is open, and nothing had opened
    // one. Every album and artist a key asked for came back and was
    // thrown away.
    let mut app = signed_in(sidebar::Section::Tracks);
    let track = crate::domain::Track {
        id: crate::domain::TrackId::from(1),
        album: "The Record".into(),
        album_id: Some("77".into()),
        artist: "Someone, Else".into(),
        artist_id: Some("88".into()),
        ..crate::domain::Track::sample("Song", "Someone", std::time::Duration::from_secs(1))
    };
    app.tracks = vec![track.clone()];

    let open = key(&mut app, KeyCode::Char('c')).expect("c opens the album");
    assert!(matches!(open, Action::OpenTrackAlbum));
    app.update(open);
    assert_eq!(
        app.open.as_ref().map(|o| o.title.as_str()),
        Some("The Record"),
        "the album is what is open, so its tracks are accepted"
    );
    // The loop fetches what the open names; a wait with nothing to fetch
    // was a pane stuck on loading until escape.
    assert_eq!(
        app.open.as_ref().and_then(|o| o.target.clone()),
        Some(Collection::Album("77".into())),
        "and the open names the album to fetch"
    );
    assert_eq!(
        app.showing(),
        Showing::Loading,
        "and the pane says it is on its way"
    );
    app.update(Action::CollectionLoaded {
        request: open_request(&app),
        tracks: vec![crate::domain::Track::sample(
            "1",
            "x",
            std::time::Duration::from_secs(1),
        )],
    });
    assert_eq!(app.showing(), Showing::Tracks, "the reply landed");
    assert_eq!(app.tracks.len(), 1);

    // Back to the list, and the artist the same way -- headed by the
    // first credited name, which is the page the reply is named for.
    // The section fetches its list again on the way back; here it is
    // put back by hand.
    app.update(Action::GoBack);
    app.tracks = vec![track.clone()];
    let open = key(&mut app, KeyCode::Char('a')).expect("a opens the artist");
    app.update(open);
    let open = app.open.as_ref().expect("an artist is open");
    assert_eq!(open.title, "Someone");
    assert!(open.round_cover, "an artist's picture is round");
    assert_eq!(
        open.target,
        Some(Collection::Artist("88".into())),
        "and the open names the artist to fetch"
    );

    // When the row carries no id, the pane is taken as the fetch is
    // asked for, under the name the row knows -- and what the fetch
    // hands back renames the wait rather than opening a second view.
    app.update(Action::GoBack);
    app.tracks = vec![track.clone()];
    app.update(Action::FetchTrackAlbum(crate::domain::TrackId::from(1)));
    assert_eq!(
        app.showing(),
        Showing::Loading,
        "waiting from the first key"
    );
    assert_eq!(
        app.open.as_ref().map(|o| o.title.as_str()),
        Some("The Record")
    );
    assert_eq!(
        app.open.as_ref().and_then(|o| o.target.clone()),
        Some(Collection::TrackAlbum(crate::domain::TrackId::from(1))),
        "the wait names the track to look up, which is what the loop fetches"
    );
    let depth = app.back.len();
    app.update(Action::OpenTarget {
        request: open_request(&app),
        target: Collection::Album("5".into()),
        title: "Fetched".into(),
        subtitle: "By".into(),
        cover: None,
    });
    let open = app.open.as_ref().expect("still open");
    assert_eq!(
        (open.title.as_str(), open.subtitle.as_str()),
        ("Fetched", "By")
    );
    assert_eq!(app.back.len(), depth, "renamed, not stacked");

    // Left before the track came back: the reply is dropped, as any
    // other late reply is.
    app.update(Action::GoBack);
    assert!(app.open.is_none());
    app.update(Action::OpenTarget {
        request: RequestId(0),
        target: Collection::Album("6".into()),
        title: "Late".into(),
        subtitle: String::new(),
        cover: None,
    });
    assert!(app.open.is_none(), "nothing was waiting for it");

    // A wait for another track, opened since, is not it either: the
    // lookup is a reply like any other, and names the request it answers.
    app.tracks = vec![track.clone()];
    app.update(Action::FetchTrackAlbum(crate::domain::TrackId::from(1)));
    let first = open_request(&app);
    app.update(Action::GoBack);
    app.tracks = vec![track.clone()];
    app.update(Action::FetchTrackArtist(crate::domain::TrackId::from(1)));
    app.update(Action::OpenTarget {
        request: first,
        target: Collection::Album("6".into()),
        title: "Late".into(),
        subtitle: String::new(),
        cover: None,
    });
    let open = app.open.as_ref().expect("the artist wait is still up");
    assert_eq!(open.title, "Someone", "and the late album did not take it");
    assert_eq!(
        open.target,
        Some(Collection::TrackArtist(crate::domain::TrackId::from(1)))
    );
}

#[test]
fn an_album_in_the_favourites_opens_with_its_heart() {
    // The album's page is built from its detail, which never says whether
    // it is a favourite, so the header drew no heart on an album opened
    // from the Albums section itself. The ids fetched at sign-in know.
    let mut app = signed_in(sidebar::Section::Albums);
    app.favourites.albums.insert("20556792".into());
    app.open = Some(OpenCollection {
        title: "good kid".into(),
        subtitle: String::new(),
        detail: String::new(),
        cover: None,
        round_cover: false,
        came_from: sidebar::Section::Albums,
        request: RequestId::default(),
        target: Some(Collection::Album("20556792".into())),
    });
    let mut page = an_artist_page();
    page.kind = crate::library::PageKind::Album;
    page.id = "20556792".into();
    page.name = "good kid".into();
    page.following = false;
    app.update(Action::ArtistLoaded {
        request: open_request(&app),
        page: Box::new(page),
    });
    assert!(
        app.artist.page.as_ref().is_some_and(|p| p.following),
        "the page says it is a favourite, which is what the header draws"
    );
}

#[test]
fn back_onto_an_artists_page_lands_where_the_user_left() {
    // The history carries the whole page, not just its contents: the
    // section and the scroll come back with it.
    let mut app = with_artist_open();
    app.artist.section = artistview::Section::Albums.index();
    app.artist.scroll = 2;
    key(&mut app, KeyCode::Char('Q'));
    assert_eq!(app.showing(), Showing::Queue);
    app.update(Action::GoBack);
    assert_eq!(app.showing(), Showing::Artist);
    assert_eq!(app.artist.section, artistview::Section::Albums.index());
    assert_eq!(app.artist.scroll, 2);
}

#[test]
fn a_track_card_without_ids_still_opens_its_album_and_artist() {
    // Reported: `a` and `c` did nothing, on every page. A card from the
    // home page names its album and artist and carries the id of
    // neither, so the guards that need one failed and the key fell
    // through in silence. `R` had the same gap and filled it by
    // fetching the track; these do the same.
    let mut app = signed_in(sidebar::Section::Music);
    app.last_main_width = 120;
    let mut card = carousel::Card::new("Song", "Someone");
    card.target = Some(carousel::Target::Track("42".into()));
    app.home.rows = vec![home::Row {
        heading: "Songs".into(),
        kind: crate::browse::RowKind::Compact,
        cards: vec![card],
        state: Default::default(),
    }];
    app.home.row = 0;
    assert!(
        app.selected_track_album().is_none(),
        "the card carries no album id"
    );

    assert!(
        matches!(
            key(&mut app, KeyCode::Char('c')),
            Some(Action::FetchTrackAlbum(ref id)) if id.0 == "42"
        ),
        "c fetches the track to find its album"
    );
    assert!(
        matches!(
            key(&mut app, KeyCode::Char('a')),
            Some(Action::FetchTrackArtist(ref id)) if id.0 == "42"
        ),
        "a fetches the track to find its artist"
    );

    // Asked for, and still on its way: a repeat of the key does nothing.
    // The row has no id to match on, so the wait is matched by the track
    // it looks up.
    app.update(Action::FetchTrackArtist("42".into()));
    let depth = app.back.len();
    assert!(
        key(&mut app, KeyCode::Char('a')).is_none(),
        "the artist is already on its way"
    );
    assert_eq!(app.back.len(), depth, "and nothing was stacked");
}

#[test]
fn a_on_an_artists_own_track_does_not_open_their_page_again() {
    // The page is up and its top tracks name the artist whose page it
    // is; `a` on one of them reopened the page, by id as by name.
    let mut app = with_artist_open();
    let page = app.artist.page.as_ref().expect("open");
    let (id, name) = (page.id.clone(), page.name.clone());
    app.artist.section = artistview::Section::Tracks.index();
    app.artist.page.as_mut().expect("open").top_tracks = vec![crate::domain::Track {
        id: crate::domain::TrackId::from(1),
        artist: name,
        artist_id: Some(id),
        ..crate::domain::Track::sample("Hit", "x", std::time::Duration::from_secs(1))
    }];
    assert!(
        app.selected_track_artist().is_some(),
        "the row names the artist"
    );
    let depth = app.back.len();
    assert!(
        key(&mut app, KeyCode::Char('a')).is_none(),
        "already looking at them"
    );
    assert_eq!(app.back.len(), depth);
}

#[test]
fn a_cut_row_of_profiles_shows_circles_without_names() {
    // The row at the fold shows as much of its circles as fits and
    // nothing of its names: a name squeezed onto a cut avatar read as
    // the text cutting into the picture. The keys still count that row
    // as visible, and moving into it scrolls it up whole.
    let mut app = signed_in(sidebar::Section::Profiles);
    app.artists = (0..9)
        .map(|i| crate::library::Artist {
            id: (i).to_string(),
            name: format!("Artist {i}"),
            picture: None,
        })
        .collect();
    app.last_main_width = 100;
    app.last_main_height = 30;

    let text = geometry::text(&geometry::draw(100, 30, |f, _a, _p| draw(f, &mut app)));
    let named = (0..9)
        .filter(|i| text.contains(&format!("Artist {i}")))
        .count();
    let (cols, landable) = app.grid_geometry();
    let drawn = grid::rows(
        30 - grid::header_rows_with(grid::Chrome::Full, &[]),
        app.grid_lines(sidebar::Section::Profiles),
    );

    assert!(
        drawn > landable,
        "the last row drawn is the cut one: {drawn} vs {landable}"
    );
    // Only the whole rows carry names; the cut one at the fold does not.
    assert_eq!(
        named,
        (cols * landable).min(9),
        "the grid says {cols}x{landable} but {named} names are drawn:\n{text}"
    );
}

#[test]
fn f_follows_the_artist_whose_page_is_open_whatever_is_selected() {
    // `A` on an artist's page is the top track under the selection, so
    // following gets a key of its own -- one that names the page's
    // artist from any section of it.
    let mut app = signed_in(sidebar::Section::Tracks);
    assert!(
        key(&mut app, KeyCode::Char('F')).is_none(),
        "no artist in sight"
    );

    let mut page = an_artist_page();
    page.id = "42".into();
    app.open = Some(OpenCollection {
        title: "Daft Punk".into(),
        subtitle: String::new(),
        detail: String::new(),
        cover: None,
        round_cover: false,
        came_from: sidebar::Section::Tracks,
        request: RequestId::default(),
        target: None,
    });
    app.update(Action::ArtistLoaded {
        request: open_request(&app),
        page: Box::new(page),
    });
    assert_eq!(
        app.selected_favourable(),
        Some((Favourable::Artist("42".into()), "Daft Punk".to_string()))
    );
    assert!(
        !app.favourites.artists.contains("42"),
        "the page said not followed"
    );

    // The reply keeps the page in step: flag and count.
    app.artist.page.as_mut().unwrap().fans = Some(100);
    app.update(Action::FollowChanged {
        what: Favourable::Artist("42".into()),
        name: "Daft Punk".into(),
        following: true,
    });
    let page = app.artist.page.as_ref().unwrap();
    assert!(
        page.following && page.fans == Some(101),
        "{:?} {:?}",
        page.following,
        page.fans
    );
    assert!(
        matches!(
            key(&mut app, KeyCode::Char('F')),
            Some(Action::ToggleFollow)
        ),
        "F on the page follows its artist"
    );
    assert!(
        matches!(
            key(&mut app, KeyCode::Char('A')),
            Some(Action::ToggleFavourite)
        ),
        "and A still favourites the track"
    );
}

#[test]
fn f_follows_the_artist_whose_card_is_selected() {
    let mut app = signed_in(sidebar::Section::Profiles);
    app.artists = vec![crate::library::Artist {
        id: (7).to_string(),
        name: "Kaaris".into(),
        picture: None,
    }];
    assert_eq!(
        app.selected_favourable(),
        Some((Favourable::Artist("7".into()), "Kaaris".to_string()))
    );
    assert!(matches!(
        key(&mut app, KeyCode::Char('F')),
        Some(Action::ToggleFollow)
    ));
}

#[test]
fn an_album_opens_as_a_page_in_the_artists_shape() {
    // Its rows sit in the artist page's slots, headed for what they
    // hold, and F on the page favourites the album rather than an
    // artist. A cut section names where its rest lives, which is what
    // See all fetches.
    use crate::library::PageKind;
    let mut app = signed_in(sidebar::Section::Albums);
    app.open = Some(OpenCollection {
        title: "good kid".into(),
        subtitle: String::new(),
        detail: String::new(),
        cover: None,
        round_cover: false,
        came_from: sidebar::Section::Albums,
        request: RequestId::default(),
        target: Some(Collection::Album("20556792".into())),
    });
    let mut page = an_artist_page();
    page.kind = PageKind::Album;
    page.id = "20556792".into();
    page.name = "good kid".into();
    page.following = true;
    app.update(Action::ArtistLoaded {
        request: open_request(&app),
        page: Box::new(page),
    });
    assert!(app.artist.page.is_some(), "the page took the pane");

    assert_eq!(
        app.selected_favourable(),
        Some((Favourable::Album("20556792".into()), "good kid".to_string()))
    );
    assert_eq!(
        artistview::Section::Albums.heading(PageKind::Album),
        "More by the artist"
    );
    assert_eq!(
        artistview::Section::AppearsOn.heading(PageKind::Album),
        "Related albums"
    );

    // Taking the album out of the favourites updates the page's heart.
    app.update(Action::FollowChanged {
        what: Favourable::Album("20556792".into()),
        name: "good kid".into(),
        following: false,
    });
    assert!(!app.artist.page.as_ref().unwrap().following);
}

#[test]
fn the_reply_marks_the_artist_and_says_so() {
    let mut app = signed_in(sidebar::Section::Tracks);
    app.update(Action::FollowChanged {
        what: Favourable::Artist("42".into()),
        name: "Daft Punk".into(),
        following: true,
    });
    assert!(app.favourites.artists.contains("42"), "followed");
    assert_eq!(app.status.as_deref(), Some("Following Daft Punk"));

    app.update(Action::FollowChanged {
        what: Favourable::Artist("42".into()),
        name: "Daft Punk".into(),
        following: false,
    });
    assert!(!app.favourites.artists.contains("42"), "and not any more");
    assert_eq!(app.status.as_deref(), Some("Unfollowed Daft Punk"));

    // The Artists section's reply is the account's list of them.
    app.update(Action::ArtistsLoaded(vec![crate::library::Artist {
        id: (9).to_string(),
        name: "Prince".into(),
        picture: None,
    }]));
    assert_eq!(
        app.favourites.artists,
        std::collections::HashSet::from(["9".to_string()])
    );
}

#[test]
fn o_sorts_an_album_page_by_title_and_keeps_track_order_until_then() {
    // Reported: O sorted the Tracks section and a playlist but not an
    // album. An album is a page, like an artist's, with the tracks in
    // `top_tracks` and the cursor in `artist.tracks` -- neither of which
    // the sort reached.
    let mut app = signed_in(sidebar::Section::Albums);
    app.open = Some(OpenCollection {
        title: "good kid".into(),
        subtitle: String::new(),
        detail: String::new(),
        cover: None,
        round_cover: false,
        came_from: sidebar::Section::Albums,
        request: RequestId::default(),
        target: Some(Collection::Album("20556792".into())),
    });
    let mut page = an_artist_page();
    page.kind = crate::library::PageKind::Album;
    page.top_tracks = some_tracks(&["Beta", "Alpha", "Gamma"]);
    app.update(Action::ArtistLoaded {
        request: open_request(&app),
        page: Box::new(page),
    });
    app.artist.section = artistview::Section::Tracks.index();
    assert_eq!(app.showing(), Showing::Artist, "an album is a page");

    let first = |app: &App| app.selected_track().expect("a track").title;
    assert_eq!(
        first(&app),
        "Beta",
        "the album keeps its track order until O"
    );
    key(&mut app, KeyCode::Char('O'));
    assert_eq!(first(&app), "Alpha", "O sorts the album by title");
    key(&mut app, KeyCode::Char('I'));
    assert_eq!(first(&app), "Gamma", "I reverses it");
    // Enter plays from the same order the screen shows.
    let (list, at, _) = app.selected_list();
    assert_eq!(list[at].title, "Gamma");
}

fn an_album_page_of(titles: &[&str]) -> App {
    let mut app = signed_in(sidebar::Section::Albums);
    app.open = Some(OpenCollection {
        title: "good kid".into(),
        subtitle: String::new(),
        detail: String::new(),
        cover: None,
        round_cover: false,
        came_from: sidebar::Section::Albums,
        request: RequestId::default(),
        target: Some(Collection::Album("20556792".into())),
    });
    let mut page = an_artist_page();
    page.kind = crate::library::PageKind::Album;
    page.top_tracks = some_tracks(titles);
    app.update(Action::ArtistLoaded {
        request: open_request(&app),
        page: Box::new(page),
    });
    app.artist.section = artistview::Section::Tracks.index();
    app
}

#[test]
fn the_filter_narrows_an_album_page_and_o_walks_only_its_columns() {
    // `/` on an album page typed into the Tracks section's filter, which
    // the page never read: the box took the keys and nothing changed.
    let mut app = an_album_page_of(&["Beta", "Alpha", "Gamma"]);
    let first = |app: &App| app.selected_track().expect("a track").title;

    key(&mut app, KeyCode::Char('/'));
    assert!(app.filtering);
    key(&mut app, KeyCode::Char('g'));
    assert_eq!(app.filter_text(), "g", "the page has a filter of its own");
    assert_eq!(first(&app), "Gamma", "and the keys act on what it keeps");
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.filter_text(), "");
    assert_eq!(first(&app), "Beta");

    // One album, no dates: the album and added columns are not drawn, so
    // the walk goes title, artist, time and back.
    key(&mut app, KeyCode::Char('O'));
    key(&mut app, KeyCode::Char('O'));
    assert_eq!(app.artist.tracks.sort, Some(tracklist::Column::Artist));
    key(&mut app, KeyCode::Char('O'));
    assert_eq!(app.artist.tracks.sort, Some(tracklist::Column::Duration));
    key(&mut app, KeyCode::Char('O'));
    assert_eq!(app.artist.tracks.sort, None);
}
