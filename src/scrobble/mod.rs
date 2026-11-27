//! Scrobbling: telling Last.fm, Libre.fm, ListenBrainz or Maloja what played.
//!
//! The services and their protocols come from sonora's `scrobble` crate. This
//! links the ones the config names, keeps their accounts in a file beside
//! the token, turns a track into a listen, and sends what the shell puts in
//! the outbox. Whether a listen counts is the services' own rule, and lives
//! with them: thirty seconds at least, and half the track or four minutes.
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
/// another token, or to another server, than the config now names.
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
        _ => Some(kept),
    }
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

/// What the shell has decided and not yet sent: the track that just
/// started, and the listens that earned a submission.
#[derive(Debug, Default)]
pub struct Outbox {
    pub now_playing: Option<Play>,
    pub finished: Vec<Play>,
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
    let plays: Vec<Play> = std::mem::take(&mut outbox.finished);
    if !plays.is_empty() {
        for l in linked.iter().filter(|l| l.enabled).cloned() {
            // What the service still owes goes first, so the order holds.
            let backlog = Arc::clone(&outbox.backlog);
            let mut batch = owed(&backlog, l.service.id());
            batch.extend(plays.iter().cloned());
            sending.push(Box::pin(async move {
                match l.service.scrobble(&l.account, &batch).await {
                    Ok(()) => tracing::info!("scrobbled {} to {}", batch.len(), l.service.id()),
                    Err(error) => {
                        tracing::warn!(
                            "scrobbling: {} refused {}, kept for the next try: {error:#}",
                            l.service.id(),
                            batch.len()
                        );
                        keep(&backlog, l.service.id(), batch);
                    }
                }
            }));
        }
    }
    sending
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::config::{ApiKeys, MalojaConfig};

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
            finished: vec![play.clone(), play],
            backlog: Backlog::default(),
        };
        let sending = drain(std::slice::from_ref(&linked), &mut outbox);
        assert_eq!(sending.len(), 2, "one now-playing, one batch of listens");
        assert!(outbox.now_playing.is_none() && outbox.finished.is_empty());

        let off = Linked {
            enabled: false,
            ..linked
        };
        outbox.finished.push(
            play_of(
                &Track::sample("S", "A", Duration::from_secs(60)),
                SystemTime::UNIX_EPOCH,
            )
            .unwrap(),
        );
        assert!(
            drain(&[off], &mut outbox).is_empty(),
            "a switched-off account sends nothing"
        );
        assert_eq!(linked_label(), "ListenBrainz");
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
