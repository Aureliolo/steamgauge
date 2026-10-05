//! HTTP access to Valve's `appreviews` endpoint.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use serde::Deserialize;
use serde_json::Value;
use tokio::sync::Mutex;

use crate::{
    Error, Result,
    query::{ReviewQuery, STORE},
};

/// Valve documents no rate limit for this endpoint, so the ceiling is unknown and can only
/// be found by exceeding it. The client paces itself and treats any push-back as
/// authoritative rather than probing for the real limit.
const BACKOFF_BASE: Duration = Duration::from_secs(2);
/// The longest single wait the client picks for itself. Valve's refusals come in windows of
/// minutes, so doubling past this only overshoots the end of one.
const BACKOFF_CAP: Duration = Duration::from_mins(5);
/// How long a run of refusals is waited out before the crawl is abandoned. Five attempts two to
/// sixteen seconds apart gave up after half a minute, inside a window that lifted on its own a
/// few minutes later, and four crawls failed that way in a row. A crawl is a long job, and
/// waiting is what the person who started it would choose.
pub const PATIENCE: Duration = Duration::from_mins(30);
pub const DEFAULT_PACE: Duration = Duration::from_millis(250);

/// Where the store keeps the pictures of its apps.
const ART: &str = "https://shared.akamai.steamstatic.com";
/// A store header is about 50 KB; anything far larger, past 2 MiB, is not the picture that was
/// asked for.
const ART_LIMIT: usize = 2_097_152;

/// A game the store lists under the words somebody typed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Listing {
    pub app_id: u32,
    pub name: String,
}

/// How long to wait before asking again after the `refusals`-th refusal in a row, having waited
/// `waited` already; `None` when that would run past `patience` and the request should fail.
/// Valve's own Retry-After wins over any guess the client could make, and is never shortened.
fn next_wait(
    refusals: u32,
    waited: Duration,
    told: Option<Duration>,
    patience: Duration,
) -> Option<Duration> {
    let guess = BACKOFF_BASE
        .saturating_mul(2_u32.saturating_pow(refusals.saturating_sub(1)))
        .min(BACKOFF_CAP);
    let wait = told.unwrap_or(guess);
    (waited + wait <= patience).then_some(wait)
}

/// Totals as Valve reports them for the query's filters, present only on the first page.
///
/// `total_reviews` is what a crawl's coverage is measured against. It moves with the
/// request parameters, so it is only comparable to a crawl made with the same ones.
#[derive(Debug, Clone, Deserialize)]
pub struct QuerySummary {
    #[serde(default)]
    pub total_reviews: u64,
    #[serde(default)]
    pub total_positive: u64,
    #[serde(default)]
    pub total_negative: u64,
    #[serde(default)]
    pub review_score_desc: String,
}

/// One page of results.
///
/// Reviews stay as raw JSON rather than a typed struct: the capture layer's job is to lose
/// nothing, and Valve adds fields over time that a fixed struct would silently discard.
#[derive(Debug, Deserialize)]
pub struct Page {
    #[serde(default)]
    pub success: u8,
    #[serde(default)]
    pub query_summary: Option<QuerySummary>,
    #[serde(default)]
    pub reviews: Vec<Value>,
    #[serde(default)]
    pub cursor: Option<String>,
}

/// Told how long the client is about to wait after Valve refused it, and with what status.
type Notice = Arc<dyn Fn(Duration, u16) + Send + Sync>;

/// Paces every request through one shared slot, so raising shard concurrency changes how
/// the work is ordered but never how hard Valve is hit.
#[derive(Clone)]
pub struct SteamClient {
    http: reqwest::Client,
    /// The store's origin, [`STORE`] everywhere but a test.
    store: String,
    /// Where the store's pictures are, [`ART`] everywhere but a test.
    art: String,
    pace: Duration,
    next_slot: Arc<Mutex<Instant>>,
    notice: Option<Notice>,
    patience: Duration,
}

impl std::fmt::Debug for SteamClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SteamClient")
            .field("pace", &self.pace)
            .field("notice", &self.notice.is_some())
            .field("patience", &self.patience)
            .finish_non_exhaustive()
    }
}

impl SteamClient {
    /// How long a run of refusals is waited out before a request fails. [`PATIENCE`] suits a
    /// crawl; somebody who typed a game into a search box would rather hear in seconds that
    /// Steam is refusing than watch the box wait for half an hour.
    #[must_use]
    pub fn with_patience(mut self, patience: Duration) -> Self {
        self.patience = patience;
        self
    }

