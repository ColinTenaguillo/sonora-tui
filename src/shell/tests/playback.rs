use super::*;

#[test]
fn no_view_paints_under_the_now_playing_bar() {
    // The pane ends where the bar begins, and every view has its own
    // idea of how much room that leaves. Chasing them one at a time has
    // found three so far, so this walks the lot: whatever is on screen,
    // nothing may reach the bar's rows.
    //
    // Height 40 down to 12: tall enough for everything, short enough
    // that the arithmetic is tight.
    for height in [12u16, 16, 20, 28, 40] {
        for section in sidebar::Section::ALL {
            let mut app = signed_in(section);
            app.tracks = (0..30)
                .map(|i| {
                    let mut t = crate::domain::Track::sample(
                        &format!("Track {i}"),
                        "An Artist",
                        std::time::Duration::from_secs(200),
                    );
                    t.id = crate::domain::TrackId::from(i as u64);
                    t
                })
                .collect();
            app.playlists = (0..20)
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
            app.albums = (0..20)
                .map(|i| crate::library::Album {
                    id: (i).to_string(),
                    title: format!("Album {i}"),
                    artist: "An Artist".into(),
                    year: None,
                    cover: None,
                    track_count: 10,
                    duration: None,
                })
                .collect();
            app.artists = (0..20)
                .map(|i| crate::library::Artist {
                    id: (i).to_string(),
                    name: format!("Artist {i}"),
                    picture: None,
                })
                .collect();
            app.update(Action::FeedLoaded(
                (0..20)
                    .map(|i| carousel::Card::new(format!("Release {i}"), "An Artist"))
                    .collect(),
            ));

            assert_clear_of_the_bar(&mut app, height, &format!("{section:?}"));
        }

        // And the views that are not sections: search, an opened album,
        // an artist's page.
        let mut app = signed_in(sidebar::Section::Music);
        app.update(Action::BeginSearch);
        for c in "daft".chars() {
            key(&mut app, KeyCode::Char(c));
        }
        app.update(Action::SearchLoaded(Box::new(some_results())));
        app.search.as_mut().unwrap().typing = false;
        for tab in 0..searchview::Tab::ALL.len() {
            app.search.as_mut().unwrap().tab = tab;
            assert_clear_of_the_bar(&mut app, height, &format!("search tab {tab}"));
        }

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
        let request = open_request(&app);
        app.update(Action::ArtistLoaded {
            request,
            page: Box::new(crate::library::ArtistPage {
            id: (0).to_string(),
            fans: None,
            following: false,
            kind: crate::library::PageKind::Artist,
            name: "Daft Punk".into(),
            picture: None,
            top_tracks_path: None,
            year: None,
            top_tracks: some_tracks(&["One", "Two", "Three"]),
            albums: (0..4)
                .map(|i| crate::library::Album {
                    id: (i).to_string(),
                    title: format!("Album {i}"),
                    artist: "Daft Punk".into(),
                    year: None,
                    cover: None,
                    track_count: 10,
                    duration: None,
                })
                .collect(),
            similar: Vec::new(),
            bio: None,
            singles: Vec::new(),
            appears_on: Vec::new(),
            radio: None,
        }),
        });
        assert_clear_of_the_bar(&mut app, height, "artist page");
    }
}

#[test]
fn a_track_that_fails_to_start_is_taken_out_of_the_bar() {
    // The bar is filled the moment a track is chosen so the keypress has
    // a visible effect. When TIDAL then answers 500 — which it did, three
    // requests at once — the cover and title of a track that never
    // started were left sitting there looking like it was playing.
    let mut app = App::default();
    let track = crate::domain::Track::sample(
        "Never Started",
        "An Artist",
        std::time::Duration::from_secs(200),
    );
    let id = track.id.clone();
    app.now_playing.track = Some(track);
    app.now_playing.playing = true;

    app.update(Action::PlaybackFailed {
        id,
        message: "500 from /tracks/0/playbackinfopostpaywall".into(),
    });

    assert!(app.now_playing.track.is_none(), "the bar is cleared");
    assert!(
        !app.now_playing.playing,
        "and it does not claim to be playing"
    );
    assert!(
        app.status.as_deref().is_some_and(|s| s.contains("500")),
        "the failure is reported: {:?}",
        app.status
    );
}

#[test]
fn a_late_failure_does_not_clear_a_track_that_has_since_started() {
    // Choose one track, then another before the first request returns:
    // the first one's failure must not wipe the second from the bar.
    let mut app = App::default();
    let first = crate::domain::Track::sample("First", "A", std::time::Duration::ZERO);
    let stale_id = first.id;

    let mut second = crate::domain::Track::sample("Second", "B", std::time::Duration::ZERO);
    second.id = crate::domain::TrackId(format!("{}-later", stale_id.0));
    app.now_playing.track = Some(second);

    app.update(Action::PlaybackFailed {
        id: stale_id,
        message: "500".into(),
    });

    assert_eq!(
        app.now_playing.track.as_ref().map(|t| t.title.as_str()),
        Some("Second"),
        "the track now playing is untouched"
    );
}

#[test]
fn a_new_stream_starts_the_progress_bar_at_the_beginning() {
    // The previous track's position used to survive into the next one.
    // If that track was the longer of the two, the ratio saturated and
    // the bar was drawn full over a track seven seconds in.
    let mut app = App::default();
    app.now_playing.position = std::time::Duration::from_secs(400);

    app.update(Action::Playback(crate::playback::PlaybackEvent::Started {
        bit_depth: None,
        sample_rate: 44_100,
        delivered: crate::domain::Quality::High,
    }));

    assert_eq!(
        app.now_playing.position,
        std::time::Duration::ZERO,
        "a stream that just started is at its beginning"
    );
}

#[test]
fn the_quality_badge_shows_the_delivered_bit_depth() {
    // The decoder cannot report bit depth, so it is threaded from the
    // playback-info response through Cmd::Play. If that ever breaks, the
    // badge silently drops to "44.1kHz" and stops confirming hi-res.
    let mut app = App::default();
    app.update(Action::Playback(crate::playback::PlaybackEvent::Started {
        bit_depth: Some(24),
        sample_rate: 44_100,
        delivered: crate::domain::Quality::HiResLossless,
    }));
    assert_eq!(app.now_playing.quality.as_deref(), Some("24-bit 44.1kHz"));
}

