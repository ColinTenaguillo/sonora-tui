//! What TIDAL has that sonora's `MusicApi` has no word for yet: its own
//! Explore page, with the moods and decades beside the genres, the pages
//! those open, the mix a track's radio is, and of an artist the picture
//! that stands in for a missing portrait and how many follow them.
//!
//! sonora's TIDAL client is private to its crate, so these few requests
//! are made here, with the session sonora keeps: read from its file each
//! time, never written. Each is asked upstream (sonorahq/sonora#1351 for
//! the pages, #1359 for the radio, #1358 for the picture); as sonora
//! answers one itself its part here goes, and with the last of them this
//! file and its line in `extras_for`.

use std::sync::Arc;

use anyhow::Context as _;
use async_trait::async_trait;
use music::MusicApi;

use super::Extras;
use crate::browse::{Home, HomeRow, RowKind};
use crate::domain::Card;

const API: &str = "https://api.tidal.com";

/// What the web player says it is. TIDAL's v2 pages answer 400 without
/// it, and stop answering a version it has retired.
const CLIENT_VERSION: &str = "2026.9.15";

/// The prefix a mix wears as a playlist id, which is sonora's own: it
/// opens and saves one under that name.
const MIX: &str = "mix:";

pub(super) struct TidalExtras {
    /// sonora's side of the same session, asked to renew it.
    api: Arc<dyn MusicApi>,
    http: reqwest::Client,
}

impl TidalExtras {
    pub(super) fn new(api: Arc<dyn MusicApi>) -> Self {
        // Bounded, so a page that never answers leaves Explore to
        // sonora's genres rather than loading for ever.
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .unwrap_or_default();
        Self { api, http }
    }

    /// One GET, as the account sonora signed in. A refused token is an
    /// expired one: sonora renews its session on its next request and
    /// keeps the result, so it is asked for anything and the file read
    /// again, once.
    async fn get(&self, path: &str, query: &[(&str, &str)]) -> anyhow::Result<String> {
        let answer = self.ask(path, query).await?;
        let answer = match answer.status() {
            reqwest::StatusCode::UNAUTHORIZED => {
                let _ = self.api.profile().await;
                self.ask(path, query).await?
            }
            _ => answer,
        };
        Ok(answer.error_for_status()?.text().await?)
    }

    async fn ask(&self, path: &str, query: &[(&str, &str)]) -> anyhow::Result<reqwest::Response> {
        let held =
            std::fs::read_to_string(super::carried::sonoras_file().context("no home folder")?)
                .context("sonora keeps no TIDAL sign-in")?;
        let (token, country) = session(&held).context("sonora's TIDAL sign-in cannot be read")?;
        let path = path.trim_start_matches('/');
        // A path names its API when it is not the first one's.
        let path = match path.starts_with("v2/") {
            true => path.to_owned(),
            false => format!("v1/{path}"),
        };
        let mut url = reqwest::Url::parse(&format!("{API}/{path}"))?;
        url.query_pairs_mut()
            .append_pair("countryCode", &country)
            .extend_pairs(query);
        let request = self.http.get(url).bearer_auth(token);
        let request = request.header("x-tidal-client-version", CLIENT_VERSION);
        Ok(request.send().await?)
    }
}

/// The token and the country of the session in sonora's file.
fn session(held: &str) -> Option<(String, String)> {
    let held: serde_json::Value = serde_json::from_str(held).ok()?;
    let text = |field: &str| Some(held.get(field)?.as_str()?.to_owned());
    Some((text("access_token")?, text("country_code")?))
}

/// What a page is asked as: the web player's own rows.
const AS_THE_WEB: [(&str, &str); 2] = [("deviceType", "BROWSER"), ("locale", "en_US")];

/// Whether a card's path is one of TIDAL's pages rather than a genre of
/// sonora's.
fn is_page(path: &str) -> bool {
    path.trim_start_matches('/').starts_with("pages/")
}

#[async_trait]
impl Extras for TidalExtras {
    async fn explore(&self) -> anyhow::Result<Option<Home>> {
        let body = self.get("pages/explore", &AS_THE_WEB).await?;
        Ok(Some(page(&body)).filter(|home| !home.rows.is_empty()))
    }

    async fn page(&self, path: &str) -> anyhow::Result<Option<Home>> {
        if !is_page(path) {
            return Ok(None);
        }
        Ok(Some(page(&self.get(path, &AS_THE_WEB).await?)))
    }

