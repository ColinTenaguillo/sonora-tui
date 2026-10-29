use super::*;

#[test]
fn a_scrobbler_that_refused_its_token_is_said_in_the_status_line() {
    // A token typed wrong in Settings read "set" and nothing else: the
    // refusal went to the log, which nobody typing a token is reading.
    let mut app = App::default();
    app.update(Action::ScrobblersLinked(
        Vec::new(),
        vec!["listenbrainz refused the token".to_string()],
    ));
    assert_eq!(
        app.status.as_deref(),
        Some("listenbrainz refused the token"),
        "the refusal is said even when nothing was linked before"
    );
    // Nothing refused and nothing linked, at start: nothing said.
    let mut quiet = App::default();
    quiet.update(Action::ScrobblersLinked(Vec::new(), Vec::new()));
    assert_eq!(quiet.status, None);
}

#[test]
fn browse_keys_do_nothing_before_login() {
    // Every browse arm is guarded on an existing session; without the
    // guards Enter would fire ActivateSelection in every state but
    // logged-out.
    let mut app = App::default();
    assert!(app.session.is_none());

    for code in [KeyCode::Char('j'), KeyCode::Char('k'), KeyCode::Char(' ')] {
        let action = app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
        assert!(action.is_none(), "{code:?} must be inert before login");
    }
    // Two exceptions: logged out, Enter starts the login flow and Tab
    // moves to the next service.
    assert!(matches!(
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        Some(Action::BeginLogin)
    ));
    assert!(matches!(
        app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
        Some(Action::NextService)
    ));
}

#[test]
fn the_help_is_not_offered_before_signing_in() {
    // The login screen has its own instructions; a key list of bindings
    // that do nothing yet would only confuse.
    let mut app = App::default();
    key(&mut app, KeyCode::Char('?'));
    assert!(!app.showing_help);
}

#[test]
fn the_settings_view_draws_what_the_config_holds() {
    // Read off the buffer, not off the state: every view added so far
    // has had at least one bug where the keys drove something other
    // than what was on screen.
    let mut app = signed_in(sidebar::Section::Settings);
    app.config
        .audio
        .set_quality(crate::domain::Quality::Lossless);

    // Wide enough for the label column and a value beside it: the
    // labels run to "Allow AI-generated content", and the sidebar takes
    // 26 columns before the pane starts.
    let buf = geometry::draw(110, 20, |f, _area, _p| draw(f, &mut app));
    let text = geometry::text(&buf);
    assert!(text.contains("Settings"), "the heading:\n{text}");
    assert!(
        text.contains(settings::describe(crate::domain::Quality::Lossless)),
        "the configured quality is what is shown:\n{text}"
    );
}

#[test]
fn playback_asks_for_the_configured_quality() {
    // The setting existed in the file for weeks and was read nowhere:
    // the request site named HI_RES_LOSSLESS outright, so changing it
    // did nothing at all. The request itself now lives in the engine's
    // fetch, which has the matching check; this is the half that hands
    // it the setting.
    let source = include_str!("../run.rs");
    assert!(
        source.contains("app.config.audio.quality()"),
        "no request site reads the configured quality"
    );
}

#[test]
fn a_changed_setting_is_written_to_the_file() {
    // There is no "apply", so a setting that only lived in memory would
    // be silently lost on quit.
    let dir = std::env::temp_dir().join("sonora-tui-test-settings");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("config.toml");
    let _ = std::fs::remove_file(&path);

    let mut app = signed_in(sidebar::Section::Settings);
    app.settings.next(); // past Service, onto Quality
    app.config_path = Some(path.clone());
    app.config
        .audio
        .set_quality(crate::domain::Quality::HiResLossless);

    app.update(Action::CarouselNext);

    let written = std::fs::read_to_string(&path).expect("the file was written");
    let back: crate::config::Config = toml::from_str(&written).expect("valid TOML");
    assert_eq!(
        back.audio.quality(),
        app.config.audio.quality(),
        "the file holds what the view shows"
    );
    assert_ne!(
        back.audio.quality(),
        crate::domain::Quality::HiResLossless,
        "and it actually changed"
    );
}

