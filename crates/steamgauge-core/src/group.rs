//! What players say across a group of games: each subject's figures over several read games,
//! counted two ways.
//!
//! **Each game the same.** A figure is the mean of the games' own figures, so a game with a
//! hundred times the reviews of another weighs the same; this answers what players of these
//! games talk about. Its interval is the sampling error within each game, for these games as
//! they are, not for games like them.
//!
//! **Every review the same.** The games' counts are added and divided once, so the game with the
//! most reviews decides most of each figure; its interval is Wilson's on the pooled counts.
//!
//! A subject also says in how many of the games at least one review in a hundred raises it, and
//! which game raises it most, so a figure one loud game makes is told from one every game shares.

use serde::Serialize;

use crate::{measure::wilson, read::SubjectCount};

/// A game's share of reviews at or above which it is said to raise a subject.
pub const RAISED_IN: f64 = 0.01;

/// Reviews raising a subject a game needs before its praise and complaint shares enter the mean
/// of each game's; fewer would let a handful of reviews weigh as much as a game's thousands.
pub const ENOUGH_TO_LEAN: u64 = 30;

const Z: f64 = 1.959_963_985;

/// One read game's counts, as the group adds them up.
#[derive(Debug, Clone, Copy)]
pub struct Counted<'a> {
    pub app_id: u32,
    /// Reviews counted, the denominator of every rate.
    pub reviews: u64,
    pub subjects: &'a [SubjectCount],
}

/// A subject's figures one way of counting.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Figures {
    /// The share of reviews raising the subject.
    pub rate: f64,
    /// The rate's 95% interval.
    pub low: f64,
    pub high: f64,
    /// Of the reviews raising it, the share praising it, mixed reviews included.
    pub praising: Option<f64>,
    /// Of the reviews raising it, the share complaining about it, mixed reviews included.
    pub complaining: Option<f64>,
    /// Of the reviews raising it, the share both praising and complaining about it.
    pub mixed: Option<f64>,
    /// The games the three shares come from: every game raising it, or, counting each game the
    /// same, those with [`ENOUGH_TO_LEAN`] reviews raising it.
    pub leaning_games: usize,
}

/// One game's part in a subject.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct GameShare {
    pub app_id: u32,
    pub raised: u64,
    pub reviews: u64,
    pub rate: f64,
    pub praising: Option<f64>,
    pub complaining: Option<f64>,
    pub mixed: Option<f64>,
}

/// One subject over the group.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Across {
    pub id: String,
    pub label: String,
    pub each_game: Figures,
    pub every_review: Figures,
    /// Games where at least [`RAISED_IN`] of the reviews raise it.
    pub raised_in: usize,
    /// The game raising it in the largest share of its reviews, where any does.
    pub most: Option<u32>,
    /// Each game's own figures, in the order the games were given.
    pub games: Vec<GameShare>,
}

#[expect(
    clippy::cast_precision_loss,
    reason = "review counts are far below 2^52"
)]
fn share(part: u64, whole: u64) -> Option<f64> {
    (whole > 0).then(|| part as f64 / whole as f64)
}