#[test]
fn the_quality_badge_omits_bit_depth_when_unknown() {
    let mut app = App::default();
    app.update(Action::Playback(crate::playback::PlaybackEvent::Started {
        bit_depth: None,
        sample_rate: 44_100,
        delivered: crate::domain::Quality::High,
    }));
    assert_eq!(app.now_playing.quality.as_deref(), Some("44.1kHz"));
}

#[test]
fn a_track_without_a_duration_takes_the_streams() {
    use std::time::Duration;
    let mut app = App::default();
    app.now_playing.track = Some(crate::domain::Track::sample("t", "a", Duration::ZERO));
    app.update(Action::Playback(crate::playback::PlaybackEvent::Length(
        Duration::from_secs(200),
    )));
    assert_eq!(
        app.now_playing.track.as_ref().unwrap().duration,
        Duration::from_secs(200)
    );
    // One that knows its own keeps it: the card's figure is the
    // catalogue's, the stream's can be a second off.
    app.now_playing.track = Some(crate::domain::Track::sample(
        "t",
        "a",
        Duration::from_secs(100),
    ));
    app.update(Action::Playback(crate::playback::PlaybackEvent::Length(
        Duration::from_secs(200),
    )));
    assert_eq!(
        app.now_playing.track.as_ref().unwrap().duration,
        Duration::from_secs(100)
    );
}

#[test]
fn no_sample_rate_means_no_badge() {
    // A local file reports none, and "0.0kHz" is not a quality.
    let mut app = App::default();
    app.update(Action::Playback(crate::playback::PlaybackEvent::Started {
        bit_depth: None,
        sample_rate: 0,
        delivered: crate::domain::Quality::Lossless,
    }));
    assert_eq!(app.now_playing.quality, None);
}

#[test]
fn the_quality_badge_keeps_the_half_rates_exact() {
    // HE-AAC decodes to 22050 Hz here: one decimal rounds it to "22.1",
    // which is not a rate that exists. Whole rates keep their ".0".
    for (rate, label) in [
        (22_050, "22.05kHz"),
        (44_100, "44.1kHz"),
        (48_000, "48.0kHz"),
    ] {
        let mut app = App::default();
        app.update(Action::Playback(crate::playback::PlaybackEvent::Started {
            bit_depth: None,
            sample_rate: rate,
            delivered: crate::domain::Quality::Low,
        }));
        assert_eq!(app.now_playing.quality.as_deref(), Some(label));
    }
}

#[test]
fn r_in_the_queue_starts_the_radio_of_the_selected_entry() {
    // The queue is a list like any other: R over one of its rows starts
    // that track's radio, not the playing track's.
    let mut app = signed_in(sidebar::Section::Tracks);
    let mut a = crate::domain::Track::sample("Playing", "A", std::time::Duration::from_secs(1));
    a.id = crate::domain::TrackId::from(1);
    let mut b = crate::domain::Track::sample("Queued", "B", std::time::Duration::from_secs(1));
    b.id = crate::domain::TrackId::from(2);
    app.queue.start_context(vec![a.clone(), b], 0, None);
    app.now_playing.track = Some(a);
    app.last_main_height = 40;
    key(&mut app, KeyCode::Char('Q'));
    assert_eq!(app.showing(), Showing::Queue, "the queue is up");

    app.queue_list.selected = 1;
    assert!(
        matches!(
            key(&mut app, KeyCode::Char('R')),
            Some(Action::PlayTrackRadio)
        ),
        "R offers the radio"
    );
    assert_eq!(
        app.track_radio(),
        Some(crate::domain::TrackId::from(2)),
        "the selected row's, not the playing one's"
    );
    app.update(Action::PlayTrackRadio);
    assert_eq!(
        app.open.as_ref().map(|o| o.title.as_str()),
        Some("Queued Radio")
    );
    assert!(
        matches!(
            app.open.as_ref().and_then(|o| o.target.clone()),
            Some(Collection::Radio(ref seed)) if seed.0 == "2"
        ),
        "and the loop fetches the station on that seed"
    );
}

#[test]
fn back_and_forward_walk_the_history_both_ways() {
    // Escape only ever went back. These are the two arrows: a run of
    // sections and opened views, stepped through in both directions.
    let mut app = with_albums(3);
    let first = app.sidebar.section();

    // Two moves through the nav, then into an album.
    app.update(Action::SidebarNext);
    let second = app.sidebar.section();
    app.update(Action::SidebarPrevious);
    assert_eq!(app.sidebar.section(), first, "back where we started");

    // Back through both moves.
    app.update(Action::GoBack);
    assert_eq!(app.sidebar.section(), second, "one step back");
    app.update(Action::GoBack);
    assert_eq!(app.sidebar.section(), first, "and another");

    // Forward again, the same way.
    app.update(Action::GoForward);
    assert_eq!(app.sidebar.section(), second, "one step forward");
    app.update(Action::GoForward);
    assert_eq!(app.sidebar.section(), first, "and back to where we were");

    // Past the end of either stack is a no-op rather than a panic.
    for _ in 0..5 {
        app.update(Action::GoForward);
    }
    assert_eq!(app.sidebar.section(), first);
}

#[test]
fn the_history_does_not_grow_without_end() {
    // Every move through the nav is a step, so a session spent walking
    // the sidebar would otherwise keep every one of them.
    let mut app = with_albums(3);
    for _ in 0..(App::HISTORY * 3) {
        app.update(Action::SidebarNext);
        app.update(Action::SidebarPrevious);
    }
    assert!(
        app.back.len() <= App::HISTORY,
        "the history is capped, got {}",
        app.back.len()
    );
    // And the recent end is what was kept: back still works.
    let before = app.sidebar.section();
    app.update(Action::GoBack);
    assert_ne!(
        app.sidebar.section(),
        before,
        "the newest step is still there"
    );
}

