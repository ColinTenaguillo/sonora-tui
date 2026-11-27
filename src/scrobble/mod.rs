//! Scrobbling: telling Last.fm, Libre.fm, ListenBrainz, Maloja or teal.fm
//! what played.
//!
//! The services and their protocols come from sonora's `scrobble` crate. This
//! links the ones the config names, keeps their accounts in a file beside
//! the token, turns a track into a listen, and sends what the shell puts in
//! the outbox. Whether a listen counts is the services' own rule, and lives
//! with them: thirty seconds at least, and half the track or four minutes,
//! which a service may narrow, as teal.fm does for a short track.
//!
//! A submission that fails is kept, per service, and goes out again with
//! the next one -- a listen earned on a train is filed when the network
//! is back. Kept in memory: the app's last drain on the way out is the
//! last try, and a backlog is capped so a service that is gone for good
//! cannot grow one without end.

use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use scrobbling::{Account, Play, Secret, Service};

use crate::config::ScrobbleConfig;
use crate::domain::Track;

/// A service with the account it is linked to, and whether listens go to
/// it: the switch of its row in Settings.
#[derive(Clone)]
pub struct Linked {
    pub service: Arc<dyn Service>,
    pub account: Account,
    pub enabled: bool,
}

/// An account as the file keeps it. The switch sits beside the account's
/// own fields, where sonora's `Account` had it until it moved to sonora's
/// settings: a file written before reads the same.
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
struct Kept {
    #[serde(flatten)]
    account: Account,
    #[serde(default)]
    enabled: bool,
}

impl std::fmt::Debug for Linked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} as {}", self.service.id(), self.account.name)
    }
}

impl Linked {
    /// What the status line calls the account: the service's name, or the
    /// server's for one the user hosts.
    pub fn label(&self) -> String {
        match self.service.id() {
            "lastfm" => "Last.fm".to_string(),
            "librefm" => "Libre.fm".to_string(),
            "tealfm" => "teal.fm".to_string(),
            "listenbrainz" => match host(&self.account.server) {
                "" => "ListenBrainz".to_string(),
                host => format!("ListenBrainz on {host}"),
            },
            _ if !self.account.name.is_empty() => self.account.name.clone(),
            other => other.to_string(),
        }
    }
}

/// What the config gives a service to link with, or nothing when it does
/// not name the service.
pub fn secret_for(config: &ScrobbleConfig, id: &str) -> Option<Secret> {
    match id {
        "lastfm" => config.lastfm.as_ref().map(|keys| Secret::Keys {
            key: keys.key.clone(),
            secret: keys.secret.clone(),
        }),
        "librefm" => config.librefm.then_some(Secret::None),
        "listenbrainz" => config
            .listenbrainz
            .as_ref()
            .filter(|t| !t.trim().is_empty())
            .map(|t| Secret::Token {
                token: t.clone(),
                // Empty is listenbrainz.org.
                server: config.listenbrainz_server.clone().unwrap_or_default(),
            }),
        "maloja" => config.maloja.as_ref().map(|m| Secret::Server {
            url: m.url.clone(),
            key: m.key.clone(),
        }),
        // Half of one is what the Settings rows hold between the first and
        // the second: not tried, so not refused in the status line.
        "tealfm" => config
            .tealfm
            .as_ref()
            .filter(|t| !t.handle.trim().is_empty() && !t.password.trim().is_empty())
            .map(|t| Secret::Password {
                identifier: t.handle.clone(),
                password: t.password.clone(),
            }),
        _ => None,
    }
}

/// Whether the config names any service at all, so a run without one
/// never opens the account file or says anything about scrobbling.
pub fn configured(config: &ScrobbleConfig) -> bool {
    scrobbling::services()
        .iter()
        .any(|s| secret_for(config, s.id()).is_some())
}

