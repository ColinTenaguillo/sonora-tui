<div align="center">

<img src=".github/readme-icon.svg" alt="" width="60" height="60">

<h1>sonora-tui</h1>

<p align="center">
  <a href="CHANGELOG.md">Changelog</a>
  ·
  <a href="#install">Install</a>
  ·
  <a href="#services">Services</a>
  ·
  <a href="#keys">Keys</a>
  ·
  <a href="CONTRIBUTING.md">Contributing</a>
</p>

[![Build](https://img.shields.io/github/actions/workflow/status/ColinTenaguillo/sonora-tui/ci.yml?style=flat-square&label=build)](https://github.com/ColinTenaguillo/sonora-tui/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/ColinTenaguillo/sonora-tui?style=flat-square&label=release)](https://github.com/ColinTenaguillo/sonora-tui/releases/latest)
[![License](https://img.shields.io/github/license/ColinTenaguillo/sonora-tui?style=flat-square&label=license)](LICENSE)
[![Built With Ratatui](https://ratatui.rs/built-with-ratatui/badge.svg)](https://ratatui.rs/)

### A terminal music player for your streaming services

TIDAL, Spotify, Apple Music, Deezer, YouTube Music, Subsonic and your own files —
in the terminal, over [sonora](https://github.com/sonorahq/sonora)'s `music` crate.

</div>

<p align="center">
  <img src="docs/demo.gif" alt="A tour: the home page, a hi-res album playing, a search, an artist page and biography, the queue, Explore, a filtered list and the key list">
</p>

> **Not a piracy tool.** Each service wants the subscription it wants — a
> paid TIDAL or Spotify Premium account, an Apple Music subscription — and so
> does this. Nothing here exports streams, keys or licences, and nothing here
> will. Unofficial throughout: no service endorses it, and the private APIs
> it speaks can change at any time.

## Features

- **TIDAL, Spotify, Apple Music, Deezer, YouTube Music, Subsonic/Navidrome**
  and local files — one service at a time, switched from Settings
- The home page, Explore, search, album and artist pages, your playlists,
  albums, artists and favourites, each in its own view
- Playback up to **24-bit hi-res** where the service delivers it, gapless,
  normalised from the service's own loudness measure, an equaliser with
  presets, a sleep timer
- A queue with shuffle, repeat and autoplay, and favouriting from anywhere
- Synced lyrics, following the track
- Scrobbling to Last.fm, Libre.fm, ListenBrainz and Maloja
- Discord Rich Presence
- The keyboard's media keys and the desktop's player widget — MPRIS on
  Linux, Now Playing on macOS
- Cover art through the terminal's image protocol, or half blocks where
  there is none
- Seven themes, and any colour by hand

## Install

Runs on macOS 12 or later (Apple Silicon, Intel) and Linux (x86_64,
aarch64). On Linux it plays through ALSA, which every desktop has.

### Homebrew

macOS or Linux. The one way in that follows the releases: `brew upgrade`
brings the next version.

```sh
brew install ColinTenaguillo/tap/sonora-tui
```

### Installer script

Downloads the binary for your platform into `~/.cargo/bin`. Run it again
for a newer release.

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/ColinTenaguillo/sonora-tui/releases/latest/download/sonora-tui-installer.sh | sh
```

### Prebuilt archive

Each [release](https://github.com/ColinTenaguillo/sonora-tui/releases/latest)
carries an archive per platform and its SHA-256. Unpack it and put
`sonora-tui` on your `PATH`.

### From source

Rust (stable), a C++ compiler for the Widevine host, CMake for the Opus
decoder, and on Linux the ALSA headers:

```sh
sudo apt install build-essential cmake pkg-config libasound2-dev   # Debian, Ubuntu
sudo dnf install gcc-c++ cmake pkg-config alsa-lib-devel            # Fedora
sudo pacman -S base-devel cmake pkgconf alsa-lib                    # Arch
```

```sh
cargo install --git https://github.com/ColinTenaguillo/sonora-tui
```

or from a checkout, `cargo build --release` and run `target/release/sonora-tui`.

## Services

The service comes from the config, and TIDAL is the default:

```toml
[provider]
kind = "tidal"   # spotify, apple, deezer, youtube, subsonic, local
```

Settings (`8`) has the same choice as its first row: pick a service with
`h`/`l`, press Enter, and the app leaves the one it is on and signs in to the
other. On the sign-in screen itself, `Tab` steps to the next service. Each
service keeps its own session, so coming back asks nothing.

Each signs in from the same screen, its own way:

| `kind` | How it signs in |
|---|---|
| `tidal` | A code and a link, confirmed in your browser. HiFi Plus for hi-res. See [TIDAL](#tidal) below. |
| `spotify` | A link opens in the browser; authorise there and come back. Needs Premium, and an account Spotify still serves to librespot: newer accounts (late 2025 on) are refused playback keys whatever their plan, see [librespot#1649](https://github.com/librespot-org/librespot/issues/1649). |
| `apple` | Paste the `Cookie` header of a signed-in music.apple.com tab. Playback needs Google's Widevine module: see below. |
| `deezer` | Paste the `Cookie` header (or just the `arl` cookie) of a signed-in deezer.com tab. |
| `youtube` | `g` to continue as a guest, or paste the `Cookie` header of a signed-in music.youtube.com tab; with several accounts on it, pick one. |
| `subsonic` | Server and user asked on the login screen the first time, then kept in the config; the password is typed each time, never stored. |
| `local` | Nothing: the first run asks for a folder, and the folders in the config are read at each start. |

No webview: the cookies come out of the browser's devtools (Network, any
request, Request Headers, the `Cookie` line). Cookies, passwords and tokens
are kept by the provider's own credential store, never written to the
config.

What each view shows is what the service answers with. The feed, the mixes
and the hi-res badge are TIDAL's; a Subsonic server has no Explore; the
local library has no radio.

### Subsonic

Navidrome, Gonic, Airsonic — anything that speaks the Subsonic API. The
first run asks for the server and the user and writes them here:

```toml
[provider]
kind = "subsonic"

[subsonic]
server   = "https://music.example.org"
username = "you"
```

### Local files

With no folder named, the sign-in screen asks for one and writes it to the
config. The folders are read at each start, from an index kept under the
cache between runs, and every track, album and artist found is the library:

```toml
[provider]
kind = "local"

[local]
folders = ["~/Music", "/media/flac"]
```

### Apple Music and Widevine

Apple's streams are encrypted, and only Google's Widevine module decrypts
them. Nothing ships it — Google publishes nothing anyone may redistribute —
so once an Apple Music account is in, the machine is searched for a
browser's copy. With none, the app offers to download the module from
Google's update service, as Chrome and Kodi get theirs, shows Google's terms
out of the archive, and installs it into its own store only on a second yes.
`SONORA_WIDEVINE_CDM=/path/to/module` names one of your own instead.
Declined, Apple Music keeps its catalogue and refuses to play. The module
runs in a process of its own, this binary started again, so its memory is
spent only while an Apple track is loaded.

### TIDAL

TIDAL grants full playback only to its own applications, so this — like
every third-party TIDAL client — sends the `client_id` of one of them, and
TIDAL believes it is talking to that application. That is a
misrepresentation, and it is worth knowing before you run it. It is nothing
private to you: your login happens in your browser, and your session token
stays on your machine, mode `0600`.

There is no legitimate way to replace them today:

- TIDAL's developer portal issues credentials, but they are refused for the
  device flow — the code-and-a-link login a terminal needs — with `Client is
  not a Limited Input Device client`.
- Those credentials also cap playback at 30-second previews, and only
  through TIDAL's Player SDK, which has no build for a terminal.
- Device-flow credentials are
  [documented as internal to TIDAL](https://github.com/hmelder/TIDAL/wiki/Authentication),
  and public requests for third-party access have gone
  [unanswered](https://github.com/orgs/tidal-music/discussions/321).

If TIDAL opens a path for third-party device clients this should change.
The pair is base64 in the source and never written to your config, so it
stays out of code search and secret scanners. If playback fails with
**"this client_id cannot stream"** or a `4005` status, the shipped pair has
been rate-capped; until a release changes it, put working ones in the config,
the same ones the other third-party clients carry:

```toml
[auth]
client_id     = "..."
client_secret = "..."
```

## Keys

Press `?` in the app for the full list.

| Key | Action |
|---|---|
| `j` / `k`, `↓` / `↑` | Down / up in the list or grid |
| `Ctrl-d` / `Ctrl-u` | Half a screen down / up |
| `h` / `l`, `←` / `→` | Left / right along a row |
| `J` / `K` | Move through the sidebar |
| `1` – `8` | Straight to a nav entry — `1` is Music, `8` is Settings |
| `Enter` | Open a playlist, album or artist, or play a track |
| `Esc`, `[` | Back one view, or out of the filter or the bio; on the sign-in screen, out of the sign-in under way |
| `]` | Forward again, through views and sections |
| `/` | Filter the current view: case and accents ignored, words in any order, letters in order will do ("kdl" finds Kendrick Lamar, below exact hits) |
| `s` | Search the catalogue |
| `o` | See all of a row |
| `b` | Open an artist's biography |
| `a` / `c` | Open the artist, or the album whose cover you are looking at |
| `R` | Radio from the selected track, or the one playing |
| `t`, `Tab` | Next tab in search; on the sign-in screen, the next service |
| `Space` | Pause / resume |
| `A` | Favourite the track, or take it out again |
| `F` | Favourite the artist, album, playlist or mix under the cursor, or the one whose view is open; again to take it out |
| `P` | Add the selected track to one of your playlists — a list with covers and sizes, `/` to search it — or to a new one named on the spot |
| `N` | New playlist |
| `m` / `D` | Rename, or delete after a `y`, the playlist open or under the cursor — yours only |
| `x` | In a playlist of yours: take the highlighted track out |
| `n` / `p` | Next / previous in the queue |
| `.` / `,` | Ten seconds forward / back in the track |
| `e` / `E` | Queue the selected track next, or after the rest |
| `Q` | What is playing next — `j`/`k` to move, `x` to remove, enter to play |
| `z` | Shuffle the queue |
| `r` | Repeat: off, all, one |
| `S` | Sleep timer: 15, 30, 45, 60 minutes, end of track, off |
| `L` | The lyrics, synced where they are — `j`/`k` to scroll, `Esc` to close |
| `?` | The key list; `j`/`k` scroll it on a short terminal |
| `q` | Quit |

`Esc` steps back rather than quitting: only `q` leaves the app.

### The queue

`Q` shows what is playing next. Three things can be in it, and they are kept
apart the way the services' own clients keep them:

- **Queued** — what you asked for by name. Survives starting another album,
  because you asked for the track rather than the album it came from.
- The **album, playlist or mix** you started. Replaced when you start
  another one.
- **Autoplay** — similar tracks, filled in behind everything else as the
  queue nears its end, and refilled as long as you keep listening. Seeded
  from the last track queued. Dropped as soon as there is something real to
  play, so a radio picked twenty minutes ago never resurfaces. On by
  default; `autoplay = false` in the config, or `i` in the queue view, stops
  the music at the end of the queue.

The keyboard's media keys work too — play/pause, next, previous — and they
reach the app whether or not the terminal has focus, since the desktop
delivers them rather than the terminal.

## Sound

Settings (`8`) has three rows the engine reads, all written to the config:

```toml
[audio]
normalisation = false   # bring every track to one level, from the service's own measure
gapless = true          # line the next track up behind this one
equalizer = "off"       # or flat, rock, pop, jazz, classical, electronic, acoustic,
                        # vocal, bass-boost, bass-reducer, treble-boost, loudness
```

The equaliser changes as you step through the presets. The other two restart
the engine, which picks the track back up where it was.

## Lyrics

`L` opens them over the main pane. Synced lyrics follow the position, the
line being sung in the middle; plain ones are a page to scroll. They come
from the sources sonora asks — the file's own tags, Binimum, Musixmatch,
LRCLIB, Kugou, NetEase — ranked its way: synced over plain, a close match
over a loose one.

## Scrobbling

Name a service in the config and listens go to it: what is playing now, and
each track once it has run half its length or four minutes, as the services
themselves count it.

```toml
[scrobble]
listenbrainz = "the user token from listenbrainz.org/settings"
lastfm = { key = "an API key", secret = "its secret" }   # from last.fm/api/account/create
librefm = true
maloja = { url = "https://maloja.example.org", key = "an API key" }
```

Last.fm and Libre.fm approve the account in the browser, once, at the first
start after. The linked accounts are kept in `scrobble.json` beside the
config, mode `0600`.

## Discord

What is playing, on your Discord profile, with the album art and a clock
that counts down the track. The name shown beside it is a Discord
application's, so register one at discord.com/developers — any name you
like — and give the config its id:

```toml
[discord]
application_id = "1234567890123456789"
```

Discord not running is not an error: it is tried again a little later.

## Themes

Seven are built in, each taken from the values its own project publishes:

| `theme = ` | |
|---|---|
| `default` | TIDAL's own colours |
| `catppuccin` | Catppuccin Mocha |
| `gruvbox` | Gruvbox dark |
| `tokyonight` | Tokyo Night Storm |
| `nord` | Nord |
| `dracula` | Dracula |
| `solarized` | Solarized dark |

```toml
[ui]
theme = "catppuccin"
```

Any colour can be set by hand, and wins over whichever theme is named:

```toml
[ui]
theme = "catppuccin"

[ui.colors]
accent = "#f5c2e7"
selection = "#585b70"
```

The eleven names are `accent`, `text`, `dim`, `heading`, `surface`,
`selection`, `quality`, `track`, `placeholder`, `border` and `on_accent`.
Values are `#rrggbb`, with the `#` optional.

A line that cannot be read is skipped, not fatal. A misspelt colour name or a
value that is not `#rrggbb` leaves that one colour as the theme had it, keeps
the rest, and says what went wrong in the status bar and the log. An unknown
theme name falls back to `default` the same way. The app always starts.

Named themes are drawn in truecolor; `default` falls back to 256 colours where
the terminal cannot do better.

## Where things live

| What | Linux | macOS |
|---|---|---|
| Config | `~/.config/sonora-tui/config.toml` | `~/Library/Application Support/sonora-tui/config.toml` |
| TIDAL session token | alongside the config, mode `0600` | same |
| Other services' sessions | the provider's credential store, under the config directory | same |
| Scrobbling accounts | `scrobble.json` alongside the config, mode `0600` | same |
| Local-files index, Widevine module | under the cache directory | same |
| Log | `~/.cache/sonora-tui/sonora-tui.log` | `~/Library/Caches/sonora-tui/sonora-tui.log` |

A config, a session or a cache left by ratidal, this app's earlier name, is
moved under the new name on the first start.

## Known limits

- **One service at a time.** Switching signs out of the view, not of the
  account: each service keeps its session.
- **Not bit-perfect.** The device's physical format is left alone, so content
  is resampled to whatever rate it is already at. The quality badge reports
  what the service delivered, not what reached the speakers.
- **No videos.** This plays audio, so the video rows on genre pages are left
  out.
- **A refused scrobble is retried** with the next batch, in memory, up to
  two hundred listens a service; nothing survives quitting but the one last
  try on the way out.
- **A card's year is what the service sends.** Spotify and YouTube Music
  send none on their home shelves, so their album cards there have no year;
  the album page and the Albums view do.

## Contributing

How the crate is laid out, what the test suites cover, and the rules the build
enforces: [CONTRIBUTING.md](CONTRIBUTING.md).

## Acknowledgements

- **[sonora](https://github.com/sonorahq/sonora)** — the services, the
  engine, the lyrics and the scrobblers. This is a terminal front for its
  `music` crate.
- **[ratatui](https://github.com/ratatui/ratatui)** and
  **[ratatui-image](https://github.com/ratatui/ratatui-image)** — the terminal
  UI and the cover art.
- **[spotify-player](https://github.com/aome510/spotify-player)** — how a
  network-backed ratatui app is put together.
- **[rmpc](https://github.com/mierak/rmpc)** — the single-flight image-encoding
  pattern.
- **[python-tidal](https://github.com/EbbLabs/python-tidal)**,
  **[tidlers](https://codeberg.org/tomkoid/tidlers)** and
  **[tiddl](https://github.com/oskvr37/tiddl)** — references for TIDAL's
  endpoints, the DASH manifest shape, and the device flow.

## Licence

GPL-3.0-or-later, like sonora whose `music` crate it links — see
[LICENSE](LICENSE). The crate is a git dependency pinned to a commit:
sonora does not publish it, which is also why this build is not on
crates.io.
