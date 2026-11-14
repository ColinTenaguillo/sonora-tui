//! A signed-in service, through its `MusicApi`.
//!
//! sonora's models come back as this crate's: a `GenreItem` is a card, a
//! `HomeFeed` a page of rows, an `Artist` the artist page. What a provider
//! does not have — the feed, the mixes, the rest of a cut row — comes back
//! empty rather than failing, so the view it fills is simply blank.

use std::sync::Arc;

use music::{GenreItem, GenreSection, MusicApi, ProviderSession, ReleaseType};

use super::Error;
use crate::browse::{Home, HomeRow, RowKind};
use crate::catalogue::{Extras, Favourable, Provider, Result};
use crate::domain::{Card, Target};
use crate::domain::{Track, TrackId};
use crate::library::{Album, Artist, ArtistPage, FavouriteIds, PageKind, Playlist};
use crate::search::Results;

pub struct Session {
    name: &'static str,
    provider: Arc<dyn Provider>,
    api: Arc<dyn MusicApi>,
    /// What the service has beyond `api`.
    extras: Arc<dyn Extras>,
    playback: Arc<dyn music::PlaybackFactory>,
    /// Whether the library is what the user starred, or everything the
    /// service has -- a folder of files -- with the stars drawn on top.
    shape: music::Shape,
}

/// The shell's state derives `Debug`; a session prints as its service.
impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name)
    }
}

impl Session {
    /// What `provider` signed in to.
    pub fn open(provider: Arc<dyn Provider>, session: ProviderSession) -> Self {
        Self {
            name: provider.label(),
            extras: super::extras_for(provider.slug(), &session.api),
            api: session.api,
            playback: session.playback,
            shape: session.shape,
            provider,
        }
    }

    /// Whether the library is the whole catalogue rather than the stars.
    fn whole(&self) -> bool {
        self.shape.is_catalog()
    }

    /// A session on a service with nothing behind it, for tests and
    /// examples: every request fails, and nothing touches the network.
    #[doc(hidden)]
    pub fn sample() -> Arc<Self> {
        use super::sample::Sample;
        Arc::new(Self::open(Arc::new(Sample), Sample::session()))
    }

    /// What the service is called, for the log and the status line.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// The service's short name, which a lyrics provider that is the
    /// service itself keys its lookups by.
    pub fn slug(&self) -> &'static str {
        self.provider.slug()
    }

    /// The engine that plays this session's tracks.
    pub fn playback(&self) -> Arc<dyn music::PlaybackFactory> {
        self.playback.clone()
    }

    /// The tracks a station built on `seed` plays, for the radio key and
    /// autoplay. Empty where the service has no stations.
    pub async fn track_radio(&self, seed: &TrackId) -> Result<Vec<Track>> {
        let (tracks, _) = self.api.track_radio(&seed.0, None).await.map_err(error)?;
        Ok(tracks.into_iter().map(track).collect())
    }

    /// The radio `R` opens on `seed`: its tracks and, where the service
    /// keeps the station as a playlist, that playlist. Named, it is listed
    /// as the playlist is, so what `F` saves is what is on screen; a name
    /// that fails, or names nothing playable, leaves the plain station.
    pub async fn radio(&self, seed: &TrackId) -> Result<(Vec<Track>, Option<String>)> {
        match self.named_radio(seed).await {
            Ok(Some((tracks, playlist))) => return Ok((tracks, Some(playlist))),
            Ok(None) => {}
            Err(error) => tracing::warn!("the radio has no playlist to save: {error:#}"),
        }
        Ok((self.track_radio(seed).await?, None))
    }
}

/// sonora reports with `anyhow`; the shell matches on `Error`. The one
/// case it tells apart, a session the service no longer accepts, is what
/// sonora calls a credentials failure.
fn error(error: anyhow::Error) -> Error {
    let refused = error.chain().any(|cause| {
        matches!(
            cause.downcast_ref::<music::SignInFailure>(),
            Some(music::SignInFailure(music::SignInProblem::Credentials))
        )
    });
    match refused {
        true => Error::Unauthorized,
        false => Error::Other(format!("{error:#}")),
    }
}

fn track(t: music::Track) -> Track {
    let artist_id = t.artist_refs.first().and_then(|a| a.id.clone());
    Track {
        id: TrackId(t.id.unwrap_or_default()),
        title: t.name,
        artist: t.artists,
        album: t.album,
        duration: t.duration,
        cover: t.cover,
        tags: t.tags,
        added: t.added_at.map(iso_date),
        explicit: t.explicit,
        ai: false,
        radio: None,
        album_id: t.album_id,
        artist_id,
    }
}

fn album(a: music::Album) -> Album {
    Album {
        id: a.id,
        title: a.name,
        artist: a.artists,
        year: (a.year != 0).then(|| a.year.to_string()),
        cover: a.cover,
        track_count: a.track_count,
        duration: None,
    }
}

fn artist(a: music::SavedArtist) -> Artist {
    Artist {
        id: a.id,
        name: a.name,
        picture: a.cover,
    }
}

fn playlist(p: music::Playlist) -> Playlist {
    Playlist {
        uuid: p.id,
        title: p.name,
        track_count: p.track_count,
        duration: None,
        creator: p.owner,
        cover: p.cover,
        owned: p.owned,
    }
}

/// At most this many artists a search shows, the service's own and the
/// ones behind the tracks and albums it found together.
const ARTISTS_SHOWN: usize = 20;

/// An artist a search may show: one the service's own search named, with
/// its picture, or one read off the tracks and albums it found.
#[derive(Debug, PartialEq, Eq)]
struct ShownArtist {
    id: String,
    name: String,
    /// The service's own picture, for an artist its search named.
    picture: Option<String>,
    /// The cover of the first track or album they were seen on: what the
    /// card shows when the service has no portrait to give for the id.
    cover: Option<String>,
}