/// The services the config names, linked: from the account file where one
/// is kept, by `connect` otherwise -- which for Last.fm and Libre.fm opens
/// the browser and waits for the approval, one after the other, since both
/// listen on the same port for it. The accounts are written back.
///
/// `fresh` is the one service a Settings row just changed: only it goes
/// through `connect`, and the others come from the file or wait for the
/// next start. Without it, Enter on the Last.fm row sent an unapproved
/// Libre.fm back to the browser first. At start it is `None`: every
/// service is connected.
///
/// With the accounts come the refusals, in words for the status line: a
/// token typed wrong in Settings showed "set" and nothing else, while the
/// log alone knew it had been turned away.
pub async fn link(
    config: &ScrobbleConfig,
    file: Option<&Path>,
    fresh: Option<&str>,
) -> (Vec<Linked>, Vec<String>) {
    let stored = file.map(load).unwrap_or_default();
    let mut linked = Vec::new();
    let mut refused = Vec::new();
    for service in scrobbling::services() {
        let Some(secret) = secret_for(config, service.id()) else {
            continue;
        };
        if let Some(kept) = still_linked(&stored, service.id(), &secret, fresh) {
            linked.push(Linked {
                service,
                account: kept.account.clone(),
                enabled: kept.enabled,
            });
            continue;
        }
        if fresh.is_some_and(|id| id != service.id()) {
            continue;
        }
        match service.connect(secret).await {
            Ok(account) => {
                tracing::info!("scrobbling: linked {} as {}", service.id(), account.name);
                linked.push(Linked {
                    service,
                    account,
                    enabled: true,
                });
            }
            Err(error) => {
                tracing::warn!("scrobbling: could not link {}: {error:#}", service.id());
                refused.push(format!("{error:#}"));
            }
        }
    }
    store(file, &linked);
    (linked, refused)
}

/// The account kept from the last run for the service `id`, while it still
/// stands: not when a Settings row of the service just changed, which links
/// it again with what was typed, nor for a ListenBrainz account linked with
/// another token, or to another server, than the config now names, nor for
/// a teal.fm account linked with another app password: one revoked and
/// replaced in the file by hand would otherwise be sent for ever.
fn still_linked<'a>(
    stored: &'a HashMap<String, Kept>,
    id: &str,
    secret: &Secret,
    fresh: Option<&str>,
) -> Option<&'a Kept> {
    if fresh == Some(id) {
        return None;
    }
    let kept = stored.get(id).filter(|k| k.account.linked())?;
    let account = &kept.account;
    match secret {
        // sonora keeps the server as it rewrote it -- scheme added, `/1`
        // dropped -- so a host is all the two have in common.
        Secret::Token { token, server }
            if account.session != token.trim() || host(&account.server) != host(server) =>
        {
            None
        }
        // sonora keeps the app password as the session, trimmed, the DID
        // as the key and the handle as the name.
        Secret::Password {
            identifier,
            password,
        } if account.session != password.trim() || !names(account, identifier) => None,
        _ => Some(kept),
    }
}

/// Whether an atproto handle or DID, as the config writes it, is the one
/// `account` was linked as. A handle is matched without its `@` and in any
/// case, as handles are.
fn names(account: &Account, identifier: &str) -> bool {
    let identifier = identifier.trim().trim_start_matches('@');
    identifier.eq_ignore_ascii_case(&account.name) || identifier == account.key
}

/// The host of a server's address, port included: what is left of it
/// without the scheme and the path. Empty for no server.
fn host(server: &str) -> &str {
    let server = server.trim();
    let server = server.split_once("://").map_or(server, |(_, rest)| rest);
    server.split('/').next().unwrap_or_default()
}

/// Write the accounts back, as they are now: linked afresh, or switched
/// off and on from the settings row. A failure is logged and no more --
/// the accounts still stand for this run.
pub fn store(file: Option<&Path>, linked: &[Linked]) {
    if let Some(file) = file {
        if let Err(error) = save(file, linked) {
            tracing::warn!("scrobbling: could not keep the accounts: {error:#}");
        }
    }
}