#[test]
fn a_finished_track_asks_for_the_next_one() {
    // The whole point: an album used to stop after the track that was
    // started, because nothing moved the queue on.
    let mut app = signed_in(sidebar::Section::Tracks);
    app.queue = crate::playback::Queue::new(some_tracks(&["One", "Two", "Three"]), 0);

    let next = app.update(Action::Playback(crate::playback::PlaybackEvent::Finished));
    assert!(
        matches!(next, Some(Action::PlayQueued)),
        "the queue moves on and asks for the track"
    );
    assert_eq!(app.queue.current().unwrap().title, "Two");
}

#[test]
fn the_last_track_of_a_queue_stops_rather_than_looping() {
    let mut app = signed_in(sidebar::Section::Tracks);
    // Without autoplay: with it, the end of the queue is followed.
    app.config.playback.autoplay = false;
    app.queue = crate::playback::Queue::new(some_tracks(&["Only"]), 0);
    app.now_playing.playing = true;

    let next = app.update(Action::Playback(crate::playback::PlaybackEvent::Finished));
    assert!(next.is_none(), "nothing follows the last track");
    assert!(!app.now_playing.playing, "and the player stops");
}

#[test]
fn n_and_p_step_through_the_queue() {
    let mut app = signed_in(sidebar::Section::Tracks);
    app.tracks = some_tracks(&["One", "Two", "Three"]);
    app.queue = crate::playback::Queue::new(app.tracks.clone(), 0);

    assert!(matches!(
        key(&mut app, KeyCode::Char('n')),
        Some(Action::QueueNext)
    ));
    assert!(matches!(
        app.update(Action::QueueNext),
        Some(Action::PlayQueued)
    ));
    assert_eq!(app.queue.current().unwrap().title, "Two");

    assert!(matches!(
        key(&mut app, KeyCode::Char('p')),
        Some(Action::QueuePrevious)
    ));
    assert!(matches!(
        app.update(Action::QueuePrevious),
        Some(Action::PlayQueued)
    ));
    assert_eq!(app.queue.current().unwrap().title, "One");
}

#[test]
fn the_modes_survive_a_track_that_fails_to_start() {
    // They used to be copied into the bar, and clearing the bar after a
    // failed track dropped the copy while the queue kept the real one —
    // so shuffle went grey on the next song while still being on.
    use crate::playback::Repeat;
    let mut app = signed_in(sidebar::Section::Tracks);
    app.tracks = some_tracks(&["One", "Two"]);
    app.queue = crate::playback::Queue::new(app.tracks.clone(), 0);
    app.update(Action::ToggleShuffle);
    app.update(Action::CycleRepeat);
    assert!(app.queue.shuffled());
    assert_eq!(app.queue.repeat, Repeat::All);

    // A track is chosen, and its request fails.
    let id = app.tracks[0].id.clone();
    app.now_playing.track = Some(app.tracks[0].clone());
    app.update(Action::PlaybackFailed {
        id,
        message: "nope".into(),
    });

    assert!(app.queue.shuffled(), "shuffle is still on");
    assert_eq!(app.queue.repeat, Repeat::All, "and so is repeat");

    // The next track starts, which is where the grey showed up.
    app.now_playing.track = Some(app.tracks[1].clone());

    // What the bar draws comes from the queue, not from a copy that the
    // reset above would have cleared.
    let modes = nowplaying::Modes {
        shuffled: app.queue.shuffled(),
        repeat: app.queue.repeat,
        sleep: None,
        spectrum: Vec::new(),
    };
    let palette = theme::Palette::detect();
    let state = app.now_playing.clone();
    let buf = geometry::draw(100, layout::NOW_PLAYING_HEIGHT, move |f, a, p| {
        nowplaying::render(f, a, p, &state, modes)
    });
    let shuffle = geometry::find(&buf, nowplaying::SHUFFLE).expect("the shuffle button");
    assert_eq!(
        buf[(shuffle.start, shuffle.row)].fg,
        palette.accent,
        "the button is still lit:\n{}",
        geometry::text(&buf)
    );
}

#[test]
fn z_and_r_reach_shuffle_and_repeat() {
    use crate::playback::Repeat;
    let mut app = signed_in(sidebar::Section::Tracks);
    app.queue = crate::playback::Queue::new(some_tracks(&["One", "Two", "Three"]), 0);

    assert!(matches!(
        key(&mut app, KeyCode::Char('z')),
        Some(Action::ToggleShuffle)
    ));
    app.update(Action::ToggleShuffle);
    assert!(app.queue.shuffled(), "shuffle is on");
    app.update(Action::ToggleShuffle);
    assert!(!app.queue.shuffled(), "and off again");

    assert!(matches!(
        key(&mut app, KeyCode::Char('r')),
        Some(Action::CycleRepeat)
    ));
    app.update(Action::CycleRepeat);
    assert_eq!(app.queue.repeat, Repeat::All);
    app.update(Action::CycleRepeat);
    assert_eq!(app.queue.repeat, Repeat::One);
    app.update(Action::CycleRepeat);
    assert_eq!(app.queue.repeat, Repeat::Off, "back round");
}

#[test]
fn changing_the_quality_moves_through_every_tier() {
    // Four tiers, wrapping: a list that stops at the ends needs two
    // keys to walk, which is more chrome than one setting is worth.
    let mut app = signed_in(sidebar::Section::Settings);
    app.settings.next(); // past Service, onto Quality
    app.config
        .audio
        .set_quality(crate::domain::Quality::HiResLossless);
    // `config_path` is None in tests, so nothing is written: an
    // earlier version of this test saved through the real path and
    // left this machine's quality on LOW.
    assert!(app.config_path.is_none());

    let mut seen = vec![app.config.audio.quality()];
    for _ in 0..settings::QUALITIES.len() - 1 {
        app.update(Action::CarouselNext);
        seen.push(app.config.audio.quality());
    }
    assert_eq!(seen, settings::QUALITIES, "best first, then down the list");

    // And round, so the best tier is never more than one key away.
    app.update(Action::CarouselNext);
    assert_eq!(
        app.config.audio.quality(),
        crate::domain::Quality::HiResLossless
    );

    // The other way too.
    app.update(Action::CarouselPrevious);
    assert_eq!(app.config.audio.quality(), crate::domain::Quality::Low);
}

