//! The shell's side of playback: commands in, events out.
//!
//! The engine itself is sonora's (`music::engine`): its threads, its output
//! device, its queue ahead of the decoder. What lives here is the
//! translation between the shell's vocabulary and the engine's, and the
//! `Fetch` that teaches the engine TIDAL is in `fetch.rs`.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::domain::TrackId;
use crate::playback::fetch::{Meta, TidalFetch};

/// What the engine's events leave out and the badge wants: the bit depth,
/// the rate and the tier of the stream that just started, and why a load
/// failed. TIDAL's fetch records them; for another service the engine's
/// own word on the stream is all there is.
pub trait StreamMeta: Send + Sync {
    fn take_meta(&self, _id: &str) -> Option<Meta> {
        None
    }
    fn take_failure(&self, _id: &str) -> Option<String> {
        None
    }
    /// What to ask for from now on. A service with one quality ignores it.
    fn set_quality(&self, _wanted: crate::domain::Quality) {}
}

/// A service whose streams say nothing about themselves.
pub struct NoMeta;

impl StreamMeta for NoMeta {}

impl StreamMeta for TidalFetch {
    fn set_quality(&self, wanted: crate::domain::Quality) {
        TidalFetch::set_quality(self, wanted);
    }
    fn take_meta(&self, id: &str) -> Option<Meta> {
        TidalFetch::take_meta(self, id)
    }
    fn take_failure(&self, id: &str) -> Option<String> {
        TidalFetch::take_failure(self, id)
    }
}

/// What an engine is started with, from the user's settings. The
/// equaliser is a shared handle: the engine keeps reading it, so a preset
/// chosen later reaches the output without a restart. The rest is read
/// once, at the start: changing a flag restarts the engine, and a changed
/// quality starts a fresh one at the next track.
#[derive(Clone, Debug)]
pub struct Options {
    pub normalisation: bool,
    pub gapless: bool,
    /// What every service is asked for, where it has levels to choose from.
    pub quality: crate::domain::Quality,
    pub equalizer: music::Equalizer,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            normalisation: false,
            gapless: true,
            quality: crate::domain::Quality::HiResLossless,
            equalizer: music::equalizer::EqualizerWriter::new(
                false,
                music::equalizer::Preset::Flat.curve(),
            )
            .reader(),
        }
    }
}

/// The engine's running spectrum, for the bar to draw: thirty-two bands,
/// each 0 to 1, read whenever a frame wants them. A handle to the
/// analyser's own figures rather than a copy, so nothing is sent per frame.
/// It listens for as long as it is kept: sonora's taps feed nothing
/// unless someone holds a `Listening`.
#[derive(Clone)]
pub struct Bands {
    spectrum: music::Spectrum,
    _listening: Arc<music::Listening>,
}

impl std::fmt::Debug for Bands {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Bands")
    }
}

impl Bands {
    fn new(spectrum: music::Spectrum) -> Self {
        Self {
            _listening: Arc::new(spectrum.listen()),
            spectrum,
        }
    }

    pub fn levels(&self) -> Vec<f32> {
        self.spectrum.bands()
    }
}

/// Not `Clone`: a command is consumed by the audio thread exactly once.
pub enum Cmd {
    /// Start an engine on this service's factory, replacing whatever was
    /// playing. Sent once a session is in hand; nothing plays before it.
    Attach {
        factory: Arc<dyn music::PlaybackFactory>,
        meta: Arc<dyn StreamMeta>,
        options: Options,
    },
    /// What to ask for from the next track on. An engine asks for one
    /// quality from its start, so another one starts a fresh engine and
    /// the same one does nothing. Sent before each `Play`, so the track
    /// being heard is left alone.
    Quality(crate::domain::Quality),
    /// Fetch and play a track. What to ask for, and with which token, the
    /// service's fetch already knows.
    Play(TrackId),
    /// The same, from a position into it: what a restarted engine picks
    /// the track back up with.
    PlayAt(TrackId, Duration),
    /// Fetch the track that plays next, so it starts at once and a gapless
    /// engine lines it up behind the one playing.
    Preload(TrackId),
    Pause,
    Resume,
    Seek(Duration),
    Volume(f32),
    Stop,
}

/// The join between one track and the next, as the two threads see it:
/// the command thread lines a track up, the event thread hears the engine
/// move on to it. Shared, so a `Play` for the track the engine already
/// started by itself is a seamless one, and a skip to a track merely lined
/// up is not -- the engine takes a seamless load of a lined-up track as
/// "already playing" and only resumes, which left the skip waiting for
/// the join.
#[derive(Debug, Default)]
struct Segue {
    lined_up: Option<TrackId>,
    joined: Option<TrackId>,
}

impl Segue {
    /// The engine says the track ended: whatever was lined up plays now.
    fn ended(&mut self) {
        self.joined = self.lined_up.take();
    }

    /// Whether a play of `id` continues what the engine already joined to.
    /// Any play settles the join either way.
    fn seamless(&mut self, id: &TrackId) -> bool {
        self.lined_up = None;
        self.joined.take().is_some_and(|joined| joined == *id)
    }
}

