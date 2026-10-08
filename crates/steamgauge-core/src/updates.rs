//! A game's own announcements on Steam, and which of them are updates.
//!
//! Steam's public news API lists what a developer posts on a game's page, and needs no key. Only
//! the developer's own posts are asked for: the same API carries press articles about a game,
//! which are about it rather than from it. Of each post four things are kept: its title, when it
//! was posted, Steam's id for it, and whether the developer marked it as patch notes. Its text is
//! a page of somebody else's markup and nothing shows it, so none of it is asked for beyond the
//! single character the API will not go below.
//!
//! Kept per game beside its reviews, so a game's page and its report show its updates with the
//! network gone, and asked again whenever the game is brought up to date.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Error, Result, SteamClient};

/// The file a game's announcements are kept in, in its folder of the library.
pub const FILE: &str = "announcements.json";

/// The feed Steam files a developer's own announcements under.
pub const FEED: &str = "steam_community_announcements";

/// The tag Steam puts on a post its developer published as patch notes.
const PATCH_NOTES: &str = "patchnotes";

/// Posts asked for in one page. Steam answers a game's whole history at once when asked for
/// enough, and a game posting weekly for ten years has about five hundred.
const PER_PAGE: u32 = 1_000;

/// Pages asked for before the history is taken as complete, so an answer that never runs out
/// cannot keep the client asking.
const PAGES: usize = 20;

/// The longest title kept, in characters. A real one is a line; anything longer is not a title.
const TITLE_CHARACTERS: usize = 200;

/// How close two posts of the same title are taken as one update posted twice: Steam lists a
/// post again when a developer re-publishes it, and a game's page would mark both.
const REPOSTED_WITHIN: i64 = 7 * 86_400;

/// One post a developer made on a game's Steam page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Announcement {
    /// Steam's id for the post, digits only, which is all its address is made from.
    pub gid: String,
    pub title: String,
    /// When it was posted, in Unix seconds.
    pub posted: i64,
    /// Whether the developer published it as patch notes.
    pub patch_notes: bool,
}

/// Everything a game's developer has posted, as last asked.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Announcements {
    /// When Steam was last asked, in Unix seconds.
    pub asked: i64,
    /// Oldest first.
    pub posts: Vec<Announcement>,
}

/// An update to a game, as its page and its report show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Update {
    pub gid: String,
    pub title: String,
    pub posted: i64,
    /// The post on Steam.
    pub link: String,
}

impl Announcements {
    /// What is kept in a game's folder, if anything is.
    #[must_use]
    pub fn load(game_dir: &Path) -> Option<Self> {
        serde_json::from_slice(&std::fs::read(game_dir.join(FILE)).ok()?).ok()
    }

    /// Keeps these in a game's folder, replacing what was there only once they are written whole.
    ///
    /// # Errors
    ///
    /// Fails if the file cannot be written.
    pub fn save(&self, game_dir: &Path) -> Result<()> {
        let partial = game_dir.join(format!("{FILE}.partial"));
        std::fs::write(&partial, serde_json::to_vec(self)?)?;
        std::fs::rename(&partial, game_dir.join(FILE))?;
        Ok(())
    }

    /// The posts that are updates, oldest first, each once.
    #[must_use]
    pub fn updates(&self) -> Vec<Update> {
        let mut found: Vec<Update> = Vec::new();
        for post in self
            .posts
            .iter()
            .filter(|post| is_update(&post.title, post.patch_notes))
        {
            let reposted = found.iter().any(|earlier| {
                earlier.title.eq_ignore_ascii_case(&post.title)
                    && post.posted - earlier.posted <= REPOSTED_WITHIN
            });
            if !reposted {
                found.push(Update {
                    gid: post.gid.clone(),
                    title: post.title.clone(),
                    posted: post.posted,
                    link: link(&post.gid),
                });
            }
        }
        found
    }
}

/// Where a post is on Steam. Made from its id alone, never from an address the API hands back:
/// Steam's store sends this on to the post on the game's own page.
#[must_use]
pub fn link(gid: &str) -> String {
    format!("https://store.steampowered.com/news/externalpost/{FEED}/{gid}")
}

