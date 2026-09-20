//! What share of the claims this build emits say nothing a reader could judge.
//!
//! A claim the splitter produces is a question the reader must answer and, in a reference set,
//! a question a labeller is paid to answer. One that carries no proposition costs exactly as
//! much as a real one and returns noise: the labellers disagree on it because there is nothing
//! to agree about, and that disagreement then reads as a hard boundary rather than as junk.
//!
//! Every cause below is a different reason a piece of a review is not a claim, counted apart
//! because the fixes are different and because a total alone cannot say whether a rule landed.
//!
//!     cargo run --release -p steamgauge-core --example empty-claims -- 1091500 1222670
//!
//! Also reports the claims that repeat, which are a separate problem with the same source:
//! boilerplate is real text and says something, but a phrase a template supplies five hundred
//! times is five hundred votes cast by one author.

use std::collections::HashMap;

/// Steam's replacement for a word its filter objected to. Whatever the reviewer wrote is gone,
/// and a claim made only of these is a claim whose content the platform deleted.
const CENSORED: char = '\u{2665}';

/// The marks a template offers, ticked or blank. Kept here rather than borrowed from the
/// splitter so this stays a measurement of the splitter rather than a restatement of it: if
/// the two lists ever disagree, this reports the difference instead of hiding it.
const BOXES: [char; 10] = [
    '\u{2610}',
    '\u{2611}',
    '\u{2612}',
    '\u{25A1}',
    '\u{2713}',
    '\u{2714}',
    '\u{2705}',
    '\u{1F532}',
    '\u{1F533}',
    '\u{2B1C}',
];
const BLANK: [char; 5] = ['\u{2610}', '\u{25A1}', '\u{1F532}', '\u{1F533}', '\u{2B1C}'];

/// Why a claim carries nothing to judge. Ordered as the checks run, most specific first.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Empty {
    /// An option the reviewer was offered and did not take. Its text means the opposite of
    /// what it says, so no answer to it is right.
    Declined,
    /// An option the reviewer did take, cut away from the heading that says what it answers.
    /// "Graphics" under "What I enjoy" is a claim; "Graphics" alone is a word.
    Orphaned,
    /// Only Steam's censorship hearts, so the content is not merely absent but deleted.
    Censored,
    /// Markup the splitter was meant to strip and did not.
    Markup,
    /// No letter and no digit anywhere: punctuation, emoji or a rule made of dashes.
    Wordless,
    /// Digits and punctuation only, which is a score with nothing said about it.
    BareNumber,
}

impl Empty {
    const fn why(self) -> &'static str {
        match self {
            Self::Declined => "an option the reviewer left blank",
            Self::Orphaned => "a ticked option, cut off from its heading",
            Self::Censored => "nothing but Steam's censorship hearts",
            Self::Markup => "markup that survived stripping",
            Self::Wordless => "no letter or digit in it",
            Self::BareNumber => "digits with nothing said about them",
        }
    }
}

/// Whether the claim is a template option, and whether it was chosen.
fn option(claim: &str) -> Option<bool> {
    let mut chars = claim.trim_start().chars();
    let first = chars.next()?;
    // A bar of marks drawn to show a score out of ten, not a column of boxes to choose from.
    if chars.next().is_some_and(|after| BOXES.contains(&after)) {
        return None;
    }
    BOXES.contains(&first).then(|| !BLANK.contains(&first))
}

/// Whether a ticked option brought its heading with it.
///
/// The heading is what supplies the polarity, and a template puts it on its own line above the
/// group. A claim that holds only the option line has lost it.
fn kept_its_heading(claim: &str) -> bool {
    claim
        .lines()
        .any(|line| !line.trim().is_empty() && !line.trim_start().starts_with(BOXES))
}