type SharedSegue = Arc<Mutex<Segue>>;

fn lock(segue: &SharedSegue) -> std::sync::MutexGuard<'_, Segue> {
    segue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Debug, Clone)]
pub enum PlaybackEvent {
    /// Emitted once a stream is decoding, carrying what was actually
    /// delivered — never the requested quality.
    Started {
        bit_depth: Option<u8>,
        sample_rate: u32,
        delivered: crate::domain::Quality,
    },
    Position(Duration),
    /// How long the stream is, once opened. The only word on it for a
    /// track that came without a duration, as YouTube Music's quick
    /// picks do: the bar had a running clock over nothing to fill.
    Length(Duration),
    /// The engine that just started, and its spectrum: one per attach.
    Spectrum(Bands),
    /// The whole of this track has arrived, so the next can be fetched
    /// without taking bandwidth from it.
    Downloaded(TrackId),
    Finished,
    /// A track that could not be fetched or opened. The shell takes back
    /// the bar it filled when the track was chosen.
    Failed {
        id: TrackId,
        message: String,
    },
    /// TIDAL rejected the session: the stored token is no good any more.
    SessionExpired,
    /// The audio device went away (a suspend, a headset unplugged) and
    /// the engine ended with it: only a fresh one plays again. `heard` is
    /// the track the output was playing and how far into it, when the
    /// engine knew.
    OutputChanged {
        heard: Option<(TrackId, Duration)>,
    },
    Error(String),
}

/// Index of the segment containing `at`, clamped to the last segment.
pub(crate) fn segment_for(durations: &[Duration], at: Duration) -> usize {
    let mut acc = Duration::ZERO;
    for (i, d) in durations.iter().enumerate() {
        acc += *d;
        if at < acc {
            return i;
        }
    }
    durations.len().saturating_sub(1)
}

/// Playback offset at which segment `index` begins.
pub(crate) fn segment_start(durations: &[Duration], index: usize) -> Duration {
    durations.iter().take(index).copied().sum()
}

/// Where a seek into a DASH stream restarts: the segment holding `at`,
/// and how much of it to skip to land on `at` itself.
pub(crate) fn restart_point(durations: &[Duration], at: Duration) -> (usize, Duration) {
    let index = segment_for(durations, at);
    (index, at.saturating_sub(segment_start(durations, index)))
}

/// Start the command thread. Commands in on a std channel (the shell
/// sends from wherever it is); events out on a tokio channel (the shell
/// awaits it). The engine itself starts on `Cmd::Attach`.
pub fn spawn() -> (
    Sender<Cmd>,
    tokio::sync::mpsc::UnboundedReceiver<PlaybackEvent>,
) {
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<Cmd>();
    let (evt_tx, evt_rx) = tokio::sync::mpsc::unbounded_channel::<PlaybackEvent>();
    std::thread::Builder::new()
        .name("sonora-tui-commands".into())
        .spawn(move || forward_commands(cmd_rx, evt_tx))
        .expect("spawning the command thread");
    (cmd_tx, evt_rx)
}

fn config(options: &Options) -> music::PlaybackConfig {
    music::PlaybackConfig {
        normalisation: options.normalisation,
        gapless: options.gapless,
        // ~4Hz progress updates, as before.
        position_interval: Duration::from_millis(250),
        gain: 1.0,
        equalizer: options.equalizer.clone(),
        quality: engine_quality(options.quality),
        // The server-side conversion, the level meter, the spectrum
        // analyzer and the download vault are sonora's desktop features;
        // the TUI has no screen for any of them yet.
        transcode: music::Transcode::default(),
        meter: music::Meter::default(),
        analyzer: music::Analyzer::default(),
        vault: None,
    }
}

/// The setting's level as sonora names it. A service takes the best it
/// serves at or below it.
fn engine_quality(wanted: crate::domain::Quality) -> music::Quality {
    use crate::domain::Quality;
    match wanted {
        Quality::HiResLossless => music::Quality::HiRes,
        Quality::Lossless => music::Quality::Lossless,
        Quality::High => music::Quality::High,
        Quality::Low => music::Quality::Low,
    }
}

/// The engine's `Player` is fire-and-forget, so each command is one call.
/// Dropping a player ends its engine, so an `Attach` replaces the last.
fn forward_commands(
    cmds: Receiver<Cmd>,
    events: tokio::sync::mpsc::UnboundedSender<PlaybackEvent>,
) {
    let mut player: Option<Box<dyn music::Player>> = None;
    // Kept across an attach: the level is set once at startup, before any
    // session, and a fresh engine starts at full.
    let mut volume = 1.0f32;
    let segue: SharedSegue = Arc::default();
    // What the running engine was started with, to start another like it.
    let mut attached: Option<Attached> = None;
    while let Ok(cmd) = cmds.recv() {
        match cmd {
            Cmd::Attach {
                factory,
                meta,
                options,
            } => {
                let fresh = Attached {
                    factory,
                    meta,
                    options,
                };
                player = Some(fresh.start(volume, &segue, &events));
                attached = Some(fresh);
            }
            Cmd::Quality(wanted) => {
                if let Some(running) = attached.as_mut().filter(|a| a.options.quality != wanted) {
                    running.options.quality = wanted;
                    player = Some(running.start(volume, &segue, &events));
                }
            }
            cmd => command(player.as_deref(), cmd, &mut volume, &segue, &events),
        }
    }
}

