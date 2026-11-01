//! One frame: the sidebar, whichever view the main pane holds, the bar.

use super::*;

/// Whether the account has what a card opens, for the heart on it.
fn liked_in(
    target: &carousel::Target,
    followed: &std::collections::HashSet<String>,
    albums: &std::collections::HashSet<String>,
    playlists: &std::collections::HashSet<String>,
) -> bool {
    match target {
        carousel::Target::Artist(id) => followed.contains(id),
        carousel::Target::Album(id) => albums.contains(id),
        carousel::Target::Playlist(uuid) => playlists.contains(uuid),
        carousel::Target::Track(_) | carousel::Target::Page(_) | carousel::Target::Section(_) => {
            false
        }
    }
}

/// The closure every renderer draws covers through: the artwork store when
/// there is one, nothing drawn when the terminal has none.
fn cover_drawer(
    art: &mut Option<artwork::Artwork>,
) -> impl FnMut(&mut ratatui::Frame, ratatui::layout::Rect, &str, artwork::Shape) -> bool + '_ {
    move |frame, area, url, shape| {
        art.as_mut()
            .is_some_and(|a| a.render_shaped(frame, area, url, shape))
    }
}

/// A line or two in the middle of an otherwise empty pane: loading,
/// nothing here, no such page on this service.
fn notice(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    palette: &theme::Palette,
    lines: Vec<ratatui::text::Line<'static>>,
) {
    let height = (lines.len() as u16).min(area.height);
    frame.render_widget(
        ratatui::widgets::Paragraph::new(lines)
            .style(palette.subtitle())
            .alignment(ratatui::layout::Alignment::Center),
        ratatui::layout::Rect {
            y: area.y + area.height / 3,
            height,
            ..area
        },
    );
}

/// What the Feed page says on a service that has none.
pub(super) fn feed_notice(service: &str) -> Vec<ratatui::text::Line<'static>> {
    vec![
        ratatui::text::Line::from(format!("{service} has no feed.")),
        ratatui::text::Line::from("TIDAL's lists new releases from the artists you follow."),
    ]
}

/// Public so `examples/screenshot.rs` renders what the app actually renders.
/// A preview that assembles the layout itself drifts from the real one, and
/// then it is checking its own copy rather than the UI.
pub fn draw(frame: &mut ratatui::Frame, app: &mut App) {
    let showing = app.showing();
    let regions = match showing {
        Showing::Zen => layout::without_sidebar(frame.area()),
        _ => layout::split(frame.area()),
    };
    let palette = app.palette;
    app.last_main_width = regions.main.width;
    app.last_main_height = regions.main.height;

    // The rule between the panes, drawn before either so neither has to
    // know it is there.
    if regions.divider.width > 0 {
        frame.render_widget(
            ratatui::widgets::Block::default()
                .borders(ratatui::widgets::Borders::LEFT)
                .border_style(palette.rule()),
            regions.divider,
        );
    }

    sidebar::render(
        frame,
        regions.sidebar,
        &palette,
        &app.sidebar,
        // The nav is never the focused pane: J and K move it from
        // wherever the user is, and nothing else drives it, so a highlight
        // that says otherwise points at a mode that does not exist.
        false,
    );

    draw_main(frame, app, showing, regions.main);
    draw_bar(frame, app, showing, regions.now_playing);
    draw_status(frame, app, regions.main);
    draw_overlays(frame, app, regions.main);
}

/// The heart test the renderers are handed. Borrowed by field, so the
/// closure lives beside the artwork being taken out and put back.
fn liked(favourites: &Favourites) -> impl Fn(&carousel::Target) -> bool + '_ {
    move |t| {
        liked_in(
            t,
            &favourites.artists,
            &favourites.albums,
            &favourites.playlists,
        )
    }
}

/// The main pane always has the keys. Tab used to move a highlight onto
/// the nav, where no key did anything — a mode with nothing behind it.
const MAIN_FOCUSED: bool = true;