#[expect(clippy::cast_precision_loss, reason = "game counts are far below 2^52")]
fn mean(values: &[f64]) -> Option<f64> {
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

fn mean_of(games: &[&GameShare], figure: impl Fn(&GameShare) -> Option<f64>) -> Option<f64> {
    mean(
        &games
            .iter()
            .filter_map(|game| figure(game))
            .collect::<Vec<_>>(),
    )
}

fn game_share(game: &Counted, id: &str) -> GameShare {
    let found = game.subjects.iter().find(|subject| subject.id == id);
    let raised = found.map_or(0, |subject| subject.mention_reviews);
    let (praised, criticised, mixed) = found.map_or((0, 0, 0), |subject| {
        (subject.praised, subject.criticised, subject.mixed)
    });
    GameShare {
        app_id: game.app_id,
        raised,
        reviews: game.reviews,
        rate: share(raised, game.reviews).unwrap_or(0.0),
        praising: share(praised + mixed, raised),
        complaining: share(criticised + mixed, raised),
        mixed: share(mixed, raised),
    }
}

/// The mean of the games' rates, and its interval from each game's own sampling error.
#[expect(
    clippy::cast_precision_loss,
    reason = "review and game counts are far below 2^52"
)]
fn each_game(games: &[GameShare]) -> Figures {
    let count = games.len() as f64;
    let rate = mean(&games.iter().map(|game| game.rate).collect::<Vec<_>>()).unwrap_or(0.0);
    let variance = games
        .iter()
        .map(|game| game.rate * (1.0 - game.rate) / game.reviews as f64)
        .sum::<f64>()
        / (count * count);
    let spread = Z * variance.sqrt();
    let leaning: Vec<&GameShare> = games
        .iter()
        .filter(|game| game.raised >= ENOUGH_TO_LEAN)
        .collect();
    Figures {
        rate,
        low: (rate - spread).max(0.0),
        high: (rate + spread).min(1.0),
        praising: mean_of(&leaning, |game| game.praising),
        complaining: mean_of(&leaning, |game| game.complaining),
        mixed: mean_of(&leaning, |game| game.mixed),
        leaning_games: leaning.len(),
    }
}

/// The games' counts added and divided once.
fn every_review(games: &[Counted], id: &str, shares: &[GameShare]) -> Figures {
    let reviews: u64 = games.iter().map(|game| game.reviews).sum();
    let raised: u64 = shares.iter().map(|game| game.raised).sum();
    let (praised, criticised, mixed) = games
        .iter()
        .filter_map(|game| game.subjects.iter().find(|subject| subject.id == id))
        .fold((0, 0, 0), |(praised, criticised, mixed), subject| {
            (
                praised + subject.praised,
                criticised + subject.criticised,
                mixed + subject.mixed,
            )
        });
    let (low, high) = wilson(raised, reviews).unwrap_or((0.0, 0.0));
    Figures {
        rate: share(raised, reviews).unwrap_or(0.0),
        low,
        high,
        praising: share(praised + mixed, raised),
        complaining: share(criticised + mixed, raised),
        mixed: share(mixed, raised),
        leaning_games: shares.iter().filter(|game| game.raised > 0).count(),
    }
}

