//! The Settings section: what the app can be told, in the app.
//!
//! Everything here is written straight back to `config.toml`, so a setting
//! changed in a session is still set in the next one. The file stays the
//! authority — this view edits it rather than shadowing it.

use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::theme::Palette;
use crate::domain::Quality;

/// A row of the view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    Service,
    Quality,
    Volume,
    Normalisation,
    Gapless,
    Equalizer,
    Autoplay,
    Explicit,
    Ai,
    NerdFont,
    Theme,
    LastfmKey,
    LastfmSecret,
    LastfmLink,
    Librefm,
    ListenbrainzToken,
    MalojaUrl,
    MalojaKey,
    Discord,
    SubsonicServer,
    SubsonicUser,
    LocalFolders,
}

/// How a row is changed: stepped with h and l, flipped, typed into,
/// grown a folder at a time, or acted on with Enter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Cycle,
    Toggle,
    /// Enter opens a box for the text; a secret is drawn as dots.
    Text { secret: bool },
    /// Enter adds a folder, x takes the last one out.
    Folders,
    /// Enter does the thing: links the account.
    Link,
}

impl Setting {
    pub const ALL: [Setting; 22] = [
        Setting::Service,
        Setting::Quality,
        Setting::Volume,
        Setting::Normalisation,
        Setting::Gapless,
        Setting::Equalizer,
        Setting::Autoplay,
        Setting::Explicit,
        Setting::Ai,
        Setting::NerdFont,
        Setting::Theme,
        Setting::LastfmKey,
        Setting::LastfmSecret,
        Setting::LastfmLink,
        Setting::Librefm,
        Setting::ListenbrainzToken,
        Setting::MalojaUrl,
        Setting::MalojaKey,
        Setting::Discord,
        Setting::SubsonicServer,
        Setting::SubsonicUser,
        Setting::LocalFolders,
    ];

    pub fn kind(&self) -> Kind {
        match self {
            Setting::Service | Setting::Quality | Setting::Volume | Setting::Equalizer | Setting::Theme => Kind::Cycle,
            Setting::Normalisation
            | Setting::Gapless
            | Setting::Autoplay
            | Setting::Explicit
            | Setting::Ai
            | Setting::NerdFont
            | Setting::Librefm => Kind::Toggle,
            Setting::LastfmKey | Setting::MalojaUrl | Setting::Discord | Setting::SubsonicServer | Setting::SubsonicUser => {
                Kind::Text { secret: false }
            }
            Setting::LastfmSecret | Setting::ListenbrainzToken | Setting::MalojaKey => {
                Kind::Text { secret: true }
            }
            Setting::LocalFolders => Kind::Folders,
            Setting::LastfmLink => Kind::Link,
        }
    }