/// What an engine was started with.
struct Attached {
    factory: Arc<dyn music::PlaybackFactory>,
    meta: Arc<dyn StreamMeta>,
    options: Options,
}

impl Attached {
    fn start(
        &self,
        volume: f32,
        segue: &SharedSegue,
        events: &tokio::sync::mpsc::UnboundedSender<PlaybackEvent>,
    ) -> Box<dyn music::Player> {
        attach(
            &*self.factory,
            self.meta.clone(),
            &self.options,
            volume,
            segue,
            events,
        )
    }
}

/// A fresh engine at the kept level, its events on a thread of their own.
fn attach(
    factory: &dyn music::PlaybackFactory,
    meta: Arc<dyn StreamMeta>,
    options: &Options,
    volume: f32,
    segue: &SharedSegue,
    events: &tokio::sync::mpsc::UnboundedSender<PlaybackEvent>,
) -> Box<dyn music::Player> {
    *lock(segue) = Segue::default();
    // An engine asks for one quality from its start, so a fetch of our own
    // hears of it here too rather than at some later play.
    meta.set_quality(options.quality);
    let (fresh, stream) = factory.start(config(options));
    fresh.set_gain(volume);
    if let Some(spectrum) = fresh.spectrum() {
        let _ = events.send(PlaybackEvent::Spectrum(Bands::new(spectrum)));
    }
    let (out, joins) = (events.clone(), segue.clone());
    if let Err(error) = std::thread::Builder::new()
        .name("sonora-tui-events".into())
        .spawn(move || forward_events(stream, out, meta, joins))
    {
        let _ = events.send(PlaybackEvent::Error(format!(
            "could not start the playback event thread: {error}"
        )));
    }
    fresh
}

/// Every command but `Attach`, on whichever engine is running. The level
/// and the end of a join are kept whether or not there is one; a `Play`
/// with none fails, so the shell takes back the bar it filled.
fn command(
    player: Option<&dyn music::Player>,
    cmd: Cmd,
    volume: &mut f32,
    segue: &SharedSegue,
    events: &tokio::sync::mpsc::UnboundedSender<PlaybackEvent>,
) {
    match &cmd {
        Cmd::Volume(v) => *volume = v.clamp(0.0, 1.0),
        Cmd::PlayAt(..) => *lock(segue) = Segue::default(),
        _ => {}
    }
    let Some(player) = player else {
        if let Cmd::Play(id) = cmd {
            let _ = events.send(PlaybackEvent::Failed {
                id,
                message: "no playback engine: sign in first".into(),
            });
        }
        return;
    };
    match cmd {
        // Taken by the loop before it gets here.
        Cmd::Attach { .. } | Cmd::Quality(_) => {}
        Cmd::Play(id) => {
            let seamless = lock(segue).seamless(&id);
            if let Err(error) = player.load(id.as_str(), Duration::ZERO, seamless) {
                tracing::error!("could not ask for track {id}: {error}");
            }
        }
        Cmd::PlayAt(id, at) => {
            if let Err(error) = player.load(id.as_str(), at, false) {
                tracing::error!("could not ask for track {id}: {error}");
            }
        }
        Cmd::Preload(id) => match player.preload(id.as_str(), true) {
            Ok(()) => lock(segue).lined_up = Some(id),
            Err(error) => tracing::warn!("could not preload track {id}: {error}"),
        },
        Cmd::Pause => player.pause(),
        Cmd::Resume => player.play(),
        Cmd::Seek(to) => player.seek(to),
        Cmd::Volume(_) => player.set_gain(*volume),
        // ponytail: the engine has no stop. Paused, the track holds
        // silent until the next load replaces it.
        Cmd::Stop => player.pause(),
    }
}

/// Turns the engine's events into the shell's. The engine says a track is
/// playing at every start, resume and seek; the shell wants `Started` once.
fn forward_events(
    mut events: Box<dyn music::PlaybackEvents>,
    out: tokio::sync::mpsc::UnboundedSender<PlaybackEvent>,
    meta: Arc<dyn StreamMeta>,
    segue: SharedSegue,
) {
    let runtime = match tokio::runtime::Builder::new_current_thread().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = out.send(PlaybackEvent::Error(format!(
                "could not build the event runtime: {error}"
            )));
            return;
        }
    };
    let mut starts = Starts::default();
    runtime.block_on(async {
        while let Some(event) = events.next().await {
            // Before the shell hears of it: its `Play` for what follows
            // reads this.
            if matches!(event, music::PlaybackEvent::Ended { .. }) {
                lock(&segue).ended();
            }
            let Some(event) = translate(event, &mut starts, meta.as_ref()) else {
                continue;
            };
            if out.send(event).is_err() {
                return;
            }
        }
    });
}