/// The accounts kept from the last run, by service id.
fn load(file: &Path) -> HashMap<String, Kept> {
    std::fs::read(file)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// The accounts, readable by the owner alone: a session key is a credential.
fn save(file: &Path, linked: &[Linked]) -> anyhow::Result<()> {
    let accounts: HashMap<&str, Kept> = linked
        .iter()
        .map(|l| {
            let kept = Kept {
                account: l.account.clone(),
                enabled: l.enabled,
            };
            (l.service.id(), kept)
        })
        .collect();
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = serde_json::to_string_pretty(&accounts)?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write as _;
    options.open(file)?.write_all(text.as_bytes())?;
    Ok(())
}

/// How many listens have begun since the start, which numbers each one.
static BEGUN: AtomicU64 = AtomicU64::new(0);

/// A track as a listen that started `at`. A track with no artist or no
/// title is nothing a service can file, so it is not one.
pub fn play_of(track: &Track, at: SystemTime) -> Option<Play> {
    if track.artist.trim().is_empty() || track.title.trim().is_empty() {
        return None;
    }
    Some(Play {
        order: BEGUN.fetch_add(1, Ordering::Relaxed),
        artist: track.artist.clone(),
        title: track.title.clone(),
        album: (!track.album.trim().is_empty()).then(|| track.album.clone()),
        duration: track.duration,
        at,
    })
}

/// A listen that ended, with how long it was heard: each service weighs
/// that by its own rule before it is sent.
#[derive(Debug, Clone)]
pub struct Listened {
    pub play: Play,
    pub played: std::time::Duration,
}

/// What the shell has decided and not yet sent: the track that just
/// started, and the listens that earned a submission.
#[derive(Debug, Default)]
pub struct Outbox {
    pub now_playing: Option<Play>,
    pub finished: Vec<Listened>,
    /// The listens a service refused, by its id, to go out again with
    /// the next batch. Shared with the requests in flight, which put a
    /// batch back here when it fails.
    pub backlog: Backlog,
}

/// Listens a service has not taken yet, by service id.
pub type Backlog = Arc<Mutex<HashMap<String, Vec<Play>>>>;

/// The most listens kept back for one service.
const BACKLOG_CAP: usize = 200;

/// `plays` back into the backlog for `service`, oldest dropped past the cap.
fn keep(backlog: &Backlog, service: &str, plays: Vec<Play>) {
    let mut all = backlog.lock().unwrap_or_else(|p| p.into_inner());
    let kept = all.entry(service.to_owned()).or_default();
    kept.extend(plays);
    if kept.len() > BACKLOG_CAP {
        let over = kept.len() - BACKLOG_CAP;
        kept.drain(..over);
    }
}

/// What `service` still owes, taken out to be sent.
fn owed(backlog: &Backlog, service: &str) -> Vec<Play> {
    backlog
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(service)
        .unwrap_or_default()
}

type Sending = Pin<Box<dyn Future<Output = ()> + Send>>;

/// Everything in the outbox, as one request per service, and the outbox
/// emptied. The caller runs them: spawned while the app runs, awaited on
/// the way out.
///
/// A service is sent the listens its own rule counts, which may be fewer
/// than the general one: teal.fm takes a short track only if it ran to the
/// end. They go one at a time, as sonora sends them, so a service that
/// refuses one partway has filed the ones before it, and only that one and
/// those after it are kept: teal.fm writes a record per listen and would
/// file a resent one twice.
pub fn drain(linked: &[Linked], outbox: &mut Outbox) -> Vec<Sending> {
    let mut sending: Vec<Sending> = Vec::new();
    if let Some(play) = outbox.now_playing.take() {
        for l in linked.iter().filter(|l| l.enabled).cloned() {
            let play = play.clone();
            sending.push(Box::pin(async move {
                if let Err(error) = l.service.now_playing(&l.account, &play).await {
                    tracing::info!(
                        "scrobbling: {} now playing failed: {error:#}",
                        l.service.id()
                    );
                }
            }));
        }
    }
    let listened: Vec<Listened> = std::mem::take(&mut outbox.finished);
    if !listened.is_empty() {
        for l in linked.iter().filter(|l| l.enabled).cloned() {
            // What the service still owes goes first, so the order holds.
            let backlog = Arc::clone(&outbox.backlog);
            let mut batch = owed(&backlog, l.service.id());
            batch.extend(
                listened
                    .iter()
                    .filter(|f| l.service.earned(&f.play, f.played))
                    .map(|f| f.play.clone()),
            );
            if batch.is_empty() {
                continue;
            }
            sending.push(Box::pin(file_one_by_one(l, batch, backlog)));
        }
    }
    sending
}

/// Sends `batch` to the service a listen at a time, and keeps for the next
/// try the one it refuses and every one after it.
async fn file_one_by_one(l: Linked, batch: Vec<Play>, backlog: Backlog) {
    let id = l.service.id();
    let total = batch.len();
    let mut left = batch.into_iter();
    while let Some(play) = left.next() {
        let one = std::slice::from_ref(&play);
        if let Err(error) = l.service.scrobble(&l.account, one).await {
            let kept: Vec<Play> = std::iter::once(play).chain(left).collect();
            tracing::warn!(
                "scrobbling: {id} refused a listen, {} kept for the next try: {error:#}",
                kept.len()
            );
            keep(&backlog, id, kept);
            return;
        }
    }
    tracing::info!("scrobbled {total} to {id}");
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::config::{ApiKeys, MalojaConfig, TealfmConfig};

    #[test]
    fn a_kept_account_stands_until_its_row_changes_or_its_token_or_server_does() {
        let token = |token: &str, server: &str| Secret::Token {
            token: token.into(),
            server: server.into(),
        };
        let account = |server: &str| Kept {
            account: Account {
                session: "t".into(),
                server: server.into(),
                ..Account::default()
            },
            enabled: true,
        };
        let stored: HashMap<String, Kept> = [("listenbrainz".to_owned(), account(""))].into();
        let lb = "listenbrainz";
        assert!(still_linked(&stored, lb, &token("t", ""), None).is_some());
        assert!(
            still_linked(&stored, lb, &token("t", ""), Some(lb)).is_none(),
            "its row just changed"
        );
        assert!(
            still_linked(&stored, lb, &token("t", ""), Some("maloja")).is_some(),
            "another's row changed"
        );
        assert!(
            still_linked(&stored, lb, &token("u", ""), None).is_none(),
            "another token"
        );
        assert!(
            still_linked(&stored, lb, &token("t", "https://lb.example.org"), None).is_none(),
            "a server now"
        );
        let own: HashMap<String, Kept> =
            [("listenbrainz".to_owned(), account("https://lb.example.org"))].into();
        assert!(
            still_linked(&own, lb, &token("t", ""), None).is_none(),
            "none now"
        );
        assert!(still_linked(&own, lb, &token("t", "lb.example.org/1/"), None).is_some());
    }

    #[test]
    fn the_account_file_keeps_the_switch_beside_the_account_as_it_always_did() {
        let dir = std::env::temp_dir().join(format!("sonora-tui-scrobble-{}", std::process::id()));
        let file = dir.join("scrobble.json");
        std::fs::create_dir_all(&dir).expect("a folder");
        // As a release before the switch left sonora's `Account` wrote it.
        std::fs::write(
            &file,
            r#"{"listenbrainz":{"session":"t","name":"me","enabled":true},"maloja":{"session":"k","enabled":false}}"#,
        )
        .expect("a file");
        let kept = load(&file);
        assert!(kept["listenbrainz"].enabled && kept["listenbrainz"].account.session == "t");
        assert!(!kept["maloja"].enabled);

        let linked = Linked {
            service: scrobbling::services().remove(2),
            account: kept["listenbrainz"].account.clone(),
            enabled: false,
        };
        save(&file, &[linked]).expect("saved");
        let again = load(&file);
        assert!(!again["listenbrainz"].enabled, "the switch is written back");
        assert_eq!(again["listenbrainz"].account.name, "me");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_tealfm_account_links_with_the_handle_and_app_password_the_config_names() {
        let config = ScrobbleConfig {
            tealfm: Some(TealfmConfig {
                handle: "alice.bsky.social".into(),
                password: "abcd-efgh-ijkl-mnop".into(),
            }),
            ..Default::default()
        };
        assert!(configured(&config));
        assert!(matches!(
            secret_for(&config, "tealfm"),
            Some(Secret::Password { identifier, password })
                if identifier == "alice.bsky.social" && password == "abcd-efgh-ijkl-mnop"
        ));

        // sonora keeps the app password as the account's session.
        let kept = |password: &str| Kept {
            account: Account {
                key: "did:plc:alice".into(),
                session: password.into(),
                name: "alice.bsky.social".into(),
                ..Account::default()
            },
            enabled: true,
        };
        let secret = secret_for(&config, "tealfm").expect("a secret");
        let stored: HashMap<String, Kept> =
            [("tealfm".to_owned(), kept("abcd-efgh-ijkl-mnop"))].into();
        assert!(still_linked(&stored, "tealfm", &secret, None).is_some());
        let revoked: HashMap<String, Kept> = [("tealfm".to_owned(), kept("old"))].into();
        assert!(
            still_linked(&revoked, "tealfm", &secret, None).is_none(),
            "another app password in the config links the account again"
        );
        let with = |handle: &str| {
            let mut config = config.clone();
            if let Some(t) = config.tealfm.as_mut() {
                t.handle = handle.into();
            }
            secret_for(&config, "tealfm").expect("a secret")
        };
        assert!(
            still_linked(&stored, "tealfm", &with(" @Alice.bsky.social "), None).is_some(),
            "the same handle, written as one is shown"
        );
        assert!(still_linked(&stored, "tealfm", &with("did:plc:alice"), None).is_some());
        assert!(
            still_linked(&stored, "tealfm", &with("bob.example"), None).is_none(),
            "another account in the config links that one"
        );

        let half = |handle: &str, password: &str| ScrobbleConfig {
            tealfm: Some(TealfmConfig {
                handle: handle.into(),
                password: password.into(),
            }),
            ..Default::default()
        };
        for typed_so_far in [half("alice.bsky.social", " "), half("", "abcd")] {
            assert!(
                secret_for(&typed_so_far, "tealfm").is_none(),
                "half an account is not tried, and not refused at every start"
            );
        }

        let tealfm = scrobbling::services()
            .into_iter()
            .find(|s| s.id() == "tealfm")
            .expect("sonora offers teal.fm");
        let linked = Linked {
            service: tealfm,
            account: kept("p").account,
            enabled: true,
        };
        assert_eq!(linked.label(), "teal.fm");
    }

    #[test]
    fn the_status_line_calls_each_service_by_its_name_or_the_account_by_its_own() {
        let label = |id: &str, name: &str| {
            Linked {
                service: scrobbling::services()
                    .into_iter()
                    .find(|s| s.id() == id)
                    .expect("sonora offers it"),
                account: Account {
                    name: name.into(),
                    ..Account::default()
                },
                enabled: true,
            }
            .label()
        };
        assert_eq!(label("lastfm", "someone"), "Last.fm");
        assert_eq!(label("librefm", "someone"), "Libre.fm");
        assert_eq!(label("tealfm", "alice.bsky.social"), "teal.fm");
        assert_eq!(label("maloja", "My Maloja"), "My Maloja");
        assert_eq!(label("maloja", ""), "maloja", "with no name, the id");
    }

    #[test]
    fn a_listenbrainz_account_on_a_server_of_ones_own_is_named_by_its_host() {
        let on = |server: &str| {
            Linked {
                service: scrobbling::services().remove(2),
                account: Account {
                    server: server.into(),
                    ..Account::default()
                },
                enabled: true,
            }
            .label()
        };
        assert_eq!(on(""), "ListenBrainz");
        assert_eq!(
            on("http://ms.local:9078/api/listenbrainz"),
            "ListenBrainz on ms.local:9078"
        );
    }

    #[test]
    fn a_listenbrainz_server_goes_with_the_token_and_none_is_listenbrainz_org() {
        let mut config = ScrobbleConfig {
            listenbrainz: Some("t".into()),
            ..Default::default()
        };
        assert!(matches!(
            secret_for(&config, "listenbrainz"),
            Some(Secret::Token { token, server }) if token == "t" && server.is_empty()
        ));
        config.listenbrainz_server = Some("https://lb.example.org".into());
        assert!(matches!(
            secret_for(&config, "listenbrainz"),
            Some(Secret::Token { server, .. }) if server == "https://lb.example.org"
        ));
    }

    #[test]
    fn the_config_gives_each_service_what_it_links_with() {
        let config = ScrobbleConfig {
            lastfm: Some(ApiKeys {
                key: "k".into(),
                secret: "s".into(),
            }),
            librefm: true,
            listenbrainz: Some("  ".into()),
            listenbrainz_server: None,
            maloja: Some(MalojaConfig {
                url: "https://m".into(),
                key: "mk".into(),
            }),
            tealfm: None,
        };
        assert!(
            matches!(secret_for(&config, "lastfm"), Some(Secret::Keys { key, secret }) if key == "k" && secret == "s")
        );
        assert!(matches!(secret_for(&config, "librefm"), Some(Secret::None)));
        assert!(
            secret_for(&config, "listenbrainz").is_none(),
            "a blank token names nothing"
        );
        assert!(
            matches!(secret_for(&config, "maloja"), Some(Secret::Server { url, .. }) if url == "https://m")
        );
        assert!(secret_for(&ScrobbleConfig::default(), "lastfm").is_none());
    }

    #[test]
    fn a_refused_batch_is_kept_and_goes_out_first_next_time_up_to_a_cap() {
        let backlog = Backlog::default();
        let play = |title: &str| Play {
            artist: "A".into(),
            title: title.into(),
            album: None,
            duration: Duration::from_secs(200),
            at: SystemTime::UNIX_EPOCH,
            order: 0,
        };
        keep(&backlog, "lastfm", vec![play("one"), play("two")]);
        assert!(
            owed(&backlog, "listenbrainz").is_empty(),
            "another service owes nothing"
        );
        let again = owed(&backlog, "lastfm");
        assert_eq!(
            again.iter().map(|p| p.title.as_str()).collect::<Vec<_>>(),
            ["one", "two"]
        );
        assert!(owed(&backlog, "lastfm").is_empty(), "taken out once");

        keep(
            &backlog,
            "lastfm",
            (0..BACKLOG_CAP + 5).map(|i| play(&i.to_string())).collect(),
        );
        let kept = owed(&backlog, "lastfm");
        assert_eq!(kept.len(), BACKLOG_CAP);
        assert_eq!(kept[0].title, "5", "the oldest go first");
    }

    #[test]
    fn a_track_is_a_listen_only_with_an_artist_and_a_title() {
        let mut track = Track::sample("Song", "Someone", Duration::from_secs(200));
        track.album = String::new();
        let play = play_of(&track, SystemTime::UNIX_EPOCH).expect("a listen");
        assert_eq!(
            (play.artist.as_str(), play.title.as_str()),
            ("Someone", "Song")
        );
        assert!(play.album.is_none(), "an empty album is no album");
        assert!(play.earned(Duration::from_secs(100)), "half of it");
        assert!(!play.earned(Duration::from_secs(99)));

        track.artist = String::new();
        assert!(play_of(&track, SystemTime::UNIX_EPOCH).is_none());
    }

    #[test]
    fn the_outbox_is_emptied_into_one_request_per_service_and_kind() {
        let service = scrobbling::services().remove(2);
        let linked = Linked {
            service,
            account: Account {
                session: "t".into(),
                ..Account::default()
            },
            enabled: true,
        };
        let play = play_of(
            &Track::sample("Song", "Someone", Duration::from_secs(200)),
            SystemTime::UNIX_EPOCH,
        )
        .expect("a listen");
        let mut outbox = Outbox {
            now_playing: Some(play.clone()),
            finished: vec![listened(&play, 200), listened(&play, 200)],
            backlog: Backlog::default(),
        };
        let sending = drain(std::slice::from_ref(&linked), &mut outbox);
        assert_eq!(sending.len(), 2, "one now-playing, one batch of listens");
        assert!(outbox.now_playing.is_none() && outbox.finished.is_empty());

        let off = Linked {
            enabled: false,
            ..linked
        };
        let short = play_of(
            &Track::sample("S", "A", Duration::from_secs(60)),
            SystemTime::UNIX_EPOCH,
        )
        .expect("a listen");
        outbox.finished.push(listened(&short, 60));
        assert!(
            drain(&[off], &mut outbox).is_empty(),
            "a switched-off account sends nothing"
        );
        assert_eq!(linked_label(), "ListenBrainz");
    }

    fn listened(play: &Play, seconds: u64) -> Listened {
        Listened {
            play: play.clone(),
            played: Duration::from_secs(seconds),
        }
    }

    /// A service that files what it is sent, refuses the title it is told
    /// to, and with `whole` counts a listen only if it played to the end.
    #[derive(Default)]
    struct Fake {
        whole: bool,
        refuse: Mutex<Option<String>>,
        filed: Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl Service for Fake {
        fn id(&self) -> &'static str {
            "fake"
        }

        fn link(&self) -> scrobbling::Link {
            scrobbling::Link::Token
        }

        async fn connect(&self, _: Secret) -> anyhow::Result<Account> {
            Ok(Account::default())
        }

        fn earned(&self, play: &Play, played: Duration) -> bool {
            play.earned(played) && (!self.whole || played >= play.duration)
        }

        async fn scrobble(&self, _: &Account, plays: &[Play]) -> anyhow::Result<()> {
            for play in plays {
                if self.refuse.lock().expect("lock").as_deref() == Some(play.title.as_str()) {
                    anyhow::bail!("refused {}", play.title);
                }
                self.filed.lock().expect("lock").push(play.title.clone());
            }
            Ok(())
        }
    }

    fn linked_to(fake: &Arc<Fake>) -> Linked {
        Linked {
            service: Arc::clone(fake) as Arc<dyn Service>,
            account: Account::default(),
            enabled: true,
        }
    }

    fn song(title: &str) -> Play {
        play_of(
            &Track::sample(title, "Someone", Duration::from_secs(100)),
            SystemTime::UNIX_EPOCH,
        )
        .expect("a listen")
    }

    fn send(linked: &[Linked], outbox: &mut Outbox) {
        futures::executor::block_on(futures::future::join_all(drain(linked, outbox)));
    }

    #[test]
    fn a_service_is_sent_only_the_listens_its_own_rule_counts() {
        let loose = Arc::new(Fake::default());
        let strict = Arc::new(Fake {
            whole: true,
            ..Fake::default()
        });
        let mut outbox = Outbox {
            finished: vec![listened(&song("Half"), 60), listened(&song("Whole"), 100)],
            ..Outbox::default()
        };
        send(&[linked_to(&loose), linked_to(&strict)], &mut outbox);
        assert_eq!(*loose.filed.lock().expect("lock"), ["Half", "Whole"]);
        assert_eq!(
            *strict.filed.lock().expect("lock"),
            ["Whole"],
            "a listen skipped before the end is not one to this service"
        );
    }

    #[test]
    fn a_batch_refused_partway_keeps_only_what_was_not_filed_yet() {
        let fake = Arc::new(Fake::default());
        *fake.refuse.lock().expect("lock") = Some("Two".into());
        let linked = [linked_to(&fake)];
        let mut outbox = Outbox {
            finished: ["One", "Two", "Three"]
                .map(|t| listened(&song(t), 100))
                .into(),
            ..Outbox::default()
        };
        send(&linked, &mut outbox);
        assert_eq!(*fake.filed.lock().expect("lock"), ["One"]);

        *fake.refuse.lock().expect("lock") = None;
        outbox.finished.push(listened(&song("Four"), 100));
        send(&linked, &mut outbox);
        assert_eq!(
            *fake.filed.lock().expect("lock"),
            ["One", "Two", "Three", "Four"],
            "one filed once, the rest in their order"
        );
    }

    fn linked_label() -> String {
        Linked {
            service: scrobbling::services().remove(2),
            account: Account::default(),
            enabled: true,
        }
        .label()
    }
}