/// Words in a title that say the post is not live for every player yet.
const NOT_LIVE: [&[&str]; 22] = [
    &["public", "test"],
    &["test", "server"],
    &["test", "branch"],
    &["stress", "test"],
    &["playtest"],
    &["beta"],
    &["experimental"],
    &["ptr"],
    &["delay"],
    &["delayed"],
    &["delays"],
    &["upcoming"],
    &["incoming"],
    &["coming"],
    &["sneak", "peek"],
    &["preview"],
    &["roadmap"],
    &["release", "date"],
    &["launch", "date"],
    &["revealed"],
    &["pre", "patch"],
    &["not", "an", "update"],
];

/// Words in a title that say the post is another kind of post: a sale, an event of the
/// developer's, or a report on the game rather than a change to it.
const NOT_A_RELEASE: [&[&str]; 18] = [
    &["sale"],
    &["discount"],
    &["free", "weekend"],
    &["bundle"],
    &["preorder"],
    &["preorders"],
    &["pre", "order"],
    &["pre", "orders"],
    &["merch"],
    &["merchandise"],
    &["community", "update"],
    &["state", "of", "the", "game"],
    &["development", "update"],
    &["dev", "update"],
    &["progress", "update"],
    &["status", "update"],
    &["update", "history"],
    &["update", "news"],
];

/// Words in a title that name a release.
const RELEASE: [&[&str]; 8] = [
    &["patch"],
    &["patches"],
    &["hotfix"],
    &["hotfixes"],
    &["update"],
    &["changelog"],
    &["patchnotes"],
    &["release", "notes"],
];

/// Whether a post is an update the game's players received, by what Steam marks and by its
/// title.
///
/// A post its developer published as patch notes is one, and so is a post whose title names a
/// patch, a hotfix, an update, a changelog or release notes, or carries a version number such as
/// 1.6.4 or v0.221.10. Neither is one where the title says it is not live for every player yet
/// (a public test, a beta, an experimental branch, a delay, something upcoming, a preview, a
/// roadmap, a release date, a pre-patch) or that it is another kind of post (a sale, a free
/// weekend, a bundle, pre-orders, merchandise, a community or development update, the state of
/// the game).
#[must_use]
pub fn is_update(title: &str, patch_notes: bool) -> bool {
    let lower = title.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    let says = |phrases: &[&[&str]]| {
        phrases
            .iter()
            .any(|phrase| words.windows(phrase.len()).any(|run| run == *phrase))
    };
    if says(&NOT_LIVE) || says(&NOT_A_RELEASE) || lower.contains("% off") {
        return false;
    }
    patch_notes || says(&RELEASE) || has_version(title)
}

/// Whether a title carries a version number: digits with at least one dot inside, as in 1.4,
/// 01.003.302 or v1.6.4b. A price ($9.99) or a share (10.5%) is not one.
fn has_version(title: &str) -> bool {
    title
        .split(|c: char| !(c.is_alphanumeric() || matches!(c, '.' | '$' | '€' | '£' | '%')))
        .map(|token| token.trim_end_matches('.'))
        .any(|token| {
            let token = token
                .strip_prefix('v')
                .or_else(|| token.strip_prefix('V'))
                .unwrap_or(token);
            let parts: Vec<&str> = token.split('.').collect();
            let Some((last, leading)) = parts.split_last() else {
                return false;
            };
            let last = last
                .strip_suffix(|c: char| c.is_ascii_lowercase())
                .unwrap_or(last);
            let digits = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
            !leading.is_empty() && leading.iter().all(|part| digits(part)) && digits(last)
        })
}

/// One page of Steam's answer: the developer's own posts on this game in it, and how many posts
/// of any kind it listed, which is what says whether an older page is there to ask for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub posts: Vec<Announcement>,
    pub listed: usize,
}

/// Reads one page of Steam's news API.
///
/// A post is kept only where it is this game's, filed under the developer's own feed, with an id
/// of digits, a title and a time; its title loses anything that is not printable text.
///
/// # Errors
///
/// Fails where the answer holds no list of news.
pub fn parse(body: &Value, app_id: u32) -> Result<Page> {
    let items = body
        .get("appnews")
        .and_then(|news| news.get("newsitems"))
        .and_then(Value::as_array)
        .ok_or(Error::MalformedPayload {
            field: "appnews.newsitems",
        })?;
    let posts = items
        .iter()
        .filter_map(|item| {
            let ours = item.get("appid").and_then(Value::as_u64) == Some(u64::from(app_id))
                && item.get("feedname").and_then(Value::as_str) == Some(FEED);
            let gid = item.get("gid").and_then(Value::as_str)?;
            let id_like =
                !gid.is_empty() && gid.len() <= 20 && gid.bytes().all(|b| b.is_ascii_digit());
            let posted = item
                .get("date")
                .and_then(Value::as_i64)
                .filter(|at| *at > 0)?;
            let title = plain(item.get("title").and_then(Value::as_str)?);
            (ours && id_like && !title.is_empty()).then(|| Announcement {
                gid: gid.to_owned(),
                title,
                posted,
                patch_notes: item
                    .get("tags")
                    .and_then(Value::as_array)
                    .is_some_and(|tags| tags.iter().any(|tag| tag.as_str() == Some(PATCH_NOTES))),
            })
        })
        .collect();
    Ok(Page {
        posts,
        listed: items.len(),
    })
}