/// Every subject any of `games` counted, in the order the first game to count it lists it, over
/// the games with reviews; none where no game has any.
#[must_use]
pub fn across(games: &[Counted]) -> Vec<Across> {
    let games: Vec<Counted> = games
        .iter()
        .copied()
        .filter(|game| game.reviews > 0)
        .collect();
    let mut subjects: Vec<(&str, &str)> = Vec::new();
    for game in &games {
        for subject in game.subjects {
            if !subjects.iter().any(|(id, _)| *id == subject.id) {
                subjects.push((subject.id.as_str(), subject.label.as_str()));
            }
        }
    }
    subjects
        .into_iter()
        .map(|(id, label)| {
            let shares: Vec<GameShare> = games.iter().map(|game| game_share(game, id)).collect();
            let most = shares
                .iter()
                .filter(|game| game.rate > 0.0)
                .fold(None::<&GameShare>, |best, game| match best {
                    Some(best) if best.rate >= game.rate => Some(best),
                    _ => Some(game),
                })
                .map(|game| game.app_id);
            Across {
                id: id.to_owned(),
                label: label.to_owned(),
                each_game: each_game(&shares),
                every_review: every_review(&games, id, &shares),
                raised_in: shares.iter().filter(|game| game.rate >= RAISED_IN).count(),
                most,
                games: shares,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subject(id: &str, raised: u64, praised: u64, criticised: u64, mixed: u64) -> SubjectCount {
        SubjectCount {
            id: id.to_owned(),
            label: id.to_uppercase(),
            mention_reviews: raised,
            primary_reviews: 0,
            claims: raised,
            praised,
            criticised,
            mixed,
            top_mention_reviews: 0,
            positive_mentions: 0,
        }
    }

    fn close(left: f64, right: f64) -> bool {
        (left - right).abs() < 1e-9
    }

    /// A large game raising performance in a tenth of its reviews, and a small one in half.
    fn two() -> (Vec<SubjectCount>, Vec<SubjectCount>) {
        (
            vec![
                subject("performance", 10_000, 2_000, 7_000, 1_000),
                subject("story", 0, 0, 0, 0),
            ],
            vec![
                subject("performance", 500, 400, 50, 50),
                subject("audio", 20, 20, 0, 0),
            ],
        )
    }

    fn found(big: &[SubjectCount], small: &[SubjectCount]) -> Vec<Across> {
        across(&[
            Counted {
                app_id: 1,
                reviews: 100_000,
                subjects: big,
            },
            Counted {
                app_id: 2,
                reviews: 1_000,
                subjects: small,
            },
        ])
    }

    fn across_one(subjects: &[SubjectCount], reviews: u64) -> Across {
        across(&[Counted {
            app_id: 9,
            reviews,
            subjects,
        }])
        .remove(0)
    }

    #[test]
    fn each_game_weighs_the_same_and_every_review_lets_the_large_game_decide() {
        let (big, small) = two();
        let performance = found(&big, &small).remove(0);
        assert_eq!(performance.id, "performance");
        assert!(close(performance.each_game.rate, f64::midpoint(0.1, 0.5)));
        assert!(close(performance.every_review.rate, 10_500.0 / 101_000.0));
        assert!(close(
            performance.each_game.praising.unwrap(),
            f64::midpoint(0.3, 0.9)
        ));
        assert!(close(
            performance.each_game.complaining.unwrap(),
            f64::midpoint(0.8, 0.2)
        ));
        assert!(close(
            performance.every_review.praising.unwrap(),
            3_450.0 / 10_500.0
        ));
        assert!(close(
            performance.every_review.complaining.unwrap(),
            8_100.0 / 10_500.0
        ));
        assert!(close(
            performance.every_review.mixed.unwrap(),
            1_050.0 / 10_500.0
        ));
        let lopsided = [subject("performance", 100, 0, 0, 100)];
        let even = [subject("performance", 100, 0, 0, 0)];
        assert!(close(
            found(&lopsided, &even)[0].each_game.mixed.unwrap(),
            0.5
        ));
        assert_eq!(
            (
                performance.each_game.leaning_games,
                performance.every_review.leaning_games
            ),
            (2, 2)
        );
    }

    #[test]
    fn a_subject_no_review_raised_leans_no_way_in_any_game() {
        let (big, small) = two();
        let story = found(&big, &small)
            .into_iter()
            .find(|one| one.id == "story")
            .unwrap();
        for figures in [&story.each_game, &story.every_review] {
            assert_eq!(figures.leaning_games, 0);
            assert_eq!(
                (figures.praising, figures.complaining, figures.mixed),
                (None, None, None)
            );
        }
        // Twenty reviews raise it in the small game, under the thirty a game's own lean needs.
        let audio = found(&big, &small)
            .into_iter()
            .find(|one| one.id == "audio")
            .unwrap();
        assert_eq!(
            (
                audio.each_game.leaning_games,
                audio.every_review.leaning_games
            ),
            (0, 1)
        );
    }

    #[test]
    fn each_interval_holds_its_rate() {
        let (big, small) = two();
        let performance = found(&big, &small).remove(0);
        let (low, high) = wilson(10_500, 101_000).unwrap();
        assert!(close(performance.every_review.low, low));
        assert!(close(performance.every_review.high, high));
        let spread = Z * ((0.1 * 0.9 / 100_000.0 + 0.5 * 0.5 / 1_000.0) / 4.0_f64).sqrt();
        assert!(close(performance.each_game.low, 0.3 - spread));
        assert!(close(performance.each_game.high, 0.3 + spread));
    }

    #[test]
    fn the_interval_of_each_games_mean_stays_within_nought_and_one() {
        let all = [subject("verdict", 1_000, 1_000, 0, 0)];
        let every = across_one(&all, 1_000).each_game;
        assert!(close(every.rate, 1.0) && close(every.low, 1.0) && close(every.high, 1.0));
        let one = [subject("verdict", 1, 1, 0, 0)];
        let few = across_one(&one, 2).each_game;
        assert!(close(few.low, 0.0) && close(few.high, 1.0), "{few:?}");
        let none = [subject("verdict", 0, 0, 0, 0)];
        let silent = across_one(&none, 1_000).each_game;
        assert!(close(silent.rate, 0.0) && close(silent.low, 0.0) && close(silent.high, 0.0));
        assert_eq!((silent.praising, silent.leaning_games), (None, 0));
    }

    #[test]
    fn a_game_with_few_reviews_raising_a_subject_leaves_its_lean_out_of_each_games_mean() {
        let (big, small) = two();
        let audio = found(&big, &small)
            .into_iter()
            .find(|subject| subject.id == "audio")
            .unwrap();
        assert_eq!(
            audio.each_game.praising, None,
            "20 reviews is under the floor"
        );
        assert_eq!(audio.each_game.complaining, None);
        assert_eq!(audio.each_game.leaning_games, 0);
        assert_eq!(audio.every_review.praising, Some(1.0));
        let enough = [subject("audio", ENOUGH_TO_LEAN, ENOUGH_TO_LEAN, 0, 0)];
        let one = across_one(&enough, 1_000);
        assert_eq!(one.each_game.praising, Some(1.0));
        assert_eq!(one.each_game.complaining, Some(0.0));
        assert_eq!(one.each_game.leaning_games, 1);
    }

    #[test]
    fn a_subject_says_how_many_games_raise_it_and_which_raises_it_most() {
        let (big, small) = two();
        let found = found(&big, &small);
        let ids: Vec<&str> = found.iter().map(|subject| subject.id.as_str()).collect();
        assert_eq!(ids, ["performance", "story", "audio"]);
        assert_eq!((found[0].raised_in, found[0].most), (2, Some(2)));
        assert_eq!((found[1].raised_in, found[1].most), (0, None));
        assert_eq!(
            (found[2].raised_in, found[2].most),
            (1, Some(2)),
            "a game without the subject counts it as never raised"
        );
        let at_the_line = [subject("story", 10, 0, 0, 0)];
        assert_eq!(across_one(&at_the_line, 1_000).raised_in, 1);
        let under = [subject("story", 9, 0, 0, 0)];
        assert_eq!(across_one(&under, 1_000).raised_in, 0);
    }

    #[test]
    fn of_two_games_raising_a_subject_equally_the_first_is_named() {
        let first = [subject("story", 100, 0, 0, 0)];
        let second = [subject("story", 100, 0, 0, 0)];
        let louder = [subject("story", 101, 0, 0, 0)];
        let named = |later: &[SubjectCount]| {
            across(&[
                Counted {
                    app_id: 1,
                    reviews: 1_000,
                    subjects: &first,
                },
                Counted {
                    app_id: 2,
                    reviews: 1_000,
                    subjects: later,
                },
            ])[0]
                .most
        };
        assert_eq!(named(&second), Some(1));
        assert_eq!(named(&louder), Some(2));
    }

    #[test]
    fn each_games_own_figures_are_kept_in_order() {
        let (big, small) = two();
        let performance = found(&big, &small).remove(0);
        assert_eq!(
            performance.games,
            [
                GameShare {
                    app_id: 1,
                    raised: 10_000,
                    reviews: 100_000,
                    rate: 0.1,
                    praising: Some(0.3),
                    complaining: Some(0.8),
                    mixed: Some(0.1),
                },
                GameShare {
                    app_id: 2,
                    raised: 500,
                    reviews: 1_000,
                    rate: 0.5,
                    praising: Some(0.9),
                    complaining: Some(0.2),
                    mixed: Some(0.1),
                },
            ]
        );
    }

    #[test]
    fn a_game_without_reviews_is_left_out_and_no_games_give_nothing() {
        let (big, _) = two();
        let empty = [subject("performance", 0, 0, 0, 0)];
        let found = across(&[
            Counted {
                app_id: 1,
                reviews: 100_000,
                subjects: &big,
            },
            Counted {
                app_id: 2,
                reviews: 0,
                subjects: &empty,
            },
        ]);
        assert_eq!(found[0].games.len(), 1);
        assert!(close(found[0].each_game.rate, 0.1));
        assert_eq!(across(&[]), Vec::<Across>::new());
    }
}
