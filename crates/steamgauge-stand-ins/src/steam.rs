//! The stand-in for Valve: how its review and store endpoints count, page and name a game.

use std::collections::HashSet;

use serde_json::{Value, json};

use crate::stand_in::{Answer, Asked};

/// Valve's totals, as the answer to a request for a page of one.
#[must_use]
pub fn summary_of(total: usize) -> Answer {
    Answer::json(&json!({"response": {
        "query_summary": {
            "num_reviews": 0,
            "total_reviews": total,
            "total_positive": total,
            "total_negative": 0,
            "review_score_desc": "Very Positive",
        },
        "cursor": "*",
    }}))
}

/// Whether the client is asking only for Valve's totals, which it does with a page of one.
#[must_use]
pub fn counting(asked: &Asked) -> bool {
    asked.input()["num_per_page"] == 1
}

/// The store's answer for `app_id`, holding `data` with the game's name.
#[must_use]
pub fn name_of_the_game(app_id: u32, name: &str) -> Answer {
    Answer::json(&json!({
        app_id.to_string(): {"success": true, "data": {"name": name}}
    }))
}

/// Where a cursor the stand-in handed out starts: `*` at the beginning, `at{n}` after `n`.
#[must_use]
pub fn offset(asked: &Asked) -> usize {
    asked.input()["cursor"]
        .as_str()
        .and_then(|cursor| cursor.strip_prefix("at"))
        .and_then(|at| at.parse().ok())
        .unwrap_or(0)
}

/// A page of `page` reviews from `from`, with the cursor for the next one.
#[must_use]
pub fn page_of(served: &[Value], from: usize, page: usize) -> Answer {
    let to = (from + page).min(served.len());
    let next = if from < served.len() { to } else { from };
    let reviews = served.get(from..to).unwrap_or(&[]);
    Answer::json(&json!({"response": {
        "query_summary": {"num_reviews": reviews.len()},
        "reviews": reviews,
        "cursor": format!("at{next}"),
    }}))
}

/// Valve as far as a crawl can tell: `reviews` newest first, `page` at a time.
///
/// Reviews are counted and served by window, with the name of game `app_id` beside them. The
/// first `stalls` walks of the window starting at `stalled` end after their first page, as
/// Steam's do now and then. It also serves the first page in last-edit order, which a crawl asks
/// before it plans.
///
/// # Panics
///
/// The returned handler panics on a review without a `timestamp_created`, or when its own
/// lock was poisoned by another panic.
pub fn valve(
    app_id: u32,
    page: usize,
    reviews: Vec<Value>,
    stalled: Option<i64>,
    stalls: usize,
) -> impl Fn(&Asked) -> Answer + Send + Sync + 'static {
    let walks = std::sync::Mutex::new(0_usize);
    move |asked| {
        if asked.path() == "/api/appdetails" {
            return name_of_the_game(app_id, "A Game");
        }
        let input = asked.input();
        let (start, end) = (
            input["date_range_start"].as_i64().unwrap_or(i64::MIN),
            input["date_range_end"].as_i64().unwrap_or(i64::MAX),
        );
        let mut served: Vec<Value> = reviews
            .iter()
            .filter(|review| (start..=end).contains(&review["timestamp_created"].as_i64().unwrap()))
            .cloned()
            .collect();
        let by_edit = input["filter"] == 2;
        let order = if by_edit {
            "timestamp_updated"
        } else {
            "timestamp_created"
        };
        served.sort_by_key(|review| std::cmp::Reverse(review[order].as_i64()));
        if counting(asked) {
            let distinct: HashSet<&str> = served
                .iter()
                .filter_map(|review| review["recommendationid"].as_str())
                .collect();
            return summary_of(distinct.len());
        }
        let from = offset(asked);
        if stalled == Some(start) && !by_edit {
            let mut walks = walks.lock().unwrap();
            if from == 0 {
                *walks += 1;
            }
            if *walks <= stalls && from > 0 {
                return page_of(&served, served.len(), page);
            }
        }
        page_of(&served, from, page)
    }
}

/// Steam answering a sweep: `pages` in last-edit order, and `total` as the corpus's size.
///
/// A sweep must ask up to a second later than `since`, the second an earlier capture was made.
///
/// # Panics
///
/// The returned handler panics when a page is asked for in any order but last edit, or with an
/// end of the window no later than `since`.
pub fn edits(
    app_id: u32,
    since: i64,
    pages: Vec<Vec<Value>>,
    total: usize,
) -> impl Fn(&Asked) -> Answer + Send + Sync + 'static {
    move |asked| {
        if asked.path() == "/api/appdetails" {
            return name_of_the_game(app_id, "A Game");
        }
        if counting(asked) {
            return summary_of(total);
        }
        assert_eq!(asked.input()["filter"], 2, "the sweep walks by last edit");
        assert!(
            asked.input()["date_range_end"].as_i64() > Some(since),
            "the sweep asks up to its own start, which no other caller has asked"
        );
        let at = offset(asked);
        let next = if at < pages.len() { at + 1 } else { at };
        let reviews = pages.get(at).cloned().unwrap_or_default();
        Answer::json(&json!({"response": {
            "query_summary": {"num_reviews": reviews.len()},
            "reviews": reviews,
            "cursor": format!("at{next}"),
        }}))
    }
}