/// The artists a service's own search found. A failed artist search is
/// none -- the search still answers with the tracks and albums it found,
/// and the artists behind them stand in -- unless the session was refused,
/// which sends the listener back to sign in as any other search would.
fn found_artists(found: anyhow::Result<Vec<music::SavedArtist>>) -> Result<Vec<Artist>> {
    match found.map_err(error) {
        Ok(found) => Ok(found.into_iter().map(artist).collect()),
        Err(Error::Unauthorized) => Err(Error::Unauthorized),
        Err(Error::Other(why)) => {
            tracing::warn!("the artist search failed: {why}");
            Ok(Vec::new())
        }
    }
}

/// The artists a search shows: the service's own first, as it ranked
/// them, each once, then the ones behind the tracks and albums it found
/// that it did not name. Those keep up to half the row, or a broad query
/// would fill it with name matches and hide them. Matched by id alone: two
/// artists can share a name, and hiding the one behind the tracks found is
/// worse than one artist showing twice under two ids.
fn merge_artists(found: Vec<Artist>, behind: Vec<ShownArtist>) -> Vec<ShownArtist> {
    let covers: std::collections::HashMap<String, Option<String>> = behind
        .iter()
        .map(|b| (b.id.clone(), b.cover.clone()))
        .collect();
    let mut ids = std::collections::HashSet::new();
    let mut named = Vec::new();
    for artist in found {
        if !ids.insert(artist.id.clone()) {
            continue;
        }
        named.push(ShownArtist {
            cover: covers.get(&artist.id).cloned().flatten(),
            id: artist.id,
            name: artist.name,
            picture: artist.picture,
        });
    }
    let behind: Vec<ShownArtist> = behind
        .into_iter()
        .filter(|b| !ids.contains(&b.id))
        .collect();
    let kept_for_behind = behind.len().min(ARTISTS_SHOWN / 2);
    named
        .into_iter()
        .take(ARTISTS_SHOWN - kept_for_behind)
        .chain(behind)
        .take(ARTISTS_SHOWN)
        .collect()
}

/// The artists shown without a picture of their own: the only ones worth
/// asking the service for a portrait.
fn unpictured(shown: &[ShownArtist]) -> Vec<String> {
    shown
        .iter()
        .filter(|s| s.picture.is_none())
        .map(|s| s.id.clone())
        .collect()
}

/// The artists as drawn: the service's picture, else a portrait, else a
/// cover they were seen on.
fn pictured(
    shown: Vec<ShownArtist>,
    portraits: &std::collections::HashMap<String, String>,
) -> Vec<Artist> {
    shown
        .into_iter()
        .map(|b| Artist {
            picture: b
                .picture
                .or_else(|| portraits.get(&b.id).cloned())
                .or(b.cover),
            id: b.id,
            name: b.name,
        })
        .collect()
}