/// A title as one line of printable text: control and direction-changing characters gone,
/// runs of space made one, and no longer than a title is.
fn plain(title: &str) -> String {
    let kept: String = title
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .filter(|c| !c.is_control() && !matches!(c, '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))
        .collect();
    kept.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(TITLE_CHARACTERS)
        .collect()
}

/// Asks Steam for everything the developer has posted on a game's page, oldest first.
///
/// Steam lists newest first; a page as full as was asked for means older ones may be there, and
/// the next is asked for from the oldest time seen, so posts sharing that second are not lost.
///
/// # Errors
///
/// Fails where Steam refuses or answers with something that is not a list of news.
pub async fn fetch(client: &SteamClient, app_id: u32, now: i64) -> Result<Announcements> {
    let mut posts: Vec<Announcement> = Vec::new();
    let mut before = None;
    for _ in 0..PAGES {
        let page = parse(
            &client.announcements(app_id, before, PER_PAGE).await?,
            app_id,
        )?;
        let oldest = page.posts.iter().map(|post| post.posted).min();
        let known = posts.len();
        for post in page.posts {
            if !posts.iter().any(|seen| seen.gid == post.gid) {
                posts.push(post);
            }
        }
        let full = page.listed >= PER_PAGE as usize;
        match oldest {
            Some(at) if full && posts.len() > known => before = Some(at),
            _ => break,
        }
    }
    posts.sort_by(|a, b| a.posted.cmp(&b.posted).then_with(|| a.gid.cmp(&b.gid)));
    Ok(Announcements { asked: now, posts })
}

/// Asks Steam for a game's announcements and keeps them beside its reviews.
///
/// # Errors
///
/// Fails where Steam refuses, or the game's folder cannot be written; what was kept before is
/// left as it was.
pub async fn refresh(
    client: &SteamClient,
    app_id: u32,
    out_dir: &Path,
    now: i64,
) -> Result<Announcements> {
    let fetched = fetch(client, app_id, now).await?;
    let game = out_dir.join(format!("appid={app_id}"));
    std::fs::create_dir_all(&game)?;
    fetched.save(&game)?;
    Ok(fetched)
}

/// What is kept of a game's announcements in a library, or nothing where Steam has not been
/// asked.
#[must_use]
pub fn kept(out_dir: &Path, app_id: u32) -> Option<Announcements> {
    Announcements::load(&out_dir.join(format!("appid={app_id}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stand_in;

    /// Steam's answer for Helldivers 2 (553850) on 8 October 2026, cut to nine of its 163 posts.
    const HELLDIVERS: &str = include_str!("news-553850.json");

    fn helldivers() -> Page {
        parse(&serde_json::from_str(HELLDIVERS).unwrap(), 553_850).unwrap()
    }

    #[test]
    fn steams_answer_is_read_to_titles_times_ids_and_its_patch_notes_mark() {
        let page = helldivers();
        assert_eq!(page.listed, 9);
        assert_eq!(page.posts.len(), 9);
        assert_eq!(
            page.posts[1],
            Announcement {
                gid: "1844751498220713".to_owned(),
                title: "Devoid of Liberty: 7.1.1".to_owned(),
                posted: 1_790_240_423,
                patch_notes: true,
            }
        );
        let marked: Vec<&str> = page
            .posts
            .iter()
            .filter(|post| post.patch_notes)
            .map(|post| post.title.as_str())
            .collect();
        assert_eq!(marked, ["Devoid of Liberty: 7.1.1"]);
    }

    #[test]
    fn the_updates_in_steams_answer_are_its_patches_and_not_its_news_or_its_promises() {
        let kept = Announcements {
            asked: 0,
            posts: helldivers().posts.into_iter().rev().collect(),
        };
        let titles: Vec<String> = kept.updates().into_iter().map(|u| u.title).collect();
        assert_eq!(
            titles,
            [
                "Patch 01.003.302",
                "HELLDIVERS 2 Devoid of Liberty Update Out Now",
                "Devoid of Liberty: 7.1.0",
                "Devoid of Liberty: 7.1.1",
            ],
            "maintenance, a warbond, a delay, a patch still to come and a state of the game are \
             not updates"
        );
    }

    #[test]
    fn a_post_that_is_not_this_games_own_announcement_is_left_out() {
        let item = |changes: serde_json::Value| {
            let mut item = serde_json::json!({
                "gid": "123", "title": "Patch 1.2", "date": 1_700_000_000,
                "feedname": FEED, "appid": 7,
            });
            for (key, value) in changes.as_object().unwrap() {
                item[key] = value.clone();
            }
            item
        };
        let body = serde_json::json!({"appnews": {"appid": 7, "newsitems": [
            item(serde_json::json!({})),
            item(serde_json::json!({"appid": 8})),
            item(serde_json::json!({"feedname": "pcgamer"})),
            item(serde_json::json!({"gid": "12a"})),
            item(serde_json::json!({"gid": ""})),
            item(serde_json::json!({"gid": "1".repeat(21)})),
            item(serde_json::json!({"gid": "1".repeat(20)})),
            item(serde_json::json!({"title": " \u{7} "})),
            item(serde_json::json!({"date": 0})),
            item(serde_json::json!({"date": "yesterday"})),
            item(serde_json::json!({"title": null})),
        ]}});
        let page = parse(&body, 7).unwrap();
        assert_eq!(page.listed, 11);
        let ids: Vec<&str> = page.posts.iter().map(|post| post.gid.as_str()).collect();
        assert_eq!(ids, ["123", "11111111111111111111"]);
        assert!(parse(&serde_json::json!({"appnews": {}}), 7).is_err());
        assert!(parse(&serde_json::json!({}), 7).is_err());
    }

    #[test]
    fn a_title_is_kept_as_one_line_of_plain_text_no_longer_than_a_title() {
        assert_eq!(
            plain("  Patch\t1.2\n\u{202e}reversed\u{2066} \u{0}notes\u{200f} "),
            "Patch 1.2 reversed notes"
        );
        let long = "a".repeat(TITLE_CHARACTERS + 1);
        assert_eq!(plain(&long).chars().count(), TITLE_CHARACTERS);
        assert_eq!(
            plain(&"é".repeat(TITLE_CHARACTERS)),
            "é".repeat(TITLE_CHARACTERS)
        );
    }

    #[test]
    fn steams_patch_notes_mark_makes_an_update_unless_the_title_says_it_is_not_live() {
        assert!(is_update("Galactic Network Maintenance", true));
        assert!(!is_update("Galactic Network Maintenance", false));
        // Valheim marks its public tests as patch notes, and only those who opt in play them.
        assert!(!is_update("Patch 0.221.13 (Public Test)", true));
        assert!(!is_update("Update Delay", true));
        assert!(!is_update("Animgraph 2 Beta Update", true));
    }

    #[test]
    fn a_title_naming_a_release_is_an_update() {
        for title in [
            "Hotfix #31 Now Live!",
            "1.6.4 Patch",
            "Stardew Valley 1.6.6 Patch Notes",
            "The Blood Price Update - Patch Notes",
            "Brace for the Long Winter Update",
            "Terraria: Journey's End Changelog",
            "Terraria 1.4.3.3 - Steam Deck Optimization Release Notes",
            "Patches galore",
            "Hotfixes for the weekend",
            "Patchnotes 4",
            "v1.11",
            "New Build -- 1.07",
            "Terraria 1.4.5.7 - Out Now for PC! (Console/Mobile Soon)",
            "Valheim 1.0 Has Arrived!",
            "Version 1.6.4b is here",
        ] {
            assert!(is_update(title, false), "{title}");
        }
    }

    #[test]
    fn a_title_that_promises_tests_or_sells_is_not_an_update() {
        for title in [
            "Patch 0.221.3 – Call To Arms (Public Test)",
            "Update 5 on the test server",
            "Update 5 on the test branch",
            "Patch 8 Stress Test Now Live",
            "Patch 8 playtest",
            "Stardew Valley Opt-In Multiplayer Beta now available",
            "Experimental update 3",
            "PTR patch notes",
            "Patch Delay Update",
            "Patch 7 delayed",
            "Patch delays",
            "A Sneak Peek into the upcoming 1.6 update",
            "Optimizing Liberty - Incoming Patch: 27th May",
            "Update 5 coming soon",
            "Patch preview",
            "Revealing our 2026 roadmap update",
            "1.6 Release Date Announced!",
            "Terraria 1.4.5 - the Launch Date Revealed at Last!",
            "Pre-Patch Announcement - Patch 9 Incoming!",
            "Not an Update",
            "Summer Sale update",
            "Discount and patch",
            "Free Weekend and update 2",
            "Bundle update",
            "Preorder the 1.5 update edition",
            "Preorders open for 1.5",
            "Pre-order the 1.5 edition",
            "Pre-orders for v2.0",
            "Merch update",
            "Merchandise 1.0",
            "Community Update #29 Playing With Mods",
            "State of the Game Update #1",
            "Development update 12",
            "Dev update: 1.2",
            "Progress update",
            "Status update on patch 3",
            "CS:GO Update History",
            "Sharing the Love - Terraria Update News!",
            "Patch 3 and 50% off",
        ] {
            assert!(!is_update(title, false), "{title}");
            assert!(!is_update(title, true), "{title}");
        }
        assert!(
            is_update("The Homecoming Update", false),
            "a word inside another is not that word"
        );
    }

    #[test]
    fn a_version_is_digits_either_side_of_a_dot_and_not_a_price_or_a_share() {
        for title in [
            "1.0",
            "v1.0",
            "V2.3",
            "01.003.302",
            "1.6.4b",
            "Patch: 7.1.0.",
            "(1.2)",
        ] {
            assert!(has_version(title), "{title}");
        }
        for title in [
            "1.", ".5", "1..2", "1.6.4bc", "1.6.4B", "40,000", "$9.99", "€9.99", "£9.99", "10.5%",
            "v.1", "vv1.2", "Dec. 20", "1", "",
        ] {
            assert!(!has_version(title), "{title}");
        }
    }

    fn post(gid: &str, title: &str, posted: i64) -> Announcement {
        Announcement {
            gid: gid.to_owned(),
            title: title.to_owned(),
            posted,
            patch_notes: false,
        }
    }

    #[test]
    fn an_update_posted_twice_within_a_week_is_one_update() {
        let kept = Announcements {
            asked: 0,
            posts: vec![
                post("1", "Patch 1.2", 0),
                post("2", "PATCH 1.2", REPOSTED_WITHIN),
                post("3", "Patch 1.2", REPOSTED_WITHIN + 1),
                post("4", "Patch 1.3", REPOSTED_WITHIN + 2),
                post("5", "Sale", REPOSTED_WITHIN + 3),
            ],
        };
        let updates = kept.updates();
        let ids: Vec<&str> = updates.iter().map(|u| u.gid.as_str()).collect();
        assert_eq!(ids, ["1", "3", "4"]);
        assert_eq!(updates[0].posted, 0);
        assert_eq!(updates[0].link, link("1"));
    }

    #[test]
    fn a_post_is_linked_through_the_store_by_its_id_alone() {
        assert_eq!(
            link("1844751498220713"),
            "https://store.steampowered.com/news/externalpost/steam_community_announcements/\
             1844751498220713"
        );
    }

    #[test]
    fn what_is_kept_is_read_back_and_nothing_kept_is_nothing() {
        let dir = crate::tempdir::Dir::new();
        assert_eq!(kept(dir.path(), 7), None);
        let game = dir.path().join("appid=7");
        std::fs::create_dir_all(&game).unwrap();
        let announcements = Announcements {
            asked: 5,
            posts: vec![post("1", "Patch 1.2", 3)],
        };
        announcements.save(&game).unwrap();
        assert_eq!(kept(dir.path(), 7), Some(announcements));
        assert!(!game.join(format!("{FILE}.partial")).exists());
        std::fs::write(game.join(FILE), b"{").unwrap();
        assert_eq!(kept(dir.path(), 7), None, "a torn file is nothing kept");
    }

    /// A page of `count` posts, newest first, the first posted at `newest` and each a second
    /// before the one above it.
    fn listing(count: usize, newest: i64, first_gid: u64) -> serde_json::Value {
        let items: Vec<serde_json::Value> = (0..u64::try_from(count).unwrap())
            .map(|step| {
                let at = i64::try_from(step).unwrap();
                serde_json::json!({
                    "gid": (first_gid + step).to_string(), "title": format!("Patch {at}"),
                    "date": newest - at, "feedname": FEED, "appid": 7,
                })
            })
            .collect();
        serde_json::json!({"appnews": {"appid": 7, "newsitems": items, "count": count}})
    }

    fn steam(
        answer: impl Fn(&stand_in::Asked) -> stand_in::Answer + Send + Sync + 'static,
    ) -> (stand_in::Server, SteamClient) {
        let server = stand_in::Server::new(answer);
        let client = SteamClient::new(std::time::Duration::ZERO)
            .unwrap()
            .with_api(&server.origin());
        (server, client)
    }

    #[tokio::test]
    async fn steam_is_asked_for_the_developers_posts_without_their_text() {
        let (server, client) = steam(|_| stand_in::Answer::json(&listing(3, 1_000, 1)));
        let fetched = fetch(&client, 7, 99).await.unwrap();
        assert_eq!(fetched.asked, 99);
        let posted: Vec<i64> = fetched.posts.iter().map(|post| post.posted).collect();
        assert_eq!(posted, [998, 999, 1_000], "oldest first");
        let asked = server.asked();
        assert_eq!(asked.len(), 1, "a page short of full is the whole history");
        assert_eq!(asked[0].path(), "/ISteamNews/GetNewsForApp/v2/");
        assert_eq!(asked[0].param("appid"), Some("7"));
        assert_eq!(asked[0].param("feeds"), Some(FEED));
        assert_eq!(asked[0].param("maxlength"), Some("1"));
        assert_eq!(asked[0].param("count"), Some("1000"));
        assert_eq!(asked[0].param("enddate"), None);
    }

    #[tokio::test]
    async fn a_full_page_is_followed_by_the_page_before_its_oldest_post() {
        let (server, client) = steam(|asked| {
            stand_in::Answer::json(&match asked.param("enddate") {
                None => listing(PER_PAGE as usize, 5_000, 1),
                // Steam answers from the time asked for, so the oldest post comes again.
                Some(at) => listing(3, at.parse().unwrap(), u64::from(PER_PAGE)),
            })
        });
        let fetched = fetch(&client, 7, 0).await.unwrap();
        let asked = server.asked();
        assert_eq!(asked.len(), 2);
        assert_eq!(asked[1].param("enddate"), Some("4001"));
        assert_eq!(fetched.posts.len(), PER_PAGE as usize + 2, "each post once");
        assert_eq!(fetched.posts[0].posted, 3_999);
    }

    #[tokio::test]
    async fn a_page_that_brings_nothing_new_ends_the_asking() {
        let (server, client) =
            steam(|_| stand_in::Answer::json(&listing(PER_PAGE as usize, 5_000, 1)));
        let fetched = fetch(&client, 7, 0).await.unwrap();
        assert_eq!(server.asked().len(), 2);
        assert_eq!(fetched.posts.len(), PER_PAGE as usize);
    }

    #[tokio::test]
    async fn an_answer_that_never_runs_out_is_asked_a_bounded_number_of_times() {
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let counting = std::sync::Arc::clone(&calls);
        let (server, client) = steam(move |_| {
            let page = counting.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let newest = 1_000_000 - i64::try_from(page).unwrap() * i64::from(PER_PAGE);
            stand_in::Answer::json(&listing(
                PER_PAGE as usize,
                newest,
                page * u64::from(PER_PAGE),
            ))
        });
        fetch(&client, 7, 0).await.unwrap();
        assert_eq!(server.asked().len(), PAGES);
    }

    #[tokio::test]
    async fn a_refresh_keeps_the_answer_and_a_refusal_keeps_what_was_kept() {
        let dir = crate::tempdir::Dir::new();
        let (_server, client) = steam(|_| stand_in::Answer::json(&listing(2, 1_000, 1)));
        let fresh = refresh(&client, 7, dir.path(), 50).await.unwrap();
        assert_eq!(kept(dir.path(), 7), Some(fresh.clone()));

        let (_server, refusing) = steam(|_| stand_in::Answer::status(403));
        assert!(refresh(&refusing, 7, dir.path(), 60).await.is_err());
        assert_eq!(kept(dir.path(), 7), Some(fresh));
    }
}