#[test]
fn the_volume_steps_and_stops_at_both_ends() {
    // A step rather than a wrap: turning the volume up past full and
    // round to silence is not what anyone means by the key.
    let mut app = signed_in(sidebar::Section::Settings);
    app.settings.next(); // past Service
    app.settings.next(); // onto Volume
    assert_eq!(app.settings.current(), settings::Setting::Volume);
    app.config.audio.volume = 1.0;

    assert!(matches!(
        app.update(Action::CarouselNext),
        Some(Action::SetVolume(_))
    ));
    assert_eq!(app.config.audio.volume, 1.0, "full is as loud as it goes");

    // Down to nothing, and no further.
    for _ in 0..40 {
        app.update(Action::CarouselPrevious);
    }
    assert_eq!(app.config.audio.volume, 0.0, "and silence is the floor");

    // One press back up is audible rather than nothing.
    app.update(Action::CarouselNext);
    assert!(
        app.config.audio.volume > 0.0,
        "a press must move it, got {}",
        app.config.audio.volume
    );
}

#[test]
fn changing_the_volume_reaches_the_player() {
    // A level you cannot hear yourself setting is one you set by
    // guessing, so the change is sent on rather than waiting for the
    // next track. Changing the quality has nothing to send.
    let mut app = signed_in(sidebar::Section::Settings);
    app.settings.next(); // past Service
    app.settings.next();
    let sent = app.update(Action::CarouselPrevious);
    assert!(
        matches!(sent, Some(Action::SetVolume(v)) if v == app.config.audio.volume),
        "the new level is handed on, got {sent:?}"
    );

    app.settings.previous(); // back to Quality
    assert!(
        app.update(Action::CarouselNext).is_none(),
        "quality is read when the next track starts, not sent now"
    );
}

#[test]
fn dot_and_comma_seek_ten_seconds_within_the_track() {
    let mut app = signed_in(sidebar::Section::Tracks);
    assert!(
        key(&mut app, KeyCode::Char('.')).is_none(),
        "nothing playing: nowhere to seek to"
    );
    app.now_playing.track = Some(crate::domain::Track::sample(
        "Song",
        "Band",
        std::time::Duration::from_secs(100),
    ));
    app.now_playing.position = std::time::Duration::from_secs(30);

    let forward = key(&mut app, KeyCode::Char('.')).expect("bound with a track up");
    assert!(matches!(forward, Action::SeekForward));
    app.update(forward);
    assert_eq!(app.now_playing.position, std::time::Duration::from_secs(40));

    for _ in 0..10 {
        app.update(Action::SeekForward);
    }
    assert_eq!(
        app.now_playing.position,
        std::time::Duration::from_secs(99),
        "stops short of the end rather than finishing the track"
    );
    let back = key(&mut app, KeyCode::Char(',')).expect("bound with a track up");
    assert!(matches!(back, Action::SeekBack));
    for _ in 0..20 {
        app.update(Action::SeekBack);
    }
    assert_eq!(
        app.now_playing.position,
        std::time::Duration::ZERO,
        "and at the start"
    );
}

#[test]
fn enter_plays_a_track_and_opens_an_album_from_the_page() {
    let mut app = with_artist_open();

    // In the track section, enter reaches the page's own tracks.
    let track = app.selected_track().expect("a track");
    assert_eq!(track.title, "One More Time", "the page's own track");

    // In a card section, it reaches that section's cards.
    app.artist.section = artistview::Section::Albums.index();
    assert!(app.on_grid(), "a card section moves like a grid");
    assert!(matches!(
        app.selected_collection(),
        Some(Collection::Album(id)) if id == "0"
    ));
}

#[test]
fn the_queue_is_a_list_the_keys_work_in() {
    // Reported: no way to like the track that is playing. The queue
    // was a modal that closed on any key it did not know, so `A` over
    // the playing row shut it instead of acting on it -- and nothing
    // else could be reached from there either.
    let mut app = signed_in(sidebar::Section::Music);
    let queued = crate::domain::Track {
        id: crate::domain::TrackId::from(7),
        ..crate::domain::Track::sample("Queued", "Someone", std::time::Duration::from_secs(1))
    };
    app.queue.play_next(queued);
    key(&mut app, KeyCode::Char('Q'));
    assert!(app.list == Some(List::Queue), "Q opens the queue");
    app.queue_list.selected = 0;

    assert_eq!(
        app.selected_track().map(|t| t.id.clone()),
        Some(crate::domain::TrackId::from(7)),
        "the row under the cursor is what the keys act on"
    );
    assert!(
        matches!(
            key(&mut app, KeyCode::Char('A')),
            Some(Action::ToggleFavourite)
        ),
        "A likes it"
    );
    assert!(app.list == Some(List::Queue), "and the queue stays open to show it");

    // A key the queue does not know does what it always does, and does
    // not close the list.
    assert!(matches!(
        key(&mut app, KeyCode::Char('z')),
        Some(Action::ToggleShuffle)
    ));
    assert!(app.list == Some(List::Queue), "shuffle is not a way out");

    // Enter is the queue's, even with nothing to jump to: falling
    // through would open whatever the hidden page had selected.
    app.queue = Default::default();
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert!(app.list == Some(List::Queue));

    // The key that opened it closes it: a step back.
    let back = key(&mut app, KeyCode::Char('Q')).expect("Q again steps back");
    app.update(back);
    assert!(app.list != Some(List::Queue), "Q again closes it");
}