    async fn artist_fans(&self, artist_id: &str) -> anyhow::Result<Option<u64>> {
        let query = [AS_THE_WEB[0], AS_THE_WEB[1], ("platform", "WEB")];
        let body = self.get(&format!("v2/artist/{artist_id}"), &query).await?;
        Ok(followers(&body))
    }

    async fn artist_picture(&self, artist_id: &str) -> anyhow::Result<Option<String>> {
        let body = self.get(&format!("artists/{artist_id}"), &[]).await?;
        Ok(artist_picture(&body))
    }

    async fn radio_playlist(&self, track_id: &str) -> anyhow::Result<Option<String>> {
        let body = self.get(&format!("tracks/{track_id}"), &[]).await?;
        Ok(track_mix(&body))
    }
}

/// The mix TIDAL keeps for a track, as the playlist id sonora knows it by.
fn track_mix(track: &str) -> Option<String> {
    let track: serde_json::Value = serde_json::from_str(track).ok()?;
    let mix = track.get("mixes")?.get("TRACK_MIX")?.as_str()?;
    Some(format!("{MIX}{mix}"))
}

/// The followers the web's artist page puts under the name. sonora reads
/// the artist from an endpoint that does not count them.
fn followers(page: &str) -> Option<u64> {
    let page: serde_json::Value = serde_json::from_str(page).ok()?;
    page["header"]["followersAmount"].as_u64()
}

/// An artist's portrait, or the album cover TIDAL names in its place for
/// one with none: sonora reads the first alone, and draws no picture
/// without it.
fn artist_picture(artist: &str) -> Option<String> {
    let artist: serde_json::Value = serde_json::from_str(artist).ok()?;
    let image = |field: &str| artist.get(field)?.as_str().filter(|uuid| !uuid.is_empty());
    let uuid = image("picture").or_else(|| image("selectedAlbumCoverFallback"))?;
    Some(cover_url(uuid, 320))
}

/// Cover art URL. TIDAL stores a dashed uuid but serves it slash-separated.
fn cover_url(uuid: &str, size: u32) -> String {
    let path = uuid.replace('-', "/");
    format!("https://resources.tidal.com/images/{path}/{size}x{size}.jpg")
}

/// Whether a module is a row of videos, which this app cannot play.
///
/// VIDEO_LIST is what a genre page's "New Music Videos" comes back as; the
/// prefix covers the neighbours TIDAL adds without warning, the way the
/// mixes page grew VIDEO_DAILY_MIX.
fn is_video_module(module_type: &str) -> bool {
    module_type.starts_with("VIDEO")
}

/// Whether a row is made of tracks, and so belongs in the grid of track
/// rows rather than in a strip of covers.
///
/// Asked of the cards rather than of the module's name: `MIXED_TYPES_LIST`
/// says only that the row mixes kinds, and Recently played comes back as
/// ten albums, mixes and playlists with no track among them.
fn holds_tracks(cards: &[Card]) -> bool {
    !cards.is_empty()
        && cards
            .iter()
            .all(|c| matches!(c.target, Some(crate::domain::Target::Track(_))))
}

/// Map the page into rows, ignoring anything unfamiliar.
///
/// Separated from the request so it can be tested against a captured
/// response: this is the part that breaks when TIDAL changes the page.
fn page(body: &str) -> Home {
    let page: PageDto = match serde_json::from_str(body) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("a TIDAL page did not parse: {e}");
            return Home::default();
        }
    };

    let mut out = Home::default();
    for module in page.rows.into_iter().flat_map(|r| r.modules) {
        // Videos are dropped wherever they appear. This plays audio, and a
        // genre page carries a whole row of them -- "New Music Videos" on
        // Hip-Hop is fifteen cards that open nothing. Told apart by the
        // module type the API gives them rather than the row's title,
        // which is whatever the locale says.
        if is_video_module(&module.module_type) {
            continue;
        }
        let cards: Vec<Card> = module
            .paged_list
            .items
            .iter()
            .filter_map(|item| item.to_card())
            .collect();

        if cards.is_empty() {
            continue;
        }

        let kind = row_kind(&module.module_type, &cards);
        let heading = match module.module_type.as_str() {
            "HIGHLIGHT_MODULE" | "SHORTCUT_LIST" if module.title.is_empty() => {
                "Shortcuts".to_string()
            }
            _ => module.title,
        };
        out.rows.push(HomeRow {
            heading,
            kind,
            cards,
            more: None,
        });
    }
    out
}