fn draw_main(
    frame: &mut ratatui::Frame,
    app: &mut App,
    showing: Showing,
    area: ratatui::layout::Rect,
) {
    // Search takes the pane while it is open, ahead of both an opened album
    // and the sidebar's section — it is where the user is looking. Drawn
    // here rather than with an early return, so the now-playing bar and the
    // status line below still get their turn.
    // Asked once, and every branch below reads it: exactly one view is
    // drawn, the one `showing` names, whatever else is set behind it.
    match showing {
        Showing::Search(_) => draw_search(frame, app, area),
        Showing::Artist => draw_artist(frame, app, area),
        // A view opened but not yet filled: its name and a line saying it is
        // on its way. Drawn here rather than by a section's renderer because
        // what it becomes is not known until the reply lands.
        Showing::Loading => {
            let title = app
                .open
                .as_ref()
                .map(|o| o.title.as_str())
                .unwrap_or_default();
            loadingview::render(frame, area, &app.palette, title);
        }
        Showing::Queue | Showing::History => draw_list(frame, app, showing, area),
        Showing::Zen => draw_zen(frame, app, area),
        // An opened row of covers is a grid, not a track list — Albums is
        // the section whose renderer draws two-line cards.
        Showing::Tracks => draw_section(frame, app, sidebar::Section::Tracks, area),
        Showing::Cards => draw_section(frame, app, sidebar::Section::Albums, area),
        // A page of rows is drawn by the home renderer, on whichever state
        // holds it.
        Showing::Rows => {
            let section = app.sidebar.section();
            draw_section(frame, app, section, area);
        }
        Showing::Section(section) => draw_section(frame, app, section, area),
    }
}

fn draw_search(frame: &mut ratatui::Frame, app: &mut App, area: ratatui::layout::Rect) {
    let Some(search) = app.search.as_ref() else {
        return;
    };
    let liked = liked(&app.favourites);
    // `artwork` needs &mut while the renderer borrows `app`, so take it
    // out for the duration and put it back — as the other views do.
    let mut art = app.artwork.take();
    searchview::render(
        frame,
        area,
        &app.palette,
        searchview::View {
            query: &search.query,
            typing: search.typing,
            pending: search.pending,
            results: &search.results,
            tab: search.tab,
            tracks: &search.tracks,
            // Whichever grid this tab reuses; the others keep their own
            // place for when the user comes back to them.
            grid: match searchview::Tab::from_index(search.tab) {
                searchview::Tab::Albums => &search.albums,
                searchview::Tab::Artists => &search.artists,
                _ => &search.playlists,
            },
            liked: &liked,
            artists: &search.top_artists,
            albums: &search.top_albums,
            top: search.top,
            scroll: search.top_scroll,
            favourites: &app.favourites.tracks,
            playing: app.now_playing.track.as_ref().map(|t| t.id.clone()),
            tier: app.now_playing.tier,
        },
        cover_drawer(&mut art),
    );
    app.artwork = art;
}

fn draw_artist(frame: &mut ratatui::Frame, app: &mut App, area: ratatui::layout::Rect) {
    let Some(page) = app.artist.page.as_ref() else {
        return;
    };
    let liked = liked(&app.favourites);
    let mut art = app.artwork.take();
    artistview::render(
        frame,
        area,
        &app.palette,
        artistview::View {
            page,
            section: artistview::Section::from_index(app.artist.section),
            tracks: &app.artist.tracks,
            filtering: app.filtering,
            rows: &app.artist.rows,
            scroll: app.artist.scroll,
            bio_open: app.artist.bio_open,
            bio_scroll: app.artist.bio_scroll,
            liked: &liked,
            favourites: &app.favourites.tracks,
            playing: app.now_playing.track.as_ref().map(|t| t.id.clone()),
            tier: app.now_playing.tier,
        },
        cover_drawer(&mut art),
    );
    app.artwork = art;
}