#[test]
fn the_queue_is_a_view_that_back_returns_to() {
    // Asked for: the lists are views of their own, on the stack with
    // the rest. Going somewhere from the queue leaves it, and back
    // returns to it -- as it does for an opened album.
    let mut app = signed_in(sidebar::Section::Music);
    key(&mut app, KeyCode::Char('Q'));
    assert!(app.list == Some(List::Queue));

    // Driven the way a keystroke is: the key names the move and
    // `update` makes it, which is where the level is taken.
    let action = key(&mut app, KeyCode::Char('J')).expect("J moves the nav");
    app.update(action);
    assert!(app.list != Some(List::Queue), "moving the nav leaves it");
    app.update(Action::GoBack);
    assert!(app.list == Some(List::Queue), "and back returns to it");
    app.update(Action::GoForward);
    assert!(app.list != Some(List::Queue), "forward steps out again");
    app.update(Action::GoBack);

    // The search box is not a level: it opens over the queue and
    // closes back onto it.
    app.update(Action::BeginSearch);
    assert!(app.search.is_some(), "the box is open");
    assert!(
        matches!(app.showing(), Showing::Search(_)),
        "and is what the keys drive"
    );
    app.update(Action::CloseSearch);
    assert!(
        app.list == Some(List::Queue) && app.search.is_none(),
        "the queue is back"
    );

    // The history over the queue is a level of its own, and back from
    // it is the queue.
    key(&mut app, KeyCode::Char('H'));
    assert!(
        app.list == Some(List::History) && app.list != Some(List::Queue),
        "one list at a time"
    );
    app.update(Action::GoBack);
    assert!(app.list == Some(List::Queue) && app.list != Some(List::History));
}

#[test]
fn opening_from_the_queue_draws_the_wait_over_it() {
    // Reported: no loading page after `a` or `c`. From the queue, the
    // key asks for the track first, and until the track came back
    // nothing on screen changed. The wait is drawn from the key on,
    // headed by the name the row knows.
    let mut app = signed_in(sidebar::Section::Music);
    app.queue.play_next(crate::domain::Track {
        id: crate::domain::TrackId::from(3),
        ..crate::domain::Track::sample(
            "A Good Day",
            "Anderson .Paak, Cordae",
            std::time::Duration::from_secs(1),
        )
    });
    key(&mut app, KeyCode::Char('Q'));
    app.queue_list.selected = 0;
    let fetch = key(&mut app, KeyCode::Char('a')).expect("a from the queue");
    assert!(
        matches!(fetch, Action::FetchTrackArtist(_)),
        "no id in hand: fetched"
    );
    app.update(fetch);
    assert!(app.list != Some(List::Queue), "the wait took the pane from the queue");

    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let text = geometry::text(terminal.backend().buffer());
    assert!(
        text.contains("Anderson .Paak"),
        "headed by the artist:\n{text}"
    );
    assert!(
        text.contains("Loading"),
        "and says it is on its way:\n{text}"
    );
    assert!(
        !text.contains("A Good Day"),
        "the queue's rows are gone:\n{text}"
    );

    // And back from the wait is the queue.
    app.update(Action::GoBack);
    assert!(app.list == Some(List::Queue));
}

#[test]
fn playing_from_a_row_of_track_cards_queues_the_row() {
    // Reported: a track played from "New Tracks" on the home page, and
    // the queue showed nothing. The list a track is played from is what
    // the queue is filled with, and a row of cards was not one of the
    // lists this knew -- so the track played alone.
    let mut app = signed_in(sidebar::Section::Music);
    app.last_main_width = 120;
    let mut cards: Vec<carousel::Card> = (1..=3)
        .map(|i| {
            let mut c = carousel::Card::new(format!("Song {i}"), "Someone");
            c.target = Some(carousel::Target::Track(i.to_string()));
            c
        })
        .collect();
    // A card that is not a track in the middle of them: it is skipped,
    // and the place of the one after it is counted among the tracks.
    let mut album = carousel::Card::new("A record", "Someone");
    album.target = Some(carousel::Target::Album("9".into()));
    cards.insert(2, album);
    app.home.rows = vec![home::Row {
        heading: "New Tracks".into(),
        kind: crate::browse::RowKind::Compact,
        cards,
        state: carousel::CarouselState {
            selected: 3,
            ..Default::default()
        },
    }];
    app.home.row = 0;

    let (list, at, named) = app.selected_list();
    assert_eq!(
        list.iter().map(|t| t.id.0.clone()).collect::<Vec<_>>(),
        vec!["1", "2", "3"],
        "the row's tracks, and only its tracks"
    );
    assert_eq!(at, 2, "the third track, not the fourth card");
    assert_eq!(named.as_deref(), Some("New Tracks"), "named after the row");

    // On the card that is not a track there is nothing to play.
    app.home.rows[0].state.selected = 2;
    assert!(app.selected_list().0.is_empty());
}

#[test]
fn an_album_opened_from_the_queue_is_a_step_forward_from_it() {
    let mut app = signed_in(sidebar::Section::Music);
    app.queue.play_next(crate::domain::Track {
        id: crate::domain::TrackId::from(7),
        album: "Q".into(),
        album_id: Some("9".into()),
        ..crate::domain::Track::sample("Queued", "S", std::time::Duration::from_secs(1))
    });
    key(&mut app, KeyCode::Char('Q'));
    app.queue_list.selected = 0;
    let open = key(&mut app, KeyCode::Char('c')).expect("c from the queue");
    app.update(open);
    assert!(app.list != Some(List::Queue), "the album took the pane");
    assert_eq!(app.open.as_ref().map(|o| o.title.as_str()), Some("Q"));
    app.update(Action::GoBack);
    assert!(app.list == Some(List::Queue), "and back is the queue");
}

#[test]
fn the_history_is_a_list_the_keys_work_in() {
    // The same rewrite as the queue's: it closed on any key, so nothing
    // could be done with a played track from where it was listed.
    let mut app = signed_in(sidebar::Section::Music);
    key(&mut app, KeyCode::Char('H'));
    assert!(app.list == Some(List::History), "H opens the history");
    assert!(matches!(
        key(&mut app, KeyCode::Char('z')),
        Some(Action::ToggleShuffle)
    ));
    assert!(app.list == Some(List::History), "shuffle is not a way out");
    assert!(
        key(&mut app, KeyCode::Enter).is_none(),
        "nothing played, nothing to queue"
    );
    assert!(app.list == Some(List::History));
    let _ = app.take_repaint();
    let back = key(&mut app, KeyCode::Esc).expect("escape steps back");
    app.update(back);
    assert!(app.list != Some(List::History), "escape leaves it");
    assert!(app.take_repaint(), "and asks for the covers under it back");

    // One list at a time: opening one is a step forward from the other.
    key(&mut app, KeyCode::Char('H'));
    key(&mut app, KeyCode::Char('Q'));
    assert!(app.list == Some(List::Queue) && app.list != Some(List::History));
}