    /// Says so whenever Valve refuses and the client settles in to wait. A wait can run to
    /// minutes, and a crawl that stops moving for minutes with nothing said looks exactly like
    /// one that has hung.
    #[must_use]
    pub fn with_notice(mut self, notice: impl Fn(Duration, u16) + Send + Sync + 'static) -> Self {
        self.notice = Some(Arc::new(notice));
        self
    }

    /// Asks a stand-in at `origin` everything it would ask the store.
    #[cfg(test)]
    pub(crate) fn with_store(mut self, origin: &str) -> Self {
        origin.clone_into(&mut self.store);
        self
    }

    /// Asks a stand-in at `origin` for every picture it would ask the store's image servers for.
    #[cfg(test)]
    pub(crate) fn with_art(mut self, origin: &str) -> Self {
        origin.clone_into(&mut self.art);
        self
    }

    /// # Errors
    ///
    /// Fails if the HTTP client cannot be constructed, which in practice means a missing or
    /// unusable TLS backend.
    pub fn new(pace: Duration) -> Result<Self> {
        let http = crate::http::builder()
            .user_agent(concat!(
                "steamgauge/",
                env!("CARGO_PKG_VERSION"),
                " (+https://github.com/Aureliolo/steamgauge)"
            ))
            .timeout(Duration::from_secs(60))
            .build()?;
        Ok(Self {
            http,
            store: STORE.to_owned(),
            art: ART.to_owned(),
            pace,
            next_slot: Arc::new(Mutex::new(Instant::now())),
            notice: None,
            patience: PATIENCE,
        })
    }

    /// How many reviews Valve reports for a window, without downloading any of them.
    ///
    /// # Errors
    ///
    /// Propagates transport and throttling failures.
    pub async fn count(&self, app_id: u32, window: Option<(i64, i64)>) -> Result<u64> {
        let mut query = ReviewQuery::new(app_id).per_page(0);
        if let Some((start, end)) = window {
            query = query.window(start, end);
        }
        let page = self.fetch(&query, app_id).await?;
        Ok(page.query_summary.map_or(0, |s| s.total_reviews))
    }

    /// The store's name for an app, so a report can say "Helldivers 2" rather than 553850.
    ///
    /// Best effort by design: a delisted or region-locked app answers with no name, and a
    /// missing name is not a reason to refuse a crawl of reviews that are being served
    /// perfectly well. The caller falls back to the id.
    pub async fn name(&self, app_id: u32) -> Option<String> {
        self.wait_turn().await;
        let url = format!(
            "{}/api/appdetails?appids={app_id}&filters=basic",
            self.store
        );
        let body: serde_json::Value = self.http.get(&url).send().await.ok()?.json().await.ok()?;
        let name = body
            .get(app_id.to_string())?
            .get("data")?
            .get("name")?
            .as_str()?
            .trim();
        (!name.is_empty()).then(|| name.to_owned())
    }

    /// Whether the store lists an app as played only in a VR headset, the one fact the reader
    /// and the labeller are told about a game.
    ///
    /// `None` when the store gives no answer, which is not the same as "no": a caller that
    /// cannot find out has to say so rather than read the game as played on a screen.
    pub async fn headset_only(&self, app_id: u32) -> Option<bool> {
        self.wait_turn().await;
        let url = format!(
            "{}/api/appdetails?appids={app_id}&filters=categories",
            self.store
        );
        let body: serde_json::Value = self.http.get(&url).send().await.ok()?.json().await.ok()?;
        crate::facts::headset_only_in(&body, app_id)
    }

    /// The games the store finds for the words somebody typed, in the store's order, so a game
    /// can be added by its name rather than a number nobody knows by heart.
    ///
    /// # Errors
    ///
    /// Fails where the store cannot be reached or answers with something other than a search.
    pub async fn search(&self, words: &str) -> Result<Vec<Listing>> {
        self.wait_turn().await;
        let words =
            percent_encoding::utf8_percent_encode(words.trim(), percent_encoding::NON_ALPHANUMERIC);
        let url = format!(
            "{}/api/storesearch/?term={words}&l=english&cc=US",
            self.store
        );
        let body: Value = self
            .http
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let items = body
            .get("items")
            .and_then(Value::as_array)
            .ok_or(Error::MalformedPayload { field: "items" })?;
        // Bundles and packages share the search; only an app has reviews of its own.
        Ok(items
            .iter()
            .filter(|item| item.get("type").and_then(Value::as_str) == Some("app"))
            .filter_map(|item| {
                let app_id = u32::try_from(item.get("id")?.as_u64()?).ok()?;
                let name = item.get("name")?.as_str()?.trim();
                (!name.is_empty()).then(|| Listing {
                    app_id,
                    name: name.to_owned(),
                })
            })
            .collect())
    }

