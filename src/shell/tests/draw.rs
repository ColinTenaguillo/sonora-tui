use super::*;

#[test]
fn a_rule_separates_the_sidebar_from_the_main_pane() {
    // The two panes abutted, so a card's left edge sat directly against
    // the nav's text.
    let mut app = signed_in(sidebar::Section::Tracks);
    app.tracks = some_tracks(&["One", "Two"]);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let buf = terminal.backend().buffer().clone();

    let regions = layout::split(ratatui::layout::Rect::new(0, 0, 100, 30));
    let x = regions.divider.x;
    let painted = (regions.divider.y..regions.divider.bottom())
        .filter(|y| buf[(x, *y)].symbol() != " ")
        .count();
    assert_eq!(
        painted,
        regions.divider.height as usize,
        "the rule runs the height of the panes\n{}",
        geometry::text(&buf)
    );
}

#[test]
fn the_scrollbar_reaches_the_bottom_of_the_pane() {
    // The pane ends where the now-playing bar begins; the bar has to
    // run all the way to that edge rather than stopping short of it.
    let mut app = signed_in(sidebar::Section::Tracks);
    app.tracks = (0..40)
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

    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let buf = terminal.backend().buffer().clone();

    let regions = layout::split(ratatui::layout::Rect::new(0, 0, 100, 30));
    let x = regions.main.x + regions.main.width - scrollbar::WIDTH;
    let last = regions.main.y + regions.main.height - 1;
    assert_ne!(
        buf[(x, last)].symbol(),
        " ",
        "the bar reaches the last row of the pane\n{}",
        geometry::text(&buf)
    );
}

#[test]
fn the_event_loop_clears_before_the_frame_that_needs_it() {
    // The flag is only worth setting if the loop acts on it. The loop
    // itself needs a terminal and a signed-in session to run, so this
    // reads the source: what must hold is that `take_repaint` is asked
    // before the draw and that a clear follows from it.
    let source = include_str!("../mod.rs");
    let loop_body = source
        .split("while !app.should_quit {")
        .nth(1)
        .expect("the event loop");
    let take = loop_body.find("take_repaint()").expect("the flag is read");
    let clear = loop_body
        .find(".clear_region(ratatui::backend::ClearType::All)")
        .expect("and acted on");
    let draw = loop_body.find("terminal.draw(").expect("before the draw");
    assert!(take < clear, "the clear follows from the flag");
    assert!(clear < draw, "and happens before the frame it is for");
}

#[test]
fn a_status_message_sits_in_the_corner() {
    // A transient notice shares the top line with the first row's
    // heading. It used to take the whole line, which hid the heading
    // for as long as the message stood. It sits in the corner instead.
    let mut app = signed_in(sidebar::Section::Music);
    app.home.rows = vec![home::Row {
        kind: crate::browse::RowKind::Carousel,
        heading: "The Hits".into(),
        cards: vec![carousel::Card::new("A Card", "An Artist")],
        state: carousel::CarouselState::default(),
    }];
    app.status = Some("Network error".into());

    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
    terminal.draw(|f| draw(f, &mut app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    let top = geometry::row(&buf, 0);

    assert!(
        top.contains("Network error"),
        "the message is shown: {top:?}"
    );
    assert!(
        top.contains("The Hits"),
        "and the heading is still readable beside it: {top:?}"
    );

    let notice = geometry::find(&buf, "Network error").expect("the notice");
    let heading = geometry::find(&buf, "The Hits").expect("the heading");
    assert!(
        notice.start > heading.end,
        "the notice sits to the right of the heading: {top:?}"
    );
}

#[test]
fn a_theme_is_what_the_screen_is_actually_painted_in() {
    // The palette could be read from the config correctly and still
    // never reach the buffer -- a renderer holding its own colours
    // would look right in every unit test of the parser.
    use crate::shell::theme;
    let mut app = signed_in(sidebar::Section::Tracks);
    app.palette = theme::named("catppuccin").expect("the theme");
    app.update(Action::TracksLoaded(some_tracks(&["A Track", "Another"])));

    let buf = geometry::draw(100, 20, |f, _a, _p| draw(f, &mut app));
    let mut drawn = std::collections::HashSet::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            drawn.insert(buf[(x, y)].fg);
        }
    }

    assert!(
        drawn.contains(&app.palette.text),
        "the theme's own text colour is on the screen: {drawn:?}"
    );
    // And nothing of the default palette is left behind it.
    assert!(
        !drawn.contains(&theme::Palette::detect().dim),
        "no colour from the palette it replaced: {drawn:?}"
    );
}

#[test]
fn a_notice_comes_down_on_its_own() {
    // It never did: nothing cleared the status but a fresh login, so a
    // message about one failed track stood in the corner -- over the
    // tab strip -- for the rest of the session.
    let mut app = signed_in(sidebar::Section::Music);
    app.say("could not play that");
    assert_eq!(app.status.as_deref(), Some("could not play that"));

    // A tick straight away leaves it up: it has to be readable.
    app.update(Action::Tick);
    assert!(app.status.is_some(), "still up a moment later");

    // Old enough, and the next tick takes it down.
    app.status_since = Some(std::time::Instant::now() - std::time::Duration::from_secs(30));
    app.update(Action::Tick);
    assert!(app.status.is_none(), "gone once it has been read");
    assert!(app.status_since.is_none(), "and its clock with it");
}

#[test]
fn a_second_notice_gets_the_whole_time_to_itself() {
    // The clock is the message's, not the app's: a new one arriving
    // late in the last one's life would otherwise vanish immediately.
    let mut app = signed_in(sidebar::Section::Music);
    app.say("the first");
    app.status_since = Some(std::time::Instant::now() - std::time::Duration::from_secs(30));

    app.say("the second");
    app.update(Action::Tick);
    assert_eq!(
        app.status.as_deref(),
        Some("the second"),
        "the new message starts its own clock"
    );
}