#[test]
fn f_favourites_the_album_playlist_or_mix_under_the_cursor_or_whose_view_is_up() {
    // An album card in the Albums section.
    let mut app = signed_in(sidebar::Section::Albums);
    app.albums = vec![crate::library::Album {
        id: (9).to_string(),
        title: "Discovery".into(),
        artist: "Daft Punk".into(),
        year: None,
        cover: None,
        track_count: 14,
        duration: None,
    }];
    assert_eq!(
        app.selected_favourable(),
        Some((Favourable::Album("9".into()), "Discovery".to_string()))
    );
    assert!(!app.is_favourite(&Favourable::Album("9".into())));
    app.update(Action::FollowChanged {
        what: Favourable::Album("9".into()),
        name: "Discovery".into(),
        following: true,
    });
    assert!(app.is_favourite(&Favourable::Album("9".into())));
    assert_eq!(app.status.as_deref(), Some("Added Discovery to favourites"));

    // The album's own view, opened from its card: the target travels
    // with the view, so F there means the album.
    let identity = app.selected_identity();
    app.open_view(identity, Vec::new());
    app.albums.clear();
    app.tracks = some_tracks(&["One More Time"]);
    assert_eq!(
        app.selected_favourable(),
        Some((Favourable::Album("9".into()), "Discovery".to_string()))
    );

    // The startup ids fill the sets, and the mixes reply the saved mixes.
    app.update(Action::FavouriteIdsLoaded(crate::library::FavouriteIds {
        albums: vec!["1".into(), "2".into()],
        playlists: vec!["u-1".into()],
        artists: vec!["7".into()],
    }));
    assert!(app.is_favourite(&Favourable::Album("2".into())));
    assert!(app.is_favourite(&Favourable::Playlist("u-1".into())));
    assert!(app.is_favourite(&Favourable::Artist("7".into())));
}

#[test]
fn the_reply_marks_the_track_it_was_for() {
    // The mark follows the account, not what was guessed when the key
    // was pressed.
    let mut app = signed_in(sidebar::Section::Tracks);
    app.tracks = some_tracks(&["One", "Two"]);
    let id = app.tracks[0].id.clone();

    app.update(Action::FavouriteChanged {
        id: id.clone(),
        favourite: true,
    });
    assert!(app.favourites.tracks.contains(&id), "added");

    app.update(Action::FavouriteChanged {
        id: id.clone(),
        favourite: false,
    });
    assert!(!app.favourites.tracks.contains(&id), "and taken out again");
}

#[test]
fn a_late_reply_marks_its_own_track_not_the_selected_one() {
    // Pressing A and moving on before the reply lands must not mark
    // whatever is selected by the time it arrives.
    let mut app = signed_in(sidebar::Section::Tracks);
    app.tracks = (0..3)
        .map(|i| {
            let mut t = crate::domain::Track::sample(
                &format!("Track {i}"),
                "An Artist",
                std::time::Duration::from_secs(200),
            );
            t.id = crate::domain::TrackId::from(i as u64);
            t
        })
        .collect();

    // The reply is for the first track; the user has since moved to the
    // third.
    app.update(Action::TrackNext);
    app.update(Action::TrackNext);
    app.update(Action::FavouriteChanged {
        id: crate::domain::TrackId::from(0),
        favourite: true,
    });

    assert!(
        app.favourites.tracks.contains(&crate::domain::TrackId::from(0)),
        "the track the reply was for"
    );
    assert!(
        !app.favourites.tracks.contains(&crate::domain::TrackId::from(2)),
        "and not the one now selected"
    );
}

#[test]
fn the_playlist_keys_open_their_prompts_and_x_takes_a_track_out() {
    let mut app = signed_in(sidebar::Section::Tracks);
    app.tracks = vec![crate::domain::Track::sample(
        "A Song",
        "Someone",
        std::time::Duration::ZERO,
    )];
    app.playlists = vec![
        owned_playlist("mine", "Mine", true),
        owned_playlist("theirs", "Theirs", false),
    ];

    assert!(key(&mut app, KeyCode::Char('P')).is_none());
    assert!(
        matches!(&app.prompt, Some(playlistedit::Prompt::AddTo { title, .. }) if title == "A Song")
    );
    // The prompt has the keyboard: q is a letter in a name, Esc closes.
    assert!(key(&mut app, KeyCode::Enter).is_none());
    assert!(key(&mut app, KeyCode::Char('q')).is_none());
    assert!(
        matches!(&app.prompt, Some(playlistedit::Prompt::AddTo { naming: Some(n), .. }) if n == "q")
    );
    match key(&mut app, KeyCode::Enter) {
        Some(Action::Edit(playlistedit::Edit::Create { name, then_add })) => {
            assert_eq!(name, "q");
            assert!(then_add.is_some());
        }
        other => panic!("expected a create, got {other:?}"),
    }
    assert!(app.prompt.is_none(), "the prompt closes on its answer");

    key(&mut app, KeyCode::Char('N'));
    assert!(matches!(
        app.prompt,
        Some(playlistedit::Prompt::NewPlaylist { .. })
    ));
    key(&mut app, KeyCode::Esc);
    assert!(app.prompt.is_none());

    // m and D act on the open playlist, and only on the account's own.
    with_open_playlist(&mut app, "theirs");
    assert!(key(&mut app, KeyCode::Char('m')).is_none());
    assert!(app.prompt.is_none(), "not yours: no prompt");
    assert!(key(&mut app, KeyCode::Char('x')).is_none(), "nor a removal");
    with_open_playlist(&mut app, "mine");
    key(&mut app, KeyCode::Char('m'));
    assert!(
        matches!(&app.prompt, Some(playlistedit::Prompt::Rename { uuid, name }) if uuid == "mine" && name == "Mine")
    );
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Char('D'));
    assert!(
        matches!(&app.prompt, Some(playlistedit::Prompt::Delete { uuid, .. }) if uuid == "mine")
    );
    key(&mut app, KeyCode::Char('n'));
    assert!(app.prompt.is_none(), "anything but y keeps it");
    assert!(matches!(
        key(&mut app, KeyCode::Char('x')),
        Some(Action::RemoveFromPlaylist { uuid, .. }) if uuid == "mine"
    ));

    // A deleted playlist that was open closes; a renamed one shows
    // its new name in the heading, from the reloaded list.
    app.update(Action::PlaylistsLoaded(vec![owned_playlist(
        "mine", "Mine 2", true,
    )]));
    let next = app.update(Action::PlaylistEdited {
        message: "renamed".into(),
        uuid: "mine".into(),
        deleted: false,
        count_delta: 0,
    });
    assert!(next.is_none());
    assert_eq!(app.open.as_ref().map(|o| o.title.as_str()), Some("Mine 2"));
    // A track in moves the count in place, no reload needed.
    app.update(Action::PlaylistEdited {
        message: "added".into(),
        uuid: "mine".into(),
        deleted: false,
        count_delta: 1,
    });
    assert_eq!(app.playlists[0].track_count, 4);
    let next = app.update(Action::PlaylistEdited {
        message: "playlist deleted".into(),
        uuid: "mine".into(),
        deleted: true,
        count_delta: 0,
    });
    assert!(matches!(next, Some(Action::CloseCollection)));
}

