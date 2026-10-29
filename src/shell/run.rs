//! The event loop: keys, ticks and replies become actions, actions become
//! state through [`App::update`], and whatever that state asks of the
//! network or the audio thread is spawned from here and reports back as
//! an action of its own.

use super::*;

/// The tail of every playlist edit: the word for the status line, and
/// the playlists again when the list itself changed -- made, renamed,
/// deleted. A track in or out only moves a count, applied in place: the
/// fetch of every playlist was most of the wait.
async fn finish_edit(
    client: &std::sync::Arc<crate::catalogue::Session>,
    tx: &tokio::sync::mpsc::UnboundedSender<Action>,
    done: crate::catalogue::Result<(String, String, bool, i32)>,
) {
    match done {
        Ok((message, uuid, deleted, count_delta)) => {
            if count_delta == 0 {
                if let Ok(p) = client.playlists().await {
                    let _ = tx.send(Action::PlaylistsLoaded(p));
                }
                // The hearts too: a new playlist of the account's own is
                // among its favourites on TIDAL, and the grid draws it
                // from this list, which was still the one from sign-in.
                if let Ok(ids) = client.favourite_ids().await {
                    let _ = tx.send(Action::FavouriteIdsLoaded(ids));
                }
            }
            let _ = tx.send(Action::PlaylistEdited {
                message,
                uuid,
                deleted,
                count_delta,
            });
        }
        Err(e) => {
            let _ = tx.send(Action::Error(e.to_string()));
        }
    }
}

fn open_collection(
    app: &App,
    target: Collection,
    title: String,
    request: RequestId,
    action_tx: &tokio::sync::mpsc::UnboundedSender<Action>,
) {
    // This used to fall out of here without a word, and a session that
    // expired while the app was running made every open do exactly
    // nothing: no request, no error, no line in the log.
    let Some(session) = &app.session else {
        tracing::warn!("cannot open a collection: there is no session");
        let _ = action_tx.send(Action::Error(
            "the session has expired — restart to sign in again".into(),
        ));
        return;
    };
    let (client, tx) = (std::sync::Arc::clone(session), action_tx.clone());
    tokio::spawn(async move {
        let loaded = match &target {
            Collection::Playlist(uuid) => client.playlist_tracks(uuid).await,
            // An album is a page of its own, the artist's shape: its tracks
            // and, under them, the rows the web draws -- more by the
            // artist, other versions, related records.
            Collection::Album(id) => {
                match client.album_page(id, &title).await {
                    Ok(page) => {
                        let page = Box::new(page);
                        let _ = tx.send(Action::ArtistLoaded { request, page });
                        return;
                    }
                    Err(e) => Err(e),
                }
            }
            // Named alongside, best-effort: a radio that cannot be saved
            // still plays.
            Collection::Radio(seed) => {
                let (tracks, playlist) =
                    tokio::join!(client.track_radio(seed), client.radio_playlist(seed));
                if let Ok(Some(playlist)) = playlist {
                    let seed = seed.clone();
                    let _ = tx.send(Action::RadioNamed { seed, playlist });
                }
                tracks
            }
            // A genre, a mood or a decade is a page of rows, not a list of
            // tracks — the same shape Explore itself is, so it comes back
            // as its own reply and takes the pane.
            Collection::Page { title, path } => {
                match client.page_of_rows(path).await {
                    Ok(home) => {
                        let _ = tx.send(Action::PageLoaded {
                            request,
                            title: title.clone(),
                            home: Box::new(home),
                        });
                        return;
                    }
                    Err(e) => Err(e),
                }
            }
            // The row knew the track and not its album or artist: the track
            // names them, and what it names takes the view as `OpenTarget`,
            // which is fetched in its turn.
            Collection::TrackAlbum(id) | Collection::TrackArtist(id) => {
                let album = matches!(target, Collection::TrackAlbum(_));
                match client.track(id).await {
                    Ok(track) => {
                        let found = if album {
                            album_of(&track)
                        } else {
                            artist_of(&track)
                        };
                        match found {
                            Some((target, title)) => {
                                // The track's own artist and sleeve head an
                                // album; an artist's page brings its own
                                // picture.
                                let (subtitle, cover) = if album {
                                    (track.artist.clone(), track.cover.clone())
                                } else {
                                    (String::new(), None)
                                };
                                let _ = tx.send(Action::OpenTarget {
                                    request,
                                    target,
                                    title,
                                    subtitle,
                                    cover,
                                });
                            }
                            None => tracing::info!(
                                "{:?} names no {}",
                                track.title,
                                if album { "album" } else { "artist" }
                            ),
                        }
                        return;
                    }
                    Err(e) => Err(e),
                }
            }
            // An artist is a page of its own — three sections rather than
            // one list — so it comes back as its own reply.
            Collection::Artist(id) => match client.artist_page(id).await {
                Ok(page) => {
                    let page = Box::new(page);
                    let _ = tx.send(Action::ArtistLoaded { request, page });
                    return;
                }
                Err(e) => Err(e),
            },
        };
        match loaded {
            Ok(tracks) => {
                let _ = tx.send(Action::CollectionLoaded { request, tracks });
            }
            Err(crate::catalogue::Error::Unauthorized) => {
                let _ = tx.send(Action::SessionExpired);
            }
            Err(e) => {
                let _ = tx.send(Action::Error(e.to_string()));
            }
        }
    });
}

