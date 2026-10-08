//! What moved since the person last looked, and the notification when an update the app made by
//! itself finds a subject that moved.
//!
//! The looks are kept in the library beside its other records (`last-seen.json`, written whole
//! by the core), because they describe the library's games and follow the library wherever it is
//! kept. What the window compares with is the record as it stood when the app opened: a look in
//! this visit is the next visit's starting point, and the card keeps saying what moved while it
//! is being read.

use std::sync::{Mutex, PoisonError};

use serde::Serialize;
use steamgauge_core::{
    moves::{Move, Side},
    read::ReadReport,
    since::{self, LastSeen, Look, Seen, Since, Standing},
};
use tauri::{AppHandle, Emitter, Manager};

use super::{cockpit::Row, library_dir, now_unix, read_report, settings::Settings, text};

/// The looks as they stood when the app opened.
#[derive(Default)]
pub struct Baseline(Mutex<Option<LastSeen>>);

/// Held while the record is read, changed and written back, so two looks at once cannot lose one.
static WRITING: Mutex<()> = Mutex::new(());

/// Reads the record before anything can look, so this visit compares with the last one.
pub fn remember(app: &AppHandle) {
    let _ = baseline(app);
}

fn baseline(app: &AppHandle) -> LastSeen {
    let state = app.state::<Baseline>();
    let mut held = state.0.lock().unwrap_or_else(PoisonError::into_inner);
    held.get_or_insert_with(|| LastSeen::load(&library_dir(app)))
        .clone()
}

fn reading_of(dir: &std::path::Path, app_id: u32) -> Option<ReadReport> {
    steamgauge_core::embed::latest_snapshot(dir, app_id)
        .ok()
        .and_then(|snapshot| read_report(&snapshot).ok())
}

/// One game's change since the cockpit was last seen.
#[derive(Debug, Clone, Serialize)]
pub struct GameSince {
    pub app_id: u32,
    pub name: String,
    #[serde(flatten)]
    pub since: Since,
}

/// What the cockpit's first card shows: when the person last looked, or never, and every game
/// with something to say since.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Lately {
    pub looked: Option<i64>,
    pub games: Vec<GameSince>,
}

/// How clearly anything in a game moved, so the clearest comes first.
fn clearest(since: &Since) -> f64 {
    since
        .moves
        .iter()
        .map(|moved| moved.shift.z.abs())
        .chain(since.recommended.map(|shift| shift.z.abs()))
        .fold(0.0, f64::max)
}

/// Every game the look saw that has something to say since: the ones that moved first, the
/// clearest first, then the most new reviews first. A game added since the look was added by
/// the person, and is not news to them.
fn since_each<'a>(
    look: &Look,
    games: impl Iterator<Item = (u32, &'a str, u64, Option<&'a ReadReport>)>,
) -> Vec<GameSince> {
    let mut found: Vec<GameSince> = games
        .filter_map(|(app_id, name, held, reading)| {
            let since = since::since(look.games.get(&app_id)?, held, reading);
            (since.standing != Standing::Nothing).then(|| GameSince {
                app_id,
                name: name.to_owned(),
                since,
            })
        })
        .collect();
    found.sort_by(|left, right| {
        clearest(&right.since)
            .total_cmp(&clearest(&left.since))
            .then(right.since.new.cmp(&left.since.new))
    });
    found
}

pub fn lately(app: &AppHandle, rows: &[Row]) -> Lately {
    let Some(look) = baseline(app).cockpit else {
        return Lately::default();
    };
    Lately {
        looked: Some(look.at),
        games: since_each(
            &look,
            rows.iter().map(|row| {
                (
                    row.app_id,
                    row.name.as_str(),
                    row.reviews,
                    row.report.as_ref(),
                )
            }),
        ),
    }
}

/// The person has looked at the cockpit, where no game is named, or at one game's page.
#[tauri::command]
pub async fn looked(app: AppHandle, app_id: Option<u32>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        remember(&app);
        let dir = library_dir(&app);
        let at = now_unix();
        let seen = |app_id: u32, held: u64| Seen::of(at, held, reading_of(&dir, app_id).as_ref());
        let _writing = WRITING.lock().unwrap_or_else(PoisonError::into_inner);
        let mut looks = LastSeen::load(&dir);
        if let Some(app_id) = app_id {
            let facts = steamgauge_core::report::crawl_facts(&dir, app_id).map_err(text)?;
            looks.page_seen(app_id, seen(app_id, facts.rows_unique));
        } else {
            let games = super::shelf(&dir)
                .games
                .into_iter()
                .map(|game| (game.app_id, seen(game.app_id, game.reviews)))
                .collect();
            looks.cockpit_seen(Look { at, games });
        }
        std::fs::create_dir_all(&dir).map_err(text)?;
        looks.save(&dir).map_err(text)
    })
    .await
    .map_err(text)?
}

