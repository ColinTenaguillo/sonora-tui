use super::*;

fn playing(section: sidebar::Section) -> App {
    let mut app = signed_in(section);
    app.tracks = some_tracks(&["Hidden Row"]);
    app.now_playing.track = Some(crate::domain::Track::sample(
        "The Boat I Row",
        "Tame Impala",
        std::time::Duration::from_secs(238),
    ));
    app.now_playing.quality = Some("24-bit 44.1kHz".into());
    app
}

fn frame(app: &mut App) -> ratatui::buffer::Buffer {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 40)).unwrap();
    terminal.draw(|f| draw(f, app)).unwrap();
    terminal.backend().buffer().clone()
}

#[test]
fn z_opens_zen_and_the_same_key_closes_it_back_onto_the_view() {
    let mut app = playing(sidebar::Section::Tracks);
    let before = app.showing();
    assert!(key(&mut app, KeyCode::Char('Z')).is_none());
    assert_eq!(app.showing(), Showing::Zen);

    let back = key(&mut app, KeyCode::Char('Z')).expect("Z leaves zen");
    app.update(back);
    assert_eq!(app.showing(), before);
}

#[test]
fn escape_leaves_zen_like_any_other_view() {
    let mut app = playing(sidebar::Section::Tracks);
    key(&mut app, KeyCode::Char('Z'));
    let back = key(&mut app, KeyCode::Esc).expect("esc leaves zen");
    app.update(back);
    assert_ne!(app.showing(), Showing::Zen);
}

#[test]
fn zen_draws_the_playing_track_centred_with_no_sidebar_and_no_hidden_view() {
    let mut app = playing(sidebar::Section::Tracks);
    key(&mut app, KeyCode::Char('Z'));
    let buf = frame(&mut app);
    let text = geometry::text(&buf);

    assert!(!text.contains("Hidden Row"), "the view under zen:\n{text}");
    assert!(
        !text.contains(sidebar::Section::Tracks.label()),
        "the sidebar:\n{text}"
    );

    let bar_top = layout::split(buf.area).now_playing.y;
    let title = geometry::find(&buf, "The Boat I Row").expect("the title");
    assert!(title.row < bar_top, "the title above the bar:\n{text}");
    geometry::assert_centred(title, &buf);
    let artist = geometry::find(&buf, "Tame Impala").expect("the artist");
    assert!(artist.row > title.row && artist.row < bar_top);
    geometry::assert_centred(artist, &buf);
    // And the bar is still there, its time and all.
    assert!(text.contains("3:58"), "the bar:\n{text}");
    // Without the names a second time: they are above it.
    let names = text.matches("The Boat I Row").count();
    assert_eq!(names, 1, "the title once:\n{text}");
    // Nor the rule above it, nor the quality badge and the spectrum.
    assert!(
        !geometry::row(&buf, bar_top).contains('─'),
        "the rule:\n{text}"
    );
    assert!(!text.contains("44.1"), "the quality:\n{text}");
}

#[test]
fn zen_with_nothing_playing_says_so() {
    let mut app = signed_in(sidebar::Section::Tracks);
    key(&mut app, KeyCode::Char('Z'));
    let text = geometry::text(&frame(&mut app));
    assert!(text.contains("Nothing playing"), "{text}");
}

#[test]
fn in_zen_the_track_keys_act_on_the_playing_track() {
    // What the user sees is what `A`, `c` and `a` act on: the hidden
    // list's selection is not on screen.
    let mut app = playing(sidebar::Section::Tracks);
    key(&mut app, KeyCode::Char('Z'));
    assert_eq!(
        app.selected_track().map(|t| t.title),
        Some("The Boat I Row".to_string())
    );
    assert!(matches!(
        key(&mut app, KeyCode::Char('A')),
        Some(Action::ToggleFavourite)
    ));
}

#[test]
fn z_needs_a_session() {
    let mut app = App::default();
    assert!(key(&mut app, KeyCode::Char('Z')).is_none());
    assert_ne!(app.showing(), Showing::Zen);
}

#[test]
fn in_zen_no_hidden_card_is_selected() {
    // `F` and the rest act on the selected card: in zen there is none.
    let mut app = with_albums(3);
    assert!(app.selected_card().is_some(), "the grid has one");
    app.now_playing = playing(sidebar::Section::Albums).now_playing;
    key(&mut app, KeyCode::Char('Z'));
    assert!(app.selected_card().is_none());
}

#[test]
fn in_zen_the_movement_keys_reach_nothing_hidden() {
    let mut app = playing(sidebar::Section::Tracks);
    key(&mut app, KeyCode::Char('Z'));
    for code in [KeyCode::Enter, KeyCode::Char('j'), KeyCode::Char('k')] {
        assert!(key(&mut app, code).is_none(), "{code:?}");
    }
    assert_eq!(app.tracklist.selected, 0);
    // The transport still works.
    assert!(matches!(
        key(&mut app, KeyCode::Char(' ')),
        Some(Action::TogglePause)
    ));
}

#[test]
fn the_lyrics_open_over_the_cover_and_ask_for_a_redraw_not_a_wipe() {
    // A wipe blanked the screen and the terminal took a moment to put the
    // large cover back. A redraw writes every cell again over what is up.
    let mut app = playing(sidebar::Section::Tracks);
    key(&mut app, KeyCode::Char('Z'));
    frame(&mut app);
    app.take_repaint();

    let open = key(&mut app, KeyCode::Char('L')).expect("L opens the lyrics");
    app.update(open);
    assert_eq!(app.take_repaint(), Repaint::Redraw);
    let with = frame(&mut app);
    assert!(geometry::text(&with).contains("Looking for the words"));
    let (top, bottom) = (
        geometry::find(&with, "╭").expect("the pane's top"),
        geometry::find(&with, "╰").expect("the pane's bottom"),
    );
    let cover = zenview::cover_area(layout::without_sidebar(with.area).main);
    assert!(
        top.row < cover.top() && bottom.row >= cover.bottom(),
        "over the cover, not beside it:\n{}",
        geometry::text(&with)
    );

    let close = key(&mut app, KeyCode::Char('L')).expect("L closes them");
    app.update(close);
    assert_eq!(app.take_repaint(), Repaint::Redraw);
}