    /// The heading a row sits under.
    pub fn group(&self) -> &'static str {
        match self {
            Setting::Service => "Streaming",
            Setting::Quality | Setting::Volume | Setting::Normalisation | Setting::Gapless | Setting::Equalizer | Setting::Autoplay | Setting::Explicit | Setting::Ai => "Playback",
            Setting::NerdFont | Setting::Theme => "Appearance",
            Setting::LastfmKey | Setting::LastfmSecret | Setting::LastfmLink | Setting::Librefm | Setting::ListenbrainzToken | Setting::MalojaUrl | Setting::MalojaKey => "Scrobbling",
            Setting::Discord => "Discord",
            Setting::SubsonicServer | Setting::SubsonicUser => "Subsonic",
            Setting::LocalFolders => "Local files",
        }
    }

    /// Whether changing this row touches the scrobblers, which are linked
    /// again when it does.
    pub fn is_scrobbling(&self) -> bool {
        self.group() == "Scrobbling"
    }

    fn label(&self) -> &'static str {
        match self {
            Setting::Service => "Service",
            Setting::Quality => "Audio quality",
            Setting::Volume => "Volume",
            Setting::Normalisation => "Normalise loudness",
            Setting::Gapless => "Gapless playback",
            Setting::Equalizer => "Equaliser",
            Setting::Autoplay => "Autoplay",
            Setting::Explicit => "Allow explicit content",
            Setting::Ai => "Allow AI-generated content",
            Setting::NerdFont => "Nerd font icons",
            Setting::Theme => "Theme",
            Setting::LastfmKey => "Last.fm API key",
            Setting::LastfmSecret => "Last.fm API secret",
            Setting::LastfmLink => "Last.fm account",
            Setting::Librefm => "Libre.fm",
            Setting::ListenbrainzToken => "ListenBrainz token",
            Setting::MalojaUrl => "Maloja server",
            Setting::MalojaKey => "Maloja API key",
            Setting::Discord => "Discord application id",
            Setting::SubsonicServer => "Subsonic server",
            Setting::SubsonicUser => "Subsonic user",
            Setting::LocalFolders => "Music folders",
        }
    }

    /// What the setting does, in the words a person would use.
    fn explain(&self) -> &'static str {
        match self {
            Setting::Service => "h or l to pick one, Enter to switch to it and sign in.",
            Setting::Quality => "What is asked for. The bar shows what arrived.",
            Setting::Volume => "Applied to the output, so full is untouched audio.",
            Setting::Normalisation => {
                "Bring every track to one level, from what the service measured."
            }
            Setting::Gapless => "Line the next track up behind this one, so the join has no gap.",
            Setting::Equalizer => "A preset, or off to leave the signal alone.",
            Setting::Autoplay => "Keep playing something similar when the queue runs out.",
            Setting::Explicit => "Off, tracks marked E will not play.",
            Setting::Ai => "Off, tracks marked AI will not play.",
            Setting::NerdFont => "Needs a nerd font. Empty boxes mean you have none.",
            Setting::Theme => "h or l to pick one; the colours change at once.",
            Setting::LastfmKey => "From last.fm/api/account/create. Enter to type it.",
            Setting::LastfmSecret => "Its secret. Enter to type it; shown as dots.",
            Setting::LastfmLink => "Enter to approve the account in the browser, once the key and secret are set.",
            Setting::Librefm => "On, the account is approved in the browser at the next link.",
            Setting::ListenbrainzToken => "The user token from listenbrainz.org/settings. Enter to type it.",
            Setting::MalojaUrl => "Your Maloja server, with its scheme. Enter to type it.",
            Setting::MalojaKey => "Its API key. Enter to type it; shown as dots.",
            Setting::Discord => "From discord.com/developers; empty turns the presence off. Enter to type it.",
            Setting::SubsonicServer => "Used at the next sign-in. Enter to type it.",
            Setting::SubsonicUser => "The password is asked on the sign-in screen. Enter to type it.",
            Setting::LocalFolders => "Enter adds a folder, x takes the last one out; the library is read again.",
        }
    }
}

/// What a text row holds now, as the box opens on it.
pub fn text_of(config: &crate::config::Config, setting: Setting) -> String {
    match setting {
        Setting::LastfmKey => config.scrobble.lastfm.as_ref().map(|k| k.key.clone()).unwrap_or_default(),
        Setting::LastfmSecret => config.scrobble.lastfm.as_ref().map(|k| k.secret.clone()).unwrap_or_default(),
        Setting::ListenbrainzToken => config.scrobble.listenbrainz.clone().unwrap_or_default(),
        Setting::MalojaUrl => config.scrobble.maloja.as_ref().map(|m| m.url.clone()).unwrap_or_default(),
        Setting::MalojaKey => config.scrobble.maloja.as_ref().map(|m| m.key.clone()).unwrap_or_default(),
        Setting::Discord => config.discord.application_id.clone(),
        Setting::SubsonicServer => config.subsonic.server.clone(),
        Setting::SubsonicUser => config.subsonic.username.clone(),
        // A folder row opens on an empty box: the text is a new folder.
        _ => String::new(),
    }
}

/// Write what was typed into the row. A pair that is empty on both sides
/// -- Last.fm's key and secret, Maloja's server and key -- is no service
/// at all, and is written as none.
pub fn set_text(config: &mut crate::config::Config, setting: Setting, text: &str) {
    let text = text.trim();
    match setting {
        Setting::LastfmKey | Setting::LastfmSecret => {
            let mut keys = config.scrobble.lastfm.take().unwrap_or_default();
            match setting {
                Setting::LastfmKey => keys.key = text.to_owned(),
                _ => keys.secret = text.to_owned(),
            }
            config.scrobble.lastfm = (!keys.key.is_empty() || !keys.secret.is_empty()).then_some(keys);
        }
        Setting::ListenbrainzToken => {
            config.scrobble.listenbrainz = (!text.is_empty()).then(|| text.to_owned());
        }
        Setting::MalojaUrl | Setting::MalojaKey => {
            let mut maloja = config.scrobble.maloja.take().unwrap_or_default();
            match setting {
                Setting::MalojaUrl => maloja.url = text.to_owned(),
                _ => maloja.key = text.to_owned(),
            }
            config.scrobble.maloja = (!maloja.url.is_empty() || !maloja.key.is_empty()).then_some(maloja);
        }
        Setting::Discord => config.discord.application_id = text.to_owned(),
        Setting::SubsonicServer => config.subsonic.server = text.to_owned(),
        Setting::SubsonicUser => config.subsonic.username = text.to_owned(),
        Setting::LocalFolders if !text.is_empty() => config.local.folders.push(text.to_owned()),
        _ => {}
    }
}