#[test]
fn the_settings_keys_do_not_reach_the_view_behind_it() {
    // The fault this app keeps making: a new view is drawn but the keys
    // still drive the last one. Moving in Settings must not disturb the
    // album grid, and opening a collection must take the keys back.
    let mut app = signed_in(sidebar::Section::Settings);
    app.albums = vec![crate::library::Album {
        id: (1).to_string(),
        title: "An Album".into(),
        artist: "An Artist".into(),
        year: None,
        cover: None,
        track_count: 1,
        duration: None,
    }];
    app.album_grid.selected = 0;

    assert!(app.on_settings());
    assert!(!app.on_grid(), "Settings is not a card grid");

    app.update(Action::TrackNext);
    assert_eq!(
        app.album_grid.selected, 0,
        "the grid behind it did not move"
    );
}

#[test]
fn a_text_setting_is_typed_in_its_box_and_written_on_enter() {
    let mut app = signed_in(sidebar::Section::Settings);
    while app.settings.current() != settings::Setting::Discord {
        app.settings.next();
    }
    app.update(Action::ActivateSelection);
    assert!(matches!(
        &app.setting_edit,
        Some(settings::Edit {
            setting: settings::Setting::Discord,
            ..
        })
    ));
    for c in "12q".chars() {
        assert!(
            key(&mut app, KeyCode::Char(c)).is_none(),
            "{c} is typed, q included"
        );
    }
    key(&mut app, KeyCode::Backspace);
    let typed = match key(&mut app, KeyCode::Enter) {
        Some(Action::SettingTyped(settings::Setting::Discord, text)) => text,
        other => panic!("Enter sends the text, got {other:?}"),
    };
    assert_eq!(typed, "12");
    assert!(app.setting_edit.is_none());
    let next = app.update(Action::SettingTyped(settings::Setting::Discord, typed));
    assert!(matches!(
        next,
        Some(Action::SettingChanged(settings::Setting::Discord))
    ));
    assert_eq!(app.config.discord.application_id, "12");

    // Esc leaves the value as it was.
    app.update(Action::ActivateSelection);
    for c in "9".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    key(&mut app, KeyCode::Esc);
    assert!(app.setting_edit.is_none());
    assert_eq!(app.config.discord.application_id, "12");

    // The folders row: Enter adds, x drops the last, and on the local
    // service the library is read again.
    while app.settings.current() != settings::Setting::LocalFolders {
        app.settings.next();
    }
    app.config.provider.kind = crate::config::ProviderKind::Local;
    let next = app.update(Action::SettingTyped(
        settings::Setting::LocalFolders,
        "~/Music".into(),
    ));
    assert!(matches!(
        next,
        Some(Action::SettingChanged(settings::Setting::LocalFolders))
    ));
    assert!(matches!(
        app.update(Action::SettingChanged(settings::Setting::LocalFolders)),
        Some(Action::SwitchProvider)
    ));
    assert_eq!(app.config.local.folders, ["~/Music"]);
}

#[test]
fn a_service_without_a_feed_says_so_instead_of_a_blank_page() {
    assert!(sample_session().has_feed(), "TIDAL has one");
    let text: String = crate::shell::draw::feed_notice("Deezer")
        .iter()
        .map(|l| l.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains("Deezer has no feed"), "{text}");
    assert!(text.contains("TIDAL"), "and says where one is:\n{text}");
}

#[test]
fn session_expired_clears_the_session_and_tracks_and_shows_the_login_modal() {
    let mut app = App {
        session: Some(sample_session()),
        tracks: vec![crate::domain::Track::sample(
            "t",
            "a",
            std::time::Duration::ZERO,
        )],
        ..App::default()
    };

    app.update(Action::SessionExpired);

    assert!(app.session.is_none(), "the stale session must be dropped");
    assert!(app.tracks.is_empty(), "stale tracks must not linger");
    assert!(
        matches!(app.login, login::LoginState::Failed(_)),
        "the login modal must reappear so Enter works again"
    );
}