fn draw_section(
    frame: &mut ratatui::Frame,
    app: &mut App,
    section: sidebar::Section,
    area: ratatui::layout::Rect,
) {
    match section {
        // Explore is a page of card rows like the home one, so it is drawn
        // by the same renderer on its own state.
        section @ (sidebar::Section::Music | sidebar::Section::Explore) => {
            draw_rows(frame, app, section, area)
        }
        sidebar::Section::Feed => draw_feed(frame, app, area),
        sidebar::Section::Settings => {
            let linked: Vec<(String, String, bool)> = app
                .scrobblers
                .iter()
                .map(|l| (l.service.id().to_string(), l.label(), l.account.enabled))
                .collect();
            settings::render(
                frame,
                area,
                &app.palette,
                &app.config,
                &app.settings,
                MAIN_FOCUSED,
                &linked,
            );
        }
        section @ (sidebar::Section::Playlists
        | sidebar::Section::Albums
        | sidebar::Section::Profiles) => draw_grid(frame, app, section, area),
        _ => draw_tracks(frame, app, area),
    }
}

fn draw_rows(
    frame: &mut ratatui::Frame,
    app: &mut App,
    section: sidebar::Section,
    area: ratatui::layout::Rect,
) {
    let palette = app.palette;
    // `artwork` needs &mut to cache what it decodes, and the closure
    // is handed to a renderer that also borrows `app` — so take it out
    // for the duration and put it back.
    let explore = section == sidebar::Section::Explore;
    let (state, loaded) = match explore {
        true => (&app.explore, app.explore_loaded),
        false => (&app.home, app.home_loaded),
    };
    // An empty page says why rather than staying blank: no such
    // page on this service, the reply still on its way, or
    // nothing in it. No early return: the bar below is drawn
    // after this match.
    if explore && app.explore_empty {
        notice(
            frame,
            area,
            &palette,
            vec![
                ratatui::text::Line::from(format!("{} has no Explore page.", app.service_name())),
                ratatui::text::Line::from("Search (s) still covers its catalogue."),
            ],
        );
    } else if state.rows.is_empty() {
        let line = match loaded {
            false => waiting::animated("Loading…").into_owned(),
            true => format!("{} has nothing to show here yet.", app.service_name()),
        };
        notice(frame, area, &palette, vec![ratatui::text::Line::from(line)]);
    } else {
        let liked = liked(&app.favourites);
        let mut art = app.artwork.take();
        home::render(
            frame,
            area,
            &palette,
            state,
            MAIN_FOCUSED,
            trackgrid::Marks {
                favourites: &app.favourites.tracks,
                playing: app.now_playing.track.as_ref().map(|t| &t.id),
                tier: app.now_playing.tier,
                liked: &liked,
            },
            cover_drawer(&mut art),
        );
        app.artwork = art;
    }
}

fn draw_feed(frame: &mut ratatui::Frame, app: &mut App, area: ratatui::layout::Rect) {
    let palette = app.palette;
    // The Feed is a page of rows like the home page, but headed and
    // filtered like a grid: its chrome is drawn here and what is
    // left of the pane goes to the row renderer. The heading rows
    // come from the grid's own count so the two cannot drift.
    let header = inputbox::render_headed(
        frame,
        scrollbar::reserve(area),
        &palette,
        "Feed",
        None,
        "Filter releases",
        &app.feed.filter,
        app.filtering,
    );
    let body = ratatui::layout::Rect {
        y: area.y + header,
        height: area.height.saturating_sub(header),
        ..area
    };
    // Only TIDAL has one. Blank, the page read as broken; it
    // says what it would show and where it is shown.
    if !app.session.as_ref().is_some_and(|s| s.has_feed()) {
        notice(frame, body, &palette, feed_notice(app.service_name()));
    } else {
        let liked = liked(&app.favourites);
        let mut art = app.artwork.take();
        home::render(
            frame,
            body,
            &palette,
            &app.feed,
            MAIN_FOCUSED,
            trackgrid::Marks {
                favourites: &app.favourites.tracks,
                playing: app.now_playing.track.as_ref().map(|t| &t.id),
                tier: app.now_playing.tier,
                liked: &liked,
            },
            cover_drawer(&mut art),
        );
        app.artwork = art;
    }
}