/// What moved in one game since its page was last seen, or where it never was, since the cockpit
/// last showed it; nothing where neither has.
#[tauri::command]
pub async fn since_last_look(app: AppHandle, app_id: u32) -> Result<Option<Since>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let looks = baseline(&app);
        let Some(seen) = looks.page(app_id) else {
            return Ok(None);
        };
        let dir = library_dir(&app);
        let facts = steamgauge_core::report::crawl_facts(&dir, app_id).map_err(text)?;
        Ok(Some(since::since(
            seen,
            facts.rows_unique,
            reading_of(&dir, app_id).as_ref(),
        )))
    })
    .await
    .map_err(text)?
}

/// A game removed from the library leaves no look behind.
pub fn forget(dir: &std::path::Path, app_id: u32) -> Result<(), String> {
    let _writing = WRITING.lock().unwrap_or_else(PoisonError::into_inner);
    let mut looks = LastSeen::load(dir);
    if looks.latest(app_id).is_none() && !looks.told.contains_key(&app_id) {
        return Ok(());
    }
    looks.forget(app_id);
    looks.save(dir).map_err(text)
}

/// A subject's name in the middle of a sentence: lower case, except a word that is an acronym.
fn mid_sentence(label: &str) -> String {
    if label == "Overall verdict" {
        return "the game as a whole".to_owned();
    }
    label
        .split(' ')
        .map(|word| {
            if word.len() > 1 && word.chars().all(|c| c.is_ascii_uppercase()) {
                word.to_owned()
            } else {
                word.to_lowercase()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn percent(share: f64) -> String {
    format!("{:.0}%", share * 100.0)
}

/// What a notification says about one subject that moved in a game's new reviews.
fn words(game: &str, moved: &Move, read: u64) -> (String, String) {
    let subject = mid_sentence(moved.label);
    let more = moved.shift.recent > moved.shift.before;
    let (title, doing) = match (moved.side, more) {
        (Side::Praise, true) => (format!("more praise for {subject}"), "praise"),
        (Side::Praise, false) => (format!("less praise for {subject}"), "praise"),
        (Side::Complaint, true) => (format!("more complaints about {subject}"), "complain about"),
        (Side::Complaint, false) => (
            format!("fewer complaints about {subject}"),
            "complain about",
        ),
    };
    (
        format!("{game}: {title}"),
        format!(
            "{} of the {} reviews read since you last looked {doing} {subject}, against {} in the \
             year before.",
            percent(moved.shift.recent),
            super::work::thousands(read),
            percent(moved.shift.before),
        ),
    )
}

/// After an update the app made by itself has been read, one notification naming the game and
/// the clearest subject that moved since the person last looked, where they asked for that and no
/// notification has named that subject since.
pub fn after_background_read(app: &AppHandle, app_id: u32) {
    if !Settings::load(app).notify_moves {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let dir = library_dir(&app);
        let Ok(facts) = steamgauge_core::report::crawl_facts(&dir, app_id) else {
            return;
        };
        let Some(reading) = reading_of(&dir, app_id) else {
            return;
        };
        let (moved, read) = {
            let _writing = WRITING.lock().unwrap_or_else(PoisonError::into_inner);
            let mut looks = LastSeen::load(&dir);
            let Some(seen) = looks.latest(app_id).cloned() else {
                return;
            };
            let found = since::since(&seen, facts.rows_unique, Some(&reading));
            let Some(moved) = found.moves.into_iter().find(|moved| {
                let side = match moved.side {
                    Side::Praise => "praise",
                    Side::Complaint => "complaint",
                };
                looks.tell(app_id, &format!("{}:{side}", moved.subject))
            }) else {
                return;
            };
            // Unremembered, the same notification would come again after every update.
            if looks.save(&dir).is_err() {
                return;
            }
            (moved, found.read_new)
        };
        let (title, body) = words(&facts.title(), &moved, read);
        notify(&app, app_id, &title, &body);
    });
}

/// Shows a desktop notification that opens the game when clicked. It waits for the click on a
/// thread of its own, since each system answers on its own time or not at all.
fn notify(app: &AppHandle, app_id: u32, title: &str, body: &str) {
    let mut note = notify_rust::Notification::new();
    note.summary(title).body(body);
    #[cfg(windows)]
    note.app_id(&app.config().identifier);
    #[cfg(target_os = "macos")]
    let _ = notify_rust::set_application(&app.config().identifier);
    #[cfg(all(unix, not(target_os = "macos")))]
    note.appname("SteamGauge")
        .action("default", "Open the game");
    let app = app.clone();
    std::thread::spawn(move || {
        let Ok(shown) = note.show() else {
            return;
        };
        let _ = shown.wait_for_response(|response: &notify_rust::NotificationResponse| {
            if response.is_default_action() {
                open_game(&app, app_id);
            }
        });
    });
}

/// Brings the window forward on a game's page.
fn open_game(app: &AppHandle, app_id: u32) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
    let _ = app.emit("open-game", app_id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use steamgauge_core::moves::Shift;

    fn shift(before: f64, recent: f64, z: f64) -> Shift {
        Shift {
            before,
            recent,
            before_reviews: 1_200,
            recent_reviews: 400,
            z,
        }
    }

    fn moved(side: Side, label: &'static str, shift: Shift) -> Move {
        Move {
            subject: "bugs",
            label,
            side,
            shift,
        }
    }

    #[test]
    fn a_notification_names_the_game_the_subject_and_which_way_it_went() {
        let (title, body) = words(
            "Alpha",
            &moved(Side::Complaint, "Bugs and crashes", shift(0.05, 0.25, 11.5)),
            1_400,
        );
        assert_eq!(title, "Alpha: more complaints about bugs and crashes");
        assert_eq!(
            body,
            "25% of the 1,400 reviews read since you last looked complain about bugs and \
             crashes, against 5% in the year before."
        );
        let (title, _) = words(
            "Alpha",
            &moved(Side::Praise, "VR and headsets", shift(0.3, 0.1, -6.0)),
            400,
        );
        assert_eq!(title, "Alpha: less praise for VR and headsets");
        let (title, _) = words(
            "Alpha",
            &moved(Side::Complaint, "Overall verdict", shift(0.3, 0.1, -6.0)),
            400,
        );
        assert_eq!(title, "Alpha: fewer complaints about the game as a whole");
        let (title, body) = words(
            "Alpha",
            &moved(Side::Praise, "Monetisation and DLC", shift(0.1, 0.3, 6.0)),
            400,
        );
        assert_eq!(title, "Alpha: more praise for monetisation and DLC");
        assert!(body.contains(" praise monetisation and DLC, against 10% "));
    }

    fn look_at(at: i64, held: u64) -> Look {
        Look {
            at,
            games: BTreeMap::from([
                (1, Seen::of(at, held, None)),
                (2, Seen::of(at, held, None)),
                (3, Seen::of(at, held, None)),
            ]),
        }
    }

    #[test]
    fn the_cockpit_lists_games_with_something_to_say_and_the_most_new_first() {
        let look = look_at(10, 1_000);
        let found = since_each(
            &look,
            [
                (1, "Alpha", 1_050, None),
                (2, "Beta", 1_000, None),
                (3, "Gamma", 1_400, None),
                (4, "Added since", 9_000, None),
            ]
            .into_iter(),
        );
        let names: Vec<&str> = found.iter().map(|game| game.name.as_str()).collect();
        assert_eq!(names, ["Gamma", "Alpha"]);
        assert_eq!(found[0].since.new, 400);
        assert_eq!(found[0].since.standing, Standing::Unread);
    }

    #[test]
    fn a_game_that_moved_comes_before_one_that_only_grew() {
        let quiet = Since {
            looked: 1,
            new: 900,
            read_new: 900,
            standing: Standing::Compared,
            from: None,
            to: None,
            recommended: None,
            moves: Vec::new(),
        };
        let moved_some = Since {
            new: 200,
            moves: vec![moved(Side::Complaint, "Bugs", shift(0.05, 0.25, -7.0))],
            ..quiet.clone()
        };
        let moved_more = Since {
            new: 100,
            recommended: Some(shift(0.8, 0.5, -9.0)),
            ..quiet.clone()
        };
        assert!((clearest(&quiet)).abs() < f64::EPSILON);
        assert!((clearest(&moved_some) - 7.0).abs() < f64::EPSILON);
        assert!((clearest(&moved_more) - 9.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_subject_reads_in_the_middle_of_a_sentence() {
        assert_eq!(mid_sentence("Story and writing"), "story and writing");
        assert_eq!(mid_sentence("VR and headsets"), "VR and headsets");
        assert_eq!(mid_sentence("Overall verdict"), "the game as a whole");
        assert_eq!(percent(0.254), "25%");
    }
}
