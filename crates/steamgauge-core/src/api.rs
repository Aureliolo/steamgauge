//! HTTP access to Valve's `appreviews` endpoint.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use serde::Deserialize;
use serde_json::Value;
use tokio::sync::Mutex;

use crate::{Error, Result, query::ReviewQuery};

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

    /// # Errors
    ///
    /// Fails if the HTTP client cannot be constructed, which in practice means a missing or
    /// unusable TLS backend.
    pub fn new(pace: Duration) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!(
                "steamgauge/",
                env!("CARGO_PKG_VERSION"),
                " (+https://github.com/Aureliolo/steamgauge)"
            ))
            .timeout(Duration::from_secs(60))
            .build()?;
        Ok(Self {
            http,
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
        let url =
            format!("https://store.steampowered.com/api/appdetails?appids={app_id}&filters=basic");
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
            "https://store.steampowered.com/api/appdetails?appids={app_id}&filters=categories"
        );
        let body: serde_json::Value = self.http.get(&url).send().await.ok()?.json().await.ok()?;
        crate::facts::headset_only_in(&body, app_id)
    }

    /// Fetches one page, retrying on throttling and transient server errors.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Throttled`] if Valve keeps refusing for longer than the client's
    /// patience, and [`Error::NoSuchCorpus`] if it answers `success: 0`.
    pub async fn fetch(&self, query: &ReviewQuery, app_id: u32) -> Result<Page> {
        let url = query.to_url();
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
