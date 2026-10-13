//! Request construction for Steam's `IUserReviewsService/GetAppReviews` web API method.
//!
//! The parameter choices encoded here are the difference between a census and a sample.
//! Measured against Cyberpunk 2077 (app 1091500), Valve's own defaults return 881,295
//! reviews where an unfiltered request returns 986,295. The 105,000 difference is not
//! evenly drawn: it hides 17.3% of all negative reviews against 9.6% of positive ones,
//! because the two default filters remove activated keys and review bombs respectively.
//! Accepting the defaults biases a corpus against exactly the opinions it exists to count.
//! The method also defaults to English alone, so every language is asked for by name.

use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};

/// Steam's public web API, which serves the reviews and what a developer posts on a game's page.
pub(crate) const API: &str = "https://api.steampowered.com";

/// The method that serves a game's reviews, a page at a time.
const GET_APP_REVIEWS: &str = "/IUserReviewsService/GetAppReviews/v1/";

/// `EUserReviewsPurchaseType_All`: activated keys and gifts as well as Steam purchases.
const EVERY_PURCHASE: u8 = 1;

/// Valve caps a page at 100 regardless of what is requested; asking for more is silently
/// clamped server-side, so the crawler plans its request budget against this number.
pub const MAX_PER_PAGE: u16 = 100;

/// Pagination order for a review query.
///
/// Valve's helpfulness ordering is deliberately absent. It stalls after a handful of pages: a
/// measured crawl of a 2,909-review title returned 21 unique reviews before its cursor began
/// repeating, all drawn from the top of the pile, and Valve documents that it widens its own
/// day range until it finds something rather than running out. Only the date-ordered modes
/// walk a corpus to exhaustion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortOrder {
    /// By creation date, newest first. The order used for a full census and for topping up
    /// against a watermark.
    #[default]
    Recent,
    /// By last-edit date, newest first. Used to catch reviews edited since the last crawl.
    Updated,
}

impl SortOrder {
    /// Valve's `EUserReviewsAppReviewsFilter` value for the order.
    #[must_use]
    pub fn as_filter(self) -> u8 {
        match self {
            Self::Recent => 1,
            Self::Updated => 2,
        }
    }
}

/// A single page request against `GetAppReviews`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewQuery {
    app_id: u32,
    order: SortOrder,
    cursor: String,
    per_page: u16,
    window: Option<(i64, i64)>,
}

impl ReviewQuery {
    /// Starts a query at the beginning of a corpus.
    #[must_use]
    pub fn new(app_id: u32) -> Self {
        Self {
            app_id,
            order: SortOrder::default(),
            cursor: "*".to_owned(),
            per_page: MAX_PER_PAGE,
            window: None,
        }
    }

    #[must_use]
    pub fn order(mut self, order: SortOrder) -> Self {
        self.order = order;
        self
    }

    /// Advances to the next page. The cursor is taken verbatim from the previous response.
    #[must_use]
    pub fn cursor(mut self, cursor: impl Into<String>) -> Self {
        self.cursor = cursor.into();
        self
    }

    /// Reviews per page, at least one: Valve reads none as its default of twenty.
    #[must_use]
    pub fn per_page(mut self, per_page: u16) -> Self {
        self.per_page = per_page.clamp(1, MAX_PER_PAGE);
        self
    }

    /// Restricts the query to reviews created within a Unix-timestamp window, both ends
    /// included.
    ///
    /// Date windows are what make a large corpus tractable: they bound how deep any single
    /// cursor walk goes, and they let independent shards be crawled and resumed in
    /// parallel rather than as one 10,000-page sequence.
    #[must_use]
    pub fn window(mut self, start: i64, end: i64) -> Self {
        self.window = Some((start, end));
        self
    }

    #[must_use]
    pub fn to_url(&self) -> String {
        self.url_at(API)
    }

    /// The method's arguments, which a service method takes as JSON rather than one query
    /// parameter each. Given as plain parameters it falls back to Valve's defaults for the
    /// ones it does not read that way, and counts a third fewer reviews.
    fn input(&self) -> serde_json::Value {
        let mut input = serde_json::json!({
            "appid": self.app_id,
            "filter": self.order.as_filter(),
            "languages": ["all"],
            "purchase_type": EVERY_PURCHASE,
            "filter_offtopic_activity": false,
            "num_per_page": self.per_page,
            "cursor": self.cursor,
        });
        if let Some((start, end)) = self.window {
            input["date_range_start"] = start.into();
            input["date_range_end"] = end.into();
        }
        input
    }

    /// The same page asked of the web API at `api`, an origin with no slash after it.
    pub(crate) fn url_at(&self, api: &str) -> String {
        format!(
            "{api}{GET_APP_REVIEWS}?input_json={}",
            utf8_percent_encode(&self.input().to_string(), NON_ALPHANUMERIC)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input_of(url: &str) -> serde_json::Value {
        let sent = url.split_once("?input_json=").unwrap().1;
        let json = percent_encoding::percent_decode_str(sent)
            .decode_utf8()
            .unwrap();
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn census_arguments_override_valve_biased_defaults() {
        let input = input_of(&ReviewQuery::new(1_091_500).to_url());
        assert_eq!(input["appid"], 1_091_500);
        assert_eq!(
            input["purchase_type"], 1,
            "every purchase, not Steam's alone"
        );
        assert_eq!(input["filter_offtopic_activity"], false);
        assert_eq!(input["languages"], serde_json::json!(["all"]));
    }

    #[test]
    fn pagination_never_uses_helpfulness_ranking() {
        assert_eq!(input_of(&ReviewQuery::new(1).to_url())["filter"], 1);
        let updated = ReviewQuery::new(1).order(SortOrder::Updated).to_url();
        assert_eq!(input_of(&updated)["filter"], 2);
    }

    #[test]
    fn first_page_uses_the_opening_cursor() {
        assert_eq!(input_of(&ReviewQuery::new(1).to_url())["cursor"], "*");
    }

    #[test]
    fn the_whole_argument_is_escaped_so_a_cursor_arrives_as_it_was_given() {
        let url = ReviewQuery::new(1).cursor("AoJ4nczavqADdb+y/wY=").to_url();
        let sent = url.split_once("?input_json=").unwrap().1;
        assert!(
            !sent.contains(['+', '/', '=', '"', '{', ' ', '&', '#'].as_slice()),
            "{url}"
        );
        assert_eq!(input_of(&url)["cursor"], "AoJ4nczavqADdb+y/wY=");
    }

    #[test]
    fn per_page_is_held_to_what_valve_will_serve() {
        assert_eq!(
            input_of(&ReviewQuery::new(1).per_page(500).to_url())["num_per_page"],
            100
        );
        assert_eq!(
            input_of(&ReviewQuery::new(1).per_page(0).to_url())["num_per_page"],
            1
        );
    }

    #[test]
    fn window_is_omitted_unless_requested() {
        let input = input_of(&ReviewQuery::new(1).to_url());
        assert!(input.get("date_range_start").is_none());
        assert!(input.get("date_range_end").is_none());
        let windowed = input_of(
            &ReviewQuery::new(1)
                .window(1_704_067_200, 1_719_792_000)
                .to_url(),
        );
        assert_eq!(windowed["date_range_start"], 1_704_067_200);
        assert_eq!(windowed["date_range_end"], 1_719_792_000);
    }

    #[test]
    fn the_reviews_are_asked_of_the_web_api_method() {
        assert!(ReviewQuery::new(1).to_url().starts_with(
            "https://api.steampowered.com/IUserReviewsService/GetAppReviews/v1/?input_json="
        ));
    }
}