#[test]
fn the_queue_moves_past_a_blocked_track_when_the_one_before_ends() {
    // AI-generated content turned off, and such a track in the middle of
    // the queue: the queue used to stop dead on it, since the refused
    // start never finished and so never moved the queue on.
    let mut app = signed_in(sidebar::Section::Tracks);
    app.config.playback.ai = false;
    let secs = |n| std::time::Duration::from_secs(n);
    let mut a = crate::domain::Track::sample("Before", "A", secs(1));
    a.id = crate::domain::TrackId::from(1);
    let mut b = crate::domain::Track::sample("Generated", "B", secs(1));
    b.id = crate::domain::TrackId::from(2);
    b.ai = true;
    let mut c = crate::domain::Track::sample("After", "C", secs(1));
    c.id = crate::domain::TrackId::from(3);
    app.queue
        .start_context(vec![a.clone(), b.clone(), c], 0, None);
    app.now_playing.track = Some(a);

    let next = app.update(Action::Playback(crate::playback::PlaybackEvent::Finished));
    assert!(matches!(next, Some(Action::PlayQueued)), "got {next:?}");
    assert_eq!(
        app.queue.current().map(|t| t.title.as_str()),
        Some("After"),
        "the blocked track is passed over"
    );
    assert!(
        app.status
            .as_ref()
            .is_some_and(|n| n.contains("AI-generated")),
        "and the user is told why: {:?}",
        app.status
    );

    // Nothing allowed is left: the queue ends, as it would without them.
    // Without autoplay, which would otherwise follow the end of it.
    app.config.playback.autoplay = false;
    let mut d = crate::domain::Track::sample("Also generated", "D", secs(1));
    d.id = crate::domain::TrackId::from(4);
    d.ai = true;
    app.queue.start_context(vec![b, d], 0, None);
    let next = app.update(Action::Playback(crate::playback::PlaybackEvent::Finished));
    assert!(next.is_none(), "got {next:?}");
    assert!(!app.now_playing.playing);
}

#[test]
fn f_on_an_open_radio_saves_the_playlist_it_is() {
    let mut app = signed_in(sidebar::Section::Tracks);
    let mut seed = crate::domain::Track::sample("Seed", "A", std::time::Duration::from_secs(1));
    seed.id = crate::domain::TrackId::from(1);
    app.now_playing.track = Some(seed);
    app.update(Action::PlayTrackRadio);
    assert_eq!(
        app.selected_favourable(),
        None,
        "not named yet: F has nothing to save"
    );

    app.update(Action::RadioNamed {
        seed: crate::domain::TrackId::from(1),
        playlist: "mix:0017".into(),
    });
    assert_eq!(
        app.selected_favourable(),
        Some((
            Favourable::Playlist("mix:0017".into()),
            "Seed Radio".to_string()
        ))
    );
    assert!(matches!(
        key(&mut app, KeyCode::Char('F')),
        Some(Action::ToggleFollow)
    ));
}

#[test]
fn r_does_nothing_inside_the_radio_it_would_open() {
    // In a radio, the rows carry their radios, and the seed's is the
    // one on screen: R over it opened the same radio over itself,
    // reloading the list and losing the place in it.
    let mut app = signed_in(sidebar::Section::Tracks);
    let secs = |n| std::time::Duration::from_secs(n);
    let mut seed = crate::domain::Track::sample("Seed", "A", secs(1));
    seed.id = crate::domain::TrackId::from(1);
    app.queue.start_context(vec![seed.clone()], 0, None);
    app.now_playing.track = Some(seed.clone());
    app.update(Action::PlayTrackRadio);
    assert!(
        app.radio_is_open(&crate::domain::TrackId::from(1)),
        "the radio is up"
    );

    let mut other = crate::domain::Track::sample("Other", "B", secs(1));
    other.id = crate::domain::TrackId::from(2);
    app.update(Action::CollectionLoaded {
        request: open_request(&app),
        tracks: vec![seed, other],
    });
    assert_eq!(app.track_radio(), Some(crate::domain::TrackId::from(1)));
    assert!(
        key(&mut app, KeyCode::Char('R')).is_none(),
        "R over the seed of the open radio does nothing"
    );
    app.tracklist.selected = 1;
    assert!(
        matches!(
            key(&mut app, KeyCode::Char('R')),
            Some(Action::PlayTrackRadio)
        ),
        "another row's radio still opens"
    );
}