/// What the event thread remembers between the engine's events, to tell
/// the shell of each start once and with what the stream is.
#[derive(Default)]
struct Starts {
    /// The track whose `Started` has gone out, so a resume or a seek on it
    /// does not reset the badge and the position.
    announced: Option<String>,
    /// What the engine said of each track it opened and has not yet
    /// played: it opens the next one while this one is still heard.
    // ponytail: a track opened and never played, or sought in, keeps its
    // entry until the engine is replaced. A few bytes each; drop them on
    // `Ended` if a session ever shows it matters.
    signals: HashMap<String, Meta>,
}

impl Starts {
    /// The `Started` of a track the shell has not been told of yet.
    fn start(&mut self, id: String, meta: &dyn StreamMeta) -> PlaybackEvent {
        let signal = self.signals.remove(&id);
        // The fetch's own record first: TIDAL's answer names the tier,
        // where the engine only knows what the decoder saw. A stream
        // nobody described still started: the badge is left blank rather
        // than the track left looking stopped.
        let found = meta.take_meta(&id).or(signal).unwrap_or(Meta {
            bit_depth: None,
            delivered: crate::domain::Quality::Lossless,
            sample_rate: 0,
        });
        self.announced = Some(id);
        PlaybackEvent::Started {
            bit_depth: found.bit_depth,
            sample_rate: found.sample_rate,
            delivered: found.delivered,
        }
    }
}

/// The engine's word on a stream, as the badge reads one, where the
/// service named its encoding. Without a name the rate alone would be
/// drawn as lossy or as lossless, and either may be wrong: no badge. The
/// tier above lossless follows from the depth and the rate.
fn signal_meta(track: &music::TrackSignal) -> Option<Meta> {
    use crate::domain::Quality;
    use music::Encoding;
    let delivered = match track.encoding? {
        Encoding::Flac { .. } | Encoding::Alac { .. } | Encoding::Pcm { .. } => Quality::Lossless,
        Encoding::Aac { .. }
        | Encoding::Vorbis { .. }
        | Encoding::Mp3 { .. }
        | Encoding::Opus { .. } => Quality::High,
        Encoding::Other => return None,
    };
    Some(Meta {
        bit_depth: track.encoding.and_then(Encoding::depth),
        delivered,
        sample_rate: track.rate,
    })
}

fn translate(
    event: music::PlaybackEvent,
    starts: &mut Starts,
    meta: &dyn StreamMeta,
) -> Option<PlaybackEvent> {
    use music::PlaybackEvent as Engine;
    match event {
        Engine::Signal { id, track } => {
            if let Some(found) = signal_meta(&track) {
                starts.signals.insert(id, found);
            }
            None
        }
        Engine::Playing { id: Some(id), .. } if starts.announced.as_deref() != Some(&id) => {
            Some(starts.start(id, meta))
        }
        Engine::Position { at, .. } => Some(PlaybackEvent::Position(at)),
        Engine::Downloaded { id: Some(id) } => Some(PlaybackEvent::Downloaded(TrackId::from(id))),
        Engine::Ended { .. } => {
            starts.announced = None;
            Some(PlaybackEvent::Finished)
        }
        Engine::Unavailable { id: Some(id) } => {
            let message = meta
                .take_failure(&id)
                .unwrap_or_else(|| "track not available for playback".into());
            Some(failed(&id, message))
        }
        Engine::Throttled { id: Some(id) } => Some(failed(
            &id,
            "TIDAL is rate limiting playback; try again shortly".into(),
        )),
        Engine::Gated => Some(PlaybackEvent::SessionExpired),
        // Only Spotify sends this, when its audio key is denied (0x0001).
        // A free account gets it, and so does a Premium one that Spotify
        // moved to its newer sign-in flow -- accounts made since late
        // 2025, more over time. librespot cannot stream for those yet:
        // librespot-org/librespot#1649. Nothing on this side fixes it.
        Engine::Refused => Some(PlaybackEvent::Error(
            "Spotify is not granting this account playback keys: free, or a newer account librespot cannot stream for yet (librespot#1649)".into(),
        )),
        Engine::Length { duration, .. } => Some(PlaybackEvent::Length(duration)),
        Engine::OutputChanged { heard } => Some(PlaybackEvent::OutputChanged {
            heard: heard.map(|(id, at)| (TrackId::from(id), at)),
        }),
        // A resume, a seek and a pause are things the shell already knows.
        Engine::Playing { .. }
        | Engine::Loading { .. }
        | Engine::Paused { .. }
        | Engine::Seeked { .. }
        | Engine::Downloaded { id: None }
        | Engine::Unavailable { id: None }
        | Engine::Throttled { id: None } => None,
    }
}

