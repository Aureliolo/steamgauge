//! What the code that reads a stranger's text promises, asked of whatever the fuzzer writes.
//!
//! Each function takes one input and panics where a promise is broken, which libFuzzer reports
//! as a crash together with the input that broke it. A panic in any of this code in the product
//! stops the reading of a whole library, so not panicking is the first promise, and the rest
//! are the ones the code downstream relies on: a claim is named by its span and nothing else,
//! so a span that moves when it is brought to its words again, or two claims that share one,
//! lose a label as surely as a crash loses a reading.

use std::{borrow::Cow, ops::Range};

use steamgauge_core::{claims, reader, said};

/// The splitter over a whole review.
///
/// # Panics
///
/// Wherever a claim it cuts breaks a promise the rest of the tool relies on.
pub fn splitter(text: &str) {
    let found = claims::claims_of(text);
    let mut previous: Option<&Range<usize>> = None;
    for (span, claim) in &found {
        assert!(
            span.start < span.end && span.end <= text.len(),
            "span {span:?} is not inside a review of {} bytes",
            text.len()
        );
        assert!(
            text.is_char_boundary(span.start) && text.is_char_boundary(span.end),
            "span {span:?} cuts a character in half"
        );
        let (start, end) = as_stored(span);
        assert_eq!(
            claims::words_at(text, start, end),
            Some((start, end)),
            "a label made on {:?} would join no claim: brought to its words again it moves",
            &text[span.clone()]
        );
        if let Some(before) = previous {
            // A heading above a template's options stays in front of every answer ticked under
            // it, so those claims share their first byte; nothing else may overlap.
            assert!(
                before.start <= span.start && before.end < span.end,
                "claims out of order: {before:?} and then {span:?}"
            );
            assert!(
                span.start >= before.end || span.start == before.start,
                "claims {before:?} and {span:?} overlap without sharing a heading"
            );
        }
        previous = Some(span);

        assert!(
            !claim.is_empty() && claim.trim() == &**claim,
            "claim {claim:?} is empty or carries whitespace at an end"
        );
        assert!(
            !claims::is_not_a_claim(claim),
            "the splitter handed back {claim:?}, which every other door refuses as no claim"
        );
        let covered = &text[span.clone()];
        match claim {
            Cow::Borrowed(words) => assert_eq!(
                *words, covered,
                "a claim borrowed from the review is not the text its span covers"
            ),
            Cow::Owned(words) => assert!(
                is_within(words, covered),
                "claim {words:?} says something its span {covered:?} does not hold"
            ),
        }
    }
    // Every door a stored claim comes back through asks this of whatever is stored, which
    // was cut by other rules and may be anything at all.
    let _ = claims::is_not_a_claim(text);
}

/// A stored span brought to its words, whatever it points at.
///
/// Labels and readings carry spans cut by earlier rules and against captures that may since
/// have changed, so the spans asked about here are the review's own claims nudged a byte or
/// two either way, positions drawn from the text, and numbers past its end.
///
/// # Panics
///
/// Wherever the span it answers with is not inside the one it was asked about, is not words,
/// or moves when asked about again.
pub fn stored_span(text: &str) {
    let length = text.len();
    let mut draw = Draw::of(text);
    let mut positions: Vec<usize> = claims::spans(text)
        .iter()
        .take(2)
        .flat_map(|span| [span.start, span.end])
        .flat_map(|at| [at.saturating_sub(1), at, at + 1])
        .collect();
    positions.extend((0..6).map(|_| draw.below(length + 3)));

    for &from in &positions {
        for &to in &positions {
            brought_to_its_words(text, stored(from), stored(to));
        }
    }
    for (from, to) in [(0, u32::MAX), (u32::MAX, u32::MAX), (u32::MAX, 0)] {
        brought_to_its_words(text, from, to);
    }
}

fn brought_to_its_words(text: &str, from: u32, to: u32) {
    let Some((start, end)) = claims::words_at(text, from, to) else {
        return;
    };
    let (at, ends) = (start as usize, end as usize);
    assert!(
        from <= start && start < end && end <= to && ends <= text.len(),
        "asked about {from}..{to}, answered {start}..{end} of a review of {} bytes",
        text.len()
    );
    assert!(
        text.is_char_boundary(at) && text.is_char_boundary(ends),
        "asked about {from}..{to}, answered {start}..{end}, which cuts a character in half"
    );
    let words = &text[at..ends];
    assert_eq!(
        words.trim(),
        words,
        "asked about {from}..{to}, answered {words:?}, which is not words"
    );
    assert_eq!(
        claims::words_at(text, start, end),
        Some((start, end)),
        "{words:?} brought to its words again moves"
    );
}