    /// The store's wide header picture of an app, a JPEG or PNG, for the window to show beside
    /// its name.
    ///
    /// Best effort, like [`Self::name`]: a game with no picture is shown without one. The
    /// picture is asked for where the store keeps every app's, and where that has moved, at the
    /// address the store's own details give, taken only when it points to that same app's
    /// pictures on the same servers, so nothing the store says can send the client elsewhere.
    pub async fn header_art(&self, app_id: u32) -> Option<Vec<u8>> {
        let pictures = format!("{}/store_item_assets/steam/apps/{app_id}/", self.art);
        if let Some(picture) = self.picture(&format!("{pictures}header.jpg")).await {
            return Some(picture);
        }
        self.wait_turn().await;
        let url = format!(
            "{}/api/appdetails?appids={app_id}&filters=basic",
            self.store
        );
        let body: Value = self.http.get(&url).send().await.ok()?.json().await.ok()?;
        let named = body
            .get(app_id.to_string())?
            .get("data")?
            .get("header_image")?
            .as_str()?;
        if !named.starts_with(&pictures) {
            return None;
        }
        self.picture(named).await
    }

    /// The picture at `url`, where it is one: a JPEG or a PNG of a sensible size.
    async fn picture(&self, url: &str) -> Option<Vec<u8>> {
        let answer = self
            .http
            .get(url)
            .send()
            .await
            .ok()?
            .error_for_status()
            .ok()?;
        if answer
            .content_length()
            .is_some_and(|length| length > ART_LIMIT as u64)
        {
            return None;
        }
        let bytes = answer.bytes().await.ok()?;
        let image =
            bytes.starts_with(&[0xFF, 0xD8, 0xFF]) || bytes.starts_with(b"\x89PNG\r\n\x1a\n");
        (image && bytes.len() <= ART_LIMIT).then(|| bytes.to_vec())
    }

    /// Fetches one page, retrying on throttling and transient server errors.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Throttled`] if Valve keeps refusing for longer than the client's
    /// patience, and [`Error::NoSuchCorpus`] if it answers `success: 0`.
    pub async fn fetch(&self, query: &ReviewQuery, app_id: u32) -> Result<Page> {
        let url = query.url_at(&self.store);
        let mut attempt = 0;
        let mut waited = Duration::ZERO;

        loop {
            attempt += 1;
            self.wait_turn().await;

            let response = self.http.get(&url).send().await?;
            let status = response.status();

            if status.is_success() {
                let page: Page = response.json().await?;
                if page.success != 1 {
                    return Err(Error::NoSuchCorpus { app_id });
                }
                return Ok(page);
            }

            let retryable = status.as_u16() == 429 || status.is_server_error();
            let wait = if retryable {
                next_wait(attempt, waited, retry_after(&response), self.patience)
            } else {
                None
            };
            let Some(wait) = wait else {
                return Err(Error::Throttled {
                    attempts: attempt,
                    status: status.as_u16(),
                });
            };
            if let Some(notice) = &self.notice {
                notice(wait, status.as_u16());
            }
            waited += wait;
            tokio::time::sleep(wait).await;
        }
    }

    async fn wait_turn(&self) {
        let now = Instant::now();
        let slot_at = {
            let mut slot = self.next_slot.lock().await;
            let at = (*slot).max(now);
            *slot = at + self.pace;
            at
        };
        if slot_at > now {
            tokio::time::sleep(slot_at - now).await;
        }
    }
}