/// `x` on the folders row.
pub fn remove_last_folder(config: &mut crate::config::Config) -> Option<String> {
    config.local.folders.pop()
}

/// A text row being typed into.
#[derive(Debug, Clone)]
pub struct Edit {
    pub setting: Setting,
    pub typed: String,
}

/// The box a text row opens: its label, what is typed, and the keys.
pub fn render_edit(frame: &mut Frame, area: Rect, palette: &Palette, edit: &Edit) {
    use ratatui::style::{Modifier, Style};
    use ratatui::widgets::{Block, Borders, Clear};
    let secret = matches!(edit.setting.kind(), Kind::Text { secret: true });
    let shown = match secret {
        true => "\u{2022}".repeat(edit.typed.chars().count()),
        false => edit.typed.clone(),
    };
    let lines = vec![
        Line::from(edit.setting.label()),
        Line::from(""),
        Line::from(vec![
            Span::styled(shown, Style::default().add_modifier(Modifier::BOLD)),
            Span::styled("_", palette.subtitle()),
        ]),
        Line::from(""),
        Line::styled("Enter to save, Esc to leave it as it was.", palette.subtitle()),
    ];
    let (w, h) = (62.min(area.width), (lines.len() as u16 + 2).min(area.height));
    let modal = Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    };
    if modal.width < 4 || modal.height < 3 {
        return;
    }
    frame.render_widget(Clear, modal);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(palette.rule())
                .padding(ratatui::widgets::Padding::horizontal(1))
                .title(" Setting ")
                .title_style(palette.page_heading()),
        ),
        modal,
    );
}

/// The qualities in the order the view steps through them, best first.
///
/// TIDAL's own order, and the one that matters: a user moving down this
/// list is giving something up, and it should read that way.
pub const QUALITIES: [Quality; 4] = [
    Quality::HiResLossless,
    Quality::Lossless,
    Quality::High,
    Quality::Low,
];

/// How each quality reads to someone who is not reading the API docs.
pub fn describe(q: Quality) -> &'static str {
    match q {
        Quality::HiResLossless => "Max — FLAC up to 24-bit",
        Quality::Lossless => "High — FLAC, CD quality",
        Quality::High => "Low — AAC 320k",
        Quality::Low => "Lowest — AAC 96k",
    }
}

#[derive(Debug, Default)]
pub struct SettingsState {
    pub selected: usize,
    /// The first row drawn. Set while drawing, since only the renderer
    /// knows how many rows the pane holds, and kept between frames so the
    /// window only moves when the selection leaves it: computed afresh each
    /// frame, `k` from the bottom scrolled the list on every press with the
    /// selection pinned to the last line.
    pub first: std::cell::Cell<usize>,
}

impl SettingsState {
    pub fn next(&mut self) {
        super::cursor::step(&mut self.selected, true, 1, Setting::ALL.len());
    }

    pub fn previous(&mut self) {
        super::cursor::step(&mut self.selected, false, 1, Setting::ALL.len());
    }

    pub fn current(&self) -> Setting {
        Setting::ALL
            .get(self.selected)
            .copied()
            .unwrap_or(Setting::Quality)
    }
}