/// Fetch a track's stream and hand it to the player.
///
/// Shared by starting a track by hand and by the queue moving on, so the two
/// cannot drift: the bar is filled in from what is known now, and the
/// response says what was actually delivered.
fn start_track(
    app: &mut App,
    track: Option<crate::domain::Track>,
    cmd_tx: &std::sync::mpsc::Sender<crate::playback::Cmd>,
) {
    let (Some(session), Some(track)) = (&app.session, track) else {
        return;
    };
    // What the user allows. Both paths into playback come through here, so a
    // blocked track cannot be reached by starting one by hand and then
    // letting the queue run on past it.
    if let Some(why) = app.why_blocked(&track) {
        tracing::info!("not playing {:?}: {why}", track.title);
        app.say(format!("{} — {why}", track.title));
        return;
    }
    // What the user asked for, not what this build prefers. The response
    // still says what was actually delivered, and the badge reads that.
    let wanted = app.config.audio.quality();
    // Remembered as it starts rather than as it ends: a track skipped half
    // way through is still one the user heard and may want to name later.
    app.queue.remember(&track);
    if let Some(path) = &app.history_file {
        crate::playback::save_history(path, app.queue.history());
    }
    // The engine fetches the stream itself and reports back as events: a
    // rejected session as `SessionExpired`, anything else as `Failed`.
    session.set_quality(wanted);
    let _ = cmd_tx.send(crate::playback::Cmd::Play(track.id.clone()));
    app.now_playing.track = Some(track);
    app.now_playing.playing = true;
}