#[test]
fn l_opens_the_lyrics_and_asks_for_the_playing_tracks_words_once() {
    let mut app = signed_in(sidebar::Section::Tracks);
    let track = crate::domain::Track::sample("Song", "Someone", std::time::Duration::from_secs(90));
    app.now_playing.track = Some(track.clone());

    let action = key(&mut app, KeyCode::Char('L')).expect("L is bound");
    assert!(matches!(action, Action::ToggleLyrics));
    let asked = app.update(action);
    assert!(
        matches!(asked, Some(Action::LoadLyrics(ref id)) if *id == track.id),
        "opening asks for the words, got {asked:?}"
    );
    assert!(app.lyrics.open);

    // Keys over the pane: j and k scroll it, space still pauses.
    assert!(key(&mut app, KeyCode::Char('j')).is_none());
    assert_eq!(app.lyrics.scroll, 1);
    assert!(matches!(
        key(&mut app, KeyCode::Char(' ')),
        Some(Action::TogglePause)
    ));

    app.update(Action::LyricsLoaded {
        id: track.id.clone(),
        found: lyricsview::Found::Nothing,
    });
    // The same track starting again is not searched again.
    let again = app.update(Action::Playback(crate::playback::PlaybackEvent::Started {
        bit_depth: None,
        sample_rate: 44_100,
        delivered: crate::domain::Quality::Lossless,
    }));
    assert!(again.is_none(), "got {again:?}");
    assert!(
        app.lyrics.following,
        "a track starting follows from the top"
    );

    assert!(matches!(
        key(&mut app, KeyCode::Esc),
        Some(Action::ToggleLyrics)
    ));
    app.update(Action::ToggleLyrics);
    assert!(!app.lyrics.open);
}

#[test]
fn a_track_that_played_long_enough_becomes_a_listen_in_the_outbox() {
    let mut app = signed_in(sidebar::Section::Tracks);
    let started = |app: &mut App, title: &str| {
        app.now_playing.track = Some(crate::domain::Track::sample(
            title,
            "Someone",
            std::time::Duration::from_secs(200),
        ));
        app.update(Action::Playback(crate::playback::PlaybackEvent::Started {
            bit_depth: None,
            sample_rate: 44_100,
            delivered: crate::domain::Quality::Lossless,
        }));
    };
    started(&mut app, "First");
    assert_eq!(
        app.scrobble_outbox
            .now_playing
            .as_ref()
            .map(|p| p.title.as_str()),
        Some("First"),
        "the services hear what started"
    );
    assert!(app.listen.is_some());

    // Skipped after twenty seconds: not a listen.
    app.scrobble_outbox.now_playing = None;
    app.now_playing.position = std::time::Duration::from_secs(20);
    started(&mut app, "Second");
    assert!(
        app.scrobble_outbox.finished.is_empty(),
        "twenty seconds of two hundred"
    );
    assert_eq!(
        app.scrobble_outbox
            .now_playing
            .as_ref()
            .map(|p| p.title.as_str()),
        Some("Second")
    );

    // Played to the end: a listen.
    app.update(Action::Playback(crate::playback::PlaybackEvent::Finished));
    assert_eq!(app.scrobble_outbox.finished.len(), 1);
    assert_eq!(app.scrobble_outbox.finished[0].title, "Second");
    assert!(app.listen.is_none(), "and nothing is in progress");

    // A track that failed to play is no listen either.
    started(&mut app, "Third");
    app.update(Action::Playback(crate::playback::PlaybackEvent::Failed {
        id: crate::domain::TrackId::from(1),
        message: "no".into(),
    }));
    assert!(app.listen.is_none());
}

#[test]
fn s_steps_the_sleep_timer_through_its_stops_and_round_to_off() {
    let mut app = signed_in(sidebar::Section::Tracks);
    assert!(matches!(
        key(&mut app, KeyCode::Char('S')),
        Some(Action::CycleSleep)
    ));
    let mut seen = Vec::new();
    for _ in 0..6 {
        app.update(Action::CycleSleep);
        seen.push(match app.sleep {
            Some(Sleep::At { minutes, .. }) => minutes.to_string(),
            Some(Sleep::EndOfTrack) => "end".to_string(),
            None => "off".to_string(),
        });
    }
    assert_eq!(seen, ["15", "30", "45", "60", "end", "off"]);
    assert_eq!(
        app.status.as_deref(),
        Some("sleep timer off"),
        "each step says what it set"
    );
}

#[test]
fn the_sleep_timer_pauses_once_when_it_is_due() {
    let mut app = signed_in(sidebar::Section::Tracks);
    app.now_playing.playing = true;
    app.sleep = Some(Sleep::At {
        when: std::time::Instant::now() - std::time::Duration::from_secs(1),
        minutes: 15,
    });
    assert!(matches!(
        app.update(Action::Tick),
        Some(Action::TogglePause)
    ));
    assert!(app.sleep.is_none(), "and is forgotten");
    assert!(app.update(Action::Tick).is_none(), "once");

    // Already paused when it goes off: nothing to do.
    app.now_playing.playing = false;
    app.sleep = Some(Sleep::At {
        when: std::time::Instant::now(),
        minutes: 15,
    });
    assert!(app.update(Action::Tick).is_none());
}

#[test]
fn sleep_at_the_end_of_the_track_pauses_there_and_preloads_nothing() {
    let mut app = signed_in(sidebar::Section::Tracks);
    app.queue = crate::playback::Queue::new(some_tracks(&["One", "Two"]), 0);
    app.now_playing.track = app.queue.current().cloned();
    app.now_playing.playing = true;
    let id = app.now_playing.track.as_ref().unwrap().id.clone();

    // Without the timer, the download done is the moment to fetch the next.
    let next = app.update(Action::Playback(
        crate::playback::PlaybackEvent::Downloaded(id.clone()),
    ));
    assert!(
        matches!(next, Some(Action::Preload(ref n)) if *n == app.queue.upcoming().unwrap().id),
        "got {next:?}"
    );

    app.sleep = Some(Sleep::EndOfTrack);
    assert!(
        app.update(Action::Playback(
            crate::playback::PlaybackEvent::Downloaded(id)
        ))
        .is_none(),
        "nothing lined up behind a track the music stops after"
    );
    let next = app.update(Action::Playback(crate::playback::PlaybackEvent::Finished));
    assert!(matches!(next, Some(Action::TogglePause)), "got {next:?}");
    assert!(app.sleep.is_none());
    assert_eq!(
        app.queue.current().unwrap().title,
        "One",
        "the queue did not move on"
    );
}