#[test]
fn enter_on_a_subsonic_config_asks_for_the_password_and_nothing_else() {
    // The device flow is TIDAL's. With a Subsonic server named, Enter
    // opens the password prompt, and every key after that is a letter
    // of the password -- `q` included -- until Enter sends it.
    let mut app = App::default();
    app.config.provider.kind = crate::config::ProviderKind::Subsonic;
    app.config.subsonic.server = "music.example.org".into();
    app.config.subsonic.username = "colin".into();

    assert!(matches!(
        key(&mut app, KeyCode::Enter),
        Some(Action::BeginLogin)
    ));
    app.update(Action::BeginLogin);
    assert!(matches!(app.login, login::LoginState::Password { .. }));

    for c in ['q', 'u', 'i', 't'] {
        assert!(
            key(&mut app, KeyCode::Char(c)).is_none(),
            "{c} is typed, not bound"
        );
    }
    assert!(key(&mut app, KeyCode::Backspace).is_none());
    match key(&mut app, KeyCode::Enter) {
        Some(Action::Connect(secret)) => assert_eq!(secret.0, "qui"),
        other => panic!("Enter sends the password, got {other:?}"),
    }
    assert!(
        matches!(&app.login, login::LoginState::Password { typed, .. } if typed.0.is_empty()),
        "the prompt stays up, emptied, while the connection is made"
    );
}

#[test]
fn tab_on_the_sign_in_screen_moves_to_the_next_service_without_signing_in() {
    // A failed sign-in offered retry or quit, and quitting was the only
    // way to another service: Settings is behind the session.
    let mut app = App {
        login: login::LoginState::Failed("no network".into()),
        ..App::default()
    };
    assert!(matches!(
        key(&mut app, KeyCode::Tab),
        Some(Action::NextService)
    ));
    let next = app.update(Action::NextService);
    assert_eq!(
        app.config.provider.kind,
        crate::config::ProviderKind::Spotify
    );
    assert!(
        matches!(app.login, login::LoginState::Idle),
        "{:?}",
        app.login
    );
    assert!(next.is_none(), "nothing signs in until Enter, got {next:?}");

    // A sign-in waiting on the browser is left the same way.
    app.login = login::LoginState::Waiting {
        code: "ABCDE".into(),
        url: "https://link.tidal.com/ABCDE".into(),
    };
    assert!(
        key(&mut app, KeyCode::Char('q')).is_none(),
        "q is swallowed while waiting"
    );
    assert!(matches!(
        key(&mut app, KeyCode::Tab),
        Some(Action::NextService)
    ));

    // A folder of files already named is read at once.
    app.config.provider.kind = crate::config::ProviderKind::Subsonic;
    app.config.local.folders = vec!["~".into()];
    assert!(matches!(
        app.update(Action::NextService),
        Some(Action::BeginLogin)
    ));
    assert_eq!(app.config.provider.kind, crate::config::ProviderKind::Local);
}

#[test]
fn a_subsonic_config_without_a_server_asks_for_it_then_the_user_then_the_password() {
    let mut app = App::default();
    app.config.provider.kind = crate::config::ProviderKind::Subsonic;
    app.update(Action::BeginLogin);
    assert!(matches!(
        &app.login,
        login::LoginState::Text { field: login::Field::Server, typed } if typed == "https://"
    ));
    let next = app.update(Action::Typed(
        login::Field::Server,
        "https://music.example.org".into(),
    ));
    assert!(matches!(next, Some(Action::BeginLogin)));
    app.update(Action::BeginLogin);
    assert!(matches!(
        &app.login,
        login::LoginState::Text {
            field: login::Field::Username,
            ..
        }
    ));
    app.update(Action::Typed(login::Field::Username, "colin".into()));
    app.update(Action::BeginLogin);
    assert!(matches!(
        &app.login,
        login::LoginState::Password { server, username, .. }
            if server == "https://music.example.org" && username == "colin"
    ));
    assert_eq!(app.config.subsonic.server, "https://music.example.org");
    assert_eq!(app.config.subsonic.username, "colin");
}

#[test]
fn a_session_opened_elsewhere_is_readied_like_a_login() {
    let mut app = App::default();
    let next = app.update(Action::SessionOpened(sample_session()));
    assert!(matches!(next, Some(Action::SessionReady)));
    assert!(app.session.is_some());
}

#[test]
fn without_a_session_an_error_routes_to_the_login_modal() {
    // status is drawn under the full-screen login modal and is invisible
    // there; the message must go somewhere the user can actually see it.
    let mut app = App::default();
    assert!(app.session.is_none());

    app.update(Action::Error("network unreachable".into()));

    assert!(
        app.status.is_none(),
        "status is painted over and must stay empty"
    );
    match app.login {
        login::LoginState::Failed(ref msg) => assert_eq!(msg, "network unreachable"),
        other => panic!("expected LoginState::Failed, got {other:?}"),
    }
}

