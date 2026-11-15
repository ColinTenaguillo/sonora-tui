//! What TIDAL has that sonora's `MusicApi` has no word for yet: the mix a
//! track's radio is.
//!
//! sonora's TIDAL client is private to its crate, so this request is made
//! here, with the session sonora keeps: read from its file each time, never
//! written. It is asked upstream (sonorahq/sonora#1359); the day sonora
//! answers it, this file goes, with its line in `extras_for`.

use std::sync::Arc;

use anyhow::Context as _;
use async_trait::async_trait;
use music::MusicApi;

use super::Extras;

const API: &str = "https://api.tidal.com";

/// What the web player says it is, as the requests this file was written
/// against said it.
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
        // Bounded, so a radio that never answers falls back to sonora's
        // station rather than loading for ever.
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
        let mut url = reqwest::Url::parse(&format!("{API}/v1/{path}"))?;
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

#[async_trait]
impl Extras for TidalExtras {
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