pub async fn run(
    terminal: &mut DefaultTerminal,
    picker: ratatui_image::picker::Picker,
) -> anyhow::Result<()> {
    let config = crate::config::Config::load()?;
    // Before the first frame: the icons are read as the screen is drawn, so
    // setting this afterwards would open the app on the wrong set.
    icons::set_nerd_font(config.ui.nerd_font);

    // What the user rebound, and what could not be read. A typo names an
    // action that does nothing, which is worth saying rather than leaving
    // the user to wonder why their key is dead.
    let (keys, mut problems) = keymap::Keymap::from_config(&config.keys);
    for problem in &problems {
        tracing::warn!("keys: {problem}");
    }

    // The colours, the same way: a theme that cannot be read costs the
    // colours it names and is said out loud, rather than stopping the app.
    let (palette, colour_problems) = theme::Palette::from_config(&config.ui);
    for problem in &colour_problems {
        tracing::warn!("theme: {problem}");
    }
    problems.extend(colour_problems);

    let mut app = App {
        config: config.clone(),
        config_path: crate::config::paths::config_file(),
        keymap: keys,
        palette,
        status: problems.first().map(|p| format!("config: {p}")),
        // Stamped like any other message, so a config warning does not sit
        // in the corner for the rest of the session. The log keeps it.
        status_since: problems.first().map(|_| std::time::Instant::now()),
        ..App::default()
    };
    // What the last run played, so the history is not blank on every
    // start: a track heard yesterday is one the user may want to name.
    // The engine reads the shared equaliser from its first frame.
    app.apply_equalizer();
    app.history_file = crate::config::paths::cache_dir().map(|d| d.join("history.json"));
    if let Some(path) = &app.history_file {
        app.queue
            .restore_history(crate::playback::load_history(path));
    }
    // Resume an existing session rather than making the user log in again.
    //
    // Every branch below logs what it decided. Without that, a spurious
    // sign-out is indistinguishable from a missing file, an unreadable one,
    // or a token judged expired — which cost several rounds of guessing.
    match crate::catalogue::provider_for(&config) {
        Ok(provider) => {
            match provider.restore().await {
                Ok(Some(session)) => {
                    tracing::info!("resuming the stored {} session", provider.name());
                    let session = crate::catalogue::Session::open(std::sync::Arc::clone(&provider), session);
                    app.session = Some(std::sync::Arc::new(session));
                }
                Ok(None) => {
                    tracing::info!(
                        "no stored {} session; showing the login screen",
                        provider.name()
                    );
                    app.login = app.login_prompt();
                }
                // Not forgotten: a launch on a bad network lands here with
                // a perfectly good token on disk, and Enter tries it again.
                Err(e) => {
                    tracing::warn!(
                        "the stored {} session could not be resumed: {e:#}",
                        provider.name()
                    );
                    app.login = login::LoginState::Failed(format!("{e:#}"));
                }
            }
            app.provider = Some(provider);
        }
        Err(e) => {
            tracing::error!("could not set up the provider: {e:#}");
            app.login = login::LoginState::Failed(format!("{e:#}"));
        }
    }

    let mut events = EventStream::new();
    // ~30fps ceiling: a burst of scroll events coalesces into one frame.
    let mut ticker = tokio::time::interval(Duration::from_millis(33));
    let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel::<Action>();
    // Discord, when the config names an application to show the track as.
    let application_id = config.discord.application_id.trim();
    if !application_id.is_empty() {
        app.presence = Some(crate::presence::spawn(application_id.to_owned()));
    }
    // A folder of files has nothing stored to resume and nothing to ask:
    // it is read now, without waiting for Enter.
    if config.provider.kind == crate::config::ProviderKind::Local && app.session.is_none() {
        let _ = action_tx.send(Action::BeginLogin);
    }
    // The scrobbling services, linked off the main thread: Last.fm and
    // Libre.fm wait on the browser the first time.
    if crate::scrobble::configured(&config.scrobble) {
        let (scrobble, tx) = (config.scrobble.clone(), action_tx.clone());
        let task = tokio::spawn(async move {
            let file = crate::config::paths::scrobble_file();
            let (linked, refused) = crate::scrobble::link(&scrobble, file.as_deref(), None).await;
            let _ = tx.send(Action::ScrobblersLinked(linked, refused));
        });
        app.link_task = Some(task.abort_handle());
    }
    let (cmd_tx, mut playback_rx) = crate::playback::spawn();

    // The OS media keys. They reach the desktop, never the terminal, so the
    // app registers itself as a media player and the commands come back as
    // the same actions the keyboard sends.
    let (media_tx, media_rx) = mediakeys::channel();
    spawn_media_keys(action_tx.clone(), media_rx);

    // A resumed session needs the same engine and fetches a fresh login
    // gets. Without this the app opens already signed in and shows nothing.
    if app.session.is_some() {
        let _ = action_tx.send(Action::SessionReady);
    }

    // Cover art. Detection ran before the terminal was put into raw mode —
    // it queries stdout and reads stdin, so it cannot happen from in here.
    let (art_tx, mut art_rx) = tokio::sync::mpsc::unbounded_channel();
    // Cards are sized from the terminal's own cell, which the picker
    // measured while probing for an image protocol. Without this they
    // assume a cell exactly twice as tall as it is wide and a cover fills
    // only part of its card on everything else.
    let cell = picker.font_size();
    carousel::set_cell_size(cell.width, cell.height);
    tracing::info!(
        "terminal cell is {}x{}px, cards are {} columns",
        cell.width,
        cell.height,
        carousel::card_width()
    );
    app.artwork = Some(artwork::Artwork::with_picker(picker, art_tx));

    // The level from the config file, so a session starts where the last
    // one left off rather than at full whatever the setting says.
    let _ = cmd_tx.send(crate::playback::Cmd::Volume(app.config.audio.volume));

    // The last request the loop sent; an open stamped after it is fetched.
    let mut fetched = RequestId::default();
    while !app.should_quit {
        // A modal that covered the artwork leaves its text there: the image
        // cells are marked `Skip`, so the next frame writes nothing over
        // them. Clearing first forces the whole screen to be sent again,
        // which is what puts the covers back.
        // Tell the desktop what is playing. Sent every frame, filtered at
        // the other end: only a real change is announced, since the position
        // moves on every tick and a metadata signal that often makes the
        // desktop redraw its player for nothing.
        let _ = media_tx.send(mediakeys::State {
            track: app.now_playing.track.clone(),
            playing: app.now_playing.playing,
            position: app.now_playing.position,
            // More than one track in the queue means there is something
            // either side of this one to reach.
            can_next: app.queue.len() > 1,
            can_previous: app.queue.len() > 1,
        });

        if app.take_repaint() {
            // Not `Terminal::clear`: since ratatui 0.30 it reads the cursor
            // position back from the terminal first, and with the event
            // stream holding stdin that read times out after two seconds
            // and the error ends the run. The screen is wiped and the
            // buffer the next draw diffs against is emptied, which is the
            // rest of what `clear` does.
            use ratatui::backend::Backend as _;
            terminal
                .backend_mut()
                .clear_region(ratatui::backend::ClearType::All)?;
            terminal.swap_buffers();
            terminal.current_buffer_mut().reset();
            terminal.swap_buffers();
        }
        terminal.draw(|frame| draw(frame, &mut app))?;

        let action = tokio::select! {
            _ = ticker.tick() => Some(Action::Tick),
            Some(action) = action_rx.recv() => Some(action),
            Some(event) = playback_rx.recv() => Some(Action::Playback(event)),
            Some(loaded) = art_rx.recv() => {
                // Not an Action: nothing about app state changes, the cache
                // just gains a decoded cover for the next frame.
                if let Some(art) = app.artwork.as_mut() {
                    art.insert(loaded);
                }
                None
            }
            maybe_event = events.next() => match maybe_event {
                // Windows emits both press and release; without this filter
                // every keystroke counts twice.
                Some(Ok(Event::Key(key))) if key.is_press() => Some(Action::Key(key)),
                Some(Ok(_)) => None,
                Some(Err(e)) => Some(Action::Error(e.to_string())),
                None => Some(Action::Quit),
            },
        };

        // Actions may cascade; apply until the chain settles.
        let mut next = action;
        while let Some(action) = next {
            // What happened, for a run that has to be replayed by reading
            // its log: the key, what it became, and what was under it.
            // Debug only -- `RUST_LOG=sonora_tui=debug` -- so the default log
            // stays as it is, and the tick is left out or it would be
            // thirty lines a second.
            if tracing::enabled!(tracing::Level::DEBUG) && !matches!(action, Action::Tick) {
                let shown: String = format!("{action:?}").chars().take(120).collect();
                tracing::debug!(
                    action = %shown,
                    showing = ?app.showing(),
                    section = ?app.sidebar.section(),
                    selected = ?app.selected_track().map(|t| t.title),
                    queued = app.queue.len(),
                    "action"
                );
            }
            // Side effects that must not block the render loop are spawned
            // here and report back through action_tx.
            match &action {
                // The service may have been switched since the start, so
                // the config read here is the app's, not the one loaded.
                Action::Connect(secret) if app.session.is_none() => {
                    let method = music::SignIn::Credentials {
                        server: app.config.subsonic.server.trim().to_owned(),
                        username: app.config.subsonic.username.trim().to_owned(),
                        password: secret.0.clone(),
                    };
                    start_sign_in(&mut app, method, &action_tx);
                }
                // The old engine falls silent; the new one attaches at
                // `SessionReady`.
                Action::SwitchProvider => {
                    let _ = cmd_tx.send(crate::playback::Cmd::Stop);
                }
                // The way in the service offers, apart from going without an
                // account: a code for TIDAL, a link for Spotify, cookies for
                // the rest. A Subsonic server went through `Connect` above.
                Action::BeginLogin
                    if app.session.is_none()
                        && app.config.provider.kind != crate::config::ProviderKind::Subsonic =>
                {
                    let method = match app.config.provider.kind {
                        // Nothing to read without a folder named; the
                        // screen says what to set.
                        crate::config::ProviderKind::Local => {
                            let folders = app.config.local.folders();
                            (!folders.is_empty()).then_some(music::SignIn::Path(folders))
                        }
                        _ => Some(
                            app.provider
                                .as_ref()
                                .map(|p| p.sign_in_options())
                                .unwrap_or_default()
                                .into_iter()
                                .find(|m| *m != music::SignIn::Anonymous)
                                .unwrap_or(music::SignIn::Default),
                        ),
                    };
                    if let Some(method) = method {
                        start_sign_in(&mut app, method, &action_tx);
                    }
                }
                Action::BeginGuest if app.session.is_none() => {
                    start_sign_in(&mut app, music::SignIn::Anonymous, &action_tx);
                }
                Action::Answer(text) => {
                    if let Some(input) = &app.login_input {
                        let _ = input.send(text.clone());
                    }
                }
                // The engine on the session's own factory, then the fetches
                // every signed-in screen needs. Both after a fresh login and
                // when a stored session is resumed: the second case is easy
                // to forget, and forgetting it means the app opens already
                // signed in and shows nothing at all.
                Action::SessionReady => {
                    if let Some(session) = &app.session {
                        let _ = cmd_tx.send(crate::playback::Cmd::Attach {
                            factory: session.playback(),
                            meta: session.stream_meta(),
                            options: app.playback_options(),
                        });
                        load_collection(std::sync::Arc::clone(session), action_tx.clone());
                    }
                }
                // A card that opens something: a playlist, an album, an
                // artist, a mix, a genre page. Anything else falls through
                // to the arm below, which plays a track. The fetch follows
                // `update`, off what it leaves open.
                Action::ActivateSelection if app.what_enter_opens().is_some() => {}
                // Taking a track out of the queue. Only the loop can do
                // it: dropping what is playing has to stop the audio
                // thread, and `update` has no channel to it.
                Action::RemoveQueued(at) => {
                    let was_playing = app.queue.remove(*at);
                    // The cursor follows: the row that was highlighted is
                    // gone, so the one that took its place is highlighted
                    // instead, and the last row steps back.
                    app.queue_list.selected = app
                        .queue_list
                        .selected
                        .min(app.queue.len().saturating_sub(1));
                    if was_playing {
                        match app.queue.current().cloned() {
                            Some(next) => start_track(&mut app, Some(next), &cmd_tx),
                            None => {
                                let _ = cmd_tx.send(crate::playback::Cmd::Stop);
                                app.now_playing.playing = false;
                                app.now_playing.track = None;
                            }
                        }
                    }
                }
                // Jump straight to a queued track, skipping what is between.
                Action::JumpInQueue(at) => {
                    if let Some(track) = app.queue.jump_to(*at).cloned() {
                        app.queue_list.selected = *at;
                        start_track(&mut app, Some(track), &cmd_tx);
                    }
                }
                Action::ToggleFollow => {
                    if let (Some(session), Some((what, name))) =
                        (&app.session, app.selected_favourable())
                    {
                        let client = std::sync::Arc::clone(session);
                        let tx = action_tx.clone();
                        let wanted = !app.is_favourite(&what);
                        tokio::spawn(async move {
                            if let Err(e) = client.set_favourite(&what, wanted).await {
                                let _ = tx.send(Action::Error(e.to_string()));
                                return;
                            }
                            let _ = tx.send(Action::FollowChanged {
                                what: what.clone(),
                                name,
                                following: wanted,
                            });
                            // The section that lists these is read again,
                            // so it shows the change rather than the last
                            // launch.
                            match what {
                                Favourable::Artist(_) => {
                                    if let Ok(a) = client.artists().await {
                                        let _ = tx.send(Action::ArtistsLoaded(a));
                                    }
                                }
                                Favourable::Album(_) => {
                                    if let Ok(a) = client.albums().await {
                                        let _ = tx.send(Action::AlbumsLoaded(a));
                                    }
                                }
                                Favourable::Playlist(_) => {
                                    if let Ok(p) = client.playlists().await {
                                        let _ = tx.send(Action::PlaylistsLoaded(p));
                                    }
                                }
                            }
                        });
                    }
                }
                // The settings that something off the loop reads: the
                // scrobblers are linked again from the config as it is now
                // -- Last.fm and Libre.fm through the browser if they have
                // not been -- and the Discord presence starts over on the
                // new application, or stops on none.
                Action::SettingChanged(setting) if setting.is_scrobbling() => {
                    let (scrobble, tx) = (app.config.scrobble.clone(), action_tx.clone());
                    let fresh = setting.scrobbler();
                    if let Some(task) = app.link_task.take() {
                        task.abort();
                    }
                    app.say("linking the scrobblers…");
                    let task = tokio::spawn(async move {
                        let (linked, refused) = match crate::scrobble::configured(&scrobble) {
                            true => {
                                let file = crate::config::paths::scrobble_file();
                                crate::scrobble::link(&scrobble, file.as_deref(), fresh).await
                            }
                            false => (Vec::new(), Vec::new()),
                        };
                        let _ = tx.send(Action::ScrobblersLinked(linked, refused));
                    });
                    app.link_task = Some(task.abort_handle());
                }
                Action::SettingChanged(settings::Setting::Discord) => {
                    if let Some(presence) = app.presence.take() {
                        let _ = presence.send(crate::presence::Presence::Off);
                    }
                    let id = app.config.discord.application_id.trim();
                    if !id.is_empty() {
                        app.presence = Some(crate::presence::spawn(id.to_owned()));
                    }
                }
                Action::Edit(edit) => {
                    if let Some(session) = &app.session {
                        let (client, tx) = (std::sync::Arc::clone(session), action_tx.clone());
                        let edit = edit.clone();
                        tokio::spawn(async move {
                            use playlistedit::Edit;
                            let done = match edit {
                                Edit::AddTo { uuid, title, track } => client
                                    .add_track_to_playlist(&uuid, &track)
                                    .await
                                    .map(|()| (format!("added to {title}"), uuid, false, 1)),
                                Edit::Create { name, then_add } => {
                                    match client.create_playlist(&name).await {
                                        Ok(uuid) => match then_add {
                                            Some(track) => client
                                                .add_track_to_playlist(&uuid, &track)
                                                .await
                                                .map(|()| (format!("added to {name}"), uuid, false, 0)),
                                            None => Ok((format!("created {name}"), uuid, false, 0)),
                                        },
                                        Err(e) => Err(e),
                                    }
                                }
                                Edit::Rename { uuid, name } => client
                                    .rename_playlist(&uuid, &name)
                                    .await
                                    .map(|()| (format!("renamed to {name}"), uuid, false, 0)),
                                Edit::Delete { uuid } => client
                                    .delete_playlist(&uuid)
                                    .await
                                    .map(|()| ("playlist deleted".to_string(), uuid, true, 0)),
                            };
                            finish_edit(&client, &tx, done).await;
                        });
                    }
                }
                Action::RemoveFromPlaylist { uuid, track } => {
                    if let Some(session) = &app.session {
                        let (client, tx) = (std::sync::Arc::clone(session), action_tx.clone());
                        let (uuid, track) = (uuid.clone(), track.clone());
                        tokio::spawn(async move {
                            let done = client
                                .remove_track_from_playlist(&uuid, &track)
                                .await
                                .map(|()| ("taken out of the playlist".to_string(), uuid, false, -1));
                            finish_edit(&client, &tx, done).await;
                        });
                    }
                }
                Action::ToggleFavourite => {
                    let selected = app.selected_track();
                    if let (Some(session), Some(track)) = (&app.session, selected.as_ref()) {
                        let client = std::sync::Arc::clone(session);
                        let (id, tx) = (track.id.clone(), action_tx.clone());
                        // What it should become, decided here so the request
                        // matches what the user saw when they pressed it.
                        let wanted = !app.favourites.tracks.contains(&id);
                        tokio::spawn(async move {
                            match client.set_track_favourite(&id, wanted).await {
                                // The reply carries the id, so a slow one
                                // cannot mark whatever is selected by the
                                // time it lands.
                                Ok(()) => {
                                    let _ = tx.send(Action::FavouriteChanged {
                                        id,
                                        favourite: wanted,
                                    });
                                }
                                Err(e) => {
                                    let _ = tx.send(Action::Error(e.to_string()));
                                }
                            }
                        });
                    }
                }
                Action::ActivateSelection => {
                    // Playing from a list queues the whole list, so what
                    // follows this track is already decided.
                    let (list, at, named) = app.selected_list();
                    tracing::debug!(
                        list = list.len(),
                        at,
                        named = ?named,
                        showing = ?app.showing(),
                        "activate: the list under the selection"
                    );
                    if !list.is_empty() {
                        let shuffled = app.queue.shuffled();
                        // A new context rather than a new queue: what the
                        // user put next by hand outlives the album it was
                        // queued over, and the radio trailing the old one
                        // goes. Repeat is the player's mode, not the
                        // queue's contents, so it is left alone.
                        app.queue.start_context(list, at, named.clone());
                        if shuffled {
                            app.queue
                                .set_shuffled(true, &mut crate::playback::clock_rng());
                        }
                    }
                    let selected = app.selected_track();
                    start_track(&mut app, selected, &cmd_tx);
                }
                // The queue decided which track; this fetches it. Same path
                // as starting one by hand, so a queued track and a chosen
                // one cannot drift apart.
                Action::PlayQueued => {
                    let next = app.queue.current().cloned();
                    start_track(&mut app, next, &cmd_tx);
                }
                // Autoplay: the queue is nearly out. The station built on
                // its last track is fetched and appended -- so what follows
                // is a queue like any other and the skip keys work on it.
                Action::Autoplay { seed } => {
                    if let Some(session) = &app.session {
                        let (client, tx) =
                            (std::sync::Arc::clone(session), action_tx.clone());
                        let seed = seed.clone();
                        tokio::spawn(async move {
                            match client.track_radio(&seed).await {
                                Ok(tracks) if !tracks.is_empty() => {
                                    tracing::info!("autoplay: {} tracks", tracks.len());
                                    let _ = tx.send(Action::QueueRadio(tracks));
                                }
                                Ok(_) => {
                                    tracing::info!("autoplay: nothing follows {seed:?}");
                                    let _ = tx.send(Action::AutoplayFailed(seed.clone()));
                                }
                                Err(e) => {
                                    tracing::warn!("autoplay failed: {e}");
                                    let _ = tx.send(Action::AutoplayFailed(seed.clone()));
                                }
                            }
                        });
                    }
                }
                Action::RunSearch(query) => {
                    let query = query.clone();
                    if let Some(session) = &app.session {
                        let (client, t) =
                            (std::sync::Arc::clone(session), action_tx.clone());
                        tokio::spawn(async move {
                            match client.search(&query).await {
                                Ok(results) => {
                                    tracing::info!("search {query:?}: {} results", results.total());
                                    let _ = t.send(Action::SearchLoaded(Box::new(results)));
                                }
                                Err(crate::catalogue::Error::Unauthorized) => {
                                    let _ = t.send(Action::SessionExpired);
                                }
                                Err(e) => {
                                    tracing::warn!("search {query:?} failed: {e}");
                                    let _ = t.send(Action::Error(e.to_string()));
                                }
                            }
                        });
                    }
                }
                Action::LoadHome => {
                    if let Some(session) = &app.session {
                        let (client, t) =
                            (std::sync::Arc::clone(session), action_tx.clone());
                        tokio::spawn(async move {
                            match client.home().await {
                                Ok(home) => {
                                    tracing::info!("loaded home: {} rows", home.rows.len());
                                    let _ = t.send(Action::HomeLoaded(Box::new(home)));
                                }
                                Err(e) => {
                                    tracing::warn!("could not load the home page: {e}");
                                    let _ = t.send(Action::Error(e.to_string()));
                                }
                            }
                        });
                    }
                }
                // The same three fetches sign-in makes: whichever of them
                // was dropped fills in, and the ones already held are
                // written back unchanged.
                Action::LoadFavourites => {
                    if let Some(session) = &app.session {
                        load_collection(std::sync::Arc::clone(session), action_tx.clone());
                    }
                }
                Action::LoadFeed => {
                    if let Some(session) = &app.session {
                        let (client, t) =
                            (std::sync::Arc::clone(session), action_tx.clone());
                        tokio::spawn(async move {
                            match client.feed().await {
                                Ok(cards) => {
                                    tracing::info!("loaded {} feed cards", cards.len());
                                    let _ = t.send(Action::FeedLoaded(cards));
                                }
                                Err(e) => {
                                    tracing::warn!("could not load the feed: {e}");
                                    let _ = t.send(Action::Error(e.to_string()));
                                }
                            }
                        });
                    }
                }
                Action::LoadExplore => {
                    if let Some(session) = &app.session {
                        let (client, t) =
                            (std::sync::Arc::clone(session), action_tx.clone());
                        tokio::spawn(async move {
                            match client.explore().await {
                                Ok(home) => {
                                    tracing::info!("loaded explore: {} rows", home.rows.len());
                                    let _ = t.send(Action::ExploreLoaded(Box::new(home)));
                                }
                                Err(e) => {
                                    tracing::warn!("could not load explore: {e}");
                                    let _ = t.send(Action::Error(e.to_string()));
                                }
                            }
                        });
                    }
                }
                Action::TogglePause => {
                    let cmd = if app.now_playing.playing {
                        crate::playback::Cmd::Pause
                    } else {
                        crate::playback::Cmd::Resume
                    };
                    let _ = cmd_tx.send(cmd);
                }
                Action::SetVolume(v) => {
                    let _ = cmd_tx.send(crate::playback::Cmd::Volume(*v));
                }
                Action::LookForCdm => {
                    let tx = action_tx.clone();
                    tokio::spawn(async move {
                        // Reads browser folders: off the loop's thread.
                        let found = tokio::task::spawn_blocking(music::drm::find)
                            .await
                            .ok()
                            .flatten();
                        let _ = tx.send(Action::CdmFound(found.map(|f| f.origin)));
                    });
                }
                // Download, show the terms, wait for the word, install. The
                // archive stays in this task: the screen only sees the terms.
                Action::DownloadCdm => {
                    let (answer, asked) = tokio::sync::oneshot::channel();
                    app.cdm_answer = Some(answer);
                    let tx = action_tx.clone();
                    tokio::spawn(async move {
                        let offer = match music::drm::offer().await {
                            Ok(offer) => offer,
                            Err(e) => {
                                tracing::warn!("widevine: cannot download the module: {e:#}");
                                let _ = tx.send(Action::CdmFailed(format!(
                                    "could not download the Widevine module: {e:#}"
                                )));
                                return;
                            }
                        };
                        let _ = tx.send(Action::CdmOffered {
                            version: offer.version().to_owned(),
                            license: offer.license().to_owned(),
                        });
                        if asked.await != Ok(true) {
                            return;
                        }
                        match tokio::task::spawn_blocking(move || offer.install()).await {
                            Ok(Ok(found)) => {
                                tracing::info!("widevine: installed into the store");
                                let _ = tx.send(Action::CdmReady(found.origin));
                            }
                            Ok(Err(e)) => {
                                tracing::warn!("widevine: cannot install the module: {e:#}");
                                let _ = tx.send(Action::CdmFailed(format!(
                                    "could not install the Widevine module: {e:#}"
                                )));
                            }
                            Err(_) => {
                                let _ = tx.send(Action::CdmFailed(
                                    "the Widevine install task went away".into(),
                                ));
                            }
                        }
                    });
                }
                Action::Preload(id) => {
                    let _ = cmd_tx.send(crate::playback::Cmd::Preload(id.clone()));
                }
                Action::LoadLyrics(id) => {
                    if let (Some(session), Some(track)) =
                        (&app.session, app.now_playing.track.clone())
                    {
                        if track.id == *id {
                            let query = lyricsview::query(&track, session.slug());
                            let (id, tx) = (id.clone(), action_tx.clone());
                            tokio::spawn(async move {
                                let found = lyricsview::search(query).await;
                                let _ = tx.send(Action::LyricsLoaded { id, found });
                            });
                        }
                    }
                }
                // A fresh engine on the same session, picking the track
                // back up where it was, paused if it was paused.
                Action::RestartEngine => {
                    if let Some(session) = &app.session {
                        let _ = cmd_tx.send(crate::playback::Cmd::Attach {
                            factory: session.playback(),
                            meta: session.stream_meta(),
                            options: app.playback_options(),
                        });
                        if let Some(track) = &app.now_playing.track {
                            let _ = cmd_tx.send(crate::playback::Cmd::PlayAt(
                                track.id.clone(),
                                app.now_playing.position,
                            ));
                            if !app.now_playing.playing {
                                let _ = cmd_tx.send(crate::playback::Cmd::Pause);
                            }
                        }
                    }
                }
                Action::SeekForward | Action::SeekBack => {
                    if let Some(to) = app.seek_target(matches!(action, Action::SeekForward)) {
                        let _ = cmd_tx.send(crate::playback::Cmd::Seek(to));
                        // Discord's clock starts again from where this lands.
                        app.presence_shown = None;
                    }
                }
                _ => {}
            }
            next = app.update(action);
            // Whatever that opened is fetched now, off the view it left
            // on screen: one site, reading the state rather than guessing
            // it from the action before `update` had run. A view opened
            // from cards already in hand carries no target and asks for
            // nothing.
            if let Some(open) = app.open.as_ref().filter(|o| o.request > fetched) {
                fetched = open.request;
                if let Some(target) = open.target.clone() {
                    open_collection(&app, target, open.title.clone(), open.request, &action_tx);
                }
            }
        }
        // What the actions above put in the outbox goes out now, off the
        // loop: a service that is slow must not hold the keys.
        for sending in crate::scrobble::drain(&app.scrobblers, &mut app.scrobble_outbox) {
            tokio::spawn(sending);
        }
        app.tell_presence();
    }
    if let Some(presence) = &app.presence {
        let _ = presence.send(crate::presence::Presence::Off);
    }
    // On the way out: the listen in progress, if it earned it, and whatever
    // is waiting. Awaited, briefly, since nothing runs after this.
    app.settle_listen(app.now_playing.position);
    let sending = crate::scrobble::drain(&app.scrobblers, &mut app.scrobble_outbox);
    if !sending.is_empty() {
        let _ = tokio::time::timeout(
            Duration::from_secs(3),
            futures::future::join_all(sending),
        )
        .await;
    }
    Ok(())
}