/// Step the current setting's value along by one, and say what it became.
///
/// Wrapping rather than stopping at the ends: there are four qualities and
/// a list that stops needs two keys to walk, which is more chrome than the
/// setting is worth.
pub fn cycle(config: &mut crate::config::Config, setting: Setting, forward: bool) {
    match setting {
        // Picked here, switched on Enter: the account signs in again, which
        // is not something to do seven times on the way through the list.
        Setting::Service => {
            let all = crate::config::ProviderKind::ALL;
            let at = all
                .iter()
                .position(|k| *k == config.provider.kind)
                .unwrap_or(0);
            let n = all.len();
            let next = if forward { at + 1 } else { at + n - 1 };
            config.provider.kind = all[next % n];
        }
        Setting::Quality => {
            let current = config.audio.quality();
            let at = QUALITIES.iter().position(|q| *q == current).unwrap_or(0);
            let n = QUALITIES.len();
            let next = if forward { at + 1 } else { at + n - 1 };
            config.audio.set_quality(QUALITIES[next % n]);
        }
        // A step rather than a wrap: turning the volume up past full and
        // round to silence is not what anyone means by the key.
        Setting::Volume => {
            let step = if forward { VOLUME_STEP } else { -VOLUME_STEP };
            config.audio.volume = (config.audio.volume + step).clamp(0.0, 1.0);
        }
        // Toggles: either direction flips them, since there are two states
        // and a key that only turned one on would be half a key.
        Setting::Normalisation => config.audio.normalisation = !config.audio.normalisation,
        Setting::Gapless => config.audio.gapless = !config.audio.gapless,
        Setting::Equalizer => {
            let at = EQUALIZERS
                .iter()
                .position(|id| *id == config.audio.equalizer)
                .unwrap_or(0);
            let n = EQUALIZERS.len();
            let next = if forward { at + 1 } else { at + n - 1 };
            config.audio.equalizer = EQUALIZERS[next % n].to_string();
        }
        Setting::Autoplay => config.playback.autoplay = !config.playback.autoplay,
        Setting::Explicit => config.playback.explicit = !config.playback.explicit,
        Setting::Ai => config.playback.ai = !config.playback.ai,
        Setting::NerdFont => {
            config.ui.nerd_font = !config.ui.nerd_font;
            // Applied at once rather than at the next start: the icons are
            // on screen while the setting is being changed, and seeing
            // whether the font has them is the whole point of the row.
            super::icons::set_nerd_font(config.ui.nerd_font);
        }
        Setting::Theme => {
            let themes = super::theme::THEMES;
            let at = themes.iter().position(|t| *t == config.ui.theme).unwrap_or(0);
            let n = themes.len();
            let next = if forward { at + 1 } else { at + n - 1 };
            config.ui.theme = themes[next % n].to_string();
        }
        Setting::Librefm => config.scrobble.librefm = !config.scrobble.librefm,
        // Typed into, grown, or acted on: h and l do nothing here.
        Setting::LastfmKey
        | Setting::LastfmSecret
        | Setting::LastfmLink
        | Setting::ListenbrainzToken
        | Setting::MalojaUrl
        | Setting::MalojaKey
        | Setting::Discord
        | Setting::SubsonicServer
        | Setting::SubsonicUser
        | Setting::LocalFolders => {}
    }
}

/// The equaliser's choices, in the order the row steps through them:
/// off, then every preset sonora's equaliser knows, by the id the config
/// file takes.
const EQUALIZERS: [&str; 13] = [
    "off",
    "flat",
    "bass-boost",
    "bass-reducer",
    "treble-boost",
    "vocal",
    "rock",
    "pop",
    "jazz",
    "classical",
    "electronic",
    "acoustic",
    "loudness",
];

/// The preset the config names, or none for `off` and anything misspelt.
pub fn equalizer_preset(config: &crate::config::Config) -> Option<music::equalizer::Preset> {
    music::equalizer::Preset::ALL
        .into_iter()
        .find(|p| p.id() == config.audio.equalizer)
}

/// How the equaliser row reads: the preset's id, or Off.
fn equalizer_value(config: &crate::config::Config) -> String {
    match equalizer_preset(config) {
        Some(preset) => preset.id().to_string(),
        None => "Off".to_string(),
    }
}

/// A toggle's value column.
fn on_off(on: bool) -> String {
    if on { "On" } else { "Off" }.to_string()
}

/// How much one press moves the volume. A twentieth: ten steps end to end
/// is a lot of pressing, and a hundred is a key that does nothing.
const VOLUME_STEP: f32 = 0.05;

/// The value column, as drawn.
/// A secret's value: whether there is one, never what it is.
fn set_or_not(text: &str) -> String {
    match text.is_empty() {
        true => "not set".to_string(),
        false => "set".to_string(),
    }
}