fn retry_after(response: &reqwest::Response) -> Option<Duration> {
    response
        .headers()
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stand_in;

    #[test]
    fn the_client_waits_out_a_window_of_minutes_rather_than_half_a_minute() {
        let mut waited = Duration::ZERO;
        let mut refusals = 0;
        while let Some(wait) = next_wait(refusals + 1, waited, None, PATIENCE) {
            refusals += 1;
            waited += wait;
        }
        assert!(
            waited >= Duration::from_mins(20),
            "gave up after {waited:?}, inside the minutes a refusal lasts"
        );
        assert!(waited <= PATIENCE);
    }

    #[test]
    fn no_single_guess_overshoots_a_window() {
        for refusals in 1..40 {
            assert!(next_wait(refusals, Duration::ZERO, None, PATIENCE).unwrap() <= BACKOFF_CAP);
        }
    }

    #[test]
    fn valve_saying_how_long_is_obeyed_and_not_shortened() {
        let told = Duration::from_secs(600);
        assert_eq!(
            next_wait(1, Duration::ZERO, Some(told), PATIENCE),
            Some(told)
        );
    }

    #[test]
    fn a_wait_past_the_patience_is_a_refusal_to_keep_waiting() {
        assert_eq!(next_wait(1, PATIENCE, None, PATIENCE), None);
        assert_eq!(
            next_wait(1, Duration::ZERO, Some(PATIENCE * 2), PATIENCE),
            None
        );
    }

    /// A client of a stand-in store answering every request with `answer`, at no pace.
    fn steam(
        answer: impl Fn(&stand_in::Asked) -> stand_in::Answer + Send + Sync + 'static,
    ) -> (stand_in::Server, SteamClient) {
        let server = stand_in::Server::new(answer);
        let client = SteamClient::new(Duration::ZERO)
            .unwrap()
            .with_store(&server.origin());
        (server, client)
    }

    fn page(summary: &serde_json::Value) -> stand_in::Answer {
        stand_in::Answer::json(&serde_json::json!({
            "success": 1,
            "query_summary": summary,
            "reviews": [],
            "cursor": "*",
        }))
    }

    /// Answers `refusals` requests in a row with `refusal`, and every one after with a page.
    fn refusing(
        refusals: usize,
        refusal: &stand_in::Answer,
    ) -> impl Fn(&stand_in::Asked) -> stand_in::Answer + Send + Sync + 'static {
        let refusal = refusal.clone();
        let seen = std::sync::atomic::AtomicUsize::new(0);
        move |_| {
            if seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst) < refusals {
                refusal.clone()
            } else {
                page(&serde_json::json!({}))
            }
        }
    }

    #[tokio::test]
    async fn a_count_asks_for_no_reviews_in_the_window_and_reads_valves_total() {
        let (server, client) = steam(|_| page(&serde_json::json!({"total_reviews": 1234})));
        assert_eq!(client.count(7, Some((100, 200))).await.unwrap(), 1234);
        let asked = &server.asked()[0];
        assert_eq!(asked.path(), "/appreviews/7");
        assert_eq!(asked.param("num_per_page"), Some("0"));
        assert_eq!(asked.param("start_date"), Some("100"));
        assert_eq!(asked.param("end_date"), Some("200"));
    }

    #[tokio::test]
    async fn the_name_is_the_stores_trimmed_and_none_where_it_is_blank() {
        let (server, client) = steam(|asked| {
            let name = if asked.param("appids") == Some("7") {
                "  Helldivers 2 "
            } else {
                " "
            };
            stand_in::Answer::json(&serde_json::json!({
                asked.param("appids").unwrap(): {"success": true, "data": {"name": name}}
            }))
        });
        assert_eq!(client.name(7).await.as_deref(), Some("Helldivers 2"));
        assert_eq!(client.name(8).await, None);
        let asked = &server.asked()[0];
        assert_eq!(asked.path(), "/api/appdetails");
        assert_eq!(asked.param("filters"), Some("basic"));
    }

    #[tokio::test]
    async fn a_search_lists_the_apps_the_store_finds_in_its_order_and_nothing_else() {
        let (server, client) = steam(|_| {
            stand_in::Answer::json(&serde_json::json!({"total": 4, "items": [
                {"type": "app", "id": 1_091_500, "name": " Cyberpunk 2077 "},
                {"type": "sub", "id": 9, "name": "A bundle"},
                {"type": "app", "id": 2_138_330, "name": "Cyberpunk 2077: Phantom Liberty"},
                {"type": "app", "id": 7, "name": "  "},
            ]}))
        });
        assert_eq!(
            client.search("  cyberpunk & co ").await.unwrap(),
            vec![
                Listing {
                    app_id: 1_091_500,
                    name: "Cyberpunk 2077".to_owned()
                },
                Listing {
                    app_id: 2_138_330,
                    name: "Cyberpunk 2077: Phantom Liberty".to_owned()
                },
            ]
        );
        let asked = &server.asked()[0];
        assert_eq!(asked.path(), "/api/storesearch/");
        assert_eq!(asked.param("term"), Some("cyberpunk%20%26%20co"));
        assert_eq!(asked.param("l"), Some("english"));
    }

    #[tokio::test]
    async fn a_search_the_store_will_not_answer_says_so() {
        let (_server, client) = steam(|_| stand_in::Answer::status(503));
        assert!(client.search("anything").await.is_err());
        let (_server, client) = steam(|_| stand_in::Answer::json(&serde_json::json!({})));
        assert!(matches!(
            client.search("anything").await,
            Err(Error::MalformedPayload { field: "items" })
        ));
    }

    const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3];
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nrest";

    /// A client of one stand-in serving both the store and its pictures.
    fn art(
        answer: impl Fn(&stand_in::Asked) -> stand_in::Answer + Send + Sync + 'static,
    ) -> (stand_in::Server, SteamClient) {
        let (server, client) = steam(answer);
        let client = client.with_art(&server.origin());
        (server, client)
    }

    #[tokio::test]
    async fn a_header_is_fetched_from_where_the_store_keeps_every_apps() {
        let (server, client) = art(|asked| {
            if asked.path() == "/store_item_assets/steam/apps/7/header.jpg" {
                stand_in::Answer::body(JPEG)
            } else {
                stand_in::Answer::status(404)
            }
        });
        assert_eq!(client.header_art(7).await.as_deref(), Some(JPEG));
        assert_eq!(
            server.asked().len(),
            1,
            "the store's details were not needed"
        );
    }

    #[tokio::test]
    async fn a_moved_header_is_found_through_the_stores_details_for_that_same_app() {
        let (server, client) = art(|asked| match asked.path() {
            "/api/appdetails" => {
                let origin = asked.header("host").unwrap_or_default().to_owned();
                stand_in::Answer::json(&serde_json::json!({"7": {"data": {
                    "header_image": format!("http://{origin}/store_item_assets/steam/apps/7/abc/header.jpg?t=1")
                }}}))
            }
            "/store_item_assets/steam/apps/7/abc/header.jpg" => stand_in::Answer::body(PNG),
            _ => stand_in::Answer::status(404),
        });
        assert_eq!(client.header_art(7).await.as_deref(), Some(PNG));
        assert_eq!(server.asked().len(), 3);
    }

    #[tokio::test]
    async fn a_header_the_details_place_anywhere_else_is_not_fetched() {
        let (server, client) = art(|asked| match asked.path() {
            "/api/appdetails" => stand_in::Answer::json(&serde_json::json!({"7": {"data": {
                "header_image": "https://elsewhere.example/store_item_assets/steam/apps/7/header.jpg"
            }}})),
            _ => stand_in::Answer::status(404),
        });
        assert_eq!(client.header_art(7).await, None);
        assert_eq!(server.asked().len(), 2, "nothing was asked of elsewhere");
    }

    #[tokio::test]
    async fn only_a_picture_of_a_sensible_size_is_taken_for_one() {
        let (_server, client) = art(|asked| match asked.path() {
            "/store_item_assets/steam/apps/1/header.jpg" => {
                stand_in::Answer::body("<html>moved</html>")
            }
            "/store_item_assets/steam/apps/2/header.jpg" => {
                let mut huge = JPEG.to_vec();
                huge.resize(ART_LIMIT + 1, 0);
                stand_in::Answer::body(huge)
            }
            "/store_item_assets/steam/apps/3/header.jpg" => stand_in::Answer::body(PNG),
            "/store_item_assets/steam/apps/4/header.jpg" => {
                let mut whole = JPEG.to_vec();
                whole.resize(ART_LIMIT, 0);
                stand_in::Answer::body(whole)
            }
            _ => stand_in::Answer::status(404),
        });
        assert_eq!(client.header_art(1).await, None);
        assert_eq!(client.header_art(2).await, None);
        assert_eq!(client.header_art(3).await.as_deref(), Some(PNG));
        assert_eq!(
            client.header_art(4).await.map(|picture| picture.len()),
            Some(ART_LIMIT),
            "a picture of exactly the limit is still a picture"
        );
    }

    #[tokio::test]
    async fn the_store_is_asked_whether_a_game_is_played_only_in_a_headset() {
        let (server, client) = steam(|asked| {
            let categories = if asked.param("appids") == Some("620980") {
                serde_json::json!([{"id": 2}, {"id": 54}])
            } else {
                serde_json::json!([{"id": 2}])
            };
            stand_in::Answer::json(&serde_json::json!({
                asked.param("appids").unwrap(): {"success": true, "data": {"categories": categories}}
            }))
        });
        assert_eq!(client.headset_only(620_980).await, Some(true));
        assert_eq!(client.headset_only(920_210).await, Some(false));
        assert_eq!(server.asked()[0].param("filters"), Some("categories"));

        let (_server, client) = steam(|_| stand_in::Answer::hang_up());
        assert_eq!(
            client.headset_only(620_980).await,
            None,
            "no answer is not a no"
        );
    }

    #[tokio::test]
    async fn an_app_valve_serves_nothing_for_is_named_rather_than_read_as_empty() {
        let (_server, client) =
            steam(|_| stand_in::Answer::json(&serde_json::json!({"success": 0})));
        let refused = client.fetch(&ReviewQuery::new(7), 7).await;
        assert!(
            matches!(refused, Err(Error::NoSuchCorpus { app_id: 7 })),
            "{refused:?}"
        );
    }

    #[tokio::test]
    async fn a_refusal_or_a_server_error_is_waited_out_and_asked_again() {
        for status in [429, 503] {
            let refusal = stand_in::Answer::status(status).header("Retry-After", "0");
            let (server, client) = steam(refusing(1, &refusal));
            let told = Arc::new(std::sync::Mutex::new(Vec::new()));
            let client = client.with_notice({
                let told = Arc::clone(&told);
                move |wait, status| told.lock().unwrap().push((wait, status))
            });
            assert!(client.fetch(&ReviewQuery::new(7), 7).await.is_ok());
            assert_eq!(server.asked().len(), 2, "{status}");
            assert_eq!(*told.lock().unwrap(), [(Duration::ZERO, status)]);
        }
    }

    #[tokio::test]
    async fn any_other_failure_is_reported_at_once_and_not_asked_again() {
        let refusal = stand_in::Answer::status(404).header("Retry-After", "0");
        let (server, client) = steam(refusing(1, &refusal));
        let refused = client.fetch(&ReviewQuery::new(7), 7).await;
        assert!(
            matches!(
                refused,
                Err(Error::Throttled {
                    attempts: 1,
                    status: 404
                })
            ),
            "{refused:?}"
        );
        assert_eq!(server.asked().len(), 1);
    }

    #[tokio::test]
    async fn valves_waits_add_up_against_the_patience() {
        // Each refusal asks for a second; the first fits a patience of one, the second would
        // run past it.
        let refusal = stand_in::Answer::status(429).header("Retry-After", "1");
        let (server, client) = steam(refusing(5, &refusal));
        let told = Arc::new(std::sync::Mutex::new(Vec::new()));
        let client = client.with_patience(Duration::from_secs(1)).with_notice({
            let told = Arc::clone(&told);
            move |wait, status| told.lock().unwrap().push((wait, status))
        });
        let refused = client.fetch(&ReviewQuery::new(7), 7).await;
        assert!(
            matches!(
                refused,
                Err(Error::Throttled {
                    attempts: 2,
                    status: 429
                })
            ),
            "{refused:?}"
        );
        assert_eq!(server.asked().len(), 2);
        assert_eq!(*told.lock().unwrap(), [(Duration::from_secs(1), 429)]);
    }

    #[tokio::test]
    async fn requests_are_spaced_by_the_pace_however_many_ask_at_once() {
        let server = stand_in::Server::new(|_| page(&serde_json::json!({})));
        let pace = Duration::from_millis(200);
        let client = SteamClient::new(pace).unwrap().with_store(&server.origin());
        let started = Instant::now();
        let (first, second, third) = tokio::join!(
            client.count(7, None),
            client.count(7, None),
            client.count(7, None)
        );
        assert!(first.is_ok() && second.is_ok() && third.is_ok());
        assert!(
            started.elapsed() >= pace * 2,
            "three requests in {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_short_patience_gives_up_in_seconds() {
        let patience = Duration::from_secs(30);
        let mut waited = Duration::ZERO;
        let mut refusals = 0;
        while let Some(wait) = next_wait(refusals + 1, waited, None, patience) {
            refusals += 1;
            waited += wait;
        }
        assert!(waited <= patience);
    }
}