/// Fetch everything a signed-in session needs.
///
/// Called both after a fresh login and when a stored session is resumed at
/// startup — the second case is easy to forget, and forgetting it means the
/// app opens already signed in and shows nothing at all.
///
/// Each fetch is its own task so a slow one does not hold up the others; the
/// home page is by far the largest response.
fn load_collection(
    catalogue: std::sync::Arc<crate::catalogue::Session>,
    tx: tokio::sync::mpsc::UnboundedSender<Action>,
) {
    let (client, t) = (std::sync::Arc::clone(&catalogue), tx.clone());
    tokio::spawn(async move {
        match client.favourite_tracks().await {
            Ok(tracks) => {
                tracing::info!("loaded {} favourite tracks", tracks.len());
                let _ = t.send(Action::TracksLoaded(tracks));
            }
            Err(crate::catalogue::Error::Unauthorized) => {
                let _ = t.send(Action::SessionExpired);
            }
            Err(e) => {
                tracing::warn!("could not load favourites: {e}");
                let _ = t.send(Action::Error(e.to_string()));
            }
        }
    });

    let (client, t) = (std::sync::Arc::clone(&catalogue), tx.clone());
    tokio::spawn(async move {
        match client.playlists().await {
            Ok(playlists) => {
                tracing::info!("loaded {} playlists", playlists.len());
                let _ = t.send(Action::PlaylistsLoaded(playlists));
            }
            // A failure here costs the sidebar list, not the session — log it
            // rather than throwing the user back to the login screen.
            Err(e) => tracing::warn!("could not load playlists: {e}"),
        }
    });

    let (client, t) = (std::sync::Arc::clone(&catalogue), tx.clone());
    tokio::spawn(async move {
        match client.albums().await {
            Ok(albums) => {
                tracing::info!("loaded {} favourite albums", albums.len());
                let _ = t.send(Action::AlbumsLoaded(albums));
            }
            Err(e) => tracing::warn!("could not load albums: {e}"),
        }
    });

    let (client, t) = (std::sync::Arc::clone(&catalogue), tx.clone());
    tokio::spawn(async move {
        match client.artists().await {
            Ok(artists) => {
                tracing::info!("loaded {} favourite artists", artists.len());
                let _ = t.send(Action::ArtistsLoaded(artists));
            }
            Err(e) => tracing::warn!("could not load artists: {e}"),
        }
    });

    // One reply with every favourited id, for the key that toggles them:
    // the sections above are pages of covers, this is the list.
    let (client, t) = (std::sync::Arc::clone(&catalogue), tx.clone());
    tokio::spawn(async move {
        match client.favourite_ids().await {
            Ok(ids) => {
                tracing::info!(
                    "loaded favourite ids: {} albums, {} playlists, {} artists",
                    ids.albums.len(),
                    ids.playlists.len(),
                    ids.artists.len()
                );
                let _ = t.send(Action::FavouriteIdsLoaded(ids));
            }
            Err(e) => tracing::warn!("could not load the favourite ids: {e}"),
        }
    });

    let (client, t) = (catalogue, tx);
    tokio::spawn(async move {
        // The same call `LoadHome` makes: one page loaded two ways is two
        // ways for it to differ, and it did.
        match client.home().await {
            Ok(home) => {
                tracing::info!("loaded home: {} rows", home.rows.len());
                let _ = t.send(Action::HomeLoaded(Box::new(home)));
            }
            Err(e) => tracing::warn!("could not load the home page: {e}"),
        }
    });
}

