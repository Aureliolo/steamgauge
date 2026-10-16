//! What players say within a subject: the claims on one side of one subject, grouped by what
//! they mean, so a page can name the particular things praised or complained about rather than
//! only the subjects.
//!
//! A claim counts towards a subject on a side where the reading filed it there, first or beside
//! another subject. Its vector is the search encoder's, from the game's preparation for search by
//! meaning, so finding themes runs no model of its own. Claims that say the same thing in other
//! words lie close together: a theme starts from the claim with the most others alike it and
//! takes them, the next starts from the most crowded claim left, and two whose centres nearly
//! coincide are then one (see [`Closeness`]). Overall verdicts and remarks about something else
//! are left out, as the summary leaves them out, and so is a theme that only lines of review
//! templates say ("🐛 Bugs: ✅ Perfect"), which has nothing to quote.
//!
//! A subject with more distinct claims than [`SAMPLE`] is grouped on a sample of them, taken by
//! key, which is a hash and so as good as random. Every claim is then counted under the theme
//! whose centre it is nearest, where it is alike that centre, so a theme's count is of all the
//! game's reviews and not of the sample.

use std::{
    cmp::Reverse,
    collections::{HashMap, HashSet},
    path::Path,
};

use serde::{Deserialize, Serialize};

use crate::{
    Result, claims::Span, meaning, reader::Polarity, search_models::ENCODER, taxonomy::SHEET,
};

/// The lines at which closeness of meaning is called sameness: [`Closeness::USED`] everywhere
/// but where they are being tuned.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Closeness {
    /// Two claims say the same thing in other words.
    pub alike: f32,
    /// Two groups of the first pass are one point it split, as it splits a point said in many
    /// ways: each group takes only what is near its own starting claim. Broad praise and broad
    /// complaint sit near each other, so this is set well above `alike`, or a game's particular
    /// faults would merge into one "it is broken".
    pub merged: f32,
    /// Two themes under two subjects are the same point said in different claims. Set high for
    /// the same reason as `merged`, or every complaint would run through every subject.
    pub shared: f32,
}

impl Closeness {
    pub const USED: Self = Self {
        alike: 0.75,
        merged: 0.9,
        shared: 0.9,
    };
}

/// Distinct claims grouped for one subject on one side; past this a sample is.
pub const SAMPLE: usize = 3_000;

/// Themes kept for one subject on one side, the most widely said first.
pub const THEMES_KEPT: usize = 8;

/// Reviews a theme needs before it is shown: fewer is a handful of people, not a theme.
pub const FEWEST_REVIEWS: u64 = 5;

/// The share of the smaller of two themes' claims they hold in common when they are the same
/// point under two subjects: claims the reading filed under both.
pub const OVERLAP: f64 = 0.5;

/// Members a group of the sample needs before it is a theme at all.
const FEWEST_MEMBERS: usize = 3;

/// The longest a quote runs before it is cut at a word.
const QUOTE_MOST: usize = 240;

/// Subjects with nothing particular inside them to find.
const NOT_A_SUBJECT: [&str; 2] = ["verdict", "offtopic"];

const FILE: &str = "themes.json";

/// Threads the pairwise comparison runs on, so a large subject takes a few cores, not all.
const THREADS: usize = 4;

/// Every subject's themes on each side, for one reading.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Themes {
    /// The encoder whose vectors these were grouped by.
    pub encoder: String,
    /// When the readings file grouped was last written, in seconds since the epoch.
    pub readings: Option<u64>,
    pub subjects: Vec<SubjectThemes>,
}

/// One subject on one side.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubjectThemes {
    pub subject: String,
    /// "praise" or "complaint".
    pub side: String,
    /// Reviews with at least one claim on this side of the subject.
    pub reviews: u64,
    pub themes: Vec<Theme>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Theme {
    /// A short claim near the theme's centre, as a reviewer put it.
    pub label: String,
    /// The claim nearest the theme's centre, whole or cut at a word.
    pub quote: Quote,
    /// Reviews saying it at least once.
    pub reviews: u64,
    /// Claims saying it, repeats included.
    pub claims: u64,
    /// The themes under other subjects on this side that say the same, most strongly first.
    pub also_in: Vec<Elsewhere>,
    /// Every claim counted under it, by review and span, so its reviews can be opened.
    pub members: Vec<(String, u32, u32)>,
}