fn draw_grid(
    frame: &mut ratatui::Frame,
    app: &mut App,
    section: sidebar::Section,
    area: ratatui::layout::Rect,
) {
    // An opened row brings its own cards and its own heading; the
    // sidebar's section only says which renderer draws them.
    let opened = app.open.as_ref().filter(|_| !app.open_cards.is_empty());
    let cards = match opened {
        Some(_) => app.open_cards.clone(),
        None => app.grid_cards(section),
    };
    // Cloned rather than borrowed: `artwork` is taken out below and
    // a live borrow of `app` here would hold across it.
    let state = match opened {
        Some(_) => app.open_grid.clone(),
        None => app.grid_state(section).clone(),
    };
    let visible = grid::filter(&cards, &state.filter);
    // `lines` comes from the same place the movement keys read it,
    // so the two cannot drift: they disagreed by a whole row of
    // cards, and the keys reached ones that were never drawn.
    let lines = app.grid_lines(section);
    let (heading, hint) = match opened {
        Some(open) => (open.title.as_str(), "Filter"),
        None => match section {
            sidebar::Section::Playlists => ("Playlists", "Filter playlists"),
            sidebar::Section::Albums => ("Albums", "Filter albums"),
            _ => ("Profiles", "Filter profiles"),
        },
    };
    let liked = liked(&app.favourites);
    let mut art = app.artwork.take();
    grid::render(
        frame,
        area,
        &app.palette,
        grid::Grid {
            filtering: app.filtering,
            chrome: grid::Chrome::Full,
            heading,
            filter_hint: hint,
            cards: &visible,
            state: &state,
            focused: MAIN_FOCUSED,
            lines,
            tabs: (&[], 0),
            liked: &liked,
        },
        cover_drawer(&mut art),
    );
    app.artwork = art;
}

fn draw_tracks(frame: &mut ratatui::Frame, app: &mut App, area: ratatui::layout::Rect) {
    let visible = tracklist::visible(&app.tracks, &app.tracklist);
    let mut art = app.artwork.take();
    tracklist::render(
        frame,
        area,
        &app.palette,
        tracklist::TrackList {
            filtering: app.filtering,
            favourites: &app.favourites.tracks,
            chrome: tracklist::Chrome::Full,
            tracks: &visible,
            state: &app.tracklist,
            focused: MAIN_FOCUSED,
            playing: app.now_playing.track.as_ref().map(|t| t.id.clone()),
            tier: app.now_playing.tier,
            banner: app.open.as_ref().map(|o| tracklist::Banner {
                title: &o.title,
                subtitle: &o.subtitle,
                detail: &o.detail,
                cover: o.cover.as_deref(),
                round: o.round_cover,
                liked: app.open_favourable().is_some_and(|w| app.is_favourite(&w)),
            }),
        },
        cover_drawer(&mut art),
    );
    app.artwork = art;
}

/// The queue and the history take the pane, as the artist page does,
/// and draw the list the Tracks section draws: a row here is a row like
/// any other, and the keys act on it. They were modals with a list of
/// their own, and a modal that closed on any key was one no key could
/// act in.
fn draw_list(
    frame: &mut ratatui::Frame,
    app: &mut App,
    showing: Showing,
    area: ratatui::layout::Rect,
) {
    let liked = liked(&app.favourites);
    let marks = trackgrid::Marks {
        favourites: &app.favourites.tracks,
        playing: app.now_playing.track.as_ref().map(|t| &t.id),
        tier: app.now_playing.tier,
        liked: &liked,
    };
    let entries = app.queue.entries();
    // The heading says whether the queue has an end: what follows the
    // last row is either silence or more of the same, and the list
    // itself cannot show which.
    let queue_heading = if app.config.playback.autoplay {
        "Queue · autoplay on"
    } else {
        "Queue · autoplay off"
    };
    let (heading, tracks, state): (&str, Vec<&crate::domain::Track>, _) =
        if showing == Showing::Queue {
            (
                queue_heading,
                entries.iter().map(|(t, _)| *t).collect(),
                &app.queue_list,
            )
        } else {
            ("History", app.queue.history().collect(), &app.history_list)
        };
    let mut art = app.artwork.take();
    queueview::render(
        frame,
        area,
        &app.palette,
        heading,
        &tracks,
        state,
        marks,
        cover_drawer(&mut art),
    );
    app.artwork = art;
}

