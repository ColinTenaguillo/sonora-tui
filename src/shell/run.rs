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
            Collection::Album(id) => match client.album_page(id, &title).await {
                Ok(page) => {
                    let page = Box::new(page);
                    let _ = tx.send(Action::ArtistLoaded { request, page });
                    return;
                }
                Err(e) => Err(e),
            },
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
            Collection::Page { title, path } => match client.page_of_rows(path).await {
                Ok(home) => {
                    let _ = tx.send(Action::PageLoaded {
                        request,
                        title: title.clone(),
                        home: Box::new(home),
                    });
                    return;
                }
                Err(e) => Err(e),
            },
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

/// The app a config opens on, before anything is spawned: the keys, the
/// colours, and the first thing either of them could not read on the
/// status line.
fn app_for(config: &crate::config::Config) -> App {
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

    App {
        config: config.clone(),
        config_path: crate::config::paths::config_file(),
        keymap: keys,
        palette,
        status: problems.first().map(|p| format!("config: {p}")),
        // Stamped like any other message, so a config warning does not sit
        // in the corner for the rest of the session. The log keeps it.
        status_since: problems.first().map(|_| std::time::Instant::now()),
        ..App::default()
    }
}

/// Resume an existing session rather than making the user log in again.
///
/// Every branch below logs what it decided. Without that, a spurious
/// sign-out is indistinguishable from a missing file, an unreadable one,
/// or a token judged expired — which cost several rounds of guessing.
async fn resume_session(app: &mut App, config: &crate::config::Config) {
    match crate::catalogue::provider_for(config) {
        Ok(provider) => {
            match provider.restore().await {
                Ok(Some(session)) => {
                    tracing::info!("resuming the stored {} session", provider.name());
                    let session =
                        crate::catalogue::Session::open(std::sync::Arc::clone(&provider), session);
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
}

/// What the last run played, so the history is not blank on every
/// start: a track heard yesterday is one the user may want to name.
fn restore_history(app: &mut App) {
    app.history_file = crate::config::paths::cache_dir().map(|d| d.join("history.json"));
    if let Some(path) = &app.history_file {
        app.queue
            .restore_history(crate::playback::load_history(path));
    }
}

/// The scrobbling services, linked off the main thread: Last.fm and
/// Libre.fm wait on the browser the first time.
fn link_scrobblers(
    app: &mut App,
    config: &crate::config::Config,
    action_tx: &tokio::sync::mpsc::UnboundedSender<Action>,
) {
    if crate::scrobble::configured(&config.scrobble) {
        let (scrobble, tx) = (config.scrobble.clone(), action_tx.clone());
        let task = tokio::spawn(async move {
            let file = crate::config::paths::scrobble_file();
            let (linked, refused) = crate::scrobble::link(&scrobble, file.as_deref(), None).await;
            let _ = tx.send(Action::ScrobblersLinked(linked, refused));
        });
        app.link_task = Some(task.abort_handle());
    }
}

/// Cover art. Detection ran before the terminal was put into raw mode —
/// it queries stdout and reads stdin, so it cannot happen from in here.
fn attach_artwork(
    app: &mut App,
    picker: ratatui_image::picker::Picker,
) -> tokio::sync::mpsc::UnboundedReceiver<artwork::Loaded> {
    let (art_tx, art_rx) = tokio::sync::mpsc::unbounded_channel();
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
    art_rx
}

/// Everything the loop waits on for its next action.
struct Inputs {
    events: EventStream,
    ticker: tokio::time::Interval,
    action_rx: tokio::sync::mpsc::UnboundedReceiver<Action>,
    playback_rx: tokio::sync::mpsc::UnboundedReceiver<crate::playback::PlaybackEvent>,
    art_rx: tokio::sync::mpsc::UnboundedReceiver<artwork::Loaded>,
}

impl Inputs {
    /// Whichever comes first, as an action, or nothing when what came
    /// changes no state.
    async fn next(&mut self, app: &mut App) -> Option<Action> {
        tokio::select! {
            _ = self.ticker.tick() => Some(Action::Tick),
            Some(action) = self.action_rx.recv() => Some(action),
            Some(event) = self.playback_rx.recv() => Some(Action::Playback(event)),
            Some(loaded) = self.art_rx.recv() => cover_loaded(app, loaded),
            maybe_event = self.events.next() => terminal_action(maybe_event),
        }
    }
}

/// Not an Action: nothing about app state changes, the cache just gains a
/// decoded cover for the next frame.
fn cover_loaded(app: &mut App, loaded: artwork::Loaded) -> Option<Action> {
    if let Some(art) = app.artwork.as_mut() {
        art.insert(loaded);
    }
    None
}

fn terminal_action(maybe_event: Option<std::io::Result<Event>>) -> Option<Action> {
    match maybe_event {
        // Windows emits both press and release; without this filter
        // every keystroke counts twice.
        Some(Ok(Event::Key(key))) if key.is_press() => Some(Action::Key(key)),
        Some(Ok(_)) => None,
        Some(Err(e)) => Some(Action::Error(e.to_string())),
        None => Some(Action::Quit),
    }
}

/// Where the side effects of an action go: back to the loop as actions of
/// their own, or to the audio thread as commands.
struct Outlets {
    action_tx: tokio::sync::mpsc::UnboundedSender<Action>,
    cmd_tx: std::sync::mpsc::Sender<crate::playback::Cmd>,
}

pub async fn run(
    terminal: &mut DefaultTerminal,
    picker: ratatui_image::picker::Picker,
) -> anyhow::Result<()> {
    let config = crate::config::Config::load()?;
    // Before the first frame: the icons are read as the screen is drawn, so
    // setting this afterwards would open the app on the wrong set.
    icons::set_nerd_font(config.ui.nerd_font);

    let mut app = app_for(&config);
    // The engine reads the shared equaliser from its first frame, and opens
    // on the output the config names.
    app.apply_equalizer();
    choose_output(&config.audio);
    restore_history(&mut app);
    resume_session(&mut app, &config).await;

    let (action_tx, action_rx) = tokio::sync::mpsc::unbounded_channel::<Action>();
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
    link_scrobblers(&mut app, &config, &action_tx);
    let (cmd_tx, playback_rx) = crate::playback::spawn();

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

    let art_rx = attach_artwork(&mut app, picker);

    // The level from the config file, so a session starts where the last
    // one left off rather than at full whatever the setting says.
    let _ = cmd_tx.send(crate::playback::Cmd::Volume(app.config.audio.volume));

    let mut inputs = Inputs {
        events: EventStream::new(),
        // ~30fps ceiling: a burst of scroll events coalesces into one frame.
        ticker: tokio::time::interval(Duration::from_millis(33)),
        action_rx,
        playback_rx,
        art_rx,
    };
    let out = Outlets { action_tx, cmd_tx };
    run_loop(terminal, &mut app, &mut inputs, &out, &media_tx).await?;
    shut_down(&mut app).await;
    Ok(())
}

/// Draw, wait for the next action, apply it and what it cascades into,
/// until the app is told to quit.
async fn run_loop(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    inputs: &mut Inputs,
    out: &Outlets,
    media_tx: &mediakeys::StateSender,
) -> anyhow::Result<()> {
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
        let _ = media_tx.send(media_state(app));

        repaint(terminal, app.take_repaint())?;
        terminal.draw(|frame| draw(frame, app))?;

        let action = inputs.next(app).await;
        settle(app, action, out, &mut fetched);
        // What the actions above put in the outbox goes out now, off the
        // loop: a service that is slow must not hold the keys.
        for sending in crate::scrobble::drain(&app.scrobblers, &mut app.scrobble_outbox) {
            tokio::spawn(sending);
        }
        app.tell_presence();
    }
    Ok(())
}

fn media_state(app: &App) -> mediakeys::State {
    mediakeys::State {
        track: app.now_playing.track.clone(),
        playing: app.now_playing.playing,
        position: app.now_playing.position,
        // More than one track in the queue means there is something
        // either side of this one to reach.
        can_next: app.queue.len() > 1,
        can_previous: app.queue.len() > 1,
    }
}

/// Actions may cascade; apply until the chain settles.
fn settle(app: &mut App, action: Option<Action>, out: &Outlets, fetched: &mut RequestId) {
    let mut next = action;
    while let Some(action) = next {
        log_action(app, &action);
        side_effects(app, &action, out);
        next = app.update(action);
        fetch_opened(app, &out.action_tx, fetched);
    }
}

/// What happened, for a run that has to be replayed by reading its log:
/// the key, what it became, and what was under it. Debug only --
/// `RUST_LOG=sonora_tui=debug` -- so the default log stays as it is, and
/// the tick is left out or it would be thirty lines a second.
fn log_action(app: &App, action: &Action) {
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
}

/// Whatever that opened is fetched now, off the view it left on
/// screen: one site, reading the state rather than guessing it from the
/// action before `update` had run. A view opened from cards already in
/// hand carries no target and asks for nothing.
fn fetch_opened(
    app: &App,
    action_tx: &tokio::sync::mpsc::UnboundedSender<Action>,
    fetched: &mut RequestId,
) {
    if let Some(open) = app.open.as_ref().filter(|o| o.request > *fetched) {
        *fetched = open.request;
        if let Some(target) = open.target.clone() {
            open_collection(app, target, open.title.clone(), open.request, action_tx);
        }
    }
}

/// Side effects that must not block the render loop are spawned here and
/// report back through action_tx.
fn side_effects(app: &mut App, action: &Action, out: &Outlets) {
    let (action_tx, cmd_tx) = (&out.action_tx, &out.cmd_tx);
    match action {
        // The service may have been switched since the start, so
        // the config read here is the app's, not the one loaded.
        Action::Connect(secret) if app.session.is_none() => connect(app, secret, action_tx),
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
            begin_login(app, action_tx);
        }
        Action::BeginGuest if app.session.is_none() => {
            start_sign_in(app, music::SignIn::Anonymous, action_tx);
        }
        Action::Answer(text) => answer(app, text),
        Action::OpenSignInWindow => open_sign_in_window(app, action_tx),
        Action::SessionReady => session_ready(app, out),
        // A card that opens something: a playlist, an album, an
        // artist, a mix, a genre page. Anything else falls through
        // to the arm below, which plays a track. The fetch follows
        // `update`, off what it leaves open.
        Action::ActivateSelection if app.what_enter_opens().is_some() => {}
        Action::RemoveQueued(at) => remove_queued(app, *at, cmd_tx),
        // Jump straight to a queued track, skipping what is between.
        Action::JumpInQueue(at) => jump_in_queue(app, *at, cmd_tx),
        Action::ToggleFollow => toggle_follow(app, action_tx),
        // The settings that something off the loop reads: the
        // scrobblers are linked again from the config as it is now
        // -- Last.fm and Libre.fm through the browser if they have
        // not been -- and the Discord presence starts over on the
        // new application, or stops on none.
        Action::SettingChanged(setting) if setting.is_scrobbling() => {
            relink_scrobblers(app, setting, action_tx);
        }
        Action::SettingChanged(settings::Setting::Discord) => restart_presence(app),
        Action::SettingChanged(settings::Setting::MatchRate) => {
            music::output::choose_match_rate(app.config.audio.match_rate);
            restart_engine(app, cmd_tx);
        }
        Action::ListOutputs => list_outputs(action_tx),
        Action::UseOutput => use_output(app, action_tx),
        Action::Edit(edit) => edit_playlist(app, edit, action_tx),
        Action::RemoveFromPlaylist { uuid, track } => {
            remove_from_playlist(app, uuid, track, action_tx);
        }
        Action::ToggleFavourite => toggle_favourite(app, action_tx),
        Action::ActivateSelection => activate_selection(app, cmd_tx),
        // The queue decided which track; this fetches it. Same path
        // as starting one by hand, so a queued track and a chosen
        // one cannot drift apart.
        Action::PlayQueued => {
            let next = app.queue.current().cloned();
            start_track(app, next, cmd_tx);
        }
        Action::Autoplay { seed } => autoplay(app, seed, action_tx),
        Action::RunSearch(query) => run_search(app, query, action_tx),
        Action::LoadHome => load_home(app, action_tx),
        // The same three fetches sign-in makes: whichever of them
        // was dropped fills in, and the ones already held are
        // written back unchanged.
        Action::LoadFavourites => {
            if let Some(session) = &app.session {
                load_collection(std::sync::Arc::clone(session), action_tx.clone());
            }
        }
        Action::LoadFeed => load_feed(app, action_tx),
        Action::LoadExplore => load_explore(app, action_tx),
        Action::TogglePause => toggle_pause(app, cmd_tx),
        Action::SetVolume(v) => {
            let _ = cmd_tx.send(crate::playback::Cmd::Volume(*v));
        }
        Action::LookForCdm => look_for_cdm(action_tx),
        Action::DownloadCdm => download_cdm(app, action_tx),
        Action::Preload(id) => {
            let _ = cmd_tx.send(crate::playback::Cmd::Preload(id.clone()));
        }
        Action::LoadLyrics(id) => load_lyrics(app, id, action_tx),
        Action::RestartEngine => restart_engine(app, cmd_tx),
        Action::SeekForward => seek(app, true, cmd_tx),
        Action::SeekBack => seek(app, false, cmd_tx),
        _ => {}
    }
}

fn connect(
    app: &mut App,
    secret: &login::Secret,
    action_tx: &tokio::sync::mpsc::UnboundedSender<Action>,
) {
    let method = music::SignIn::Credentials {
        server: app.config.subsonic.server.trim().to_owned(),
        username: app.config.subsonic.username.trim().to_owned(),
        password: secret.0.clone(),
    };
    start_sign_in(app, method, action_tx);
}

fn begin_login(app: &mut App, action_tx: &tokio::sync::mpsc::UnboundedSender<Action>) {
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
        start_sign_in(app, method, action_tx);
    }
}

/// The service's sign-in page in a window, whose cookies answer the prompt.
/// With no window to open here, the paste box comes back.
fn open_sign_in_window(app: &App, action_tx: &tokio::sync::mpsc::UnboundedSender<Action>) {
    match sign_in_window(app) {
        Some((outcome, input)) => {
            tokio::spawn(await_window(outcome, input, action_tx.clone()));
        }
        None => {
            tracing::info!("no sign-in window here; the cookies are pasted");
            let _ = action_tx.send(after_window(super::signin_window::Outcome::Closed));
        }
    }
}

/// A window asked for on the sign-in under way, and where its answer goes.
/// `None` without a provider that signs in on a page, a sign-in waiting on
/// an answer, or a main thread to open the window on.
fn sign_in_window(
    app: &App,
) -> Option<(
    tokio::sync::oneshot::Receiver<super::signin_window::Outcome>,
    tokio::sync::mpsc::UnboundedSender<String>,
)> {
    let provider = app.provider.as_ref()?;
    let input = app.login_input.clone()?;
    let outcome = super::signin_window::open(&provider.web_sign_in()?, provider.name())?;
    Some((outcome, input))
}

/// Waits on the window, unless the sign-in is cancelled first: then
/// `outcome` is dropped, which is what tells the window to close.
async fn await_window(
    outcome: tokio::sync::oneshot::Receiver<super::signin_window::Outcome>,
    input: tokio::sync::mpsc::UnboundedSender<String>,
    tx: tokio::sync::mpsc::UnboundedSender<Action>,
) {
    let outcome = tokio::select! {
        outcome = outcome => outcome.unwrap_or(super::signin_window::Outcome::Closed),
        () = input.closed() => return,
    };
    let _ = tx.send(after_window(outcome));
}

/// What a window's end does: its cookies answer the prompt; closed, or
/// never opened, it leaves the paste box up.
fn after_window(outcome: super::signin_window::Outcome) -> Action {
    match outcome {
        super::signin_window::Outcome::Cookies(header) => Action::Answer(header),
        super::signin_window::Outcome::Closed => Action::PasteCookies,
        super::signin_window::Outcome::Failed(why) => {
            tracing::warn!("the sign-in window did not open: {why}");
            Action::PasteCookies
        }
    }
}

fn answer(app: &App, text: &str) {
    if let Some(input) = &app.login_input {
        let _ = input.send(text.to_owned());
    }
}

/// The engine on the session's own factory, then the fetches every
/// signed-in screen needs. Both after a fresh login and when a stored
/// session is resumed: the second case is easy to forget, and forgetting
/// it means the app opens already signed in and shows nothing at all.
fn session_ready(app: &App, out: &Outlets) {
    if let Some(session) = &app.session {
        let _ = out.cmd_tx.send(crate::playback::Cmd::Attach {
            factory: session.playback(),
            meta: session.stream_meta(),
            options: app.playback_options(),
        });
        load_collection(std::sync::Arc::clone(session), out.action_tx.clone());
    }
}

/// Taking a track out of the queue. Only the loop can do it: dropping
/// what is playing has to stop the audio thread, and `update` has no
/// channel to it.
fn remove_queued(app: &mut App, at: usize, cmd_tx: &std::sync::mpsc::Sender<crate::playback::Cmd>) {
    let was_playing = app.queue.remove(at);
    // The cursor follows: the row that was highlighted is
    // gone, so the one that took its place is highlighted
    // instead, and the last row steps back.
    app.queue_list.selected = app
        .queue_list
        .selected
        .min(app.queue.len().saturating_sub(1));
    if was_playing {
        match app.queue.current().cloned() {
            Some(next) => start_track(app, Some(next), cmd_tx),
            None => {
                let _ = cmd_tx.send(crate::playback::Cmd::Stop);
                app.now_playing.playing = false;
                app.now_playing.track = None;
            }
        }
    }
}

fn jump_in_queue(app: &mut App, at: usize, cmd_tx: &std::sync::mpsc::Sender<crate::playback::Cmd>) {
    if let Some(track) = app.queue.jump_to(at).cloned() {
        app.queue_list.selected = at;
        start_track(app, Some(track), cmd_tx);
    }
}

fn toggle_follow(app: &App, action_tx: &tokio::sync::mpsc::UnboundedSender<Action>) {
    if let (Some(session), Some((what, name))) = (&app.session, app.selected_favourable()) {
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
            reload_followed(&client, &tx, what).await;
        });
    }
}