/// The artists a page shows with no picture of their own. TIDAL's home
/// lists the ones played lately that way.
fn faceless(sections: &[GenreSection]) -> Vec<String> {
    let items = sections.iter().flat_map(|section| &section.items);
    let mut ids: Vec<String> = items
        .filter_map(|item| match item {
            GenreItem::Artist(artist) if artist.cover.is_none() => Some(artist.id.clone()),
            _ => None,
        })
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

/// The page with each of those given the portrait found for it.
fn portrayed(
    mut sections: Vec<GenreSection>,
    portraits: &std::collections::HashMap<String, String>,
) -> Vec<GenreSection> {
    for item in sections.iter_mut().flat_map(|section| &mut section.items) {
        if let GenreItem::Artist(artist) = item {
            artist.cover = artist
                .cover
                .take()
                .or_else(|| portraits.get(&artist.id).cloned());
        }
    }
    sections
}

/// The artists behind what a search found: each once, the most frequent
/// first. A video channel credited in an artist's place is not one.
fn artists_behind(tracks: &[music::Track], albums: &[music::Album]) -> Vec<ShownArtist> {
    let mut seen: Vec<(ShownArtist, usize)> = Vec::new();
    let refs = tracks
        .iter()
        .flat_map(|t| t.artist_refs.iter().map(move |a| (a, &t.cover)))
        .chain(
            albums
                .iter()
                .flat_map(|al| al.artist_refs.iter().map(move |a| (a, &al.cover))),
        );
    for (artist, cover) in refs.filter(|(artist, _)| !artist.channel) {
        let Some(id) = &artist.id else { continue };
        match seen.iter_mut().find(|(b, _)| &b.id == id) {
            Some(entry) => {
                entry.1 += 1;
                if entry.0.cover.is_none() {
                    entry.0.cover = cover.clone();
                }
            }
            None => seen.push((
                ShownArtist {
                    id: id.clone(),
                    name: artist.name.clone(),
                    picture: None,
                    cover: cover.clone(),
                },
                1,
            )),
        }
    }
    seen.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    seen.truncate(ARTISTS_SHOWN);
    seen.into_iter().map(|(b, _)| b).collect()
}

/// Whether `again` is the start of `picks`, track for track.
fn same_tracks(again: &[GenreItem], picks: &[GenreItem]) -> bool {
    let id = |item: &GenreItem| match item {
        GenreItem::Track(t) => t.id.clone(),
        _ => None,
    };
    !again.is_empty()
        && again.len() <= picks.len()
        && again
            .iter()
            .zip(picks)
            .all(|(a, p)| id(a).is_some() && id(a) == id(p))
}

/// A shelf's title as text. The local provider names its shelves by the
/// translation key sonora's own screens look up, and TIDAL the shelves of
/// a genre; this is the English of each, and any other title is already
/// text.
fn heading(title: String) -> String {
    match title.as_str() {
        "home-recently-added" => "Recently added".into(),
        "home-playlists" => "Playlists".into(),
        "home-favorite-albums" => "Favorite albums".into(),
        "home-artists" => "Artists".into(),
        "home-collection-albums" => "Albums from your collection".into(),
        "home-recently-played" => "Recently played".into(),
        "nav-playlists" => "Playlists".into(),
        "nav-albums" => "Albums".into(),
        "nav-songs" => "Songs".into(),
        _ => title,
    }
}

/// A shelf item as a card: what the home page and a genre page draw.
fn card(item: GenreItem) -> Card {
    match item {
        GenreItem::Track(t) => Card {
            title: t.name,
            subtitle: t.artists,
            cover_url: t.cover,
            target: t.id.map(Target::Track),
            duration: t.duration,
            ..Card::default()
        },
        GenreItem::Album(a) => Card {
            // Before the fields that move out of `a`.
            detail: released(&a),
            title: a.name,
            subtitle: a.artists,
            cover_url: a.cover,
            target: Some(Target::Album(a.id)),
            ..Card::default()
        },
        GenreItem::Artist(a) => Card {
            title: a.name,
            cover_url: a.cover,
            round: true,
            target: Some(Target::Artist(a.id)),
            ..Card::default()
        },
        GenreItem::Playlist(p) => Card {
            title: p.name,
            subtitle: p.owner,
            detail: tracks_held(p.track_count),
            cover_url: p.cover,
            target: Some(Target::Playlist(p.id)),
            ..Card::default()
        },
        GenreItem::Genre(g) => Card {
            title: g.name,
            cover_url: g.cover,
            target: Some(Target::Page(g.id)),
            ..Card::default()
        },
    }
}

/// The line under an album card: its year, or with none, as on YouTube's
/// home, the kind of release, as sonora shows.
fn released(album: &music::Album) -> String {
    match album.year {
        0 => album.release_type.label().to_owned(),
        year => year.to_string(),
    }
}

/// The line under a playlist card. Zero is also a count the provider does
/// not report, as for a mix, so it says nothing rather than "0 tracks".
fn tracks_held(count: u32) -> String {
    match count {
        0 => String::new(),
        n => format!("{n} tracks"),
    }
}

fn rows(sections: Vec<GenreSection>) -> Home {
    Home {
        rows: sections
            .into_iter()
            .map(|section| {
                let cards: Vec<Card> = section.items.into_iter().map(card).collect();
                // A row of links is the Explore shape; anything with
                // artwork is a strip of covers.
                let kind = match cards
                    .iter()
                    .all(|c| matches!(c.target, Some(Target::Page(_))))
                {
                    true if !cards.is_empty() => RowKind::Links,
                    _ => RowKind::Carousel,
                };
                HomeRow {
                    heading: heading(section.title),
                    kind,
                    cards,
                    more: None,
                }
            })
            .filter(|row| !row.cards.is_empty())
            .collect(),
    }
}

/// How many of an artist's top tracks the page shows. A service answers
/// with up to a hundred; the web clients show a handful, and a page of
/// them is a list, not a page.
const TOP_TRACKS: usize = 10;

/// The page an artist or an album is shown on, from what the service said.
fn page(id: &str, a: music::Artist) -> ArtistPage {
    let (albums, singles): (Vec<_>, Vec<_>) = a
        .albums
        .into_iter()
        .partition(|a| !matches!(a.release_type, ReleaseType::Single | ReleaseType::Ep));
    ArtistPage {
        kind: PageKind::Artist,
        id: id.to_owned(),
        name: a.name,
        fans: a.monthly_listeners,
        picture: a.cover_large,
        bio: a.biography,
        top_tracks: a
            .top_tracks
            .into_iter()
            .take(TOP_TRACKS)
            .map(track)
            .collect(),
        albums: albums.into_iter().map(album).collect(),
        singles: singles.into_iter().map(album).collect(),
        ..ArtistPage::default()
    }
}

/// `YYYY-MM-DD` from unix seconds: what the ADDED column reads, as TIDAL's
/// ISO timestamps gave it. Days to a civil date, after Howard Hinnant.
fn iso_date(epoch: i64) -> String {
    let days = epoch.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

impl Session {
    /// The edits the service allows on the account's own playlists. The
    /// id of the new one comes back, so a track can go straight into it.
    pub async fn create_playlist(&self, name: &str) -> Result<String> {
        self.api.create_playlist(name).await.map_err(error)
    }

    pub async fn rename_playlist(&self, uuid: &str, name: &str) -> Result<()> {
        self.api.rename_playlist(uuid, name).await.map_err(error)
    }

    pub async fn delete_playlist(&self, uuid: &str) -> Result<()> {
        self.api.delete_playlist(uuid).await.map_err(error)
    }

    pub async fn add_track_to_playlist(&self, uuid: &str, id: &TrackId) -> Result<()> {
        self.api
            .add_track_to_playlist(uuid, &id.0)
            .await
            .map_err(error)
    }

    pub async fn remove_track_from_playlist(&self, uuid: &str, id: &TrackId) -> Result<()> {
        self.api
            .remove_track_from_playlist(uuid, &id.0)
            .await
            .map_err(error)
    }

    pub async fn playlists(&self) -> Result<Vec<Playlist>> {
        let lists = self.api.playlists().await.map_err(error)?;
        Ok(lists.into_iter().map(playlist).collect())
    }

    pub async fn albums(&self) -> Result<Vec<Album>> {
        let albums = match self.whole() {
            true => self.api.all_albums().await,
            false => self.api.saved_albums().await,
        }
        .map_err(error)?;
        Ok(albums.into_iter().map(album).collect())
    }

    pub async fn artists(&self) -> Result<Vec<Artist>> {
        let artists = match self.whole() {
            true => self.api.all_artists().await,
            false => self.api.saved_artists().await,
        }
        .map_err(error)?;
        Ok(artists.into_iter().map(artist).collect())
    }

    /// The Tracks section: the stars, or every track where the library is
    /// a folder of files.
    pub async fn favourite_tracks(&self) -> Result<Vec<Track>> {
        let tracks = match self.whole() {
            true => self.api.all_tracks().await,
            false => self.api.saved_tracks().await,
        }
        .map_err(error)?;
        Ok(tracks.into_iter().map(track).collect())
    }

    /// Read off the saved listings: sonora has no list of ids on its own.
    /// Every playlist listed is in the library, the saved mixes among them.
    pub async fn favourite_ids(&self) -> Result<FavouriteIds> {
        let (albums, artists, playlists) = tokio::join!(
            self.api.saved_albums(),
            self.api.saved_artists(),
            self.api.playlists()
        );
        Ok(FavouriteIds {
            albums: albums.map_err(error)?.into_iter().map(|a| a.id).collect(),
            artists: artists.map_err(error)?.into_iter().map(|a| a.id).collect(),
            playlists: playlists
                .map_err(error)?
                .into_iter()
                .map(|p| p.id)
                .collect(),
        })
    }

    pub async fn set_track_favourite(&self, id: &TrackId, on: bool) -> Result<()> {
        self.api.set_track_saved(&id.0, on).await.map_err(error)
    }

    pub async fn set_favourite(&self, what: &Favourable, on: bool) -> Result<()> {
        let api = &self.api;
        match what {
            Favourable::Artist(id) => api.set_artist_saved(id, on).await,
            Favourable::Album(id) => api.set_album_saved(id, on).await,
            Favourable::Playlist(id) => match on {
                true => api.add_playlist_to_library(id).await,
                false => api.remove_playlist_from_library(id).await,
            },
        }
        .map_err(error)
    }

    pub async fn playlist_tracks(&self, uuid: &str) -> Result<Vec<Track>> {
        let tracks = self.api.playlist_tracks(uuid).await.map_err(error)?;
        Ok(tracks.into_iter().map(track).collect())
    }

    /// The album's tracks, and under them the rail the service draws:
    /// related records and artists. The rail is extra, so a service that
    /// cannot answer for it leaves it empty rather than failing the page.
    pub async fn album_page(&self, id: &str, title: &str) -> Result<ArtistPage> {
        let (detail, rail) = tokio::join!(self.api.album(id), self.api.album_catalogue(id, None));
        let detail = detail.map_err(error)?;
        let rail = rail.unwrap_or_default();
        Ok(ArtistPage {
            kind: PageKind::Album,
            id: id.to_owned(),
            // Named as the card was: the view waiting for this reply is
            // headed by the card's title.
            name: title.to_owned(),
            year: (detail.album.year != 0).then(|| detail.album.year.to_string()),
            picture: detail.album.cover_large.or(detail.album.cover),
            top_tracks: detail.tracks.into_iter().map(track).collect(),
            appears_on: rail.also_like.into_iter().map(album).collect(),
            similar: rail.similar.into_iter().map(artist).collect(),
            ..ArtistPage::default()
        })
    }

    /// The artist's page, with what the service's catalogue adds to it:
    /// the top tracks where the page carried fewer than are shown, and
    /// what the artist appears on.
    pub async fn artist_page(&self, id: &str) -> Result<ArtistPage> {
        let (artist, more, fans) = tokio::join!(
            self.api.artist(id),
            self.api.artist_catalogue(id, &[]),
            self.extras.artist_fans(id)
        );
        let mut page = page(id, artist.map_err(error)?);
        // Where sonora counted none: the page is drawn without the count
        // rather than not at all.
        match fans {
            Ok(fans) => page.fans = page.fans.or(fans),
            Err(error) => tracing::warn!("no count of the artist's followers: {error:#}"),
        }
        if let Ok(more) = more {
            if !more.top_tracks.is_empty() {
                page.top_tracks = more
                    .top_tracks
                    .into_iter()
                    .take(TOP_TRACKS)
                    .map(track)
                    .collect();
            }
            page.appears_on = more.appears_on.into_iter().map(album).collect();
        }
        if page.picture.is_none() {
            page.picture = self.picture_of(id).await;
        }
        Ok(page)
    }

    /// What the service draws an artist with when sonora found no portrait
    /// of them, where it has a word on that. The page is drawn without
    /// one rather than not at all.
    async fn picture_of(&self, artist_id: &str) -> Option<String> {
        let picture = self.extras.artist_picture(artist_id).await;
        picture
            .inspect_err(|error| tracing::warn!("no picture for the artist: {error:#}"))
            .ok()?
    }

    pub async fn track(&self, id: &TrackId) -> Result<Track> {
        Ok(track(self.api.track(&id.0).await.map_err(error)?))
    }

    /// Portraits for the artists shown without a picture: no request at all
    /// when none lacks one.
    async fn portraits(&self, wanted: Vec<String>) -> std::collections::HashMap<String, String> {
        if wanted.is_empty() {
            return Default::default();
        }
        self.api.artist_images(wanted).await.unwrap_or_default()
    }

    /// Tracks, albums, playlists, and artists: the ones the service found by
    /// name first, then the ones behind the tracks and albums it found.
    pub async fn search(&self, query: &str) -> Result<Results> {
        let (tracks, albums, playlists, artists) = tokio::join!(
            self.api.search(query),
            self.api.search_albums(query),
            self.api.search_playlists(query),
            self.api.search_artists(query)
        );
        let (tracks, albums) = (tracks.map_err(error)?, albums.map_err(error)?);
        let playlists = playlists.map_err(error)?;
        let shown = merge_artists(found_artists(artists)?, artists_behind(&tracks, &albums));
        let portraits = self.portraits(unpictured(&shown)).await;
        Ok(Results {
            query: query.to_owned(),
            tracks: tracks.into_iter().map(track).collect(),
            albums: albums.into_iter().map(album).collect(),
            artists: pictured(shown, &portraits),
            playlists: playlists.into_iter().map(playlist).collect(),
        })
    }

    /// The home page: the service's feed, with what it played lately and
    /// its picks ahead of it where the service has them.
    pub async fn home(&self) -> Result<Home> {
        let feed = self.api.home().await.map_err(error)?;
        let mut sections = feed.sections;
        if let Some(picks) = feed.quick_picks.filter(|p| !p.is_empty()) {
            sections.insert(
                0,
                GenreSection {
                    title: "Quick picks".into(),
                    items: picks.into_iter().map(GenreItem::Track).collect(),
                },
            );
        }
        // Subsonic and a folder of files have no history to draw on, and
        // sonora fills both shelves from the same random draw: one row of
        // it is enough.
        let repeats = match sections.first() {
            Some(picks) if picks.title == "Quick picks" => {
                same_tracks(&feed.listen_again, &picks.items)
            }
            _ => false,
        };
        if !feed.listen_again.is_empty() && !repeats {
            sections.insert(
                0,
                GenreSection {
                    title: "Listen again".into(),
                    items: feed.listen_again,
                },
            );
        }
        Ok(rows(self.with_portraits(sections).await))
    }

    /// The station as the playlist the service keeps it as, when it keeps
    /// one and it has tracks.
    async fn named_radio(&self, seed: &TrackId) -> anyhow::Result<Option<(Vec<Track>, String)>> {
        let Some(playlist) = self.extras.radio_playlist(&seed.0).await? else {
            return Ok(None);
        };
        let tracks = self.api.playlist_tracks(&playlist).await?;
        let tracks: Vec<Track> = tracks.into_iter().map(track).collect();
        Ok((!tracks.is_empty()).then_some((tracks, playlist)))
    }

    /// The page with a portrait for each artist it shows without one,
    /// where the service has it. Asked for only when one is missing, and
    /// a page is drawn without them rather than not at all.
    async fn with_portraits(&self, sections: Vec<GenreSection>) -> Vec<GenreSection> {
        let ids = faceless(&sections);
        if ids.is_empty() {
            return sections;
        }
        // The service's own word first, where it has one: it knows what
        // stands in for an artist it keeps no portrait of, and sonora's
        // lookup is then only for the artists it left out.
        let asked = ids
            .iter()
            .map(|id| async move { Some((id.clone(), self.picture_of(id).await?)) });
        let mut portraits: std::collections::HashMap<String, String> =
            futures::future::join_all(asked)
                .await
                .into_iter()
                .flatten()
                .collect();
        let left: Vec<String> = ids
            .into_iter()
            .filter(|id| !portraits.contains_key(id))
            .collect();
        if !left.is_empty() {
            let found = self.api.artist_images(left).await;
            let found = found.inspect_err(|error| tracing::warn!("no portraits: {error:#}"));
            portraits.extend(found.unwrap_or_default());
        }
        portrayed(sections, &portraits)
    }

    /// The page a card of Explore or of such a page opens: the service's
    /// own when the path is one of its, else sonora's page of the genre.
    pub async fn page_of_rows(&self, path: &str) -> Result<Home> {
        if let Some(page) = self.extras.page(path).await.map_err(error)? {
            return Ok(page);
        }
        let genre = self.api.genre(path).await.map_err(error)?;
        Ok(rows(self.with_portraits(genre.sections).await))
    }

    /// Explore: the service's own page where it has one, else sonora's
    /// genres. A page that fails is not the end of Explore: the genres
    /// are still there to show.
    pub async fn explore(&self) -> Result<Home> {
        match self.extras.explore().await {
            Ok(Some(page)) => return Ok(page),
            Ok(None) => {}
            Err(error) => tracing::warn!("no Explore page of the service's own: {error:#}"),
        }
        let genres = self.api.genres().await.map_err(error)?;
        Ok(rows(vec![GenreSection {
            title: "Genres".into(),
            items: genres.into_iter().map(GenreItem::Genre).collect(),
        }]))
    }
}

#[cfg(test)]
mod tests {
    /// A service whose own pages are these, or that fails to give them.
    struct Pages(Option<&'static str>);

    #[async_trait::async_trait]
    impl crate::catalogue::Extras for Pages {
        async fn explore(&self) -> anyhow::Result<Option<crate::browse::Home>> {
            match self.0 {
                Some(heading) => Ok(Some(one_row(heading))),
                None => anyhow::bail!("down"),
            }
        }
        async fn page(&self, path: &str) -> anyhow::Result<Option<crate::browse::Home>> {
            Ok(path.starts_with("pages/").then(|| one_row("Its own page")))
        }
        async fn artist_picture(&self, id: &str) -> anyhow::Result<Option<String>> {
            Ok((id == "a").then(|| "https://x/a.jpg".into()))
        }
        async fn radio_playlist(&self, _: &str) -> anyhow::Result<Option<String>> {
            Ok(Some("mix:1".into()))
        }
    }

    fn one_row(heading: &str) -> crate::browse::Home {
        crate::browse::Home {
            rows: vec![crate::browse::HomeRow {
                heading: heading.into(),
                kind: crate::browse::RowKind::Links,
                cards: vec![crate::domain::Card::default()],
                more: None,
            }],
        }
    }

    fn session_with(extras: Pages) -> super::Session {
        use crate::catalogue::sample::Sample;
        let signed_in = Sample::session();
        super::Session {
            name: "Sample",
            provider: std::sync::Arc::new(Sample),
            api: signed_in.api,
            extras: std::sync::Arc::new(extras),
            playback: signed_in.playback,
            shape: signed_in.shape,
        }
    }

    fn wait<T>(what: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("a runtime")
            .block_on(what)
    }

    #[test]
    fn an_artist_shown_without_a_picture_is_given_the_portrait_found() {
        let artist = |id: &str, cover: Option<&str>| {
            super::GenreItem::Artist(music::SavedArtist {
                id: id.into(),
                name: id.to_uppercase(),
                cover: cover.map(Into::into),
                added_at: None,
            })
        };
        let page = vec![super::GenreSection {
            title: "Recently played".into(),
            items: vec![
                artist("a", None),
                artist("b", Some("https://x/own.jpg")),
                artist("a", None),
                super::GenreItem::Track(music::Track::default()),
            ],
        }];
        assert_eq!(super::faceless(&page), ["a"], "each once, and only those");

        let found = [("a".to_owned(), "https://x/a.jpg".to_owned())].into();
        let page = super::portrayed(page, &found);
        let covers: Vec<_> = page[0]
            .items
            .iter()
            .filter_map(|item| match item {
                super::GenreItem::Artist(artist) => Some(artist.cover.as_deref()),
                _ => None,
            })
            .collect();
        assert_eq!(
            covers,
            [
                Some("https://x/a.jpg"),
                Some("https://x/own.jpg"),
                Some("https://x/a.jpg")
            ],
            "its own picture is kept"
        );
    }

    #[test]
    fn a_page_is_drawn_with_the_pictures_found_and_without_the_others() {
        // "a" from the service's own word; "b" is left to sonora, which for
        // the sample service fails: the page comes back all the same.
        let artist = |id: &str| {
            super::GenreItem::Artist(music::SavedArtist {
                id: id.into(),
                ..Default::default()
            })
        };
        let page = vec![super::GenreSection {
            title: "Recently played".into(),
            items: vec![artist("a"), artist("b")],
        }];
        let page = wait(session_with(Pages(None)).with_portraits(page));
        let covers: Vec<_> = page[0]
            .items
            .iter()
            .map(|item| match item {
                super::GenreItem::Artist(artist) => artist.cover.as_deref(),
                _ => None,
            })
            .collect();
        assert_eq!(covers, [Some("https://x/a.jpg"), None]);
    }

    #[test]
    fn an_artist_sonora_has_no_portrait_of_is_asked_of_the_service() {
        let session = session_with(Pages(None));
        assert_eq!(
            wait(session.picture_of("a")).as_deref(),
            Some("https://x/a.jpg")
        );
        assert_eq!(
            wait(session.picture_of("b")),
            None,
            "the service has none either"
        );
        let plain = super::Session::sample();
        assert_eq!(
            wait(plain.picture_of("a")),
            None,
            "a service with no word on it"
        );
    }

    #[test]
    fn explore_is_the_services_own_page_where_it_has_one() {
        let explore = wait(session_with(Pages(Some("Moods"))).explore()).expect("a page");
        assert_eq!(explore.rows[0].heading, "Moods");
    }

    #[test]
    fn an_explore_page_that_fails_leaves_sonoras_genres() {
        // Not an error: sonora is asked instead, and for the sample service
        // that is a page with nothing on it.
        let explore = wait(session_with(Pages(None)).explore()).expect("sonora's answer");
        assert!(explore.rows.is_empty());
    }

    #[test]
    fn a_page_of_the_services_own_is_asked_of_it_and_a_genre_of_sonora() {
        let session = session_with(Pages(Some("Moods")));
        let page = wait(session.page_of_rows("pages/m_party")).expect("its page");
        assert_eq!(page.rows[0].heading, "Its own page");
        // The sample service has no page for any genre: this one was
        // sonora's to answer.
        let genre = wait(session.page_of_rows("genre_pop")).expect("sonora's");
        assert!(genre.rows.is_empty());
    }

    #[test]
    fn a_radio_whose_playlist_cannot_be_listed_is_the_plain_station() {
        // The sample service lists no playlist and plays no station: the
        // name is dropped with the list it could not give, and what comes
        // back is the station's own answer, here its failure.
        let seed = crate::domain::TrackId::from(1);
        let named = wait(session_with(Pages(None)).radio(&seed));
        assert!(matches!(named, Err(super::Error::Other(_))), "{named:?}");
    }

    #[test]
    fn a_listen_again_row_that_repeats_the_quick_picks_is_recognised() {
        let track = |id: &str| {
            super::GenreItem::Track(music::Track {
                id: Some(id.into()),
                ..Default::default()
            })
        };
        let picks = vec![track("a"), track("b"), track("c")];
        assert!(super::same_tracks(&[track("a"), track("b")], &picks));
        assert!(!super::same_tracks(&[track("b"), track("a")], &picks));
        assert!(
            !super::same_tracks(&[], &picks),
            "an empty row repeats nothing"
        );
    }

    #[test]
    fn the_services_artists_lead_and_the_ones_behind_follow() {
        // What the service found by name comes first, as it ranked them;
        // the artists behind the tracks and albums follow, once each, so a
        // narrow match by name does not hide them. Pictures come from the
        // service, then a portrait, then a cover they were seen on --
        // asked for only for the artists shown without one, once each.
        let named = |id: &str, picture: Option<&str>| Artist {
            id: id.into(),
            name: id.to_uppercase(),
            picture: picture.map(Into::into),
        };
        let seen = |id: &str, name: &str, cover: Option<&str>| super::ShownArtist {
            id: id.into(),
            name: name.into(),
            picture: None,
            cover: cover.map(Into::into),
        };
        // A service may name one artist twice -- YouTube's top result and
        // its artists shelf -- and it is shown once.
        let found = vec![
            named("a", Some("https://x/a")),
            named("b", None),
            named("a", Some("https://x/a")),
            named("e", None),
        ];
        let behind = vec![
            seen("a", "A", None),
            // Another artist who shares a name with one the service found
            // is still the one behind those tracks: both are shown.
            seen("b2", "b", None),
            seen("e", "E", Some("https://cover/e")),
            seen("c", "C", Some("https://cover/c")),
            seen("d", "D", None),
        ];
        let shown = super::merge_artists(found, behind);
        let ids: Vec<&str> = shown.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(
            ids,
            ["a", "b", "e", "b2", "c", "d"],
            "found first, each once"
        );
        assert_eq!(super::unpictured(&shown), ["b", "e", "b2", "c", "d"]);

        let portraits: std::collections::HashMap<String, String> = [
            ("b".to_string(), "https://face/b".to_string()),
            ("d".to_string(), "https://face/d".to_string()),
        ]
        .into();
        let artists = super::pictured(shown, &portraits);
        let pictures: Vec<Option<&str>> = artists.iter().map(|a| a.picture.as_deref()).collect();
        assert_eq!(
            pictures,
            [
                Some("https://x/a"),
                Some("https://face/b"),
                Some("https://cover/e"),
                None,
                Some("https://cover/c"),
                Some("https://face/d"),
            ],
            "the service's, a portrait, else a cover -- for a named one too"
        );

        // A broad query fills the row with name matches; the artists behind
        // the results keep up to half of it.
        let many: Vec<Artist> = (0..25)
            .map(|i| named(&format!("n{i}"), Some("p")))
            .collect();
        let five: Vec<super::ShownArtist> = (0..5)
            .map(|i| seen(&format!("b{i}"), &format!("ShownArtist {i}"), None))
            .collect();
        let shown = super::merge_artists(many.clone(), five);
        assert_eq!(shown.len(), super::ARTISTS_SHOWN);
        assert!(
            shown[15..].iter().all(|s| s.id.starts_with('b')),
            "five behind, last"
        );
        assert_eq!(
            super::merge_artists(many, Vec::new()).len(),
            super::ARTISTS_SHOWN
        );
        assert!(super::unpictured(&super::merge_artists(
            vec![named("p", Some("p"))],
            Vec::new()
        ))
        .is_empty());
    }

    #[test]
    fn a_failed_artist_search_is_none_but_a_refused_session_is_still_refused() {
        assert!(matches!(
            super::found_artists(Err(anyhow::anyhow!("rate limited"))),
            Ok(found) if found.is_empty()
        ));
        let refused = anyhow::Error::from(music::SignInFailure(music::SignInProblem::Credentials));
        assert!(matches!(
            super::found_artists(Err(refused)),
            Err(super::Error::Unauthorized)
        ));
        let one = music::SavedArtist {
            id: "a".into(),
            name: "A".into(),
            cover: Some("https://x/a".into()),
            added_at: None,
        };
        let kept = super::found_artists(Ok(vec![one])).unwrap();
        assert_eq!(kept[0].picture.as_deref(), Some("https://x/a"));
    }

    #[test]
    fn the_artists_behind_a_search_come_once_each_most_frequent_first() {
        let track = |artists: &[(&str, Option<&str>)]| {
            let mut t = music::Track {
                id: Some("t".into()),
                ..Default::default()
            };
            t.artist_refs = artists
                .iter()
                .map(|(name, id)| music::ArtistRef {
                    name: (*name).into(),
                    id: id.map(Into::into),
                    ..Default::default()
                })
                .collect();
            t
        };
        let mut tracks = vec![
            track(&[("Jul", Some("j"))]),
            track(&[("SCH", Some("s")), ("Jul", Some("j"))]),
            track(&[("Unknown", None)]),
        ];
        tracks[1].cover = Some("https://x/sch.jpg".into());
        // A fan upload on YouTube credits a plain video channel, which has
        // no artist page to open.
        let mut upload = track(&[("Fan uploads", Some("c"))]);
        upload.artist_refs[0].channel = true;
        tracks.push(upload);
        let found = super::artists_behind(&tracks, &[]);
        let names: Vec<(&str, &str)> = found
            .iter()
            .map(|b| (b.id.as_str(), b.name.as_str()))
            .collect();
        assert_eq!(names, [("j", "Jul"), ("s", "SCH")]);
        // The first cover seen stands in for a portrait: Jul's first track
        // had none, the second had one.
        assert_eq!(found[0].cover.as_deref(), Some("https://x/sch.jpg"));
    }

    #[test]
    fn a_shelf_named_by_a_translation_key_reads_as_english() {
        assert_eq!(
            super::heading("home-recently-added".into()),
            "Recently added"
        );
        assert_eq!(super::heading("nav-songs".into()), "Songs");
        assert_eq!(super::heading("Quick picks".into()), "Quick picks");
        // Every key sonora's providers title a shelf with: a new one shows
        // on screen as the key itself until it is given its English here.
        for key in [
            "home-recently-added",
            "home-recently-played",
            "home-playlists",
            "home-favorite-albums",
            "home-artists",
            "home-collection-albums",
            "nav-playlists",
            "nav-albums",
            "nav-songs",
        ] {
            assert_ne!(super::heading(key.into()), key, "{key} has no English");
        }
    }

    use std::time::Duration;

    use super::*;

    #[test]
    fn unix_seconds_become_the_day_the_column_shows() {
        assert_eq!(iso_date(0), "1970-01-01");
        assert_eq!(iso_date(1_616_247_907), "2021-03-20");
        assert_eq!(iso_date(951_782_400), "2000-02-29", "a leap day");
        assert_eq!(iso_date(-86_400), "1969-12-31", "before the epoch");
    }

    fn a_track(id: &str) -> music::Track {
        music::Track {
            id: Some(id.into()),
            name: "So What".into(),
            playable: true,
            artists: "Miles Davis".into(),
            artist_refs: vec![music::ArtistRef {
                name: "Miles Davis".into(),
                id: Some("ar-1".into()),
                ..Default::default()
            }],
            album: "Kind of Blue".into(),
            album_id: Some("al-1".into()),
            cover: Some("https://x/c.jpg".into()),
            duration: Duration::from_secs(562),
            added_at: Some(1_616_247_907),
            added_by: None,
            playcount: None,
            popularity: 0,
            explicit: false,
            track_number: 1,
            disc_number: 1,
            tags: Vec::new(),
            languages: Vec::new(),
            credits: Vec::new(),
            ..Default::default()
        }
    }

    #[test]
    fn a_track_keeps_what_the_rows_show_and_the_keys_open() {
        let t = track(a_track("tr-9"));
        assert_eq!(t.id, TrackId::from("tr-9"));
        assert_eq!(t.artist_id.as_deref(), Some("ar-1"), "a opens the artist");
        assert_eq!(t.album_id.as_deref(), Some("al-1"), "c opens the album");
        assert_eq!(t.added.as_deref(), Some("2021-03-20"));
    }

    #[test]
    fn shelf_items_become_cards_that_open_something() {
        let t = card(GenreItem::Track(a_track("tr-9")));
        assert_eq!(t.target, Some(Target::Track("tr-9".into())));
        assert_eq!(t.duration, Duration::from_secs(562), "the bar needs it");

        let g = card(GenreItem::Genre(music::Genre {
            id: "rock".into(),
            name: "Rock".into(),
            cover: None,
        }));
        assert_eq!(g.target, Some(Target::Page("rock".into())));
    }

    #[test]
    fn an_artist_card_is_round_and_a_playlist_card_counts_its_tracks() {
        let a = card(GenreItem::Artist(music::SavedArtist {
            id: "ar-1".into(),
            name: "Daft Punk".into(),
            cover: None,
            added_at: None,
        }));
        assert!(a.round, "an artist is a portrait");
        assert_eq!(a.target, Some(Target::Artist("ar-1".into())));

        let playlist = |track_count| {
            card(GenreItem::Playlist(music::Playlist {
                id: "pl-1".into(),
                name: "Mix".into(),
                owner: "TIDAL".into(),
                owner_id: String::new(),
                owned: false,
                collaborative: false,
                blend: false,
                public: true,
                cover: None,
                track_count,
                modified_at: None,
            }))
        };
        let p = playlist(12);
        assert_eq!(p.detail, "12 tracks");
        assert_eq!(p.subtitle, "TIDAL");
        assert_eq!(p.target, Some(Target::Playlist("pl-1".into())));
        assert_eq!(playlist(0).detail, "", "no count is no line, as a mix has");
    }

    #[test]
    fn an_album_card_without_a_year_says_what_kind_of_release_it_is() {
        // YouTube's home shelves carry "Single • Artist", never a year.
        let album = |year, release_type| {
            card(GenreItem::Album(music::Album {
                id: "al-1".into(),
                name: "Le Plus Beau".into(),
                artists: "TeddyBear".into(),
                artist_refs: Vec::new(),
                cover: None,
                cover_large: None,
                release_type,
                year,
                track_count: 0,
                release_date: String::new(),
                label: String::new(),
                copyrights: Vec::new(),
                added_at: None,
            }))
        };
        assert_eq!(album(0, ReleaseType::Single).detail, "Single");
        assert_eq!(album(0, ReleaseType::Ep).detail, "EP");
        assert_eq!(
            album(2013, ReleaseType::Single).detail,
            "2013",
            "a year wins"
        );
    }

    #[test]
    fn a_shelf_of_links_is_drawn_as_links_and_an_empty_one_is_dropped() {
        let home = rows(vec![
            GenreSection {
                title: "Genres".into(),
                items: vec![GenreItem::Genre(music::Genre {
                    id: "rock".into(),
                    name: "Rock".into(),
                    cover: None,
                })],
            },
            GenreSection {
                title: "Nothing".into(),
                items: Vec::new(),
            },
        ]);
        assert_eq!(home.rows.len(), 1);
        assert_eq!(home.rows[0].kind, RowKind::Links);
    }

    #[test]
    fn eps_and_singles_go_under_the_albums() {
        let release = |name: &str, release_type| music::Album {
            id: name.into(),
            name: name.into(),
            artists: String::new(),
            artist_refs: Vec::new(),
            cover: None,
            cover_large: None,
            release_type,
            year: 1959,
            track_count: 5,
            release_date: String::new(),
            label: String::new(),
            copyrights: Vec::new(),
            added_at: None,
        };
        let p = page(
            "ar-1",
            music::Artist {
                name: "Miles".into(),
                cover_large: None,
                banner: None,
                biography: None,
                monthly_listeners: Some(7),
                top_tracks: (0..30).map(|i| a_track(&format!("t{i}"))).collect(),
                albums: vec![
                    release("Kind of Blue", ReleaseType::Album),
                    release("Blue Haze", ReleaseType::Ep),
                ],
            },
        );
        assert_eq!(p.id, "ar-1");
        assert_eq!(p.fans, Some(7));
        assert_eq!(p.albums.len(), 1);
        assert_eq!(p.singles[0].title, "Blue Haze");
        assert_eq!(p.albums[0].year.as_deref(), Some("1959"));
        assert_eq!(
            p.top_tracks.len(),
            TOP_TRACKS,
            "a handful, as the web shows"
        );
    }

    #[test]
    fn a_refused_session_is_the_one_error_the_shell_tells_apart() {
        let refused = anyhow::Error::from(music::SignInFailure(music::SignInProblem::Credentials));
        assert!(matches!(error(refused), Error::Unauthorized));
        // Under a context it is the same thing.
        let wrapped = anyhow::Error::from(music::SignInFailure(music::SignInProblem::Credentials))
            .context("loading");
        assert!(matches!(error(wrapped), Error::Unauthorized));
        assert!(matches!(error(anyhow::anyhow!("down")), Error::Other(m) if m == "down"));
    }
}
