use super::*;

#[test]
fn r_does_nothing_where_there_is_no_track() {
    // It is repeat's own key in capitals, so a stray press with no
    // track selected and nothing playing falls through rather than
    // opening an empty view.
    let mut app = signed_in(sidebar::Section::Profiles);
    assert!(app.track_radio().is_none());
    assert!(key(&mut app, KeyCode::Char('R')).is_none());
}

#[test]
fn h_does_not_shut_a_see_all_that_is_still_arriving() {
    // The window between pressing `o` and the reply landing: the view
    // is open and carries no cards, which reads as a track list. `h`
    // shut it there -- and that is the whole of the time the user is
    // waiting, so the key appeared to close the page at random.
    // Opened by the key rather than by hand: what `o` builds is the
    // thing under test, and a view assembled here would not have caught
    // this.
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
    if let Some(action) = app.on_key(crossterm::event::KeyEvent::from(KeyCode::Char('o'))) {
        app.update(action);
    }
    assert!(app.open.is_some(), "the see-all opened");

    // The waiting state a fetched see-all sits in: no cards yet.
    app.open_cards.clear();
    app.awaiting = true;

    let acted = app.on_key(crossterm::event::KeyEvent::from(KeyCode::Char('h')));
    assert!(
        !matches!(acted, Some(Action::CloseCollection)),
        "h left the view alone while it was loading, got {acted:?}"
    );
    assert!(app.open.is_some(), "and it is still open");
}