/// The section that lists these is read again, so it shows the change
/// rather than the last launch.
async fn reload_followed(
    client: &crate::catalogue::Session,
    tx: &tokio::sync::mpsc::UnboundedSender<Action>,
    what: Favourable,
) {
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
}

fn relink_scrobblers(
    app: &mut App,
    setting: &settings::Setting,
    action_tx: &tokio::sync::mpsc::UnboundedSender<Action>,
) {
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

fn restart_presence(app: &mut App) {
    if let Some(presence) = app.presence.take() {
        let _ = presence.send(crate::presence::Presence::Off);
    }
    let id = app.config.discord.application_id.trim();
    if !id.is_empty() {
        app.presence = Some(crate::presence::spawn(id.to_owned()));
    }
}

fn edit_playlist(
    app: &App,
    edit: &playlistedit::Edit,
    action_tx: &tokio::sync::mpsc::UnboundedSender<Action>,
) {
    if let Some(session) = &app.session {
        let (client, tx) = (std::sync::Arc::clone(session), action_tx.clone());
        let edit = edit.clone();
        tokio::spawn(async move {
            let done = apply_edit(&client, edit).await;
            finish_edit(&client, &tx, done).await;
        });
    }
}

async fn apply_edit(
    client: &crate::catalogue::Session,
    edit: playlistedit::Edit,
) -> crate::catalogue::Result<(String, String, bool, i32)> {
    use playlistedit::Edit;
    match edit {
        Edit::AddTo { uuid, title, track } => client
            .add_track_to_playlist(&uuid, &track)
            .await
            .map(|()| (format!("added to {title}"), uuid, false, 1)),
        Edit::Create { name, then_add } => match client.create_playlist(&name).await {
            Ok(uuid) => match then_add {
                Some(track) => client
                    .add_track_to_playlist(&uuid, &track)
                    .await
                    .map(|()| (format!("added to {name}"), uuid, false, 0)),
                None => Ok((format!("created {name}"), uuid, false, 0)),
            },
            Err(e) => Err(e),
        },
        Edit::Rename { uuid, name } => client
            .rename_playlist(&uuid, &name)
            .await
            .map(|()| (format!("renamed to {name}"), uuid, false, 0)),
        Edit::Delete { uuid } => client
            .delete_playlist(&uuid)
            .await
            .map(|()| ("playlist deleted".to_string(), uuid, true, 0)),
    }
}

fn remove_from_playlist(
    app: &App,
    uuid: &str,
    track: &crate::domain::TrackId,
    action_tx: &tokio::sync::mpsc::UnboundedSender<Action>,
) {
    if let Some(session) = &app.session {
        let (client, tx) = (std::sync::Arc::clone(session), action_tx.clone());
        let (uuid, track) = (uuid.to_owned(), track.clone());
        tokio::spawn(async move {
            let done = client
                .remove_track_from_playlist(&uuid, &track)
                .await
                .map(|()| ("taken out of the playlist".to_string(), uuid, false, -1));
            finish_edit(&client, &tx, done).await;
        });
    }
}

fn toggle_favourite(app: &App, action_tx: &tokio::sync::mpsc::UnboundedSender<Action>) {
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

fn activate_selection(app: &mut App, cmd_tx: &std::sync::mpsc::Sender<crate::playback::Cmd>) {
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
    start_track(app, selected, cmd_tx);
}

/// Autoplay: the queue is nearly out. The station built on its last
/// track is fetched and appended -- so what follows is a queue like any
/// other and the skip keys work on it.
fn autoplay(
    app: &App,
    seed: &crate::domain::TrackId,
    action_tx: &tokio::sync::mpsc::UnboundedSender<Action>,
) {
    if let Some(session) = &app.session {
        let (client, tx) = (std::sync::Arc::clone(session), action_tx.clone());
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

fn run_search(app: &App, query: &str, action_tx: &tokio::sync::mpsc::UnboundedSender<Action>) {
    let query = query.to_owned();
    if let Some(session) = &app.session {
        let (client, t) = (std::sync::Arc::clone(session), action_tx.clone());
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

fn load_home(app: &App, action_tx: &tokio::sync::mpsc::UnboundedSender<Action>) {
    if let Some(session) = &app.session {
        let (client, t) = (std::sync::Arc::clone(session), action_tx.clone());
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

fn load_feed(app: &App, action_tx: &tokio::sync::mpsc::UnboundedSender<Action>) {
    if let Some(session) = &app.session {
        let (client, t) = (std::sync::Arc::clone(session), action_tx.clone());
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

fn load_explore(app: &App, action_tx: &tokio::sync::mpsc::UnboundedSender<Action>) {
    if let Some(session) = &app.session {
        let (client, t) = (std::sync::Arc::clone(session), action_tx.clone());
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

fn toggle_pause(app: &App, cmd_tx: &std::sync::mpsc::Sender<crate::playback::Cmd>) {
    let cmd = if app.now_playing.playing {
        crate::playback::Cmd::Pause
    } else {
        crate::playback::Cmd::Resume
    };
    let _ = cmd_tx.send(cmd);
}

fn look_for_cdm(action_tx: &tokio::sync::mpsc::UnboundedSender<Action>) {
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

/// Download, show the terms, wait for the word, install. The archive
/// stays in this task: the screen only sees the terms.
fn download_cdm(app: &mut App, action_tx: &tokio::sync::mpsc::UnboundedSender<Action>) {
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
        install_cdm(offer, &tx).await;
    });
}

async fn install_cdm(offer: music::drm::Offer, tx: &tokio::sync::mpsc::UnboundedSender<Action>) {
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
}

fn load_lyrics(
    app: &App,
    id: &crate::domain::TrackId,
    action_tx: &tokio::sync::mpsc::UnboundedSender<Action>,
) {
    if let (Some(session), Some(track)) = (&app.session, app.now_playing.track.clone()) {
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

/// A fresh engine on the same session, picking the track back up where
/// it was, paused if it was paused.
/// The outputs the system offers, read off the loop: listing opens
/// devices on some systems.
fn list_outputs(action_tx: &tokio::sync::mpsc::UnboundedSender<Action>) {
    let tx = action_tx.clone();
    tokio::spawn(async move {
        if let Ok(devices) = tokio::task::spawn_blocking(music::output::devices).await {
            let _ = tx.send(Action::OutputsListed(settings::listed(devices)));
        }
    });
}

/// The output and sample-rate following the config names, before anything
/// plays. One that is not connected gives way to the system's default.
pub(crate) fn choose_output(audio: &crate::config::AudioConfig) {
    music::output::choose_device(Some(audio.output.clone()).filter(|id| !id.is_empty()));
    music::output::choose_match_rate(audio.match_rate);
}

/// Opens the output the config names, off the loop since CoreAudio can
/// take ten seconds to give up on a device, and plays on it once it has
/// opened. The stream it opened is held a moment, for the engine starting
/// again to join rather than open the device a second time.
fn use_output(app: &App, action_tx: &tokio::sync::mpsc::UnboundedSender<Action>) {
    let id = Some(app.config.audio.output.clone()).filter(|id| !id.is_empty());
    let label = settings::output_label(&app.config.audio.output, &app.outputs);
    let tx = action_tx.clone();
    tokio::spawn(async move {
        let wanted = id.clone();
        let opened = tokio::task::spawn_blocking(move || music::output::open(wanted.as_deref()))
            .await
            .map_err(anyhow::Error::from)
            .and_then(|opened| opened);
        match opened {
            Ok(held) => {
                music::output::choose_device(id);
                let _ = tx.send(Action::OutputOpened(Ok(label)));
                tokio::time::sleep(Duration::from_secs(2)).await;
                drop(held);
            }
            Err(error) => {
                let _ = tx.send(Action::OutputOpened(Err(format!(
                    "could not open {label}: {error:#}"
                ))));
            }
        }
    });
}

fn restart_engine(app: &App, cmd_tx: &std::sync::mpsc::Sender<crate::playback::Cmd>) {
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

fn seek(app: &mut App, forward: bool, cmd_tx: &std::sync::mpsc::Sender<crate::playback::Cmd>) {
    if let Some(to) = app.seek_target(forward) {
        let _ = cmd_tx.send(crate::playback::Cmd::Seek(to));
        // Discord's clock starts again from where this lands.
        app.presence_shown = None;
    }
}

async fn shut_down(app: &mut App) {
    if let Some(presence) = &app.presence {
        let _ = presence.send(crate::presence::Presence::Off);
    }
    // On the way out: the listen in progress, if it earned it, and whatever
    // is waiting. Awaited, briefly, since nothing runs after this.
    app.settle_listen(app.now_playing.position);
    let sending = crate::scrobble::drain(&app.scrobblers, &mut app.scrobble_outbox);
    if !sending.is_empty() {
        let _ =
            tokio::time::timeout(Duration::from_secs(3), futures::future::join_all(sending)).await;
    }
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
            let session =
                crate::catalogue::Session::open(std::sync::Arc::clone(&provider), session);
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
            let session =
                crate::catalogue::Session::open(std::sync::Arc::clone(&provider), session);
            let _ = tx.send(Action::SessionOpened(std::sync::Arc::new(session)));
        }
        Err(e) => {
            tracing::warn!("could not sign in to {name}: {e:#}");
            let _ = tx.send(Action::Error(format!("{e:#}")));
        }
    }
}

/// Make every cell of a wiped buffer differ from a blank one, so the draw
/// that diffs against it writes its blank cells too.
///
/// A cover is one escape sequence in its top-left cell that paints the
/// whole rectangle, and the diff goes row by row, so whatever sits over
/// the cover is written after it -- but only if it is written at all. A
/// modal's empty cells equal the wiped buffer's and are left out, and the
/// cover comes back in full underneath the modal that is meant to hide it.
/// Make the next draw write every cell, after clearing the screen when
/// `how` asks for a wipe.
///
/// Not `Terminal::clear`: since ratatui 0.30 it reads the cursor position
/// back from the terminal first, and with the event stream holding stdin
/// that read times out after two seconds and the error ends the run. The
/// buffer the next draw diffs against is emptied, which is the rest of what
/// `clear` does.
fn repaint<B: ratatui::backend::Backend>(
    terminal: &mut ratatui::Terminal<B>,
    how: Repaint,
) -> Result<(), B::Error> {
    if how == Repaint::None {
        return Ok(());
    }
    if how == Repaint::Wipe {
        terminal
            .backend_mut()
            .clear_region(ratatui::backend::ClearType::All)?;
    }
    terminal.swap_buffers();
    terminal.current_buffer_mut().reset();
    unsettle(terminal.current_buffer_mut());
    terminal.swap_buffers();
    Ok(())
}

fn unsettle(buf: &mut ratatui::buffer::Buffer) {
    use ratatui::buffer::CellDiffOption;
    for cell in &mut buf.content {
        cell.set_diff_option(CellDiffOption::Skip);
    }
}

#[cfg(test)]
mod tests {
    use super::{after_window, open_sign_in_window, repaint, unsettle};
    use crate::shell::signin_window::Outcome;
    use crate::shell::{Action, App};

    #[test]
    fn a_sign_in_window_answers_with_its_cookies_and_anything_else_brings_the_box_back() {
        assert!(matches!(
            after_window(Outcome::Cookies("arl=q".into())),
            Action::Answer(header) if header == "arl=q"
        ));
        for outcome in [Outcome::Closed, Outcome::Failed("no webkit".into())] {
            assert!(matches!(after_window(outcome), Action::PasteCookies));
        }
    }

    #[test]
    fn with_no_window_to_open_the_paste_box_comes_back() {
        // No provider, no sign-in under way, no main-thread host: the
        // screen must not be left waiting on a window that never comes.
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        open_sign_in_window(&App::default(), &tx);
        assert!(matches!(rx.try_recv(), Ok(Action::PasteCookies)));
    }

    use crate::shell::Repaint;
    use ratatui::backend::{Backend as _, TestBackend};
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::widgets::Paragraph;
    use ratatui::Terminal;

    /// A terminal showing "lyrics" in its first cells, as the last frame left it.
    fn shown() -> Terminal<TestBackend> {
        let mut terminal = Terminal::new(TestBackend::new(6, 1)).unwrap();
        terminal
            .draw(|f| f.render_widget(Paragraph::new("lyrics"), f.area()))
            .unwrap();
        terminal
    }

    #[test]
    fn a_redraw_leaves_the_screen_up_and_writes_every_cell_again() {
        // A wipe blanks the terminal until the next frame lands, and a large
        // cover takes the terminal a moment to put back: a blank flash.
        let mut terminal = shown();
        repaint(&mut terminal, Repaint::Redraw).unwrap();
        assert_eq!(
            terminal.backend().buffer()[(0, 0)].symbol(),
            "l",
            "nothing wiped"
        );
        // The screen no longer matches the last frame -- as when a modal
        // went over cells an image protocol marks `Skip`. The same frame
        // again writes nothing by diff; after a redraw it writes it all.
        let mut terminal = shown();
        terminal
            .backend_mut()
            .clear_region(ratatui::backend::ClearType::All)
            .unwrap();
        repaint(&mut terminal, Repaint::Redraw).unwrap();
        terminal
            .draw(|f| f.render_widget(Paragraph::new("lyrics"), f.area()))
            .unwrap();
        assert_eq!(
            super::super::geometry::row(terminal.backend().buffer(), 0),
            "lyrics",
            "every cell written again"
        );
    }

    #[test]
    fn a_wipe_clears_the_screen_and_none_leaves_it() {
        let mut terminal = shown();
        repaint(&mut terminal, Repaint::None).unwrap();
        assert_eq!(terminal.backend().buffer()[(0, 0)].symbol(), "l");
        repaint(&mut terminal, Repaint::Wipe).unwrap();
        assert_eq!(terminal.backend().buffer()[(0, 0)].symbol(), " ");
    }

    #[test]
    fn a_config_that_cannot_be_read_is_named_in_the_status_and_one_that_can_says_nothing() {
        let good = crate::config::Config::default();
        let app = super::app_for(&good);
        assert_eq!(app.status, None, "a clean config says nothing");
        assert_eq!(app.status_since, None);

        let mut keys = good.clone();
        keys.keys.insert("down".into(), "not a key".into());
        let app = super::app_for(&keys);
        let status = app.status.unwrap_or_default();
        assert!(status.starts_with("config: "), "{status}");
        assert!(status.contains("not a key"), "the key is named: {status}");
        assert!(app.status_since.is_some(), "stamped, so it clears");

        let mut colours = good.clone();
        colours.ui.colors.insert("accent".into(), "blue-ish".into());
        let status = super::app_for(&colours).status.unwrap_or_default();
        assert!(status.contains("blue-ish"), "the colour is named: {status}");
    }

    #[test]
    fn the_app_starts_on_the_keys_and_colours_the_config_names() {
        use crossterm::event::KeyCode;
        let mut config = crate::config::Config::default();
        config.keys.insert("down".into(), "J".into());
        config.ui.colors.insert("accent".into(), "#ff0000".into());
        let app = super::app_for(&config);
        assert_eq!(app.config.keys, config.keys, "the config is kept to edit");
        assert_eq!(app.keymap.resolve(KeyCode::Char('J')), KeyCode::Char('j'));
        assert_eq!(app.palette.accent, ratatui::style::Color::Rgb(255, 0, 0));
    }

    #[test]
    fn a_blank_cell_over_a_wiped_screen_is_still_written() {
        let area = Rect::new(0, 0, 3, 1);
        let next = Buffer::empty(area);
        let mut wiped = Buffer::empty(area);
        assert!(wiped.diff(&next).is_empty(), "a plain wipe writes nothing");
        unsettle(&mut wiped);
        assert_eq!(wiped.diff(&next).len(), 3, "every blank cell is written");
    }
}