fn draw_zen(frame: &mut ratatui::Frame, app: &mut App, area: ratatui::layout::Rect) {
    let liked = app
        .now_playing
        .track
        .as_ref()
        .is_some_and(|t| app.favourites.tracks.contains(&t.id));
    let mut art = app.artwork.take();
    zenview::render(
        frame,
        area,
        &app.palette,
        app.now_playing.track.as_ref(),
        liked,
        cover_drawer(&mut art),
    );
    app.artwork = art;
}

fn draw_bar(
    frame: &mut ratatui::Frame,
    app: &mut App,
    showing: Showing,
    area: ratatui::layout::Rect,
) {
    // Straight from the queue, so the bar cannot disagree with what
    // actually plays next.
    let modes = nowplaying::Modes {
        shuffled: app.queue.shuffled(),
        repeat: app.queue.repeat,
        sleep: app.sleep_mark(),
        // Only while something plays: a paused track's last frame
        // frozen under the badge read as the music still going.
        spectrum: match (&app.spectrum, app.now_playing.playing) {
            (Some(bands), true) => bands.levels(),
            _ => Vec::new(),
        },
    };
    if showing == Showing::Zen {
        nowplaying::render_transport_only(frame, area, &app.palette, &app.now_playing, modes);
    } else {
        let mut art = app.artwork.take();
        nowplaying::render_with_cover(
            frame,
            area,
            &app.palette,
            &app.now_playing,
            modes,
            cover_drawer(&mut art),
        );
        app.artwork = art;
    }
}

fn draw_status(frame: &mut ratatui::Frame, app: &App, main: ratatui::layout::Rect) {
    if let Some(status) = &app.status {
        // Tucked into the top right rather than laid across the whole line.
        // Taking the line meant covering the tab strip that lives there:
        // drawn over it the two read as one garbled line, and clearing it
        // first hid the tabs for as long as the message stood.
        let width = (status.chars().count() as u16 + 2).min(main.width);
        let notice = ratatui::layout::Rect {
            x: main.x + main.width - width,
            y: main.y,
            width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(Line::styled(
                format!(" {} ", waiting::animated(status)),
                app.palette.notice(),
            )),
            notice,
        );
    }
}

/// What opens over the panes: the sign-in, the DRM notice, the lyrics,
/// the prompts and the help.
fn draw_overlays(frame: &mut ratatui::Frame, app: &mut App, main: ratatui::layout::Rect) {
    let palette = app.palette;
    if app.session.is_none() {
        login::render(
            frame,
            frame.area(),
            &app.login,
            app.service_name(),
            app.guest_allowed(),
        );
    }

    if app.cdm.shown() {
        drmview::render(frame, main, &palette, &app.cdm);
    }

    if app.lyrics.open {
        // Centred on the whole terminal's width, not the main pane's, but
        // above the now-playing bar so the progress stays in sight.
        let across = ratatui::layout::Rect {
            x: frame.area().x,
            width: frame.area().width,
            ..main
        };
        lyricsview::render(
            frame,
            across,
            &palette,
            &app.lyrics,
            app.now_playing.track.as_ref(),
            app.now_playing.position,
        );
    }

    // Last, so it covers whatever is underneath.
    if let Some(prompt) = &app.prompt {
        let owned: Vec<crate::library::Playlist> =
            app.playlists.iter().filter(|p| p.owned).cloned().collect();
        let mut art = app.artwork.take();
        playlistedit::render(frame, frame.area(), &palette, prompt, &owned, art.as_mut());
        app.artwork = art;
    }
    if let Some(edit) = &app.setting_edit {
        settings::render_edit(frame, frame.area(), &palette, edit);
    }
    if app.showing_help {
        // Clamped here, where the height is known: the key handler only
        // counts.
        app.help_scroll = app.help_scroll.min(help::overflow(frame.area().height));
        help::render(frame, frame.area(), &palette, app.help_scroll);
    }
}