fn failed(id: &str, message: String) -> PlaybackEvent {
    PlaybackEvent::Failed {
        id: TrackId::from(id),
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Quality;

    #[test]
    fn the_length_of_a_stream_reaches_the_shell() {
        let got = translate(
            music::PlaybackEvent::Length {
                id: Some("t".into()),
                duration: Duration::from_secs(90),
            },
            &mut Starts::default(),
            &NoMeta,
        );
        assert!(matches!(got, Some(PlaybackEvent::Length(d)) if d == Duration::from_secs(90)));
    }

    #[test]
    fn the_bands_move_while_music_reaches_the_spectrum() {
        let spectrum = music::Spectrum::new();
        let bands = Bands::new(spectrum.clone());
        let mut tap = spectrum.attach();
        tap.format(44_100, 1);
        // A second of a 1 kHz tone, more than the analyser's window.
        for i in 0..44_100 {
            tap.push(
                0,
                (std::f32::consts::TAU * 1_000.0 * i as f32 / 44_100.0).sin(),
            );
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while bands.levels().iter().all(|&level| level == 0.0) {
            assert!(
                std::time::Instant::now() < deadline,
                "the spectrum never moved: nothing listens to its taps"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn a_lost_output_reaches_the_shell() {
        // The engine ends after it: dropped here, every later play
        // fails with "cannot reach the playback engine".
        let heard = Some(("t".to_owned(), Duration::from_secs(31)));
        let got = translate(
            music::PlaybackEvent::OutputChanged { heard },
            &mut Starts::default(),
            &NoMeta,
        );
        assert!(
            matches!(&got, Some(PlaybackEvent::OutputChanged { heard: Some((id, at)) })
                if *id == TrackId::from("t") && *at == Duration::from_secs(31)),
            "{got:?}"
        );
    }

    fn fetch() -> TidalFetch {
        TidalFetch::new(crate::domain::Quality::Lossless).unwrap()
    }

    #[test]
    fn a_skip_to_the_track_lined_up_is_a_fresh_load_and_the_join_is_seamless() {
        let (a, b) = (TrackId::from("a"), TrackId::from("b"));
        // The engine joined to b by itself: play it seamlessly, once.
        let mut segue = Segue {
            lined_up: Some(b.clone()),
            ..Segue::default()
        };
        segue.ended();
        assert!(segue.seamless(&b));
        assert!(!segue.seamless(&b), "settled");
        // Skipped to b while a still played: b is lined up, not playing.
        let mut segue = Segue {
            lined_up: Some(b.clone()),
            ..Segue::default()
        };
        assert!(!segue.seamless(&b));
        segue.ended();
        assert!(!segue.seamless(&b), "the skip dropped the line-up");
        // Joined to b, but the queue went elsewhere.
        let mut segue = Segue {
            lined_up: Some(b),
            ..Segue::default()
        };
        segue.ended();
        assert!(!segue.seamless(&a));
    }

    #[test]
    fn a_track_is_announced_once_however_often_the_engine_says_playing() {
        // `Started` resets the position and the badge. The engine says
        // `Playing` on every resume and after every seek, and a resume
        // that drew the bar back to zero would be a bug the user sees.
        let fetch = fetch();
        let mut starts = Starts::default();
        let playing = |at| music::PlaybackEvent::Playing {
            id: Some("1".into()),
            at,
        };
        // Stand in for `open`, which is what records the meta.
        fetch.record_meta(
            "1",
            crate::playback::fetch::Meta {
                bit_depth: Some(24),
                delivered: Quality::HiResLossless,
                sample_rate: 96_000,
            },
        );
        match translate(playing(Duration::ZERO), &mut starts, &fetch) {
            Some(PlaybackEvent::Started {
                bit_depth,
                sample_rate,
                delivered,
            }) => {
                assert_eq!(
                    (bit_depth, sample_rate, delivered),
                    (Some(24), 96_000, Quality::HiResLossless)
                );
            }
            other => panic!("expected Started, got {other:?}"),
        }
        assert_eq!(starts.announced.as_deref(), Some("1"));

        // A resume, or the engine confirming a seek: same track, no badge reset.
        assert!(translate(playing(Duration::from_secs(30)), &mut starts, &fetch).is_none());
        assert!(translate(playing(Duration::ZERO), &mut starts, &fetch).is_none());
    }

    #[test]
    fn a_stream_that_says_nothing_about_itself_is_still_announced() {
        // Another service's fetch records no meta. The track started all
        // the same, and a shell that never hears so draws it stopped.
        let mut starts = Starts::default();
        let event = translate(
            music::PlaybackEvent::Playing {
                id: Some("sub-1".into()),
                at: Duration::ZERO,
            },
            &mut starts,
            &NoMeta,
        );
        assert!(
            matches!(event, Some(PlaybackEvent::Started { sample_rate: 0, .. })),
            "{event:?}"
        );
        assert_eq!(starts.announced.as_deref(), Some("sub-1"));
    }

    fn signal(id: &str, encoding: Option<music::Encoding>, rate: u32) -> music::PlaybackEvent {
        music::PlaybackEvent::Signal {
            id: id.into(),
            track: music::TrackSignal {
                encoding,
                rate,
                channels: 2,
                normalisation: music::Normalisation::Off,
            },
        }
    }

    /// What the shell is told once `id` plays, after the engine's events
    /// before it.
    fn started(
        before: Vec<music::PlaybackEvent>,
        id: &str,
        meta: &dyn StreamMeta,
    ) -> (Option<u8>, u32, Quality) {
        let mut starts = Starts::default();
        for event in before {
            assert!(translate(event, &mut starts, meta).is_none());
        }
        let playing = music::PlaybackEvent::Playing {
            id: Some(id.into()),
            at: Duration::ZERO,
        };
        match translate(playing, &mut starts, meta) {
            Some(PlaybackEvent::Started {
                bit_depth,
                sample_rate,
                delivered,
            }) => (bit_depth, sample_rate, delivered),
            other => panic!("expected Started, got {other:?}"),
        }
    }

    #[test]
    fn what_the_engine_says_of_a_stream_fills_the_badge_of_any_service() {
        let flac = music::Encoding::Flac {
            depth: 24,
            rate: 96_000,
        };
        assert_eq!(
            started(vec![signal("a", Some(flac), 96_000)], "a", &NoMeta),
            (Some(24), 96_000, Quality::Lossless)
        );
        let vorbis = music::Encoding::Vorbis { kbps: 320 };
        assert_eq!(
            started(vec![signal("a", Some(vorbis), 44_100)], "a", &NoMeta),
            (None, 44_100, Quality::High),
            "lossy, whatever its bit rate"
        );
        for unnamed in [None, Some(music::Encoding::Other)] {
            assert_eq!(
                started(vec![signal("a", unnamed, 48_000)], "a", &NoMeta),
                (None, 0, Quality::Lossless),
                "an encoding nobody named is drawn as nothing, not guessed"
            );
        }
    }

    #[test]
    fn a_track_lined_up_keeps_its_own_signal_until_it_plays() {
        // The engine opens the next track while this one plays, at times
        // before it has said this one plays, and a seek opens this one
        // again: each start reads its own track's signal.
        let alac = music::Encoding::Alac { depth: 16 };
        let mp3 = music::Encoding::Mp3 { kbps: 128 };
        let mut starts = Starts::default();
        let play = |starts: &mut Starts, events: Vec<music::PlaybackEvent>| {
            events
                .into_iter()
                .filter_map(|event| translate(event, starts, &NoMeta))
                .last()
        };
        let playing = |id: &str| music::PlaybackEvent::Playing {
            id: Some(id.into()),
            at: Duration::ZERO,
        };
        play(
            &mut starts,
            vec![
                signal("a", Some(mp3), 44_100),
                signal("b", Some(alac), 44_100),
                playing("a"),
            ],
        );
        let next = play(
            &mut starts,
            vec![signal("a", Some(mp3), 44_100), playing("b")],
        );
        assert!(
            matches!(
                next,
                Some(PlaybackEvent::Started {
                    bit_depth: Some(16),
                    delivered: Quality::Lossless,
                    ..
                })
            ),
            "{next:?}"
        );
        assert!(!starts.signals.contains_key("b"), "read once");
    }

    #[test]
    fn a_fetch_that_records_its_own_stream_is_believed_over_the_engine() {
        // TIDAL's answer names the tier; the decoder only knows the rate.
        let fetch = fetch();
        fetch.record_meta(
            "1",
            Meta {
                bit_depth: Some(24),
                delivered: Quality::HiResLossless,
                sample_rate: 96_000,
            },
        );
        assert_eq!(
            started(vec![signal("1", None, 96_000)], "1", &fetch),
            (Some(24), 96_000, Quality::HiResLossless)
        );
    }

    /// A service's fetch that writes down the quality it is told to ask for.
    #[derive(Default)]
    struct Asked(Mutex<Vec<Quality>>);

    impl StreamMeta for Asked {
        fn set_quality(&self, wanted: Quality) {
            self.0.lock().unwrap().push(wanted);
        }
    }

    #[test]
    fn an_engine_starts_at_the_quality_the_settings_ask_for() {
        // Both who fetch hear of it: sonora's factory through its config,
        // and a fetch of our own through its handle.
        let cases = [
            (Quality::HiResLossless, "hi-res"),
            (Quality::Lossless, "lossless"),
            (Quality::High, "high"),
            (Quality::Low, "low"),
        ];
        for (quality, named) in cases {
            let engine = Recorder::default();
            let asked = Arc::new(Asked::default());
            let (out, _events) = tokio::sync::mpsc::unbounded_channel();
            let options = Options {
                quality,
                ..Options::default()
            };
            attach(&engine, asked.clone(), &options, 1.0, &Arc::default(), &out);
            assert_eq!(
                engine.0.lock().unwrap().first().map(String::as_str),
                Some(format!("start {named}").as_str())
            );
            assert_eq!(*asked.0.lock().unwrap(), [quality]);
        }
    }

    #[test]
    fn a_failed_load_names_the_track_and_the_reason() {
        let fetch = fetch();
        let event = translate(
            music::PlaybackEvent::Unavailable {
                id: Some("655911".into()),
            },
            &mut Starts::default(),
            &fetch,
        );
        match event {
            Some(PlaybackEvent::Failed { id, message }) => {
                assert_eq!(id, TrackId::from(655911));
                assert_eq!(message, "track not available for playback");
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn the_rest_of_what_the_engine_says_reaches_the_shell_in_its_own_words() {
        let tell = |event| translate(event, &mut Starts::default(), &NoMeta);
        let id = || Some("7".to_owned());
        let at = Duration::from_secs(12);
        assert!(matches!(
            tell(music::PlaybackEvent::Position { id: id(), at }),
            Some(PlaybackEvent::Position(got)) if got == at
        ));
        assert!(matches!(
            tell(music::PlaybackEvent::Downloaded { id: id() }),
            Some(PlaybackEvent::Downloaded(got)) if got == TrackId::from("7")
        ));
        assert!(matches!(
            tell(music::PlaybackEvent::Throttled { id: id() }),
            Some(PlaybackEvent::Failed { id, message })
                if id == TrackId::from("7") && message.contains("rate limiting")
        ));
        assert!(matches!(
            tell(music::PlaybackEvent::Refused),
            Some(PlaybackEvent::Error(message)) if message.contains("playback keys")
        ));
    }

    /// Track 1 already announced to the shell.
    fn playing_one() -> Starts {
        Starts {
            announced: Some("1".to_owned()),
            ..Starts::default()
        }
    }

    #[test]
    fn a_gated_load_expires_the_session() {
        let fetch = fetch();
        let mut starts = playing_one();
        assert!(matches!(
            translate(music::PlaybackEvent::Gated, &mut starts, &fetch),
            Some(PlaybackEvent::SessionExpired)
        ));
    }

    #[test]
    fn the_end_of_a_track_clears_the_announcement() {
        // So the same track played twice in a row is announced twice.
        let fetch = fetch();
        let mut starts = playing_one();
        assert!(matches!(
            translate(
                music::PlaybackEvent::Ended {
                    id: Some("1".into())
                },
                &mut starts,
                &fetch
            ),
            Some(PlaybackEvent::Finished)
        ));
        assert_eq!(starts.announced, None);
    }

    #[test]
    fn what_the_shell_already_knows_is_not_repeated() {
        let fetch = fetch();
        let mut starts = playing_one();
        for event in [
            music::PlaybackEvent::Paused {
                id: Some("1".into()),
                at: Duration::ZERO,
            },
            music::PlaybackEvent::Seeked {
                id: Some("1".into()),
                at: Duration::from_secs(3),
            },
            music::PlaybackEvent::Playing {
                id: Some("1".into()),
                at: Duration::from_secs(3),
            },
        ] {
            assert!(
                translate(event.clone(), &mut starts, &fetch).is_none(),
                "{event:?} reached the shell"
            );
        }
    }

    #[test]
    fn segment_index_for_a_timestamp_is_found_from_durations() {
        // 4s segments: t=0 -> 0, t=5 -> 1, t=9 -> 2.
        let durs = vec![
            Duration::from_secs(4),
            Duration::from_secs(4),
            Duration::from_secs(4),
        ];
        assert_eq!(segment_for(&durs, Duration::from_secs(0)), 0);
        assert_eq!(segment_for(&durs, Duration::from_secs(5)), 1);
        assert_eq!(segment_for(&durs, Duration::from_secs(9)), 2);

        // The edges themselves. A segment covers [start, start + d): four
        // seconds in is the first instant of the second segment, not the
        // last of the first. Seeking to a segment boundary is what the
        // player does every time it crosses one, so the off-by-one here
        // would land a seek a whole segment early.
        assert_eq!(
            segment_for(&durs, Duration::from_secs(4)),
            1,
            "4s starts segment 1"
        );
        assert_eq!(
            segment_for(&durs, Duration::from_secs(8)),
            2,
            "8s starts segment 2"
        );
    }

    #[test]
    fn a_timestamp_past_the_end_clamps_to_the_last_segment() {
        let durs = vec![Duration::from_secs(4), Duration::from_secs(4)];
        assert_eq!(segment_for(&durs, Duration::from_secs(999)), 1);
    }

    #[test]
    fn an_empty_timeline_yields_the_first_segment() {
        assert_eq!(segment_for(&[], Duration::from_secs(5)), 0);
    }

    #[test]
    fn a_seek_restarts_at_its_segment_and_skips_the_rest_of_it() {
        let durs = vec![Duration::from_secs(4); 3];
        assert_eq!(restart_point(&durs, Duration::ZERO), (0, Duration::ZERO));
        assert_eq!(
            restart_point(&durs, Duration::from_millis(5500)),
            (1, Duration::from_millis(1500)),
            "the second segment, a second and a half in"
        );
        assert_eq!(
            restart_point(&durs, Duration::from_secs(8)),
            (2, Duration::ZERO),
            "a boundary is the start of the segment after it"
        );
        assert_eq!(
            restart_point(&durs, Duration::from_secs(40)),
            (2, Duration::from_secs(32)),
            "past the end: the last segment, and rodio skips to its end"
        );
    }

    #[test]
    fn segment_start_offset_accumulates_preceding_durations() {
        let durs = vec![
            Duration::from_secs(4),
            Duration::from_secs(4),
            Duration::from_secs(4),
        ];
        assert_eq!(segment_start(&durs, 0), Duration::ZERO);
        assert_eq!(segment_start(&durs, 2), Duration::from_secs(8));
    }

    /// An engine that writes down what it was asked, and a stream that
    /// ends at once, so the command thread can be driven to the end.
    #[derive(Clone, Default)]
    struct Recorder(Arc<Mutex<Vec<String>>>);

    impl Recorder {
        fn note(&self, call: String) {
            self.0.lock().unwrap().push(call);
        }
    }

    impl music::Player for Recorder {
        fn load(&self, id: &str, at: Duration, seamless: bool) -> anyhow::Result<()> {
            self.note(format!("load {id} {}s seamless={seamless}", at.as_secs()));
            Ok(())
        }
        fn load_paused_at(&self, id: &str, _at: Duration) -> anyhow::Result<()> {
            self.note(format!("load_paused_at {id}"));
            Ok(())
        }
        fn preload(&self, id: &str, segue: bool) -> anyhow::Result<()> {
            self.note(format!("preload {id} segue={segue}"));
            Ok(())
        }
        fn play(&self) {
            self.note("play".into());
        }
        fn pause(&self) {
            self.note("pause".into());
        }
        fn seek(&self, to: Duration) {
            self.note(format!("seek {}s", to.as_secs()));
        }
        fn set_gain(&self, gain: f32) {
            self.note(format!("gain {gain}"));
        }
    }

    struct Silent;

    #[async_trait::async_trait]
    impl music::PlaybackEvents for Silent {
        async fn next(&mut self) -> Option<music::PlaybackEvent> {
            None
        }
    }

    impl music::PlaybackFactory for Recorder {
        fn start(
            &self,
            config: music::PlaybackConfig,
        ) -> (Box<dyn music::Player>, Box<dyn music::PlaybackEvents>) {
            self.note(format!("start {}", config.quality.id()));
            (Box::new(self.clone()), Box::new(Silent))
        }
    }

    #[test]
    fn a_play_at_an_offset_forgets_the_track_lined_up() {
        // Joined by the engine, then started over at an offset: a later
        // play of the same track is a fresh load, not a seamless join.
        let b = TrackId::from("b");
        let segue: SharedSegue = Arc::new(Mutex::new(Segue {
            lined_up: Some(b.clone()),
            joined: Some(b.clone()),
        }));
        let (out, _events) = tokio::sync::mpsc::unbounded_channel();
        let mut volume = 1.0;
        command(
            None,
            Cmd::PlayAt(b, Duration::from_secs(5)),
            &mut volume,
            &segue,
            &out,
        );
        let after = lock(&segue);
        assert_eq!(
            (after.lined_up.as_ref(), after.joined.as_ref()),
            (None, None)
        );
    }

    #[test]
    fn each_command_reaches_the_engine_as_one_call_and_a_play_before_sign_in_fails() {
        let engine = Recorder::default();
        let (tx, rx) = std::sync::mpsc::channel();
        let (out, mut events) = tokio::sync::mpsc::unbounded_channel();
        let id = |s: &str| TrackId::from(s);
        for cmd in [
            Cmd::Play(id("early")),
            // Kept for the engine that starts later, clamped.
            Cmd::Volume(0.25),
            Cmd::Pause,
            Cmd::Attach {
                factory: Arc::new(engine.clone()),
                meta: Arc::new(NoMeta),
                options: Options::default(),
            },
            // The quality it runs at already: the same engine.
            Cmd::Quality(crate::domain::Quality::HiResLossless),
            Cmd::Play(id("a")),
            Cmd::Preload(id("b")),
            // Lined up but not joined: a skip to it is a fresh load.
            Cmd::Play(id("b")),
            Cmd::PlayAt(id("c"), Duration::from_secs(30)),
            Cmd::Pause,
            Cmd::Resume,
            Cmd::Seek(Duration::from_secs(12)),
            Cmd::Volume(4.0),
            Cmd::Stop,
            // Another one: a fresh engine, at the level kept, for the
            // track that follows.
            Cmd::Quality(crate::domain::Quality::Low),
            Cmd::Play(id("d")),
        ] {
            tx.send(cmd).unwrap();
        }
        drop(tx);
        forward_commands(rx, out);

        assert_eq!(
            *engine.0.lock().unwrap(),
            [
                "start hi-res",
                "gain 0.25",
                "load a 0s seamless=false",
                "preload b segue=true",
                "load b 0s seamless=false",
                "load c 30s seamless=false",
                "pause",
                "play",
                "seek 12s",
                "gain 1",
                "pause",
                "start low",
                "gain 1",
                "load d 0s seamless=false",
            ]
        );
        assert!(matches!(
            events.try_recv(),
            Ok(PlaybackEvent::Failed { id, message })
                if id.as_str() == "early" && message.contains("sign in")
        ));
    }
}