/// Signs in to `provider`, in its own task so the UI stays responsive. A
/// stored session is tried first: a launch on a bad network lands here with
/// a perfectly good token on disk. What the service shows on the way -- the
/// code to confirm in the browser -- reaches the login screen as actions;
/// so does the outcome.
fn start_sign_in(
    app: &mut App,
    method: music::SignIn,
    action_tx: &tokio::sync::mpsc::UnboundedSender<Action>,
) {
    if let Some(provider) = app.provider.clone() {
        let (typed, input) = tokio::sync::mpsc::unbounded_channel();
        app.login_input = Some(typed);
        let task = tokio::spawn(sign_in(provider, method, input, action_tx.clone()));
        app.login_task = Some(task.abort_handle());
    }
}

/// The sign-in itself, in its own task so the UI stays responsive. The
/// provider's questions arrive on `input`, answered through `Answer`.
async fn sign_in(
    provider: std::sync::Arc<dyn crate::catalogue::Provider>,
    method: music::SignIn,
    input: music::InputSource,
    tx: tokio::sync::mpsc::UnboundedSender<Action>,
) {
    let name = provider.name();
    if provider.stored() {
        if let Ok(Some(session)) = provider.restore().await {
            tracing::info!("resumed the stored {name} session");
            let session = crate::catalogue::Session::open(std::sync::Arc::clone(&provider), session);
            let _ = tx.send(Action::SessionOpened(std::sync::Arc::new(session)));
            return;
        }
    }
    let prompt: music::PromptSink = {
        let tx = tx.clone();
        std::sync::Arc::new(move |prompt| {
            // Save the user copying a URL out of a TUI. Best effort: the
            // link stays on screen, so a failure here costs nothing. A
            // `Url` prompt is not opened here: the provider opens it
            // itself, and listens for the browser to come back.
            if let music::SignInPrompt::Code { url, .. } = &prompt {
                login::open_in_browser(url);
            }
            let _ = tx.send(Action::LoginPrompt(prompt));
        })
    };
    match provider.sign_in(method, prompt, input).await {
        Ok(session) => {
            tracing::info!("signed in to {name}");
            let session = crate::catalogue::Session::open(std::sync::Arc::clone(&provider), session);
            let _ = tx.send(Action::SessionOpened(std::sync::Arc::new(session)));
        }
        Err(e) => {
            tracing::warn!("could not sign in to {name}: {e:#}");
            let _ = tx.send(Action::Error(format!("{e:#}")));
        }
    }
}