/// TRACK_LIST is the web client's grid of track rows and the
/// shortcut grid its wide one; everything else with items is a
/// strip of covers.
fn row_kind(module_type: &str, cards: &[Card]) -> RowKind {
    match module_type {
        "TRACK_LIST" => RowKind::Compact,
        "HIGHLIGHT_MODULE" | "SHORTCUT_LIST" => RowKind::Shortcuts,
        // Explore's genres, moods and decades: page links with
        // no artwork behind them.
        "PAGE_LINKS_CLOUD" | "PAGE_LINKS" => RowKind::Links,
        // MIXED_TYPES_LIST is what Recently played comes back
        // as, and what is in it decides how it is drawn rather
        // than what the module is called: a real response holds
        // ten albums, mixes and playlists and not one track.
        // Drawn as a track grid, a card there opened its album
        // where every other row of the same shape plays a
        // track -- the row looked like one thing and behaved
        // like another.
        "MIXED_TYPES_LIST" if holds_tracks(cards) => RowKind::Compact,
        _ => RowKind::Carousel,
    }
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct PageDto {
    rows: Vec<RowDto>,
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct RowDto {
    modules: Vec<ModuleDto>,
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct ModuleDto {
    title: String,
    #[serde(rename = "type")]
    module_type: String,
    #[serde(rename = "pagedList")]
    paged_list: PagedListDto,
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct PagedListDto {
    items: Vec<ItemDto>,
}

/// One entry in a module. The shape varies by module: an album carousel has
/// the fields inline, while a mix or playlist row wraps them.
#[derive(serde::Deserialize, Default, Clone)]
#[serde(default)]
struct ItemDto {
    title: String,
    /// A v2 mix is titled through these rather than `title`/`subTitle`.
    #[serde(rename = "titleTextInfo")]
    title_text_info: Option<TextInfo>,
    #[serde(rename = "subtitleTextInfo")]
    subtitle_text_info: Option<TextInfo>,
    /// A v2 mix's artwork: a list with a `size` per entry, where v1 keys a
    /// map by size.
    #[serde(rename = "mixImages")]
    mix_images: Option<Vec<SizedImage>>,
    /// An artist is named rather than titled, and carries a picture rather
    /// than a cover. Without these the Rising page's row of fifteen
    /// artists yielded no cards at all and the row vanished.
    name: Option<String>,
    picture: Option<String>,
    /// An artist without a photograph: the record cover the web client
    /// shows in its place. Read only when `picture` is null.
    #[serde(rename = "selectedAlbumCoverFallback")]
    picture_fallback: Option<String>,
    /// Seconds. Present on tracks; a playlist's is its whole running time,
    /// which is not what a player's progress bar wants.
    duration: Option<u64>,
    /// Albums and tracks are numbered; playlists are uuids; a mix's is a
    /// string. One field for all three, since two Rust fields renamed onto
    /// the same JSON key is a duplicate-field error that fails the whole
    /// page rather than the one item.
    id: Option<serde_json::Value>,
    uuid: Option<String>,
    #[serde(rename = "numberOfTracks")]
    number_of_tracks: Option<u32>,
    /// An album's, `YYYY-MM-DD`: the year on its card.
    #[serde(rename = "releaseDate")]
    release_date: Option<String>,
    #[serde(rename = "cover")]
    cover: Option<String>,
    #[serde(rename = "squareImage")]
    square_image: Option<String>,
    #[serde(rename = "image")]
    image: Option<String>,
    /// A mix's artwork: a whole URL per size, rather than the uuid every
    /// other item carries. Reading it as a uuid is what left the Mixes
    /// page empty.
    images: Option<MixImages>,
    /// What kind of mix this is: DAILY_MIX and its siblings are built from
    /// the user's own listening, ARTIST_MIX is a radio station, and
    /// VIDEO_DAILY_MIX is video, which this app cannot play.
    #[serde(rename = "mixType")]
    mix_type: Option<String>,
    /// Where an Explore link goes: "pages/genre_hip_hop" and the like.
    /// These carry no id at all, so without it every card on that page had
    /// nothing to open.
    #[serde(rename = "apiPath")]
    api_path: Option<String>,
    artists: Vec<ArtistDto>,
    #[serde(rename = "subTitle")]
    sub_title: Option<String>,
    /// A track has no cover of its own; it carries its album, and the cover
    /// is on that. Whole rows of tracks came back with grey placeholders
    /// until this was read.
    album: Option<AlbumRef>,
    /// Mixes and playlists nest their real payload here.
    item: Option<Box<ItemDto>>,
}

/// A mix's artwork, one entry per size. Only the smallest is wanted: a
/// cover is a few cells across, and the 1500px one is a slow download for
/// the same picture.
#[derive(serde::Deserialize, Default, Debug, Clone)]
struct MixImages {
    #[serde(rename = "SMALL")]
    small: Option<MixImage>,
    #[serde(rename = "MEDIUM")]
    medium: Option<MixImage>,
}

#[derive(serde::Deserialize, Default, Debug, Clone)]
struct MixImage {
    url: Option<String>,
}

#[derive(serde::Deserialize, Default, Debug, Clone)]
#[serde(default)]
struct TextInfo {
    text: String,
}

#[derive(serde::Deserialize, Default, Debug, Clone)]
#[serde(default)]
struct SizedImage {
    size: String,
    url: Option<String>,
}

#[derive(serde::Deserialize, Default, Clone)]
#[serde(default)]
struct AlbumRef {
    cover: Option<String>,
    /// A track card carries its album's name so the player and the track
    /// list can show it. Only the cover was read here before, so a track
    /// played from the home page had no album at all.
    title: String,
}

#[derive(serde::Deserialize, Default, Clone)]
#[serde(default)]
struct ArtistDto {
    name: String,
}

impl ItemDto {
    fn to_card(&self) -> Option<Card> {
        // Mixes and playlists wrap their real payload in `item`; reading the
        // outer object gives an untitled shell and no card at all.
        if let Some(inner) = &self.item {
            return inner.to_card();
        }
        // An artist has a `name` where everything else has a `title`, and
        // a v2 mix has neither.
        let title = if !self.title.is_empty() {
            self.title.clone()
        } else if let Some(name) = self.name.clone().filter(|n| !n.is_empty()) {
            name
        } else {
            self.title_text_info
                .as_ref()
                .map(|t| t.text.clone())
                .unwrap_or_default()
        };
        if title.is_empty() {
            return None;
        }

        // Artists when there are any named, otherwise whatever subtitle the
        // module supplied ("Created by me", "Track radio", and so on). A
        // v2 mix lists its artists under other keys -- `artistName`, not
        // `name` -- so its list is present and nameless, and joining it
        // drew ", , , ," where the web draws the mix's own subtitle.
        let names: Vec<&str> = self
            .artists
            .iter()
            .map(|a| a.name.as_str())
            .filter(|n| !n.is_empty())
            .collect();
        let subtitle = if names.is_empty() {
            self.sub_title
                .clone()
                .or_else(|| self.subtitle_text_info.as_ref().map(|t| t.text.clone()))
                .unwrap_or_default()
        } else {
            names.join(", ")
        };

        // Mixes use `image`, albums `cover`, playlists `squareImage`, and a
        // track has none of them — its artwork belongs to its album.
        let cover = self
            .cover
            .as_ref()
            .or(self.square_image.as_ref())
            .or(self.image.as_ref())
            .or(self.picture.as_ref())
            .or(self.picture_fallback.as_ref())
            .or_else(|| self.album.as_ref()?.cover.as_ref())
            .map(|uuid| cover_url(uuid, 320))
            // A mix names a whole URL per size instead, so there is nothing
            // to compose — the smallest is the one a few cells wide wants.
            .or_else(|| {
                let images = self.images.as_ref()?;
                images
                    .small
                    .as_ref()
                    .or(images.medium.as_ref())?
                    .url
                    .clone()
            })
            .or_else(|| {
                let images = self.mix_images.as_ref()?;
                images
                    .iter()
                    .find(|i| i.size == "SMALL")
                    .or_else(|| images.first())?
                    .url
                    .clone()
            });

        let target = self.target();
        Some(Card {
            title,
            subtitle,
            cover_url: cover,
            // A track's album, for the row and the player; an album's
            // year, as the card draws it everywhere else. Only a track
            // carries a nested album — that is what `target` tells them
            // apart by — so this is empty on everything else without
            // needing to ask which kind it is.
            detail: match &target {
                Some(crate::domain::Target::Album(_)) => self
                    .release_date
                    .as_deref()
                    .and_then(|d| d.get(..4))
                    .unwrap_or_default()
                    .to_string(),
                _ => self
                    .album
                    .as_ref()
                    .map(|a| a.title.clone())
                    .unwrap_or_default(),
            },
            // Only a track's duration means anything to the player; a
            // playlist's is the sum of its contents.
            duration: match (&target, self.duration) {
                (Some(crate::domain::Target::Track(_)), Some(secs)) => {
                    std::time::Duration::from_secs(secs)
                }
                _ => std::time::Duration::ZERO,
            },
            // An artist is a face, and the artist cards everywhere else
            // draw it round. Square here, a row of uploaders read as a
            // row of records.
            round: matches!(target, Some(crate::domain::Target::Artist(_))),
            target,
        })
    }

    /// What this item opens.
    ///
    /// The API does not label these by kind, so they are told apart by what
    /// they carry: a uuid is a playlist, a nested album means this is a
    /// track, and a track count on a numbered item is an album.
    fn target(&self) -> Option<crate::domain::Target> {
        use crate::domain::Target;

        if let Some(uuid) = &self.uuid {
            return Some(Target::Playlist(uuid.clone()));
        }
        // An Explore link has a path and no id of any kind.
        if let Some(path) = &self.api_path {
            return Some(Target::Page(path.clone()));
        }
        // A mix is told apart by its id being a string.
        if let Some(id) = self.id.as_ref().and_then(|v| v.as_str()) {
            return Some(Target::Playlist(format!("{MIX}{id}")));
        }
        let id = self.id.as_ref()?.as_u64()?.to_string();
        if self.album.is_some() {
            return Some(Target::Track(id));
        }
        if self.number_of_tracks.is_some() {
            return Some(Target::Album(id));
        }
        // What is left with a name rather than a title is an artist: the
        // feed's Recently played and Rising's row of fifteen. They opened
        // nothing before.
        if self.name.is_some() {
            return Some(Target::Artist(id));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Target;

    #[test]
    fn the_session_is_read_from_what_sonora_keeps() {
        let held = r#"{"access_token":"a","refresh_token":"r","expires_at":1,
            "user_id":"7","country_code":"FR","lane":"app"}"#;
        assert_eq!(session(held), Some(("a".into(), "FR".into())));
        assert_eq!(session("{}"), None);
        assert_eq!(session("not json"), None);
    }

    #[test]
    fn a_tracks_radio_is_its_mix_under_the_id_sonora_saves_one_by() {
        let track = r#"{"id":1,"title":"T","mixes":{"TRACK_MIX":"0017abc"}}"#;
        assert_eq!(track_mix(track), Some("mix:0017abc".into()));
        assert_eq!(track_mix(r#"{"id":1,"mixes":{}}"#), None, "no station kept");
        assert_eq!(track_mix("not json"), None);
    }

    #[test]
    fn an_artist_with_no_portrait_is_drawn_with_the_cover_tidal_names() {
        let at = |path: &str| format!("https://resources.tidal.com/images/{path}/320x320.jpg");
        let both = r#"{"id":1,"picture":"aa-bb","selectedAlbumCoverFallback":"cc-dd"}"#;
        assert_eq!(artist_picture(both), Some(at("aa/bb")), "their own first");
        let none = r#"{"id":1,"picture":null,"selectedAlbumCoverFallback":"cc-dd"}"#;
        assert_eq!(artist_picture(none), Some(at("cc/dd")));
        assert_eq!(artist_picture(r#"{"id":1,"picture":""}"#), None);
        assert_eq!(artist_picture("not json"), None);
    }

    #[test]
    fn an_artists_page_gives_how_many_follow_them() {
        let page = r#"{"header":{"followersAmount":3701,"biography":null}}"#;
        assert_eq!(followers(page), Some(3701));
        assert_eq!(followers(r#"{"header":{"followersAmount":null}}"#), None);
        assert_eq!(followers("not json"), None);
    }

    #[test]
    fn only_a_page_of_tidals_is_answered_here() {
        assert!(is_page("pages/genre_hip_hop"));
        assert!(is_page("/pages/m_1970s"));
        assert!(!is_page("genre_pop"), "a genre of sonora's");
    }

    #[test]
    fn explore_reads_as_rows_of_links_to_its_pages() {
        let body = r#"{"rows":[
            {"modules":[{"type":"PAGE_LINKS_CLOUD","title":"Moods & Activities",
                "pagedList":{"items":[
                    {"title":"Party","icon":"party","apiPath":"pages/m_party","imageId":"party"},
                    {"title":"Focus","icon":"focus","apiPath":"pages/m_focus","imageId":"focus"}
                ]}}]},
            {"modules":[{"type":"VIDEO_LIST","title":"New Music Videos",
                "pagedList":{"items":[{"id":5,"title":"A Video"}]}}]},
            {"modules":[{"type":"ALBUM_LIST","title":"New Albums",
                "pagedList":{"items":[{"id":2,"title":"An Album","numberOfTracks":9}]}}]}
        ]}"#;
        let home = page(body);
        let headings: Vec<&str> = home.rows.iter().map(|r| r.heading.as_str()).collect();
        assert_eq!(
            headings,
            ["Moods & Activities", "New Albums"],
            "videos play nothing here"
        );
        assert_eq!(home.rows[0].kind, RowKind::Links);
        assert_eq!(home.rows[0].cards[0].title, "Party");
        assert_eq!(
            home.rows[0].cards[0].target,
            Some(Target::Page("pages/m_party".into()))
        );
        assert_eq!(
            home.rows[1].cards[0].target,
            Some(Target::Album("2".into()))
        );
    }

    #[test]
    fn each_card_opens_what_it_is_told_apart_by_what_it_carries() {
        // The pages do not label their items by kind.
        let body = r#"{"rows":[
            {"modules":[{"type":"MIXED_TYPES_LIST","title":"Mixed",
                "pagedList":{"items":[
                    {"uuid":"p-1","title":"A Playlist"},
                    {"id":"00ab","title":"A Mix"},
                    {"id":3,"title":"An Album","numberOfTracks":9},
                    {"id":4,"name":"An Artist"}
                ]}}]},
            {"modules":[{"type":"TRACK_LIST","title":"Tracks",
                "pagedList":{"items":[
                    {"id":5,"title":"A Track","duration":61,"album":{"id":3,"title":"An Album"}}
                ]}}]}
        ]}"#;
        let home = page(body);
        let targets: Vec<_> = home.rows[0]
            .cards
            .iter()
            .map(|c| c.target.clone())
            .collect();
        assert_eq!(
            targets,
            [
                Some(Target::Playlist("p-1".into())),
                Some(Target::Playlist("mix:00ab".into())),
                Some(Target::Album("3".into())),
                Some(Target::Artist("4".into())),
            ]
        );
        assert_eq!(home.rows[0].kind, RowKind::Carousel, "no track among them");
        assert!(home.rows[0].cards[3].round, "an artist's picture is round");
        assert_eq!(home.rows[1].kind, RowKind::Compact, "a grid of track rows");
        let track = &home.rows[1].cards[0];
        assert_eq!(track.target, Some(Target::Track("5".into())));
        assert_eq!(track.duration, std::time::Duration::from_secs(61));
    }

    #[test]
    fn each_kind_of_card_finds_its_picture_under_the_name_tidal_gives_it() {
        // An album's is `cover`, a playlist's `squareImage`, an artist's
        // `picture`, and a track borrows its album's.
        let body = r#"{"rows":[{"modules":[{"type":"MIXED_TYPES_LIST","title":"Mixed",
            "pagedList":{"items":[
                {"id":3,"title":"An Album","numberOfTracks":9,"cover":"aa-bb"},
                {"uuid":"p-1","title":"A Playlist","squareImage":"cc-dd"},
                {"id":4,"name":"An Artist","picture":"ee-ff"},
                {"id":5,"title":"A Track","album":{"id":3,"title":"An Album","cover":"aa-bb"}}
            ]}}]}]}"#;
        let home = page(body);
        let covers: Vec<_> = home.rows[0]
            .cards
            .iter()
            .map(|card| card.cover_url.as_deref())
            .collect();
        let at = |path: &str| format!("https://resources.tidal.com/images/{path}/320x320.jpg");
        assert_eq!(
            covers,
            [
                Some(at("aa/bb").as_str()),
                Some(at("cc/dd").as_str()),
                Some(at("ee/ff").as_str()),
                Some(at("aa/bb").as_str()),
            ]
        );
    }

    #[test]
    fn a_page_that_is_not_one_reads_as_no_rows() {
        assert!(page("not json").rows.is_empty());
        assert!(page(
            r#"{"rows":[{"modules":[{"type":"ALBUM_LIST","title":"Empty",
            "pagedList":{"items":[]}}]}]}"#
        )
        .rows
        .is_empty());
    }
}