/// One theme in a list across subjects: the subject, its place in that subject's list, and it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ranked<'a> {
    pub subject: &'a str,
    pub theme: usize,
    pub said: &'a Theme,
}

/// A theme under another subject, by the subject and its place in that subject's list.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Elsewhere {
    pub subject: String,
    pub theme: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Quote {
    pub review_id: String,
    pub at: Span,
    pub text: String,
}

impl Themes {
    /// The themes found for the reading at `snapshot`, where they were found for it as it now
    /// is, by the encoder in use.
    #[must_use]
    pub fn load(snapshot: &Path) -> Option<Self> {
        let themes: Self =
            serde_json::from_slice(&std::fs::read(snapshot.join(FILE)).ok()?).ok()?;
        (themes.encoder == ENCODER.name && themes.readings == meaning::readings_written(snapshot))
            .then_some(themes)
    }

    /// Every theme on one side across subjects, the most widely said first, with a point said
    /// under several subjects listed once, where the most reviews say it.
    #[must_use]
    pub fn ranked(&self, side: &str) -> Vec<Ranked<'_>> {
        let mut all: Vec<Ranked<'_>> = self
            .subjects
            .iter()
            .filter(|subject| subject.side == side)
            .flat_map(|subject| {
                subject
                    .themes
                    .iter()
                    .enumerate()
                    .map(move |(theme, said)| Ranked {
                        subject: &subject.subject,
                        theme,
                        said,
                    })
            })
            .collect();
        all.sort_by_key(|one| Reverse(one.said.reviews));
        let names = |one: &Ranked<'_>, other: &Ranked<'_>| {
            one.said
                .also_in
                .iter()
                .any(|place| place.subject == other.subject && place.theme == other.theme)
        };
        let mut kept: Vec<Ranked<'_>> = Vec::new();
        for one in all {
            if !kept
                .iter()
                .any(|held| names(held, &one) || names(&one, held))
            {
                kept.push(one);
            }
        }
        kept
    }

    /// Written beside its name and moved over it, so the file is never half one finding.
    ///
    /// # Errors
    ///
    /// Fails if the file cannot be written.
    pub fn save(&self, snapshot: &Path) -> Result<()> {
        let partial = snapshot.join(format!("{FILE}.partial"));
        std::fs::write(&partial, serde_json::to_vec(self)?)?;
        std::fs::rename(partial, snapshot.join(FILE))?;
        Ok(())
    }
}

const fn side_of(polarity: Polarity) -> Option<usize> {
    match polarity {
        Polarity::Praise => Some(0),
        Polarity::Complaint => Some(1),
        Polarity::Neutral => None,
    }
}

const SIDES: [&str; 2] = ["praise", "complaint"];

/// One subject on one side while it is gathered: each distinct claim, with where it was said.
#[derive(Default)]
struct Gathered {
    claims: HashMap<[u8; 32], Vec<(u32, Span)>>,
}

/// A distinct claim's vector, as the preparation kept it.
struct Point {
    bytes: Vec<i8>,
    /// The length of `bytes` as a vector, which is what a dot product is divided by.
    length: f32,
}

impl Point {
    #[expect(
        clippy::cast_precision_loss,
        reason = "a sum of squared bytes over a thousand dimensions is far below 2^24"
    )]
    fn new(bytes: &[i8]) -> Self {
        let squares: i32 = bytes.iter().map(|&b| i32::from(b) * i32::from(b)).sum();
        Self {
            bytes: bytes.to_vec(),
            length: (squares as f32).sqrt().max(f32::MIN_POSITIVE),
        }
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "a dot product of bytes over a thousand dimensions is far below 2^24"
    )]
    fn similarity(&self, other: &Self) -> f32 {
        let dot: i32 = self
            .bytes
            .iter()
            .zip(&other.bytes)
            .map(|(&a, &b)| i32::from(a) * i32::from(b))
            .sum();
        dot as f32 / (self.length * other.length)
    }

    fn towards(&self, centre: &[f32]) -> f32 {
        self.bytes
            .iter()
            .zip(centre)
            .map(|(&byte, &value)| f32::from(byte) * value)
            .sum::<f32>()
            / self.length
    }
}