/// Budgets from none to more than any review holds: a reader is exported with its own.
const BUDGETS: [usize; 9] = [0, 1, 4, 6, 12, 32, 128, 512, usize::MAX];

/// The window the reader reads each claim in, as the reading pass builds it: the claims of
/// the review rejoined, and each claim's place in them.
///
/// The offsets are what a tokenizer would hand back for the rejoined review, drawn from the
/// text: a word cut into pieces of one to four characters, sometimes with the space before it,
/// sometimes with a multi-byte character spelt out as a token per byte, all of which the
/// tokenizers the readers ship with do. Then again through offsets that describe nothing, which
/// is what a tokenizer saved with padding or truncation hands back.
///
/// # Panics
///
/// Wherever a claim is not inside its own window, the window is not the piece of the review
/// the budget buys, or offsets of any shape make it index out of bounds.
pub fn reader_window(text: &str) {
    let claims = claims::split(text);
    let review = claims.join(" ");
    let mut draw = Draw::of(text);
    let spelt_out = draw.below(4) == 0;
    let offsets = tokenised(&review, &mut draw, spelt_out);

    let mut at = 0;
    for claim in &claims {
        let claim: &str = claim;
        let asked = reader::Asked {
            claim,
            language: "english",
            review: &review,
            at,
            headset_only: false,
        };
        for budget in BUDGETS {
            window(&asked, &offsets, budget, spelt_out);
        }
        at += claim.len() + 1;
    }

    let nonsense: Vec<(usize, usize)> = (0..draw.below(12))
        .map(|_| (draw.below(review.len() + 4), draw.below(review.len() + 4)))
        .collect();
    let mut at = 0;
    for claim in &claims {
        let claim: &str = claim;
        let asked = reader::Asked {
            claim,
            language: "english",
            review: &review,
            at,
            headset_only: false,
        };
        for budget in BUDGETS {
            let plain = reader::window_around(&asked, &nonsense, budget, false);
            assert!(
                review.contains(&plain),
                "offsets that describe nothing gave a window that is not a piece of the review: {plain:?}"
            );
            let _ = reader::window_around(&asked, &nonsense, budget, true);
        }
        at += claim.len() + 1;
    }
}

fn window(asked: &reader::Asked<'_>, offsets: &[(usize, usize)], budget: usize, spelt_out: bool) {
    let review = asked.review;
    let (at, ends) = (asked.at, asked.at + asked.claim.len());
    let Some((opens, closes)) = reader::centred(offsets, at, asked.claim.len(), budget) else {
        assert!(
            offsets.is_empty() && !review.contains(|ch: char| !ch.is_whitespace()),
            "a review with words in it had no tokens to centre on"
        );
        return;
    };
    assert!(
        opens <= at && ends <= closes && closes <= review.len(),
        "the window {opens}..{closes} does not hold its claim at {at}..{ends}, budget {budget}"
    );

    let plain = reader::window_around(asked, offsets, budget, false);
    assert_eq!(
        plain,
        review[opens..closes],
        "the window is not the piece of the review it was centred on"
    );
    let marked = reader::window_around(asked, offsets, budget, true);
    let mark = reader::MARK;
    assert_eq!(
        marked,
        format!(
            "{}{mark} {} {mark}{}",
            &review[opens..at],
            asked.claim,
            &review[ends..closes]
        ),
        "the mark is not round the claim and nothing else"
    );

    if spelt_out {
        return;
    }
    // The pair is the claim, the window and four special tokens. Where the budget cannot hold
    // the claim twice over, the window is the claim alone.
    let inside = offsets
        .iter()
        .filter(|&&(start, end)| start >= opens && end <= closes)
        .count();
    let claimed = offsets
        .iter()
        .filter(|&&(start, end)| end > at && start < ends)
        .count();
    assert!(
        inside + claimed + 4 <= budget || inside == claimed,
        "a window of {inside} tokens beside a claim of {claimed} does not fit a budget of {budget}"
    );
    if budget >= 2 * offsets.len() + 4 {
        assert_eq!(
            (opens, closes),
            (offsets[0].0, offsets[offsets.len() - 1].1),
            "a budget that fits the whole review kept less than all of it"
        );
    }
}