/// `linked` is each scrobbler linked right now, as (service id, label).
fn value_of(config: &crate::config::Config, setting: Setting, linked: &[(String, String)]) -> String {
    let linked_as = |id: &str| {
        linked
            .iter()
            .find(|(service, _)| service == id)
            .map(|(_, label)| format!("linked as {label}"))
    };
    match setting {
        Setting::Theme => match config.ui.theme.is_empty() {
            true => "default".to_string(),
            false => config.ui.theme.clone(),
        },
        Setting::LastfmSecret | Setting::ListenbrainzToken | Setting::MalojaKey => {
            set_or_not(&text_of(config, setting))
        }
        Setting::LastfmLink => match config.scrobble.lastfm.is_some() {
            false => "set the key and secret first".to_string(),
            true => linked_as("lastfm").unwrap_or_else(|| "not linked".to_string()),
        },
        Setting::Librefm => match (config.scrobble.librefm, linked_as("librefm")) {
            (false, _) => "Off".to_string(),
            (true, Some(as_who)) => format!("On, {as_who}"),
            (true, None) => "On, not linked".to_string(),
        },
        Setting::LastfmKey | Setting::MalojaUrl | Setting::Discord | Setting::SubsonicServer | Setting::SubsonicUser => {
            let text = text_of(config, setting);
            match text.is_empty() {
                true => "not set".to_string(),
                false => text,
            }
        }
        Setting::LocalFolders => match config.local.folders.len() {
            0 => "none".to_string(),
            _ => config.local.folders.join(", "),
        },
        Setting::Service => config.provider.kind.label().to_string(),
        Setting::Quality => describe(config.audio.quality()).to_string(),
        Setting::Volume => volume_bar(config.audio.volume),
        Setting::Normalisation => on_off(config.audio.normalisation),
        Setting::Gapless => on_off(config.audio.gapless),
        Setting::Equalizer => equalizer_value(config),
        Setting::Autoplay => on_off(config.playback.autoplay),
        Setting::Explicit => on_off(config.playback.explicit),
        Setting::Ai => on_off(config.playback.ai),
        // Drawn with the icons themselves: a terminal cannot be asked what
        // font it has, so the row shows the glyphs and lets the user see.
        Setting::NerdFont => {
            if config.ui.nerd_font {
                format!(
                    "On   {} {} {}",
                    super::icons::music(),
                    super::icons::albums(),
                    super::icons::favourite()
                )
            } else {
                "Off".to_string()
            }
        }
    }
}

/// The volume as a bar and a percentage.
///
/// A number alone says what it is; the bar says where it sits between the
/// ends, which is what the key is moving.
fn volume_bar(v: f32) -> String {
    const CELLS: usize = 10;
    let v = v.clamp(0.0, 1.0);
    let full = (v * CELLS as f32).round() as usize;
    format!(
        "{}{}  {:>3}%",
        "█".repeat(full),
        "░".repeat(CELLS - full),
        (v * 100.0).round() as u32
    )
}