/// For each point, the others within `alike` of it.
fn neighbours(points: &[Point], alike: f32) -> Vec<Vec<usize>> {
    let chunk = points.len().div_ceil(THREADS).max(1);
    let mut found: Vec<Vec<usize>> = vec![Vec::new(); points.len()];
    std::thread::scope(|scope| {
        for (at, rows) in found.chunks_mut(chunk).enumerate() {
            scope.spawn(move || {
                for (offset, row) in rows.iter_mut().enumerate() {
                    let this = at * chunk + offset;
                    *row = points
                        .iter()
                        .enumerate()
                        .filter(|&(other, point)| {
                            other != this && points[this].similarity(point) >= alike
                        })
                        .map(|(other, _)| other)
                        .collect();
                }
            });
        }
    });
    found
}

/// Groups of points that say the same thing: the most crowded point with its unclaimed
/// neighbours, then the most crowded left, while a group would have [`FEWEST_MEMBERS`].
fn communities(points: &[Point], alike: f32) -> Vec<Vec<usize>> {
    let near = neighbours(points, alike);
    let mut order: Vec<usize> = (0..points.len()).collect();
    order.sort_by_key(|&point| (std::cmp::Reverse(near[point].len()), point));
    let mut taken = vec![false; points.len()];
    let mut found = Vec::new();
    for seed in order {
        if taken[seed] || near[seed].len() + 1 < FEWEST_MEMBERS {
            continue;
        }
        let members: Vec<usize> = std::iter::once(seed)
            .chain(near[seed].iter().copied().filter(|&other| !taken[other]))
            .collect();
        if members.len() < FEWEST_MEMBERS {
            continue;
        }
        for &member in &members {
            taken[member] = true;
        }
        found.push(members);
    }
    found
}

/// The groups again, with any two whose centres are within `merged` made one, the closest pair
/// first, until no pair is.
fn merged(points: &[Point], mut groups: Vec<Vec<usize>>, merged: f32) -> Vec<Vec<usize>> {
    loop {
        let centres: Vec<Vec<f32>> = groups.iter().map(|group| centre(points, group)).collect();
        let mut closest: Option<(f32, usize, usize)> = None;
        for one in 0..groups.len() {
            for other in one + 1..groups.len() {
                let closeness = dot(&centres[one], &centres[other]);
                if closeness >= merged && closest.is_none_or(|(best, _, _)| closeness > best) {
                    closest = Some((closeness, one, other));
                }
            }
        }
        let Some((_, one, other)) = closest else {
            return groups;
        };
        let taken = groups.remove(other);
        groups[one].extend(taken);
    }
}

/// Whether a claim reads as a sentence someone wrote, rather than a line of a review template
/// ("🐛 Bugs: ✅ Perfect"): a few words, and mostly letters.
fn reads_as_words(text: &str) -> bool {
    let letters = text.chars().filter(|c| c.is_alphabetic()).count();
    let shown = text.chars().filter(|c| !c.is_whitespace()).count();
    text.split_whitespace().count() >= 3 && letters * 10 >= shown * 8
}