#[test]
fn with_a_session_an_error_still_routes_to_the_status_bar() {
    let mut app = App {
        session: Some(sample_session()),
        ..App::default()
    };

    app.update(Action::Error("playback failed".into()));

    assert_eq!(app.status.as_deref(), Some("playback failed"));
    assert!(
        matches!(app.login, login::LoginState::Idle),
        "an in-session error must not disturb login state"
    );
}

#[test]
fn a_cookie_sign_in_takes_what_is_pasted_and_sends_it_as_the_answer() {
    // Deezer, Apple and YouTube want the cookies of a signed-in tab.
    // Enter contacts the service; its Secret prompt opens the box;
    // every key after that is part of the paste -- `q` included --
    // until Enter sends it and the box is taken down.
    let mut app = App::default();
    app.config.provider.kind = crate::config::ProviderKind::Deezer;

    assert!(matches!(
        key(&mut app, KeyCode::Enter),
        Some(Action::BeginLogin)
    ));
    app.update(Action::BeginLogin);
    assert!(
        matches!(app.login, login::LoginState::Busy(_)),
        "{:?}",
        app.login
    );
    assert!(
        key(&mut app, KeyCode::Enter).is_none(),
        "Enter while the service is contacted does not start a second sign-in"
    );

    app.update(Action::LoginPrompt(music::SignInPrompt::Secret));
    assert!(matches!(app.login, login::LoginState::Secret { .. }));
    for c in ['a', 'r', 'l', '=', 'q'] {
        assert!(
            key(&mut app, KeyCode::Char(c)).is_none(),
            "{c} is typed, not bound"
        );
    }
    match key(&mut app, KeyCode::Enter) {
        Some(Action::Answer(text)) => assert_eq!(text, "arl=q"),
        other => panic!("Enter sends the cookies, got {other:?}"),
    }
    app.update(Action::Answer("arl=q".into()));
    assert!(
        matches!(app.login, login::LoginState::Busy(_)),
        "{:?}",
        app.login
    );
}

#[test]
fn a_link_prompt_waits_on_the_browser_and_a_guest_key_needs_a_guest_service() {
    let mut app = App::default();
    app.config.provider.kind = crate::config::ProviderKind::Spotify;
    app.update(Action::LoginPrompt(music::SignInPrompt::Url(
        "https://accounts.spotify.com/authorize".into(),
    )));
    assert!(matches!(app.login, login::LoginState::Url(_)));
    assert!(
        key(&mut app, KeyCode::Enter).is_none(),
        "nothing to do but wait"
    );
    // Esc steps back to the start of the screen, as everywhere else;
    // it quit the app, which nothing else in it does.
    assert!(matches!(
        key(&mut app, KeyCode::Esc),
        Some(Action::CancelLogin)
    ));
    app.update(Action::CancelLogin);
    assert!(matches!(app.login, login::LoginState::Idle));
    assert!(
        matches!(key(&mut app, KeyCode::Char('q')), Some(Action::Quit)),
        "q still quits"
    );

    // `g` is a guest sign-in only where the service has one.
    let mut app = App::default();
    let g = KeyCode::Char(App::GUEST_KEY);
    assert!(key(&mut app, g).is_none(), "TIDAL has no guest");
    app.provider = Some(std::sync::Arc::new(music::youtube::YouTubeProvider::new()));
    assert!(app.guest_allowed());
    assert!(matches!(key(&mut app, g), Some(Action::BeginGuest)));
    app.update(Action::BeginGuest);
    assert!(matches!(app.login, login::LoginState::Busy(_)));
}