/// Width of the label column, so the values line up in one column.
///
/// Measured from the labels rather than written down: a fixed width was one
/// short of the longest label, so two rows had their value pressed against
/// the text with no gap at all. Adding a setting cannot break the alignment
/// now.
fn label_width() -> usize {
    const GAP: usize = 2;
    Setting::ALL
        .iter()
        .map(|s| s.label().chars().count())
        .max()
        .unwrap_or(0)
        + GAP
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    palette: &Palette,
    config: &crate::config::Config,
    state: &SettingsState,
    focused: bool,
    linked: &[(String, String)],
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    frame.render_widget(
        Paragraph::new(Line::styled("Settings", palette.page_heading())),
        Rect { height: 1, ..area },
    );

    // The bar's column is reserved like everywhere else: twenty-five rows
    // three lines each overhang most panes, and the page gave no sign.
    let area = super::scrollbar::reserve(area);
    let mut y = area.y + 2;
    let bottom = area.y + area.height;
    // Three lines a row. On a pane too short for them all, the list is
    // scrolled just far enough that the selected row is drawn: a setting
    // the keys can reach but the screen cannot show is a setting nobody
    // can read while changing it.
    // Three lines a row and one more wherever a heading comes.
    let fits = area.height.saturating_sub(2) as usize;
    let height_of = |rows: &[Setting]| -> usize {
        let headings = rows
            .windows(2)
            .filter(|w| w[0].group() != w[1].group())
            .count()
            + 1;
        rows.len() * 3 + headings
    };
    // The window moves only when the selection leaves it: up to the
    // selection, or down just far enough that the selected row is whole.
    // And back up when the pane has grown, rather than leave the bottom of
    // the list hanging over blank lines.
    let selected = state.selected.min(Setting::ALL.len() - 1);
    let mut first = state.first.get().min(selected);
    while first < selected && height_of(&Setting::ALL[first..=selected]) > fits {
        first += 1;
    }
    while first > 0 && height_of(&Setting::ALL[first - 1..]) <= fits {
        first -= 1;
    }
    state.first.set(first);
    let mut group: Option<&str> = None;
    let mut shown = 0;
    for (i, setting) in Setting::ALL.iter().enumerate().skip(first) {
        if y >= bottom {
            break;
        }
        shown += 1;
        // A heading where the group changes, so twenty-five rows read as
        // seven short lists.
        if group != Some(setting.group()) {
            group = Some(setting.group());
            frame.render_widget(
                Paragraph::new(Line::styled(
                    format!("  {}", setting.group()),
                    palette.section_heading(),
                )),
                Rect {
                    x: area.x,
                    y,
                    width: area.width,
                    height: 1,
                },
            );
            y += 1;
            if y >= bottom {
                break;
            }
        }
        let selected = focused && i == state.selected;
        // The same grey band a selected row gets everywhere else, so a
        // selection here reads as the selections do in the rest of the app.
        if selected {
            frame.render_widget(
                ratatui::widgets::Block::default()
                    .style(ratatui::style::Style::default().bg(palette.selection)),
                Rect {
                    x: area.x,
                    y,
                    width: area.width,
                    height: 1,
                },
            );
        }
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!("  {:width$}", setting.label(), width = label_width()),
                    palette.title(),
                ),
                Span::styled(value_of(config, *setting, linked), palette.accent_text()),
            ])),
            Rect {
                x: area.x,
                y,
                width: area.width,
                height: 1,
            },
        );
        y += 1;

        if y < bottom {
            frame.render_widget(
                Paragraph::new(Line::styled(
                    format!(
                        "  {:width$}{}",
                        "",
                        setting.explain(),
                        width = label_width()
                    ),
                    palette.subtitle(),
                )),
                Rect {
                    x: area.x,
                    y,
                    width: area.width,
                    height: 1,
                },
            );
            y += 2;
        }
    }

    super::scrollbar::render(
        frame,
        Rect {
            y: area.y + 2,
            height: area.height.saturating_sub(2),
            ..area
        },
        palette,
        Setting::ALL.len(),
        first,
        shown,
    );

    if y < bottom {
        frame.render_widget(
            Paragraph::new(Line::styled(
                "  h l  change   Enter  type or do   x  drop a folder      saved to config.toml",
                palette.subtitle(),
            )),
            Rect {
                x: area.x,
                y,
                width: area.width,
                height: 1,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_typed_pair_is_one_service_and_an_emptied_pair_is_none() {
        let mut config = crate::config::Config::default();
        set_text(&mut config, Setting::LastfmKey, " abc ");
        assert_eq!(config.scrobble.lastfm.as_ref().map(|k| k.key.as_str()), Some("abc"));
        set_text(&mut config, Setting::LastfmSecret, "s3");
        assert_eq!(config.scrobble.lastfm.as_ref().map(|k| k.secret.as_str()), Some("s3"));
        assert_eq!(value_of(&config, Setting::LastfmSecret, &[]), "set", "a secret is never shown");
        assert_eq!(value_of(&config, Setting::LastfmLink, &[]), "not linked");
        let linked = vec![("lastfm".to_string(), "Last.fm".to_string())];
        assert_eq!(value_of(&config, Setting::LastfmLink, &linked), "linked as Last.fm");
        set_text(&mut config, Setting::LastfmKey, "");
        set_text(&mut config, Setting::LastfmSecret, "");
        assert!(config.scrobble.lastfm.is_none(), "both empty is no service");

        set_text(&mut config, Setting::LocalFolders, "~/Music");
        set_text(&mut config, Setting::LocalFolders, "/media/flac");
        assert_eq!(value_of(&config, Setting::LocalFolders, &[]), "~/Music, /media/flac");
        assert_eq!(remove_last_folder(&mut config).as_deref(), Some("/media/flac"));
        assert_eq!(config.local.folders, ["~/Music"]);

        cycle(&mut config, Setting::Theme, true);
        assert_eq!(config.ui.theme, super::super::theme::THEMES[1]);
        cycle(&mut config, Setting::Theme, false);
        assert_eq!(config.ui.theme, super::super::theme::THEMES[0]);
    }

    #[test]
    fn every_row_has_a_group_and_the_groups_are_contiguous() {
        let mut seen: Vec<&str> = Vec::new();
        for s in Setting::ALL {
            match seen.last() {
                Some(last) if *last == s.group() => {}
                _ => {
                    assert!(!seen.contains(&s.group()), "{:?} is split in two", s.group());
                    seen.push(s.group());
                }
            }
        }
        let buf = crate::shell::geometry::draw(120, 90, |f, area, palette| {
            render(f, area, palette, &crate::config::Config::default(), &SettingsState::default(), true, &[]);
        });
        let text = crate::shell::geometry::text(&buf);
        for group in ["Scrobbling", "Discord", "Local files", "TIDAL"] {
            assert!(text.contains(group), "{group} is headed:\n{text}");
        }
    }

    #[test]
    fn the_three_content_settings_are_toggles_with_tidals_own_defaults() {
        let mut config = crate::config::Config::default();

        // TIDAL's own: explicit and AI allowed, autoplay on. Turning
        // something off is a choice the user makes.
        assert!(config.playback.explicit, "explicit starts allowed");
        assert!(config.playback.ai, "so does AI");
        assert!(config.playback.autoplay, "the music carries on by default");

        for (setting, read) in [
            (
                Setting::Autoplay,
                (|c: &crate::config::Config| c.playback.autoplay)
                    as fn(&crate::config::Config) -> bool,
            ),
            (Setting::Explicit, |c| c.playback.explicit),
            (Setting::Ai, |c| c.playback.ai),
        ] {
            let before = read(&config);
            cycle(&mut config, setting, true);
            assert_ne!(read(&config), before, "{setting:?} did not flip");
            // Either direction: two states, so a key that only turned it on
            // would be half a key.
            cycle(&mut config, setting, false);
            assert_eq!(read(&config), before, "{setting:?} did not flip back");
        }
    }

    #[test]
    fn a_toggle_reads_on_or_off_rather_than_true_or_false() {
        let mut config = crate::config::Config::default();
        config.playback.autoplay = false;
        assert_eq!(value_of(&config, Setting::Autoplay, &[]), "Off");
        config.playback.autoplay = true;
        assert_eq!(value_of(&config, Setting::Autoplay, &[]), "On");
    }

    #[test]
    fn a_short_pane_scrolls_to_the_selected_row() {
        // Ten rows of three lines do not fit a 24-line terminal; the last
        // setting was reachable with the keys and never drawn.
        let config = crate::config::Config::default();
        let mut state = SettingsState::default();
        for _ in 0..Setting::ALL.len() {
            state.next();
        }
        let last = state.current();
        assert_eq!(Some(last), Setting::ALL.last().copied());
        let buf = crate::shell::geometry::draw(120, 24, |f, area, palette| {
            render(f, area, palette, &config, &state, true, &[]);
        });
        let text = crate::shell::geometry::text(&buf);
        assert!(text.contains(last.label()), "the selected row is drawn:\n{text}");
        assert!(!text.contains(Setting::Quality.label()), "and the top scrolled off:\n{text}");
    }

    #[test]
    fn the_window_moves_only_when_the_selection_leaves_it() {
        // From the bottom, `k` scrolled the list on every press, with the
        // selection pinned to the last line, instead of walking up the rows
        // on screen and scrolling once the top one was reached.
        let config = crate::config::Config::default();
        let mut state = SettingsState::default();
        let draw = |state: &SettingsState| {
            crate::shell::geometry::draw(120, 24, |f, area, palette| {
                render(f, area, palette, &config, state, true, &[]);
            });
            state.first.get()
        };
        for _ in 0..Setting::ALL.len() - 1 {
            state.next();
            draw(&state);
        }
        let at_bottom = draw(&state);
        assert!(at_bottom > 0, "the last row needs a scrolled window");
        state.previous();
        assert_eq!(draw(&state), at_bottom, "one up stays in the window");
        while state.selected > at_bottom {
            state.previous();
            assert_eq!(draw(&state), at_bottom, "still in the window at {}", state.selected);
        }
        state.previous();
        assert_eq!(draw(&state), state.selected, "past the top, the window follows by one");
    }

    #[test]
    fn a_short_pane_shows_a_scrollbar_and_a_tall_one_does_not() {
        // The page overhangs most terminals and gave no sign of it: a list
        // that ended because the pane did looked like one at its end.
        let config = crate::config::Config::default();
        let thumb = |state: &SettingsState, height: u16| -> Vec<u16> {
            let buf = crate::shell::geometry::draw(120, height, |f, area, palette| {
                render(f, area, palette, &config, state, true, &[]);
            });
            (0..height)
                .filter(|y| buf[(119, *y)].symbol() == "\u{2588}")
                .collect()
        };
        let mut state = SettingsState::default();
        let top = thumb(&state, 24);
        assert_eq!(top.first(), Some(&2), "the thumb starts under the title: {top:?}");
        for _ in 0..Setting::ALL.len() {
            state.next();
        }
        let bottom = thumb(&state, 24);
        assert_eq!(bottom.last(), Some(&23), "and reaches the bottom at the last row: {bottom:?}");
        assert!(thumb(&state, 100).is_empty(), "a pane that holds it all has no bar");
    }

    #[test]
    fn every_value_starts_on_the_same_column() {
        // Reported as the value pressed against the label with no gap. The
        // label column was a fixed 16, one short of "Allow AI-generated
        // content" at 26 -- so two rows ran into their own value. Measured
        // from the labels now, so adding a setting cannot break it.
        let fixed = crate::shell::icons::Fixed::at(false);
        let _ = &fixed;
        let config = crate::config::Config::default();
        let state = SettingsState::default();
        let buf = crate::shell::geometry::draw(120, 100, |f, area, palette| {
            render(f, area, palette, &config, &state, false, &[]);
        });
        let text = crate::shell::geometry::text(&buf);

        let mut columns = Vec::new();
        for setting in Setting::ALL {
            let label = setting.label();
            let row = text
                .lines()
                .find(|l| l.contains(label))
                .unwrap_or_else(|| panic!("{label} is drawn"));
            let at = row.find(label).expect("the label");
            // Byte offsets added to a char count: they agree only while
            // nothing left of the value is multi-byte, which is why the
            // labels are all ASCII.
            let value_at = row[at + label.len()..]
                .find(|c: char| !c.is_whitespace())
                .map(|off| at + label.chars().count() + off);
            columns.push((label, value_at));
        }

        let first = columns[0].1.unwrap_or_else(|| panic!("the first row has a value:\n{text}"));
        for (label, at) in &columns {
            let at = at.unwrap_or_else(|| panic!("{label} has no value beside it"));
            assert_eq!(
                at, first,
                "{label:?} starts its value at {at}, the first row at {first}:\n{text}"
            );
        }

        // And there is a gap: a value against the text is what was reported.
        let longest = Setting::ALL
            .iter()
            .map(|s| s.label().chars().count())
            .max()
            .unwrap_or(0);
        assert!(
            first > longest,
            "the value column starts at {first}, inside the longest label at {longest}"
        );
    }

    #[test]
    fn the_nerd_font_row_toggles_the_icons_as_it_is_changed() {
        // The point of the row is finding out whether the font is
        // installed, so the icons change while it is being chosen rather
        // than at the next start. There is no detecting it: a terminal does
        // not say what font it has, and a nerd font advances one cell just
        // as the replacement box does.
        let fixed = crate::shell::icons::Fixed::at(false);
        let _ = &fixed;
        let mut config = crate::config::Config::default();
        assert!(!config.ui.nerd_font, "off by default, as lazygit has it");

        let plain = crate::shell::icons::music();
        cycle(&mut config, Setting::NerdFont, true);
        assert!(config.ui.nerd_font, "the setting turned on");
        assert_ne!(
            crate::shell::icons::music(),
            plain,
            "and the icons changed with it"
        );

        // Either direction flips it: two states, so a key that only turned
        // it on would be half a key.
        cycle(&mut config, Setting::NerdFont, false);
        assert!(!config.ui.nerd_font, "and back again");
        assert_eq!(crate::shell::icons::music(), plain);
    }

    #[test]
    fn the_row_shows_the_icons_it_is_offering() {
        // "On" alone says nothing about whether they will draw. The row
        // carries a few of the glyphs so the answer is on screen -- which
        // is the whole reason there is no auto-detection here.
        let fixed = crate::shell::icons::Fixed::at(false);
        let mut config = crate::config::Config::default();

        config.ui.nerd_font = false;
        assert_eq!(value_of(&config, Setting::NerdFont, &[]), "Off");

        config.ui.nerd_font = true;
        fixed.set(true);
        let on = value_of(&config, Setting::NerdFont, &[]);
        assert!(on.starts_with("On"), "it says so: {on:?}");
        assert!(
            on.chars().any(|c| (0xE000..=0xF8FF).contains(&(c as u32))),
            "and shows the glyphs themselves: {on:?}"
        );
    }
}