fn why_empty(claim: &str) -> Option<Empty> {
    let text = claim.trim();
    if text.is_empty() {
        return Some(Empty::Wordless);
    }
    match option(text) {
        Some(false) => return Some(Empty::Declined),
        Some(true) if !kept_its_heading(text) => return Some(Empty::Orphaned),
        _ => {}
    }
    let bare: String = text.chars().filter(|c| *c != CENSORED).collect();
    if bare.trim().is_empty() {
        return Some(Empty::Censored);
    }
    let stripped = bare.trim();
    // A tag, not a bracketed aside: Chinese writes without spaces, so "no space inside the
    // brackets" alone called every bracketed Chinese sentence markup.
    let tag = stripped
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'));
    if tag.is_some_and(|name| {
        !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '=' | '*' | '"' | '.'))
    }) {
        return Some(Empty::Markup);
    }
    if !stripped.chars().any(char::is_alphanumeric) {
        return Some(Empty::Wordless);
    }
    if !stripped.chars().any(char::is_alphabetic) {
        return Some(Empty::BareNumber);
    }
    None
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app_ids: Vec<u32> = std::env::args()
        .skip(1)
        .filter_map(|one| one.parse().ok())
        .collect();
    if app_ids.is_empty() {
        return Err("usage: empty-claims <app id> [app id...]".into());
    }
    let out = std::env::var("OUT_DIR_DATA").unwrap_or_else(|_| "data".to_owned());
    let show: usize = std::env::var("SHOW")
        .ok()
        .and_then(|one| one.parse().ok())
        .unwrap_or(4);

    let mut all: HashMap<Empty, (usize, Vec<String>)> = HashMap::new();
    let mut claims = 0usize;
    let mut repeats: HashMap<String, usize> = HashMap::new();

    for app_id in app_ids {
        let snapshot = steamgauge_core::embed::latest_snapshot(std::path::Path::new(&out), app_id)?;
        let mut here = 0usize;
        let mut empty_here = 0usize;
        steamgauge_core::capture::for_each_body(&snapshot, |_, _, text| {
            for claim in steamgauge_core::claims::split(text) {
                here += 1;
                *repeats.entry(claim.trim().to_lowercase()).or_default() += 1;
                if let Some(cause) = why_empty(&claim) {
                    empty_here += 1;
                    let slot = all.entry(cause).or_insert_with(|| (0, Vec::new()));
                    slot.0 += 1;
                    if slot.1.len() < show {
                        slot.1.push(claim.trim().replace('\n', " / "));
                    }
                }
            }
            Ok(())
        })?;
        #[expect(clippy::cast_precision_loss, reason = "counts, not measurements")]
        let share = empty_here as f64 / here.max(1) as f64 * 100.0;
        println!("{app_id:>9}  {here:>9} claims, {empty_here} of them empty ({share:.3}%)");
        claims += here;
    }

    println!("\n{claims} claims in all");
    let mut causes: Vec<(&Empty, &(usize, Vec<String>))> = all.iter().collect();
    causes.sort_by_key(|(_, (n, _))| std::cmp::Reverse(*n));
    for (cause, (n, examples)) in causes {
        #[expect(clippy::cast_precision_loss, reason = "counts, not measurements")]
        let share = *n as f64 / claims.max(1) as f64 * 100.0;
        println!("\n{n:>8} ({share:.3}%)  {}", cause.why());
        for one in examples {
            println!("           {:?}", one.chars().take(64).collect::<String>());
        }
    }

    let mut common: Vec<(&String, &usize)> = repeats.iter().filter(|(_, n)| **n > 1).collect();
    common.sort_by_key(|(text, n)| (std::cmp::Reverse(**n), text.len()));
    let duplicated: usize = common.iter().map(|(_, n)| **n - 1).sum();
    #[expect(clippy::cast_precision_loss, reason = "counts, not measurements")]
    let share = duplicated as f64 / claims.max(1) as f64 * 100.0;
    println!("\n{duplicated} claims ({share:.1}%) repeat text another claim already carries");
    for (text, n) in common.iter().take(12) {
        println!(
            "    {n:>6}x  {:?}",
            text.chars().take(58).collect::<String>()
        );
    }
    Ok(())
}