#[test]
fn a_protected_service_asks_for_the_module_and_the_keys_answer_it() {
    // Apple Music's streams need Widevine; TIDAL's do not, so TIDAL
    // never has the machine searched.
    let mut app = signed_in(sidebar::Section::Tracks);
    assert!(
        app.update(Action::SessionReady).is_none(),
        "no provider, nothing to look for"
    );

    app.provider = Some(std::sync::Arc::new(music::apple::AppleProvider::new()));
    assert!(matches!(
        app.update(Action::SessionReady),
        Some(Action::LookForCdm)
    ));
    assert_eq!(app.cdm, drmview::Cdm::Looking);
    assert!(
        app.update(Action::SessionReady).is_none(),
        "searched once a run"
    );

    app.update(Action::CdmFound(None));
    assert_eq!(app.cdm, drmview::Cdm::Wanted);
    assert!(
        key(&mut app, KeyCode::Char('q')).is_none(),
        "the question takes every key"
    );
    // Spelt as a constant: the help's scan reads every `Char('x')` in
    // this file as a binding, and this one is the modal's own.
    const YES: char = 'y';
    let asked = key(&mut app, KeyCode::Char(YES)).and_then(|a| app.update(a));
    assert!(matches!(asked, Some(Action::DownloadCdm)), "got {asked:?}");
    assert_eq!(app.cdm, drmview::Cdm::Offering);

    app.update(Action::CdmOffered {
        version: "4.10".into(),
        license: "terms".into(),
    });
    assert!(app.cdm.asking());
    assert!(matches!(
        key(&mut app, KeyCode::Char('n')),
        Some(Action::AnswerCdm(false))
    ));
    app.update(Action::AnswerCdm(false));
    assert_eq!(app.cdm, drmview::Cdm::Declined);
    assert!(!app.cdm.shown(), "and the screen is clear again");

    let mut app = signed_in(sidebar::Section::Tracks);
    app.update(Action::CdmFound(Some(music::drm::Origin::Installed)));
    assert_eq!(app.cdm, drmview::Cdm::Ready(music::drm::Origin::Installed));
    assert!(app.status.as_deref().unwrap_or("").contains("browser"));
}

#[test]
fn the_service_row_picks_a_service_and_enter_switches_to_it() {
    let mut app = signed_in(sidebar::Section::Settings);
    app.config_path = None;
    assert_eq!(
        app.settings.current(),
        settings::Setting::Service,
        "the first row"
    );
    app.now_playing.track = Some(crate::domain::Track::sample(
        "Song",
        "A",
        std::time::Duration::from_secs(1),
    ));
    app.tracks = some_tracks(&["One"]);

    // Picking changes the config and nothing else yet.
    app.update(Action::CarouselNext);
    assert_eq!(
        app.config.provider.kind,
        crate::config::ProviderKind::Spotify
    );
    assert!(app.session.is_some(), "still signed in to the old one");
    assert!(app.status.as_deref().unwrap_or("").contains("Spotify"));

    // Enter is the switch: the session, the lists and the bar go, the
    // config stays, and the new service is asked to sign in.
    let next = app.update(Action::ActivateSelection);
    assert!(matches!(next, Some(Action::SwitchProvider)), "got {next:?}");
    // Stamped a few opens on the old service.
    app.requests = 7;
    let next = app.update(Action::SwitchProvider);
    assert!(
        app.requests >= 7,
        "the request counter survives the switch: the loop drops any open stamped below what it last fetched"
    );
    assert!(matches!(next, Some(Action::BeginLogin)), "got {next:?}");
    assert!(app.session.is_none());
    assert!(app.tracks.is_empty());
    assert!(app.now_playing.track.is_none());
    assert_eq!(
        app.config.provider.kind,
        crate::config::ProviderKind::Spotify
    );
    assert_eq!(app.provider.as_ref().map(|p| p.name()), Some("Spotify"));
    assert_eq!(
        app.settings.current(),
        settings::Setting::Service,
        "the row keeps its place"
    );
}

#[test]
fn the_equaliser_setting_reaches_the_shared_equaliser_at_once() {
    let mut app = signed_in(sidebar::Section::Settings);
    app.config_path = None;
    app.config.audio.equalizer = "rock".into();
    app.apply_equalizer();
    assert!(app.equalizer.enabled());
    assert_eq!(
        app.equalizer.gains(),
        music::equalizer::Preset::Rock.gains()
    );

    app.config.audio.equalizer = "off".into();
    app.apply_equalizer();
    assert!(!app.equalizer.enabled());
    assert_eq!(
        app.equalizer.gains(),
        music::equalizer::Preset::Flat.gains(),
        "flat for next time"
    );
}