/// Offsets as a tokenizer with no padding and no truncation reports them: in order, none
/// across whitespace, and none over whitespace alone.
fn tokenised(review: &str, draw: &mut Draw, spelt_out: bool) -> Vec<(usize, usize)> {
    let mut offsets = Vec::new();
    let mut chars = review.char_indices().peekable();
    let mut space_before: Option<usize> = None;
    while let Some((at, ch)) = chars.next() {
        if ch.is_whitespace() {
            space_before = Some(at);
            continue;
        }
        let mut word = vec![(at, ch)];
        while let Some(&(next, ch)) = chars.peek() {
            if ch.is_whitespace() {
                break;
            }
            word.push((next, ch));
            chars.next();
        }
        let ends = word.last().map_or(at, |&(last, ch)| last + ch.len_utf8());
        let mut piece_start = match space_before.take() {
            Some(space) if draw.below(2) == 0 => space,
            _ => at,
        };
        let mut taken = 0;
        while taken < word.len() {
            let size = 1 + draw.below(4);
            let upto = (taken + size).min(word.len());
            let piece_end = word.get(upto).map_or(ends, |&(next, _)| next);
            let single = upto - taken == 1 && word[taken].1.len_utf8() > 1;
            let repeats = if spelt_out && single {
                word[taken].1.len_utf8()
            } else {
                1
            };
            for _ in 0..repeats {
                offsets.push((piece_start, piece_end));
            }
            piece_start = piece_end;
            taken = upto;
        }
    }
    offsets
}

/// The terms a claim is counted under.
///
/// # Panics
///
/// Wherever a term is not the shape every term is, or the cut that counts a term and the one
/// that finds the claims it was counted from disagree.
pub fn terms(text: &str) {
    let mut found: Vec<String> = Vec::new();
    said::each_term(text, |term| found.push(term.to_owned()));
    let mut again: Vec<String> = Vec::new();
    said::each_term(text, |term| again.push(term.to_owned()));
    assert_eq!(
        found, again,
        "the same claim cut twice gave different terms"
    );

    for term in &found {
        assert!(
            term.chars().count() >= 2,
            "{term:?} is a single character, which is never a term"
        );
        assert!(
            !term.starts_with(' ') && !term.ends_with(' ') && !term.contains("  "),
            "{term:?} is not words joined by one space"
        );
        assert!(
            !term.chars().any(|ch| ch.is_whitespace() && ch != ' '),
            "{term:?} holds whitespace other than the space that joins its words"
        );
        assert!(
            term.split(' ').count() <= 3,
            "{term:?} is longer than a modifier, a word and the word it pairs with"
        );
    }
    for term in [found.first(), found.get(found.len() / 2), found.last()]
        .into_iter()
        .flatten()
    {
        assert!(
            said::mentions(text, term),
            "{term:?} was counted from a claim that does not mention it"
        );
    }
    for pair in found.windows(2) {
        assert_eq!(
            said::one_word_inflected(&pair[0], &pair[1]),
            said::one_word_inflected(&pair[1], &pair[0]),
            "{:?} and {:?} are one word in two forms one way round and not the other",
            pair[0],
            pair[1]
        );
    }
}

/// Whether `inner` is `outer` with characters taken out and none put in or moved.
fn is_within(inner: &str, outer: &str) -> bool {
    let mut rest = outer.chars();
    inner.chars().all(|ch| rest.any(|other| other == ch))
}

fn as_stored(span: &Range<usize>) -> (u32, u32) {
    (stored(span.start), stored(span.end))
}

fn stored(at: usize) -> u32 {
    u32::try_from(at).unwrap_or(u32::MAX)
}

/// Choices drawn from the input itself, so a crash reproduces from its input alone and every
/// mutation of the text is also a new tokenisation and a new set of spans.
struct Draw(u64);

impl Draw {
    fn of(text: &str) -> Self {
        // FNV-1a.
        Self(text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        }))
    }

    /// A number below `bound`, which must not be zero.
    fn below(&mut self, bound: usize) -> usize {
        // splitmix64.
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut mixed = self.0;
        mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        mixed ^= mixed >> 31;
        usize::try_from(mixed % bound as u64).unwrap_or(0)
    }
}
