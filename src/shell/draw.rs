//! One frame: the sidebar, whichever view the main pane holds, the bar.

use super::*;

/// Public so `examples/screenshot.rs` renders what the app actually renders.
/// A preview that assembles the layout itself drifts from the real one, and
/// then it is checking its own copy rather than the UI.
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

pub fn draw(frame: &mut ratatui::Frame, app: &mut App) {
    // Borrowed by field, so the closure lives beside the artwork being
    // taken out and put back below.
    let (followed, fav_albums, fav_playlists) = (
        &app.favourites.artists,
        &app.favourites.albums,
        &app.favourites.playlists,
    );
    let liked = |t: &carousel::Target| liked_in(t, followed, fav_albums, fav_playlists);
    let regions = layout::split(frame.area());
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

    // The main pane always has the keys. Tab used to move a highlight onto
    // the nav, where no key did anything — a mode with nothing behind it.
    let main_focused = true;
    // Search takes the pane while it is open, ahead of both an opened album
    // and the sidebar's section — it is where the user is looking. Drawn
    // here rather than with an early return, so the now-playing bar and the
    // status line below still get their turn.
    // Asked once, and every branch below reads it: exactly one view is
    // drawn, the one `showing` names, whatever else is set behind it.
    let showing = app.showing();
    if let (Showing::Search(_), Some(search)) = (showing, app.search.as_ref()) {
        // `artwork` needs &mut while the renderer borrows `app`, so take it
        // out for the duration and put it back — as the other views do.
        let mut art = app.artwork.take();
        searchview::render(
            frame,
            regions.main,
            &palette,
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

    if let (Showing::Artist, Some(page)) = (showing, app.artist.page.as_ref()) {
        let mut art = app.artwork.take();
        artistview::render(
            frame,
            regions.main,
            &palette,
            artistview::View {
                page,
                section: artistview::Section::from_index(app.artist.section),
                tracks: &app.artist.tracks,
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

    // A view opened but not yet filled: its name and a line saying it is
    // on its way. Drawn here rather than by a section's renderer because
    // what it becomes is not known until the reply lands.
    if showing == Showing::Loading {
        let title = app
            .open
            .as_ref()
            .map(|o| o.title.as_str())
            .unwrap_or_default();
        loadingview::render(frame, regions.main, &palette, title);
    }
    let section = match showing {
        // Already drawn above, or drawn below in the queue's case; this
        // keeps the match from drawing a section under them.
        Showing::Artist
        | Showing::Search(_)
        | Showing::Loading
        | Showing::Queue
        | Showing::History => None,
        // An opened row of covers is a grid, not a track list — Albums is
        // the section whose renderer draws two-line cards.
        Showing::Tracks => Some(sidebar::Section::Tracks),
        Showing::Cards => Some(sidebar::Section::Albums),
        // A page of rows is drawn by the home renderer, on whichever state
        // holds it.
        Showing::Rows => Some(app.sidebar.section()),
        Showing::Section(section) => Some(section),
    };

    if let Some(showing) = section {
        match showing {
            // Explore is a page of card rows like the home one, so it is drawn
            // by the same renderer on its own state.
            section @ (sidebar::Section::Music | sidebar::Section::Explore) => {
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
                        regions.main,
                        &palette,
                        vec![
                            ratatui::text::Line::from(format!(
                                "{} has no Explore page.",
                                app.service_name()
                            )),
                            ratatui::text::Line::from("Search (s) still covers its catalogue."),
                        ],
                    );
                } else if state.rows.is_empty() {
                    let line = match loaded {
                        false => waiting::animated("Loading…").into_owned(),
                        true => format!("{} has nothing to show here yet.", app.service_name()),
                    };
                    notice(
                        frame,
                        regions.main,
                        &palette,
                        vec![ratatui::text::Line::from(line)],
                    );
                } else {
                    let mut art = app.artwork.take();
                    home::render(
                        frame,
                        regions.main,
                        &palette,
                        state,
                        main_focused,
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
            sidebar::Section::Feed => {
                // The Feed is a page of rows like the home page, but headed and
                // filtered like a grid: its chrome is drawn here and what is
                // left of the pane goes to the row renderer. The heading rows
                // come from the grid's own count so the two cannot drift.
                let header = inputbox::render_headed(
                    frame,
                    scrollbar::reserve(regions.main),
                    &palette,
                    "Feed",
                    None,
                    "Filter releases",
                    &app.feed.filter,
                    app.filtering,
                );
                let body = ratatui::layout::Rect {
                    y: regions.main.y + header,
                    height: regions.main.height.saturating_sub(header),
                    ..regions.main
                };
                // Only TIDAL has one. Blank, the page read as broken; it
                // says what it would show and where it is shown.
                if !app.session.as_ref().is_some_and(|s| s.has_feed()) {
                    notice(frame, body, &palette, feed_notice(app.service_name()));
                } else {
                    let mut art = app.artwork.take();
                    home::render(
                        frame,
                        body,
                        &palette,
                        &app.feed,
                        main_focused,
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
            sidebar::Section::Settings => {
                let linked: Vec<(String, String, bool)> = app
                    .scrobblers
                    .iter()
                    .map(|l| (l.service.id().to_string(), l.label(), l.account.enabled))
                    .collect();
                settings::render(
                    frame,
                    regions.main,
                    &palette,
                    &app.config,
                    &app.settings,
                    main_focused,
                    &linked,
                );
            }
            section @ (sidebar::Section::Playlists
            | sidebar::Section::Albums
            | sidebar::Section::Profiles) => {
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
                let mut art = app.artwork.take();
                grid::render(
                    frame,
                    regions.main,
                    &palette,
                    grid::Grid {
                        filtering: app.filtering,
                        chrome: grid::Chrome::Full,
                        heading,
                        filter_hint: hint,
                        cards: &visible,
                        state: &state,
                        focused: main_focused,
                        lines,
                        tabs: (&[], 0),
                        liked: &liked,
                    },
                    cover_drawer(&mut art),
                );
                app.artwork = art;
            }
            _ => {
                let visible = tracklist::filter(&app.tracks, &app.tracklist.filter);
                let mut art = app.artwork.take();
                tracklist::render(
                    frame,
                    regions.main,
                    &palette,
                    tracklist::TrackList {
                        filtering: app.filtering,
                        favourites: &app.favourites.tracks,
                        chrome: tracklist::Chrome::Full,
                        tracks: &visible,
                        state: &app.tracklist,
                        focused: main_focused,
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
        }
    }

    // The queue and the history take the pane, as the artist page does,
    // and draw the list the Tracks section draws: a row here is a row like
    // any other, and the keys act on it. They were modals with a list of
    // their own, and a modal that closed on any key was one no key could
    // act in.
    if matches!(showing, Showing::Queue | Showing::History) {
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
            regions.main,
            &palette,
            heading,
            &tracks,
            state,
            marks,
            cover_drawer(&mut art),
        );
        app.artwork = art;
    }

    let mut art = app.artwork.take();
    nowplaying::render_with_cover(
        frame,
        regions.now_playing,
        &palette,
        &app.now_playing,
        // Straight from the queue, so the bar cannot disagree with what
        // actually plays next.
        nowplaying::Modes {
            shuffled: app.queue.shuffled(),
            repeat: app.queue.repeat,
            sleep: app.sleep_mark(),
            // Only while something plays: a paused track's last frame
            // frozen under the badge read as the music still going.
            spectrum: match (&app.spectrum, app.now_playing.playing) {
                (Some(bands), true) => bands.levels(),
                _ => Vec::new(),
            },
        },
        cover_drawer(&mut art),
    );
    app.artwork = art;

    if let Some(status) = &app.status {
        // Tucked into the top right rather than laid across the whole line.
        // Taking the line meant covering the tab strip that lives there:
        // drawn over it the two read as one garbled line, and clearing it
        // first hid the tabs for as long as the message stood.
        let width = (status.chars().count() as u16 + 2).min(regions.main.width);
        let notice = ratatui::layout::Rect {
            x: regions.main.x + regions.main.width - width,
            y: regions.main.y,
            width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(Line::styled(
                format!(" {} ", waiting::animated(status)),
                palette.notice(),
            )),
            notice,
        );
    }

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
        drmview::render(frame, regions.main, &palette, &app.cdm);
    }

    if app.lyrics.open {
        lyricsview::render(
            frame,
            regions.main,
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