/// The unit vector the members point along together.
fn centre(points: &[Point], members: &[usize]) -> Vec<f32> {
    let width = points.first().map_or(0, |point| point.bytes.len());
    let mut sum = vec![0.0_f32; width];
    for &member in members {
        let point = &points[member];
        for (total, &byte) in sum.iter_mut().zip(&point.bytes) {
            *total += f32::from(byte) / point.length;
        }
    }
    let length = sum.iter().map(|value| value * value).sum::<f32>().sqrt();
    if length > 0.0 {
        for value in &mut sum {
            *value /= length;
        }
    }
    sum
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// A quote no longer than [`QUOTE_MOST`], cut at a word where it is longer.
fn cut(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= QUOTE_MOST {
        return text.to_owned();
    }
    let end = text
        .char_indices()
        .nth(QUOTE_MOST)
        .map_or(text.len(), |(at, _)| at);
    let kept = text[..end]
        .rfind(char::is_whitespace)
        .map_or(&text[..end], |space| &text[..space]);
    format!("{}…", kept.trim_end())
}

/// A theme while its counts are gathered.
struct Found {
    group: usize,
    centre: Vec<f32>,
    label: [u8; 32],
    quote: [u8; 32],
    keys: Vec<[u8; 32]>,
}

/// Finds the themes of the reading at `snapshot` from its prepared vectors.
///
/// # Errors
///
/// Fails if the reading, the capture or the vectors cannot be read.
pub fn find(snapshot: &Path, closeness: Closeness) -> Result<Themes> {
    let said = Said::gather(snapshot)?;
    let found = said.grouped(snapshot, closeness)?;
    let mut themes = said.kept(&found);
    Said::link(&mut themes, &found, closeness.shared);
    Ok(Themes {
        encoder: ENCODER.name.to_owned(),
        readings: meaning::readings_written(snapshot),
        subjects: said.subjects(themes),
    })
}

/// What a reading says, gathered for grouping: each subject on each side with where its claims
/// were said, the sample of each that is grouped, and the sample's text.
struct Said {
    gathered: Vec<Gathered>,
    reviews: Vec<String>,
    samples: Vec<Vec<[u8; 32]>>,
    texts: HashMap<[u8; 32], String>,
}

/// The themes kept for each subject on each side, by group, each beside its place in the found.
type Kept = Vec<Vec<(Theme, usize)>>;

impl Said {
    fn gather(snapshot: &Path) -> Result<Self> {
        let subjects = SHEET.len();
        let left_out: Vec<bool> = SHEET
            .iter()
            .map(|row| NOT_A_SUBJECT.contains(&row.id))
            .collect();

        // Each claim, by review, with the subjects and sides it counts towards.
        let mut filed: HashMap<String, Vec<(Span, Vec<usize>)>> = HashMap::new();
        crate::read::for_each_full_reading(
            &snapshot.join("readings.parquet"),
            |id, at, subject, _, polarity, also| {
                let mut into = Vec::new();
                if let Some(first) =
                    subject.and_then(|name| SHEET.iter().position(|row| row.id == name))
                    && let Some(side) = side_of(Polarity::from_name(polarity))
                    && !left_out[first]
                {
                    into.push(first * 2 + side);
                }
                for (other, said) in also.iter() {
                    if let Some(side) = side_of(said)
                        && other < subjects
                        && !left_out[other]
                    {
                        into.push(other * 2 + side);
                    }
                }
                if !into.is_empty() {
                    filed.entry(id.to_owned()).or_default().push((at, into));
                }
            },
        )?;

        let mut gathered: Vec<Gathered> = (0..subjects * 2).map(|_| Gathered::default()).collect();
        let mut reviews: Vec<String> = Vec::new();
        crate::capture::for_each_row(snapshot, |row, text| {
            let Some(claims) = filed.get(&row.recommendationid) else {
                return Ok(());
            };
            let review = u32::try_from(reviews.len()).unwrap_or(u32::MAX);
            reviews.push(row.recommendationid.clone());
            for (at, into) in claims {
                let Some(raw) = text.get(at.0 as usize..at.1 as usize) else {
                    continue;
                };
                let key = meaning::key_of(raw);
                for &group in into {
                    gathered[group]
                        .claims
                        .entry(key)
                        .or_default()
                        .push((review, *at));
                }
            }
            Ok(())
        })?;

        // The sample of each subject on each side, and the vectors it needs.
        let samples: Vec<Vec<[u8; 32]>> = gathered
            .iter()
            .map(|group| {
                let mut keys: Vec<[u8; 32]> = group.claims.keys().copied().collect();
                keys.sort_unstable();
                keys.truncate(SAMPLE);
                keys
            })
            .collect();
        let wanted: HashSet<[u8; 32]> = samples.iter().flatten().copied().collect();

        // Labels and quotes are taken from the sample alone, so only its text is fetched, in a walk
        // of its own: the text of every claim of a game with a million reviews runs to hundreds of
        // megabytes.
        let mut texts: HashMap<[u8; 32], String> = HashMap::with_capacity(wanted.len());
        crate::capture::for_each_row(snapshot, |row, text| {
            let Some(claims) = filed.get(&row.recommendationid) else {
                return Ok(());
            };
            let mut cleaned = None;
            for (at, _) in claims {
                let Some(raw) = text.get(at.0 as usize..at.1 as usize) else {
                    continue;
                };
                let key = meaning::key_of(raw);
                if !wanted.contains(&key) || texts.contains_key(&key) {
                    continue;
                }
                let cleaned = cleaned.get_or_insert_with(|| crate::claims::claims_of(text));
                let said = cleaned
                    .iter()
                    .find(|(range, _)| range.start == at.0 as usize && range.end == at.1 as usize)
                    .map_or_else(
                        || raw.trim().to_owned(),
                        |(_, clean)| clean.trim().to_owned(),
                    );
                texts.insert(key, said);
            }
            Ok(())
        })?;
        Ok(Self {
            gathered,
            reviews,
            samples,
            texts,
        })
    }

    /// The themes of each subject on each side, with every claim, sampled or not, under the
    /// nearest of its group that it is alike.
    fn grouped(&self, snapshot: &Path, closeness: Closeness) -> Result<Vec<Found>> {
        let Self {
            gathered,
            samples,
            texts,
            ..
        } = self;
        let Closeness {
            alike,
            merged: merge_at,
            ..
        } = closeness;
        let subjects = SHEET.len();
        let wanted: HashSet<[u8; 32]> = samples.iter().flatten().copied().collect();
        let mut points: HashMap<[u8; 32], Point> = HashMap::with_capacity(wanted.len());
        meaning::each_vector(snapshot, |key, _, bytes| {
            if wanted.contains(key) {
                points.entry(*key).or_insert_with(|| Point::new(bytes));
            }
        })?;

        let mut found: Vec<Found> = Vec::new();
        for (group, sample) in samples.iter().enumerate() {
            let keys: Vec<[u8; 32]> = sample
                .iter()
                .filter(|key| points.contains_key(*key))
                .copied()
                .collect();
            let held: Vec<Point> = keys.iter().filter_map(|key| points.remove(key)).collect();
            for members in merged(&held, communities(&held, alike), merge_at) {
                let centre = centre(&held, &members);
                let mut nearest: Vec<(f32, usize)> = members
                    .iter()
                    .map(|&member| (held[member].towards(&centre), member))
                    .collect();
                nearest.sort_by(|a, b| b.0.total_cmp(&a.0));
                let readable =
                    |key: &[u8; 32]| texts.get(key).is_some_and(|text| reads_as_words(text));
                let Some(quote) = nearest
                    .iter()
                    .map(|&(_, member)| keys[member])
                    .find(|key| readable(key))
                else {
                    continue;
                };
                // Of the members nearest the centre, the shortest that reads as words names it
                // best: the typical claim, put briefly.
                let label = nearest
                    .iter()
                    .take(8)
                    .map(|&(_, member)| keys[member])
                    .filter(|key| readable(key))
                    .min_by_key(|key| texts.get(key).map_or(usize::MAX, String::len))
                    .unwrap_or(quote);
                found.push(Found {
                    group,
                    centre,
                    label,
                    quote,
                    keys: Vec::new(),
                });
            }
            // The sample's vectors are not wanted again; another group may share a few.
            for (key, point) in keys.into_iter().zip(held) {
                points.insert(key, point);
            }
        }

        // Every claim, sampled or not, under the nearest theme of its group that it is alike.
        let mut by_group: Vec<Vec<usize>> = vec![Vec::new(); subjects * 2];
        for (at, theme) in found.iter().enumerate() {
            by_group[theme.group].push(at);
        }
        let mut groups_of: HashMap<[u8; 32], Vec<usize>> = HashMap::new();
        for (group, gathered) in gathered.iter().enumerate() {
            if by_group[group].is_empty() {
                continue;
            }
            for key in gathered.claims.keys() {
                groups_of.entry(*key).or_default().push(group);
            }
        }
        meaning::each_vector(snapshot, |key, _, bytes| {
            let Some(groups) = groups_of.get(key) else {
                return;
            };
            let point = Point::new(bytes);
            for &group in groups {
                let best = by_group[group]
                    .iter()
                    .map(|&theme| (point.towards(&found[theme].centre), theme))
                    .max_by(|a, b| a.0.total_cmp(&b.0));
                if let Some((closeness, theme)) = best
                    && closeness >= alike
                {
                    found[theme].keys.push(*key);
                }
            }
        })?;
        Ok(found)
    }

    fn kept(&self, found: &[Found]) -> Kept {
        let Self {
            gathered,
            reviews,
            texts,
            ..
        } = self;
        let subjects = SHEET.len();
        let quote_of = |key: &[u8; 32], group: usize| -> Quote {
            let (review, at) = gathered[group]
                .claims
                .get(key)
                .and_then(|said| said.first())
                .copied()
                .unwrap_or((0, (0, 0)));
            Quote {
                review_id: reviews.get(review as usize).cloned().unwrap_or_default(),
                at,
                text: cut(texts.get(key).map_or("", String::as_str)),
            }
        };

        let mut themes: Vec<Vec<(Theme, usize)>> = vec![Vec::new(); subjects * 2];
        for (at, theme) in found.iter().enumerate() {
            let mut said: Vec<(u32, Span)> = theme
                .keys
                .iter()
                .flat_map(|key| gathered[theme.group].claims.get(key).into_iter().flatten())
                .copied()
                .collect();
            // In the capture's order, which is the order every other list of claims is shown in.
            said.sort_unstable();
            let reviews_saying = said
                .iter()
                .map(|&(review, _)| review)
                .collect::<HashSet<_>>()
                .len() as u64;
            let members: Vec<(String, u32, u32)> = said
                .into_iter()
                .map(|(review, span)| {
                    (
                        reviews.get(review as usize).cloned().unwrap_or_default(),
                        span.0,
                        span.1,
                    )
                })
                .collect();
            if reviews_saying < FEWEST_REVIEWS {
                continue;
            }
            themes[theme.group].push((
                Theme {
                    label: texts
                        .get(&theme.label)
                        .map_or_else(String::new, |text| cut(text)),
                    quote: quote_of(&theme.quote, theme.group),
                    reviews: reviews_saying,
                    claims: members.len() as u64,
                    also_in: Vec::new(),
                    members,
                },
                at,
            ));
        }
        for kept in &mut themes {
            kept.sort_by_key(|(theme, _)| Reverse(theme.reviews));
            kept.truncate(THEMES_KEPT);
        }
        themes
    }

    /// Names, beside each theme, the themes under other subjects on its side that are the same
    /// point.
    fn link(themes: &mut Kept, found: &[Found], shared: f32) {
        // A theme said under two subjects on the same side is the same point, and saying so is
        // what tells a reader it runs through the game rather than sitting in one corner of it.
        let shown: Vec<(usize, usize)> = themes
            .iter()
            .flat_map(|kept| kept.iter().map(|(_, at)| (found[*at].group, *at)))
            .collect();
        let keys_of: HashMap<usize, HashSet<[u8; 32]>> = shown
            .iter()
            .map(|&(_, at)| (at, found[at].keys.iter().copied().collect()))
            .collect();
        // How strongly two themes are one point: the larger of the share of claims they hold in
        // common and the closeness of their centres, each against its own line.
        let same_point = |one: usize, other: usize| -> Option<f64> {
            let (a, b) = (&keys_of[&one], &keys_of[&other]);
            #[expect(
                clippy::cast_precision_loss,
                reason = "claim counts are far below 2^52"
            )]
            let overlap = a.intersection(b).count() as f64 / a.len().min(b.len()).max(1) as f64;
            let closeness = dot(&found[one].centre, &found[other].centre);
            (overlap >= OVERLAP || closeness >= shared).then(|| overlap.max(f64::from(closeness)))
        };
        let place_of: HashMap<usize, Elsewhere> = themes
            .iter()
            .enumerate()
            .flat_map(|(group, kept)| {
                kept.iter().enumerate().map(move |(theme, (_, at))| {
                    (
                        *at,
                        Elsewhere {
                            subject: SHEET[group / 2].id.to_owned(),
                            theme,
                        },
                    )
                })
            })
            .collect();
        for kept in themes.iter_mut() {
            for (theme, at) in kept.iter_mut() {
                let group = found[*at].group;
                let mut also: Vec<(f64, Elsewhere)> = shown
                    .iter()
                    .filter(|&&(other_group, _)| {
                        other_group / 2 != group / 2 && other_group % 2 == group % 2
                    })
                    .filter_map(|&(_, other)| {
                        Some((same_point(*at, other)?, place_of[&other].clone()))
                    })
                    .collect();
                also.sort_by(|a, b| b.0.total_cmp(&a.0));
                theme.also_in = also.into_iter().map(|(_, place)| place).collect();
            }
        }
    }

    fn subjects(&self, themes: Kept) -> Vec<SubjectThemes> {
        let Self { gathered, .. } = self;
        themes
            .into_iter()
            .enumerate()
            .filter(|(_, kept)| !kept.is_empty())
            .map(|(group, kept)| {
                let said_by: HashSet<u32> = gathered[group]
                    .claims
                    .values()
                    .flatten()
                    .map(|&(review, _)| review)
                    .collect();
                SubjectThemes {
                    subject: SHEET[group / 2].id.to_owned(),
                    side: SIDES[group % 2].to_owned(),
                    reviews: said_by.len() as u64,
                    themes: kept.into_iter().map(|(theme, _)| theme).collect(),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A vector of bytes along `direction`, nudged by `noise` in a dimension of its own.
    fn along(direction: usize, noise: usize) -> Point {
        let mut bytes = vec![0_i8; 32];
        bytes[direction] = 100;
        bytes[8 + noise % 24] = 30;
        Point::new(&bytes)
    }

    #[test]
    fn a_point_is_alike_itself_and_unlike_one_at_right_angles() {
        let one = along(0, 0);
        assert!((one.similarity(&one) - 1.0).abs() < 1e-6);
        let other = Point::new(&{
            let mut bytes = vec![0_i8; 32];
            bytes[1] = 100;
            bytes
        });
        assert!(one.similarity(&other).abs() < 1e-6);
    }

    #[test]
    fn claims_saying_the_same_thing_form_one_theme_each() {
        let points: Vec<Point> = (0..6)
            .map(|noise| along(0, noise))
            .chain((0..4).map(|noise| along(1, noise)))
            .chain(std::iter::once(along(2, 0)))
            .collect();
        let found = communities(&points, Closeness::USED.alike);
        assert_eq!(found.len(), 2, "{found:?}");
        assert_eq!(found[0].len(), 6, "the most crowded comes first");
        assert!(found[0].iter().all(|&member| member < 6));
        assert_eq!(found[1].len(), 4);
        assert!(found[1].iter().all(|&member| (6..10).contains(&member)));
    }

    #[test]
    fn two_claims_alone_are_not_a_theme() {
        let points = vec![along(0, 0), along(0, 1), along(1, 0)];
        assert_eq!(
            communities(&points, Closeness::USED.alike),
            Vec::<Vec<usize>>::new()
        );
    }

    #[test]
    fn a_point_is_counted_in_one_theme_only() {
        let points: Vec<Point> = (0..5).map(|noise| along(0, noise)).collect();
        let found = communities(&points, 0.0);
        assert_eq!(found, [vec![0, 1, 2, 3, 4]]);
    }

    #[test]
    fn a_centre_is_a_unit_vector_along_its_members() {
        let points: Vec<Point> = (0..4).map(|noise| along(0, noise)).collect();
        let centre = centre(&points, &[0, 1, 2, 3]);
        assert!((dot(&centre, &centre) - 1.0).abs() < 1e-5);
        let index = centre
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(at, _)| at);
        assert_eq!(index, Some(0));
        assert!(points.iter().all(|point| point.towards(&centre) > 0.9));
    }

    #[test]
    fn groups_with_nearly_one_centre_are_made_one() {
        let points: Vec<Point> = (0..3)
            .map(|noise| along(0, noise))
            .chain((3..6).map(|noise| along(0, noise)))
            .chain((0..3).map(|noise| along(1, noise)))
            .collect();
        let groups = merged(
            &points,
            vec![vec![0, 1, 2], vec![6, 7, 8], vec![3, 4, 5]],
            Closeness::USED.merged,
        );
        assert_eq!(groups, [vec![0, 1, 2, 3, 4, 5], vec![6, 7, 8]]);
    }

    #[test]
    fn a_line_of_a_review_template_does_not_read_as_words() {
        assert!(reads_as_words("The UI is really rough."));
        assert!(!reads_as_words("🐛 Bugs: ✅ Perfect"));
        assert!(!reads_as_words("{ Bugs }--- ☑ Never heard of"));
        assert!(!reads_as_words("Great game"));
    }

    fn theme(label: &str, reviews: u64, also_in: Vec<Elsewhere>) -> Theme {
        Theme {
            label: label.to_owned(),
            quote: Quote {
                review_id: "1".to_owned(),
                at: (0, 4),
                text: label.to_owned(),
            },
            reviews,
            claims: reviews,
            also_in,
            members: Vec::new(),
        }
    }

    fn side(subject: &str, side: &str, themes: Vec<Theme>) -> SubjectThemes {
        SubjectThemes {
            subject: subject.to_owned(),
            side: side.to_owned(),
            reviews: 1_000,
            themes,
        }
    }

    #[test]
    fn a_side_is_ranked_across_subjects_with_a_shared_point_once() {
        let in_genre = Elsewhere {
            subject: "genre".to_owned(),
            theme: 0,
        };
        let in_gameplay = Elsewhere {
            subject: "gameplay".to_owned(),
            theme: 1,
        };
        let found = Themes {
            encoder: ENCODER.name.to_owned(),
            readings: None,
            subjects: vec![
                side(
                    "gameplay",
                    "praise",
                    vec![
                        theme("combos", 300, vec![]),
                        theme("like slay the spire", 90, vec![in_genre]),
                    ],
                ),
                side(
                    "gameplay",
                    "complaint",
                    vec![theme("too easy", 500, vec![])],
                ),
                side(
                    "genre",
                    "praise",
                    vec![theme("slay the spire but better", 800, vec![in_gameplay])],
                ),
                side("price", "praise", vec![theme("worth it", 90, vec![])]),
            ],
        };
        let ranked: Vec<(&str, usize, &str)> = found
            .ranked("praise")
            .iter()
            .map(|one| (one.subject, one.theme, one.said.label.as_str()))
            .collect();
        assert_eq!(
            ranked,
            [
                ("genre", 0, "slay the spire but better"),
                ("gameplay", 0, "combos"),
                ("price", 0, "worth it"),
            ],
            "the shared point is kept where most say it, and ties keep the sheet's order"
        );
        assert_eq!(found.ranked("complaint").len(), 1);
    }

    #[test]
    fn themes_written_are_read_back_only_for_the_reading_and_encoder_they_were_found_for() {
        let dir = crate::tempdir::Dir::new();
        let snapshot = dir.path();
        std::fs::write(snapshot.join("readings.parquet"), b"stand-in").unwrap();
        let found = Themes {
            encoder: ENCODER.name.to_owned(),
            readings: meaning::readings_written(snapshot),
            subjects: vec![side(
                "gameplay",
                "praise",
                vec![theme(
                    "combos",
                    300,
                    vec![Elsewhere {
                        subject: "genre".to_owned(),
                        theme: 2,
                    }],
                )],
            )],
        };
        found.save(snapshot).unwrap();
        assert_eq!(Themes::load(snapshot), Some(found.clone()));

        let other = Themes {
            encoder: "another-encoder".to_owned(),
            ..found.clone()
        };
        other.save(snapshot).unwrap();
        assert_eq!(Themes::load(snapshot), None, "another encoder's themes");

        let earlier = Themes {
            readings: found.readings.map(|at| at - 1),
            ..found
        };
        earlier.save(snapshot).unwrap();
        assert_eq!(
            Themes::load(snapshot),
            None,
            "themes of a reading since replaced"
        );
    }

    #[test]
    fn a_long_quote_is_cut_at_a_word() {
        let long = "word ".repeat(100);
        let kept = cut(&long);
        assert!(kept.ends_with("word…"), "{kept}");
        assert!(kept.chars().count() <= QUOTE_MOST + 1);
        assert_eq!(cut("  short  "), "short");
    }
}
