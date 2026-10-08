//! Rendering a report as one self-contained page.
//!
//! Self-contained is the whole constraint. No fonts, no scripts, no stylesheets from
//! anywhere: a corpus that never left the machine must not start leaving it the moment
//! somebody looks at it, and a page that phones out is also a page that stops working when
//! the network does or when the host it depended on goes away.
//!
//! The page answers one question in three widening steps. A sentence says what reading only
//! the top of the pile would have told you and what everyone actually said. A table gives
//! every category the same treatment. Every row opens onto the reviews it counted, each
//! linking back to Steam, because a rate nobody can check is just an assertion.

use std::fmt::Write as _;

use crate::{
    read::SubjectCount,
    report::{AppReport, Example, Report, coverage},
    taxonomy::SHEET,
};

/// Characters of a review shown before it is folded away behind a control.
const PREVIEW_CHARS: usize = 320;

/// Languages listed before the tail is summarised as a count.
const LANGUAGES_SHOWN: usize = 12;

/// The timeline's own coordinate space. The SVG scales to whatever width it is given, so
/// these are only the numbers the shapes are drawn in.
const WIDTH: f64 = 1000.0;
const HEIGHT: f64 = 160.0;
const SPARK_HEIGHT: f64 = 40.0;

use crate::read::Month;

/// Reviews a month needs before its rates are drawn.
const ENOUGH_FOR_A_RATE: u64 = Month::ENOUGH_FOR_A_RATE;

/// The numeric columns of a game's table, in order, each with what it counts.
///
/// One list rather than two, so a column cannot reach the table without saying what it means.
/// `{top}` is how many reviews Steam ranks as most helpful, which is a per-game setting.
const COLUMNS: [(&str, &str); 6] = [
    (
        "Mention rate",
        "The share of all reviews that say something about the category, with the number of \
         reviews beside it. These are the headline figures and they add up to more than 100%.",
    ),
    (
        "Main subject",
        "The share whose single main subject is that category. Unlike mention rates, these add \
         up to the number of reviews.",
    ),
    (
        "Top of the pile",
        "The mention rate over the {top} reviews Steam ranks as most helpful, which is roughly \
         what somebody sees before deciding.",
    ),
    (
        "Bias",
        "How much the top of the pile overstates a category. 0\u{d7} means none of those few \
         dozen reviews raised it, which is weak evidence rather than proof of absence.",
    ),
    (
        "Said about it",
        "Of the reviews raising a category, how many praise it, how many complain about it, \
         and how many do both. A subject half the players love and half hate, and one every \
         player has mixed feelings about, are different findings that a single share hides.",
    ),
    (
        "Recommended",
        "The share of the reviews raising a category that still recommended the game, against \
         this game's own baseline. Marked warm or cold only where the gap is wider than the \
         number of reviews behind it can explain.",
    ),
];

/// A rate below which no game can be said to talk about a subject more than another.
///
/// It is the point at which the page stops printing a number and prints "less than" instead,
/// which is the same judgement: under it there is a rate, and there is nothing to rank.
const TOO_SMALL_TO_RANK: f64 = 0.001;

/// Renders the whole report.
#[must_use]
pub fn render(report: &Report) -> String {
    let mut out = String::with_capacity(1 << 18);
    out.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n");
    out.push_str("<meta charset=\"utf-8\">\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    let _ = writeln!(out, "<title>{}</title>", escape(&page_title(report)));
    let _ = writeln!(out, "<style>{STYLE}</style>");
    out.push_str("</head>\n<body>\n");

    out.push_str("<a class=\"skip\" href=\"#main\">Skip to the numbers</a>\n");
    page_header(&mut out, report);
    out.push_str("<main id=\"main\">\n");
    contents(&mut out, report);
    overview(&mut out, report);
    let several = report.apps.len() > 1;
    for app in &report.apps {
        game(&mut out, app, several);
    }
    out.push_str("</main>\n");
    page_footer(&mut out, report);
    let _ = writeln!(out, "<script>{SCRIPT}</script>");
    out.push_str("</body>\n</html>\n");
    out
}

fn page_title(report: &Report) -> String {
    match report.apps.as_slice() {
        [only] => format!("{} reviews | SteamGauge", only.crawl.title()),
        _ => "Steam reviews | SteamGauge".to_owned(),
    }
}

fn page_header(out: &mut String, report: &Report) {
    out.push_str("<header class=\"page\">\n<div class=\"wrap\">\n");
    let _ = writeln!(out, "<h1>{}</h1>", escape(&page_title(report)));
    out.push_str(
        "<p class=\"lede\">What every reviewer said, counted, against what the loudest \
         handful said.</p>\n",
    );
    // Named for the state it turns on rather than for the thing it changes: a control called
    // "Theme" announced as pressed or not pressed says nothing about which theme that is.
    out.push_str(
        "<button class=\"theme\" type=\"button\" data-theme-toggle aria-pressed=\"false\">\
         <span aria-hidden=\"true\">◐</span> Dark theme</button>\n",
    );
    filter(out, report);
    out.push_str("</div>\n</header>\n");
}

/// Narrows every table on the page to the categories whose name matches.
///
/// Hidden in the markup rather than by a stylesheet rule, so a reader without scripting is
/// never offered a box that does nothing. Two dozen categories across dozens of games is more
/// than anyone can scan for one subject.
fn filter(out: &mut String, report: &Report) {
    // Written once and handed to the script as well, so clearing the box puts back the
    // wording the page was rendered with rather than a second phrasing of the same thing.
    let showing = format!(
        "all {} of them{}",
        SHEET.len(),
        // Not "in every table": a report of several games also carries one comparing the
        // corpora themselves, which has no categories in it to narrow.
        if report.apps.len() > 1 {
            ", everywhere they appear"
        } else {
            ""
        }
    );
    let _ = writeln!(
        out,
        "<form class=\"filter\" role=\"search\" hidden data-filter \
         aria-label=\"Filter the report by category\">\n\
         <label for=\"filter-categories\">Show categories matching</label>\n\
         <input id=\"filter-categories\" type=\"search\" autocomplete=\"off\" spellcheck=\"false\" \
         placeholder=\"price, story, crashes\u{2026}\" data-filter-input>\n\
         <span class=\"filter-count\" role=\"status\" data-filter-count \
         data-showing-everything=\"{0}\">{0}</span>\n</form>",
        escape(&showing)
    );
}

/// A way to reach each game once there is more than one to reach.
fn contents(out: &mut String, report: &Report) {
    if report.apps.len() < 2 {
        return;
    }
    out.push_str("<nav class=\"contents\" id=\"games\" aria-label=\"Games in this report\">\n<div class=\"wrap\">\n<ul>\n");
    for app in &report.apps {
        let _ = writeln!(
            out,
            "<li><a href=\"#app-{}\"><span class=\"nav-name\">{}</span>\
             <span class=\"nav-count\">{}</span></a></li>",
            app.app_id(),
            escape(&app.crawl.title()),
            thousands(app.reading.reviews)
        );
    }
    out.push_str("</ul>\n</div>\n</nav>\n");
}

/// Every game against every other, which is the only view a census across games can give
/// and no single game's page ever can.
fn overview(out: &mut String, report: &Report) {
    if report.apps.len() < 2 {
        return;
    }
    let total: u64 = report.apps.iter().map(|app| app.reading.reviews).sum();

    out.push_str("<section class=\"overview\">\n<div class=\"wrap\">\n");
    out.push_str("<h2>Across these games</h2>\n");
    corpora(out, report);
    let _ = writeln!(
        out,
        "<p class=\"note\">Mention rates for {} reviews of {} games. The game that raises each \
         subject most is named beside it, and its cell outlined, where one clears every other \
         by more than rounding. Read down a column for one game and across a row to compare \
         them all; the table scrolls sideways. Deeper shading is a higher rate, and the rates \
         are not comparable to any other corpus.</p>",
        thousands(total),
        report.apps.len()
    );

    matrix(out, report);
    most_overstated(out, report);
    widest_apart(out, report);
    pooled_agreement(out, report);
    out.push_str("</div>\n</section>\n");
}

/// The corpora themselves, before the subjects in them.
///
/// The section compared what the games talk about and never the games themselves, so how big
/// each one is, how warmly it is reviewed and how well the classifier does on it were one
/// visit per game away from each other.
fn corpora(out: &mut String, report: &Report) {
    out.push_str("<div class=\"scroll\">\n<table class=\"corpora\">\n<thead><tr>");
    heading(out, "Game", false);
    for column in ["Reviews counted", "Recommended", "Agreement"] {
        heading(out, column, true);
    }
    out.push_str("</tr></thead>\n<tbody>\n");
    for app in &report.apps {
        let measured = app
            .agreement
            .report()
            .and_then(crate::measure::ClaimAgreement::rate);
        // Sorted on the number rather than on the text of it: "982,291" reads as less than
        // "2,000" to anything comparing strings, and a game with no reference set has to sort
        // as unmeasured rather than as zero agreement.
        let sortable = |value: Option<f64>| value.unwrap_or(-1.0);
        let _ = writeln!(
            out,
            "<tr class=\"row\"><th scope=\"row\"><a href=\"#app-{}\">{}</a></th>\
             <td class=\"num\" data-value=\"{}\">{}</td>\
             <td class=\"num\" data-value=\"{:.6}\">{}</td>\
             <td class=\"num\" data-value=\"{:.6}\">{}</td></tr>",
            app.app_id(),
            escape(&app.crawl.title()),
            app.reading.reviews,
            thousands(app.reading.reviews),
            sortable(app.positive_baseline()),
            app.positive_baseline()
                .map_or_else(|| nothing("no reviews"), percent),
            sortable(measured),
            measured.map_or_else(|| nothing(&unmeasured(&app.agreement)), percent)
        );
    }
    out.push_str("</tbody>\n</table>\n</div>\n");
    out.push_str(
        "<p class=\"note\">Agreement is measured over each game's labelled claims, and only \
         over the claims the model was willing to answer, so a game's figure carries a band \
         several points wide and the pooled one at the end of this section is the one worth \
         quoting. It counts agreement with a labeller, which is not the same as being \
         right. A game the model trained on carries no figure: agreeing with labels it \
         learned from would measure memory.</p>\n",
    );
}

/// Why a game carries no agreement figure, in the few words a dash can be read out as.
fn unmeasured(measurement: &crate::report::Measurement) -> String {
    match measurement {
        crate::report::Measurement::Unlabelled => "no reference set".to_owned(),
        crate::report::Measurement::Unscored(_) => "labelled, not scored".to_owned(),
        crate::report::Measurement::Learned => "trained on its labels".to_owned(),
        crate::report::Measurement::Measured(report) => {
            format!("declined all {} labelled claims", report.matched)
        }
    }
}

/// Every category against every game, loudest subject first.
///
/// A census of a genre runs to dozens of games, and a screen holds about eight of them. The
/// answer a row exists to give, which game raises this subject most, was carried only by an
/// outline around one cell, so on a wide table it was usually off the side of the screen and
/// a reader had no way of knowing it was there. Naming that game in a column beside the
/// category means the row still answers its question at any width.
fn matrix(out: &mut String, report: &Report) {
    let mut order: Vec<(&str, &str, u64)> = Vec::new();
    for category in SHEET {
        let pooled: u64 = report
            .apps
            .iter()
            .flat_map(|app| app.reading.subjects.iter())
            .filter(|c| c.id == category.id)
            .map(|c| c.mention_reviews)
            .sum();
        order.push((category.id, category.label, pooled));
    }
    order.sort_by_key(|(_, _, pooled)| std::cmp::Reverse(*pooled));

    out.push_str("<div class=\"scroll\">\n<table class=\"matrix\">\n<thead><tr>");
    out.push_str("<th scope=\"col\">Category</th><th scope=\"col\" class=\"leader\">Highest</th>");
    for app in &report.apps {
        let _ = write!(
            out,
            "<th scope=\"col\" class=\"num\"><a href=\"#app-{}\">{}</a></th>",
            app.app_id(),
            escape(&app.crawl.title())
        );
    }
    out.push_str("</tr></thead>\n<tbody>\n");

    for (id, label, pooled) in order {
        if pooled == 0 {
            continue;
        }
        let rates: Vec<Option<f64>> = report
            .apps
            .iter()
            .map(|app| {
                app.reading
                    .subjects
                    .iter()
                    .find(|c| c.id == id)
                    .and_then(|c| app.rate(c.mention_reviews))
            })
            .collect();
        let loudest = belongs_to(&rates);

        let _ = write!(
            out,
            "<tr data-name=\"{}\"><th scope=\"row\">{}</th>",
            escape(&label.to_lowercase()),
            escape(label)
        );
        match loudest.and_then(|index| Some((report.apps.get(index)?, rates[index]?))) {
            Some((app, rate)) => {
                let _ = write!(
                    out,
                    "<td class=\"leader\"><b>{}</b> <a href=\"#app-{}\">{}</a></td>",
                    percent(rate),
                    app.app_id(),
                    escape(&app.crawl.title())
                );
            }
            None => {
                let _ = write!(
                    out,
                    "<td class=\"leader\">{}</td>",
                    nothing("no game leads this")
                );
            }
        }
        for (index, rate) in rates.iter().enumerate() {
            match *rate {
                Some(rate) => {
                    // Square-rooted so the common categories do not wash every other cell
                    // out; the number is always there for anyone reading exactly.
                    let heat = (rate * 2.0).sqrt().min(1.0);
                    let top = if loudest == Some(index) {
                        " loudest"
                    } else {
                        ""
                    };
                    let _ = write!(
                        out,
                        "<td class=\"num heat{top}\" style=\"--heat:{heat:.3}\">{}{}</td>",
                        percent(rate),
                        if top.is_empty() {
                            ""
                        } else {
                            "<span class=\"read-aloud\">, the highest of these games</span>"
                        }
                    );
                }
                None => {
                    let _ = write!(out, "<td class=\"num\">{}</td>", nothing("no reviews"));
                }
            }
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</tbody>\n</table>\n</div>\n");
}

/// The game a row belongs to, where the table can honestly say there is one.
///
/// Which game a subject belongs to is the question a reader brings to a row, and reading it
/// off six shades of the same colour is guesswork. Two things have to hold before the table
/// answers it. The rate has to be one the page prints as a number rather than as "less than":
/// three reviews in a million beating two is not a subject belonging to a game. And the
/// leader has to be told apart from the runner-up in the figures actually printed, because a
/// reader who sees the same number twice with one of them outlined is being shown a rounding
/// difference dressed as a finding.
fn belongs_to(rates: &[Option<f64>]) -> Option<usize> {
    let (best, highest) = rates
        .iter()
        .enumerate()
        .filter_map(|(index, rate)| rate.map(|rate| (index, rate)))
        .max_by(|a, b| a.1.total_cmp(&b.1))?;
    if highest < TOO_SMALL_TO_RANK {
        return None;
    }
    let leader = percent(highest);
    rates
        .iter()
        .enumerate()
        .all(|(index, rate)| index == best || rate.is_none_or(|rate| percent(rate) != leader))
        .then_some(best)
}

/// The claim the whole tool exists to make, over every game at once.
///
/// Each game's section opens with the same sentence about that game, and a single corpus can
/// always be answered with "that is just that game". Made of every game in the report, it
/// cannot be, and the reader is told how many corpora it took.
fn most_overstated(out: &mut String, report: &Report) {
    let Some((category, factor)) = report.worst_bias() else {
        return;
    };
    let Some(overall) = category.rate() else {
        return;
    };
    let _ = writeln!(
        out,
        "<p class=\"headline\">Across these {games} games the top of the pile overstates \
         <strong>{}</strong> by <strong>{factor:.1}\u{d7}</strong>: {} of the {} reviews at the \
         top of those {games} piles raise it, against {} of all {}.</p>",
        escape(&category.label.to_lowercase()),
        thousands(category.top_mentions),
        thousands(category.top_reviews),
        percent(overall),
        thousands(category.reviews),
        games = report.apps.len()
    );
}

/// The one finding only a table of several games can carry.
///
/// Every game's own section opens with a sentence; this one opened with a grid and left the
/// reader to find the interesting row. What a cross-game view is for is the subject that
/// belongs to one game and not the others, so that is what it now says.
fn widest_apart(out: &mut String, report: &Report) {
    let rate = |app: &AppReport, id: &str| {
        app.reading
            .subjects
            .iter()
            .find(|c| c.id == id)
            .and_then(|c| app.rate(c.mention_reviews))
    };

    let mut widest: Option<(f64, &str, &AppReport, f64, &AppReport, f64)> = None;
    for category in SHEET {
        let mut rates: Vec<(&AppReport, f64)> = report
            .apps
            .iter()
            .filter_map(|app| rate(app, category.id).map(|found| (app, found)))
            .collect();
        rates.sort_by(|a, b| a.1.total_cmp(&b.1));
        let (Some(low), Some(high)) = (rates.first(), rates.last()) else {
            continue;
        };
        let spread = high.1 - low.1;
        if widest.is_none_or(|(found, ..)| spread > found) {
            widest = Some((spread, category.label, high.0, high.1, low.0, low.1));
        }
    }

    let Some((spread, label, most, high, least, low)) = widest else {
        return;
    };
    if spread < TOO_SMALL_TO_RANK {
        return;
    }
    let _ = writeln!(
        out,
        "<p class=\"headline\">The subject these games disagree about most is \
         <strong>{}</strong>: {} of {} reviews raise it, against {} of {}. A rate is about \
         one corpus, and this is what that means.</p>",
        escape(&label.to_lowercase()),
        percent(high),
        escape(&most.crawl.title()),
        percent(low),
        escape(&least.crawl.title())
    );
}

/// What the model is measured to get wrong, over every game at once.
///
/// One game's few hundred labelled claims put a wide band around its own figure. The pooled
/// number is the one worth quoting, and it exists only when every game in the report has
/// labelled claims behind it: pooling the measured ones and reporting that as the set's
/// agreement would quietly be an average over whichever games happen to be labelled.
fn pooled_agreement(out: &mut String, report: &Report) {
    let measured: Vec<crate::measure::ClaimAgreement> = report
        .apps
        .iter()
        .filter_map(|app| app.agreement.report().cloned())
        .collect();
    if measured.len() != report.apps.len() || measured.len() < 2 {
        return;
    }
    agreement_note(out, &crate::measure::pooled(&measured));
}

fn game(out: &mut String, app: &AppReport, several: bool) {
    let _ = writeln!(
        out,
        "<section class=\"game\" id=\"app-{}\">\n<div class=\"wrap\">",
        app.app_id()
    );
    let _ = writeln!(out, "<h2>{}</h2>", escape(&app.crawl.title()));
    facts(out, app);
    headline(out, app);
    in_short(out, app);
    over_time(out, app);
    updates(out, app);
    categories(out, app);
    who_said_it(out, app);
    induced(out, app);
    top_of_the_pile(out, app);
    languages(out, app);
    trust(out, app);
    // A section runs to a hundred rows and the evidence under them, so the way out of one is
    // worth stating rather than leaving to a scrollbar.
    let _ = writeln!(
        out,
        "<p class=\"back\"><a href=\"#{}\">{}</a></p>",
        if several { "games" } else { "main" },
        if several {
            "Back to the games"
        } else {
            "Back to the top"
        }
    );
    out.push_str("</div>\n</section>\n");
}

fn facts(out: &mut String, app: &AppReport) {
    let crawl = &app.crawl;
    out.push_str("<dl class=\"facts\">\n");
    // Two different numbers, and a reader who sees only the smaller one beside a coverage
    // figure taken over the larger one is left to work out why they disagree. Rates are over
    // what was counted; coverage is over what was captured, blank reviews included.
    fact(out, "Reviews counted", &thousands(app.reading.reviews));
    fact(
        out,
        "Captured",
        &format!(
            "{} of the {} Valve reports ({})",
            thousands(crawl.rows_unique),
            thousands(crawl.valve_total_reviews),
            coverage(crawl.coverage)
        ),
    );
    if !crawl.review_score_desc.is_empty() {
        fact(out, "Steam calls it", &crawl.review_score_desc);
    }
    fact(out, "Snapshot", &crate::time::day(crawl.snapshot_unix));
    if let Some(swept) = crawl.swept_unix {
        fact(
            out,
            "Brought up to date",
            &format!(
                "{}, {} added or edited since the snapshot",
                crate::time::day(swept),
                thousands(crawl.rows_swept)
            ),
        );
    }
    out.push_str("</dl>\n");
    // A count of a corpus that has since changed is a count of a corpus nobody can open,
    // and the page has to say so before a reader trusts a rate over it.
    if let Some(swept) = crawl.swept_unix
        && app.reading.captured_unix < swept
    {
        let _ = writeln!(
            out,
            "<p class=\"warn\">The capture was brought up to date on {} and these counts \
             were made before that, on the corpus as it stood on {}. Read it again to count \
             what arrived.</p>",
            crate::time::day(swept),
            crate::time::day(app.reading.captured_unix.max(crawl.snapshot_unix))
        );
    }
}

fn fact(out: &mut String, term: &str, value: &str) {
    let _ = writeln!(
        out,
        "<div><dt>{}</dt><dd>{}</dd></div>",
        escape(term),
        escape(value)
    );
}

/// The finding, in a sentence, before any table asks anyone to read a number.
fn headline(out: &mut String, app: &AppReport) {
    let Some((category, factor)) = app.worst_bias() else {
        return;
    };
    let Some(overall) = app.rate(category.mention_reviews) else {
        return;
    };

    // The sentence names a category and the reader's next question is always which reviews,
    // so the name is the way to them rather than something to go hunting for in the table.
    let named = if app.examples.iter().any(|(id, _)| *id == category.id) {
        format!(
            "<a href=\"#panel-{}-{}\"><strong>{}</strong></a>",
            app.app_id(),
            escape(&category.id),
            escape(&category.label.to_lowercase())
        )
    } else {
        format!(
            "<strong>{}</strong>",
            escape(&category.label.to_lowercase())
        )
    };

    // Counted rather than given as a share of the top of the pile. A few dozen reviews is a
    // number a reader can hold, and the point of the sentence is that a claim about a whole
    // corpus is being made out of a handful, which "10.0% of them" hides.
    out.push_str("<p class=\"headline\">\n");
    let _ = write!(
        out,
        "The top of the pile overstates {named} by <strong>{factor:.1}\u{d7}</strong>: {} of \
         the {} reviews Steam ranks most helpful raise it, against {} of all {}.",
        thousands(category.top_mention_reviews),
        thousands(app.reading.top_helpful),
        percent(overall),
        thousands(app.reading.reviews),
    );
    out.push_str("\n</p>\n");
}

/// The game in a paragraph, for the reader who will not read the table. Every clause is a
/// count from the reading with words around it, and it is drawn from the same figures the
/// table shows, so the two cannot disagree.
fn in_short(out: &mut String, app: &AppReport) {
    let text = crate::picture::in_short(&app.reading);
    if text.is_empty() {
        return;
    }
    let _ = writeln!(out, "<p class=\"in-short\">{}</p>", escape(&text));
}

/// The month the line falls furthest, as a sentence rather than as a shape to point at.
///
/// Months too small to carry a rate are left out for the same reason they are left out of the
/// sparkline: four reviews and one thumb up is 25% and is not the month a game was hated in.
///
/// A game whose launch month is both its busiest and its angriest, which is most of the ones
/// worth reporting on, would otherwise have that month and its size named twice in three
/// lines.
fn worst_month(months: &[crate::read::Month], busiest: Option<&crate::read::Month>) -> String {
    let coldest = months
        .iter()
        .filter(|month| month.reviews >= ENOUGH_FOR_A_RATE)
        .filter_map(|month| month.positive_share().map(|share| (month, share)))
        .min_by(|a, b| a.1.total_cmp(&b.1));
    coldest.map_or_else(String::new, |(month, share)| {
        if busiest.is_some_and(|peak| peak.label == month.label) {
            return format!(
                " It falls furthest in that same month, when {} of them recommended the game.",
                percent(share)
            );
        }
        format!(
            " It falls furthest in {}, when {} of {} reviews recommended the game.",
            escape(&crate::time::month_name(&month.label)),
            percent(share),
            thousands(month.reviews)
        )
    })
}

/// Reviews per month, and how many of them recommended the game.
///
/// A mention rate is one number for a corpus that took years to gather. A game review-bombed
/// for a fortnight and quiet since produces much the same rate as one grumbled about steadily
/// for a decade, and a reader shown only the rate cannot tell which they are looking at.
///
/// Drawn as inline SVG: a chart that fetched a plotting library would not be a self-contained
/// page, and this one is two shapes.
fn over_time(out: &mut String, app: &AppReport) {
    let months = &app.reading.months;
    if months.len() < 2 {
        return;
    }
    let first = crate::time::month_name(&months[0].label);
    let last = crate::time::month_name(&months[months.len() - 1].label);
    // The chart is drawn against its own busiest month and nothing else says how big that is,
    // which leaves every bar on it a shape with no size.
    let peak = months.iter().max_by_key(|month| month.reviews);
    let tallest = peak.map_or(1, |month| month.reviews).max(1);

    out.push_str("<h3>When it was said</h3>\n");
    let _ = writeln!(
        out,
        "<p class=\"note\">Reviews per month, {} to {}, against a busiest month of {} in {}. \
         The line is the share of each month that recommended the game, from none at the \
         bottom to all at the top; the dashed line is half.{} Point at a month to read \
         it.</p>",
        escape(&first),
        escape(&last),
        thousands(tallest),
        escape(&peak.map_or_else(String::new, |month| crate::time::month_name(&month.label))),
        // The dip in that line is what a reader looks for and the one thing on the chart a
        // pointer is needed to read. It costs a clause to say, and a pointer is a thing not
        // every reader has.
        worst_month(months, peak)
    );

    #[expect(
        clippy::cast_precision_loss,
        reason = "a corpus spans hundreds of months at most"
    )]
    let step = WIDTH / months.len() as f64;

    let _ = writeln!(
        out,
        "<figure class=\"timeline\">\n<svg viewBox=\"0 0 {WIDTH:.0} {HEIGHT:.0}\" \
         preserveAspectRatio=\"none\" role=\"img\" \
         aria-label=\"Reviews per month from {} to {}\">",
        escape(&first),
        escape(&last)
    );

    for (index, month) in months.iter().enumerate() {
        #[expect(
            clippy::cast_precision_loss,
            reason = "counts and month numbers are both far below 2^53"
        )]
        let (x, tall) = (
            index as f64 * step,
            month.reviews as f64 / tallest as f64 * HEIGHT,
        );
        let _ = writeln!(
            out,
            "<rect class=\"bar\" x=\"{x:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{tall:.2}\" />",
            HEIGHT - tall,
            (step * 0.82).max(0.5)
        );
    }

    // The share line is halfway up when half a month recommended the game, which is the
    // reading nobody can do off a line with nothing to measure it against.
    let _ = writeln!(
        out,
        "<line class=\"midline\" x1=\"0\" y1=\"{0:.2}\" x2=\"{WIDTH:.0}\" y2=\"{0:.2}\" />",
        HEIGHT / 2.0
    );

    let line: Vec<String> = months
        .iter()
        .enumerate()
        .filter_map(|(index, month)| {
            let share = month.positive_share()?;
            #[expect(
                clippy::cast_precision_loss,
                reason = "a corpus spans hundreds of months at most"
            )]
            let x = index as f64 * step + step / 2.0;
            Some(format!("{x:.2},{:.2}", (1.0 - share) * HEIGHT))
        })
        .collect();
    if line.len() > 1 {
        let _ = writeln!(
            out,
            "<polyline class=\"share\" points=\"{}\" />",
            line.join(" ")
        );
    }
    update_marks(out, app, step);
    month_targets(out, months, step);

    let _ = writeln!(
        out,
        // The two ends of the axis sit at the two ends of the caption, and read as one word
        // to anything that hears the page rather than seeing it.
        "</svg>\n<figcaption><span>{}</span><span class=\"read-aloud\"> to </span>\
         <span>{}</span></figcaption>\n</figure>",
        escape(&first),
        escape(&last)
    );
}

/// Last, and the full height of the chart: a quiet month is a bar one pixel tall, which is
/// nothing to aim at. These are what the pointer actually finds.
fn month_targets(out: &mut String, months: &[Month], step: f64) {
    for (index, month) in months.iter().enumerate() {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a corpus spans hundreds of months at most"
        )]
        let x = index as f64 * step;
        let _ = writeln!(
            out,
            "<rect class=\"hit\" x=\"{x:.2}\" y=\"0\" width=\"{:.2}\" height=\"{HEIGHT:.0}\">\
             <title>{}: {} reviews, {} recommended</title></rect>",
            step.max(0.5),
            escape(&crate::time::month_name(&month.label)),
            thousands(month.reviews),
            month
                .positive_share()
                .map_or_else(|| "no".to_owned(), percent)
        );
    }
}

/// A line down the chart where each update was posted, `step` wide a month. Drawn under the
/// pointer's targets, so a month stays readable where an update falls in it; the list under the
/// chart names each one.
fn update_marks(out: &mut String, app: &AppReport, step: f64) {
    for around in &app.updates.around {
        let Some(at) = crate::before_after::position(around.update.posted, &app.reading.months)
        else {
            continue;
        };
        let _ = writeln!(
            out,
            "<line class=\"update\" x1=\"{0:.2}\" y1=\"0\" x2=\"{0:.2}\" y2=\"{HEIGHT:.0}\" />",
            at * step
        );
    }
}

/// Updates whose four weeks either side the page sets out; the rest are listed.
const UPDATES_COMPARED: usize = 4;

/// The updates the developer posted, and what changed across the biggest of them.
fn updates(out: &mut String, app: &AppReport) {
    use crate::before_after::{ENOUGH, WINDOW_DAYS, biggest};

    out.push_str("<h3>Around its updates</h3>\n");
    let Some(asked) = app.updates.asked else {
        out.push_str(
            "<p class=\"note\">Steam has not been asked for the updates this game's developer \
             posted. Bringing the game up to date asks.</p>\n",
        );
        return;
    };
    let around = &app.updates.around;
    if around.is_empty() {
        let _ = writeln!(
            out,
            "<p class=\"note\">Nothing this game's developer had posted on Steam by {} reads as \
             an update.</p>",
            crate::time::day(asked)
        );
        return;
    }
    let _ = writeln!(
        out,
        "<p class=\"note\">The dashed lines on the chart are the {} updates this game's developer \
         posted on Steam by {}: posts Steam marks as patch notes, and posts whose titles name a \
         patch, a hotfix, an update or a version. Below, the biggest of them by the reviews that \
         followed, each with the {WINDOW_DAYS} days before it against the {WINDOW_DAYS} days \
         after. A change is a share that moved further than chance would move it, three \
         standard errors and at least two points, and a share is compared only where each side \
         holds at least {ENOUGH} reviews. A change happened across the update; that alone does \
         not make the update the reason for it.</p>",
        thousands(around.len() as u64),
        crate::time::day(asked)
    );
    let chosen = biggest(around, UPDATES_COMPARED);
    if chosen.is_empty() {
        let _ = writeln!(
            out,
            "<p class=\"note\">None of them has {ENOUGH} reviews on each side, so none is \
             compared.</p>"
        );
    }
    for one in chosen {
        update_findings(out, one);
    }

    let _ = writeln!(
        out,
        "<details class=\"every-update\">\n<summary>Every update ({})</summary>\n\
         <ol class=\"updates-list\">",
        thousands(around.len() as u64)
    );
    for one in around.iter().rev() {
        let said = if one.enough {
            match changes_of(one) {
                0 => "nothing changed beyond chance".to_owned(),
                1 => "1 change".to_owned(),
                many => format!("{many} changes"),
            }
        } else {
            "too few reviews either side to compare".to_owned()
        };
        let _ = writeln!(
            out,
            "<li><span class=\"when\">{}</span> <span>{} <span class=\"said\">{said}</span></span></li>",
            crate::time::day(one.update.posted),
            steam_post(&one.update)
        );
    }
    out.push_str("</ol>\n</details>\n");
}

/// The changes across an update, the share recommending the game among them.
fn changes_of(one: &crate::before_after::Around) -> usize {
    one.changes + usize::from(one.recommended.is_some_and(|share| share.change))
}

/// An update's title, linked to its post on Steam.
fn steam_post(update: &crate::updates::Update) -> String {
    format!(
        "<a href=\"{}\" rel=\"noopener noreferrer\" target=\"_blank\">{}</a>",
        escape(&update.link),
        escape(&update.title)
    )
}

/// One update's four weeks either side, as sentences: a share is said only where it changed.
fn update_findings(out: &mut String, one: &crate::before_after::Around) {
    use crate::before_after::WINDOW_DAYS;

    out.push_str("<div class=\"update-around\">\n");
    let _ = writeln!(
        out,
        "<h4>{} <span class=\"when\">{}</span></h4>",
        steam_post(&one.update),
        crate::time::day(one.update.posted)
    );
    let after = if one.after_whole {
        format!("the {WINDOW_DAYS} days after")
    } else {
        let days = (one.after.to - one.after.from) / 86_400;
        format!("the {days} days after it that the capture holds")
    };
    let nearby = match one.nearby {
        0 => String::new(),
        1 => " One other update was posted within these weeks, and they hold its effect too."
            .to_owned(),
        many => format!(
            " {many} other updates were posted within these weeks, and they hold their effects \
             too."
        ),
    };
    let _ = writeln!(
        out,
        "<p class=\"note\">{} reviews in the {WINDOW_DAYS} days before, {} in {after}.{nearby}</p>",
        thousands(one.before.reviews),
        thousands(one.after.reviews)
    );
    if changes_of(one) == 0 {
        out.push_str(
            "<p class=\"note\">Nothing changed beyond chance: not the share recommending the \
             game, and not the praise or complaints of any subject.</p>\n</div>\n",
        );
        return;
    }
    out.push_str("<ul class=\"changes\">\n");
    let moved = |share: &crate::before_after::Compared| {
        format!(
            "{} from {} to {}",
            if share.after > share.before {
                "rose"
            } else {
                "fell"
            },
            percent(share.before),
            percent(share.after)
        )
    };
    if let Some(share) = one.recommended.filter(|share| share.change) {
        let _ = writeln!(
            out,
            "<li class=\"change\"><strong>Recommending the game</strong>: {} of reviews</li>",
            moved(&share)
        );
    }
    for subject in &one.subjects {
        for (side, share) in [
            ("praise", subject.praise),
            ("complaints", subject.complaint),
        ] {
            if share.change {
                let _ = writeln!(
                    out,
                    "<li class=\"change {}\"><strong>{}</strong>: {side} {} of reviews</li>",
                    if (side == "praise") == (share.after > share.before) {
                        "better"
                    } else {
                        "worse"
                    },
                    escape(subject.label),
                    moved(&share)
                );
            }
        }
    }
    out.push_str("</ul>\n</div>\n");
}

fn categories(out: &mut String, app: &AppReport) {
    out.push_str("<h3>What players talk about</h3>\n");
    if let Some(baseline) = app.positive_baseline() {
        let _ = writeln!(
            out,
            "<p class=\"note baseline\">{} of all {} reviews recommend this game. Every figure \
             in the last column is worth reading against that.</p>",
            percent(baseline),
            thousands(app.reading.reviews)
        );
    }
    out.push_str(
        "<p class=\"note\">A review counts towards every category it says something about, so \
         these add up to more than 100%. Select a row to read the reviews behind it.</p>\n",
    );
    legend(out, app);

    let mut rows: Vec<&SubjectCount> = app.reading.subjects.iter().collect();
    rows.sort_by_key(|category| std::cmp::Reverse(category.mention_reviews));
    let widest = rows.first().map_or(0, |c| c.mention_reviews).max(1);

    out.push_str("<div class=\"scroll\">\n<table class=\"categories\">\n<thead><tr>");
    heading(out, "Category", false);
    for (name, _) in COLUMNS {
        heading(out, name, true);
    }
    out.push_str("</tr></thead>\n<tbody>\n");

    for category in rows {
        category_row(out, app, category, widest);
    }
    out.push_str("</tbody>\n</table>\n</div>\n");
}

/// What each column means, folded away.
///
/// Five columns need five definitions and two of them were going unexplained, but a reader
/// who already knows them should not have to scroll past six lines of prose in every one of
/// six sections to reach the table. A native disclosure: it needs no scripting, and printing
/// opens it along with everything else folded.
fn legend(out: &mut String, app: &AppReport) {
    out.push_str(
        "<details class=\"legend\">\n\
         <summary>What the columns mean, and where the quoted reviews come from</summary>\n\
         <dl>\n",
    );
    let top = thousands(app.reading.top_helpful);
    for (name, meaning) in COLUMNS {
        let _ = writeln!(
            out,
            "<div><dt>{name}</dt><dd>{}</dd></div>",
            meaning.replace("{top}", &top)
        );
    }
    out.push_str("</dl>\n");
    out.push_str(
        "<p>The reviews behind a row are a couple from the top of the pile where the category \
         reaches it, and the rest drawn at random from everything filed there, so they are \
         evidence rather than a selection.</p>\n</details>\n",
    );
}

/// A column header that can reorder the table.
///
/// A button rather than a clickable cell, so it is reachable and announced without the page
/// reinventing what a button is. With scripting off it is an inert label and the table keeps
/// the order it was rendered in.
fn heading(out: &mut String, label: &str, numeric: bool) {
    let class = if numeric { " class=\"num\"" } else { "" };
    let _ = write!(
        out,
        "<th scope=\"col\"{class} aria-sort=\"none\">\
         <button type=\"button\" class=\"sort\" data-sort>{}\
         <span class=\"arrow\" aria-hidden=\"true\"></span></button></th>",
        escape(label)
    );
}

fn category_row(out: &mut String, app: &AppReport, category: &SubjectCount, widest: u64) {
    let examples = app
        .examples
        .iter()
        .find(|(id, _)| *id == category.id)
        .map(|(_, quoted)| quoted.as_slice())
        .unwrap_or_default();
    let has_examples = !examples.is_empty();
    let panel = format!("panel-{}-{}", app.app_id(), category.id);

    let _ = write!(
        out,
        "<tr class=\"row\" data-name=\"{}\"",
        escape(&category.label.to_lowercase())
    );
    // Named as expandable but not dressed as a control. A `role` here would take the row out
    // of the table for a screen reader, which is a worse trade than it sounds: the cells stop
    // being cells. The control that opens it is built by the script, which is also the only
    // circumstance in which there is anything to open.
    if has_examples {
        let _ = write!(out, " data-expands=\"{panel}\"");
    }
    out.push('>');

    let measured = app
        .agreement
        .report()
        .and_then(|report| report.subjects.iter().find(|s| s.id == category.id));

    let _ = write!(
        out,
        "<th scope=\"row\"><span class=\"name\">{}{}{}</span></th>",
        escape(&category.label),
        measured.map(thinly_measured).unwrap_or_default(),
        if has_examples {
            "<span class=\"chevron\" aria-hidden=\"true\"></span>"
        } else {
            ""
        }
    );

    #[expect(
        clippy::cast_precision_loss,
        reason = "review counts are far below 2^53"
    )]
    let share = category.mention_reviews as f64 / widest as f64;
    let _ = write!(
        out,
        "<td class=\"num bar-cell\" data-value=\"{}\">\
         <span class=\"bar\" style=\"--fill:{:.4}\"></span>\
         <span class=\"value\">{}</span><span class=\"count\">{}</span></td>",
        category.mention_reviews,
        share,
        app.rate(category.mention_reviews)
            .map_or_else(|| nothing("no reviews"), percent),
        thousands(category.mention_reviews)
    );
    rate_cell(out, app.rate(category.primary_reviews));
    rate_cell(out, app.top_rate(category.top_mention_reviews));
    bias_cell(out, app.bias(category));
    polarity_cell(out, category);
    verdict_cell(out, category, app.positive_baseline());
    out.push_str("</tr>\n");

    if has_examples {
        let _ = writeln!(out, "<tr class=\"panel\" id=\"{panel}\"><td colspan=\"7\">");
        if let Some(measured) = measured {
            // The share of all claims the model filed here, declined ones included, because
            // the labels the correction rests on were measured over declined claims too.
            let observed = (app.reading.claims > 0).then(|| {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "claim counts are far below 2^53"
                )]
                let share = category.claims as f64 / app.reading.claims as f64;
                share
            });
            how_well_this_row_is_known(out, measured, observed);
        }
        sparkline(out, app, &category.id);
        what_they_said(out, app, category, examples);
        out.push_str("</td></tr>");
    }
}

use crate::measure::{ENOUGH_TO_CORRECT_A_ROW, ENOUGH_TO_JUDGE_A_ROW};

/// Recall below which the number in this row is standing on very little.
const THINLY_FOUND: f64 = 0.25;

/// A mark against a rate the model is measured to miss most of.
///
/// Only on the rows that earn it. Decorating every row with its own score would make the
/// table harder to read and the warning worth less exactly where it matters.
fn thinly_measured(measured: &crate::measure::SubjectAgreement) -> String {
    if measured.labelled < ENOUGH_TO_JUDGE_A_ROW {
        return String::new();
    }
    let Some(recall) = measured.recall() else {
        return String::new();
    };
    if recall >= THINLY_FOUND {
        return String::new();
    }
    format!(
        "<span class=\"thin\" title=\"Found in only {} of the claims measured to make it, \
         so this rate is a floor rather than a count\">\u{2757}\
         <span class=\"read-aloud\">measured to miss most of this subject</span></span>",
        percent(recall)
    )
}

/// What the reference set says about this row alone.
///
/// The page carries one figure for how often the model agrees overall, and that figure is no
/// guide at all to a particular row: the same run finds nine claims in ten of one subject and
/// one in twenty of another.
fn how_well_this_row_is_known(
    out: &mut String,
    measured: &crate::measure::SubjectAgreement,
    observed_claim_share: Option<f64>,
) {
    if measured.labelled == 0 {
        let _ = writeln!(
            out,
            "<p class=\"note\">No labelled claim is about this subject, so nothing here is \
             measured.</p>"
        );
        return;
    }
    let found = measured
        .recall()
        .map_or_else(|| "none of them".to_owned(), percent);
    let right = measured
        .precision()
        .map_or_else(|| "nothing it claimed".to_owned(), percent);
    let class = if measured.labelled >= ENOUGH_TO_JUDGE_A_ROW
        && measured.recall().is_some_and(|r| r < THINLY_FOUND)
    {
        "note warn"
    } else {
        "note"
    };
    let _ = writeln!(
        out,
        "<p class=\"{class}\">Measured on this row: of the {} labelled claims about it, the \
         model found {found}; of the claims it filed here, {right} were labelled that way. A \
         rate built on a subject it misses is a floor, not a count.{}</p>",
        thousands(measured.labelled),
        // Which subject the misses went to is the difference between a row that is merely hard
        // and a row whose claims are sitting under a neighbour's name.
        measured
            .mistaken_for
            .map_or_else(String::new, |(label, count)| {
                format!(
                    " Where it was read wrongly, it was most often read as \
                     <strong>{}</strong> ({count} of the labelled claims).",
                    escape(label)
                )
            })
    );

    // The measured errors are not only a warning, they are an estimate of the true rate. The
    // observed share is too high by what was filed here wrongly and too low by what was missed
    // or declined, and both are measured, so the arithmetic is the one every prevalence study
    // does. Only where the model finds the subject clearly better than chance: otherwise the
    // observed share carries no information about the true one and the figure is invented.
    if measured.labelled >= ENOUGH_TO_CORRECT_A_ROW
        && let Some(observed) = observed_claim_share
    {
        match measured.corrected(observed) {
            Some(corrected) => {
                let _ = writeln!(
                    out,
                    "<p class=\"note\">Corrected for those errors, the share of claims about \
                     this is about <strong>{}</strong>, against the {} the model read. That is \
                     an estimate from {} labels, and it moves with them.</p>",
                    percent(corrected),
                    percent(observed),
                    thousands(measured.labelled)
                );
            }
            None => {
                let _ = writeln!(
                    out,
                    "<p class=\"note\">The model finds this subject little better than chance, \
                     so its rate cannot be corrected: the share it read says almost nothing \
                     about the share there is.</p>"
                );
            }
        }
    }
}

/// One category's mention rate month by month.
///
/// The same argument as the volume chart, one level down: a category at 5% of a corpus may
/// have been 40% of one month and absent since, and only the shape says which.
fn sparkline(out: &mut String, app: &AppReport, id: &str) {
    let Some(slot) = SHEET.iter().position(|c| c.id == id) else {
        return;
    };
    let months = &app.reading.months;
    if months.len() < 3 {
        return;
    }
    // A month with three reviews in it can be 100% of anything, and one such month would set
    // the scale for every month that has something to say.
    let rates: Vec<Option<f64>> = months
        .iter()
        .map(|month| {
            (month.reviews >= ENOUGH_FOR_A_RATE)
                .then(|| month.rate(slot))
                .flatten()
        })
        .collect();
    let peak = rates
        .iter()
        .flatten()
        .copied()
        .fold(0.0_f64, f64::max)
        .max(f64::EPSILON);

    #[expect(
        clippy::cast_precision_loss,
        reason = "a corpus spans hundreds of months at most"
    )]
    let step = WIDTH / (months.len() - 1).max(1) as f64;
    // Kept on the corpus's own axis rather than stretched across the months that survived, so
    // a line that stops halfway means the game went quiet there and not that the subject did.
    let drawn: Vec<usize> = rates
        .iter()
        .enumerate()
        .filter_map(|(index, rate)| rate.map(|_| index))
        .collect();
    let points: Vec<String> = drawn
        .iter()
        .filter_map(|&index| {
            let rate = rates[index]?;
            #[expect(
                clippy::cast_precision_loss,
                reason = "a corpus spans hundreds of months at most"
            )]
            let x = index as f64 * step;
            Some(format!("{x:.2},{:.2}", (1.0 - rate / peak) * SPARK_HEIGHT))
        })
        .collect();
    let (Some(&opens), Some(&closes)) = (drawn.first(), drawn.last()) else {
        return;
    };
    if points.len() < 3 {
        return;
    }

    let name = |index: usize| escape(&crate::time::month_name(&months[index].label));
    let _ = writeln!(
        out,
        "<figure class=\"spark\">\n<svg viewBox=\"0 0 {WIDTH:.0} {SPARK_HEIGHT:.0}\" \
         preserveAspectRatio=\"none\" role=\"img\" aria-label=\"Mention rate by month\">\
         <line class=\"axis\" x1=\"0\" y1=\"{SPARK_HEIGHT:.0}\" x2=\"{WIDTH:.0}\" \
         y2=\"{SPARK_HEIGHT:.0}\" />\
         <polyline points=\"{}\" /></svg>\n\
         <figcaption>Mention rate by month, peaking at {}. Drawn from {} to {} on an axis \
         running to {}: a month with fewer than {ENOUGH_FOR_A_RATE} reviews carries no \
         rate.</figcaption>\n</figure>",
        points.join(" "),
        percent(peak),
        name(opens),
        name(closes),
        name(months.len() - 1)
    );
}

/// A rate, carrying the number it was formatted from so the table can be reordered by it.
fn rate_cell(out: &mut String, rate: Option<f64>) {
    match rate {
        Some(rate) => {
            let _ = write!(
                out,
                "<td class=\"num\" data-value=\"{rate:.9}\">{}</td>",
                percent(rate)
            );
        }
        None => {
            let _ = write!(
                out,
                "<td class=\"num\" data-value=\"-1\">{}</td>",
                nothing("no reviews")
            );
        }
    }
}

/// Bias is the point, so it gets a bar that leaves the middle rather than a bare number.
fn bias_cell(out: &mut String, factor: Option<f64>) {
    let Some(factor) = factor else {
        let _ = write!(
            out,
            "<td class=\"num\" data-value=\"-1\">{}</td>",
            nothing("not measured")
        );
        return;
    };
    // Log scale: twice as often and half as often are the same distance from the middle,
    // which a linear scale would draw as wildly different.
    let offset = (factor.log2() / 3.0).clamp(-1.0, 1.0);
    let side = if offset >= 0.0 { "over" } else { "under" };
    let _ = write!(
        out,
        "<td class=\"num bias\" data-value=\"{factor:.6}\"><span class=\"gauge {side}\" style=\"--offset:{:.4}\"></span>\
         <span class=\"value\">{factor:.1}\u{d7}</span></td>",
        offset.abs()
    );
}

/// Whether a topic is raised by people recommending the game or refusing to.
///
/// Shown against the corpus baseline, because a category where 80% recommend the game is
/// only interesting once a reader knows whether 80% is high or low for that game.
///
/// Coloured against the interval the count is entitled to rather than against a fixed
/// distance from the baseline. Eight reviews all recommending the game is 100% and says
/// nothing; a fixed threshold paints it the same green as a thousand reviews at 98%, which
/// is the one reading the column exists to prevent.
/// What the reviews raising a subject actually say about it, as three shares in one bar.
///
/// The mixed share is the point of the column. A subject a third of players praise and a third
/// complain about is a fight; one where every review says both is a subject with a real
/// trade-off in it, and a single positive share renders those two identically.
///
/// Sorted on the complaint share, because a table of subjects is read looking for what is
/// wrong, and sorting on praise puts the answer at the bottom.
fn polarity_cell(out: &mut String, category: &SubjectCount) {
    let said = category.praised + category.criticised + category.mixed;
    if said == 0 {
        let _ = write!(
            out,
            "<td class=\"num\" data-value=\"-1\">{}</td>",
            nothing("none raised it")
        );
        return;
    }

    let share = |part: u64| share_of(part, said);
    let (praise, gripe, both) = (
        share(category.praised),
        share(category.criticised),
        share(category.mixed),
    );
    let _ = write!(
        out,
        "<td class=\"num said\" data-value=\"{gripe:.9}\">\
         <span class=\"mix\" aria-hidden=\"true\">\
         <span class=\"praise\" style=\"--part:{praise:.4}\"></span>\
         <span class=\"gripe\" style=\"--part:{gripe:.4}\"></span>\
         <span class=\"both\" style=\"--part:{both:.4}\"></span></span>\
         <span class=\"value\">{} praise</span>\
         <span class=\"count\">{} gripe, {} both</span></td>",
        percent(praise),
        percent(gripe),
        percent(both)
    );
}

fn verdict_cell(out: &mut String, category: &SubjectCount, baseline: Option<f64>) {
    let Some(share) = category.positive_share() else {
        let _ = write!(
            out,
            "<td class=\"num\" data-value=\"-1\">{}</td>",
            nothing("none raised it")
        );
        return;
    };
    let spread = crate::measure::wilson(category.positive_mentions, category.mention_reviews);
    let tone = match (baseline, spread) {
        (Some(baseline), Some((low, _))) if low > baseline => " warmer",
        (Some(baseline), Some((_, high))) if high < baseline => " colder",
        _ => "",
    };
    let _ = write!(
        out,
        "<td class=\"num verdict-share{tone}\" data-value=\"{share:.9}\">{}</td>",
        percent(share)
    );
}

/// Differences listed under a game before the rest are counted rather than shown. A large game
/// clears the bar on dozens of them, and the clearest are the ones a reader acts on.
const DIFFERENCES_SHOWN: usize = 20;

/// Each kind of reviewer, how many of them there are and how many recommend the game, and every
/// subject one kind praises or complains about more or less often than everyone else by more
/// than chance.
fn who_said_it(out: &mut String, app: &AppReport) {
    use crate::who::{CLEAR, ENOUGH};

    out.push_str("<h3>Who said it</h3>\n");
    let listed = crate::who::kinds(&app.reading);
    if listed.is_empty() {
        out.push_str(
            "<p class=\"note\">Who wrote each review was not counted when this game was read. \
             Reading it again counts it.</p>\n",
        );
        return;
    }
    let _ = writeln!(
        out,
        "<p class=\"note\">Steam records beside every review how long its writer had played, \
         whether mostly on a Steam Deck, whether the game was in early access, and whether they \
         got it free. Each kind of reviewer is set against everyone else, and a difference is \
         listed only where it is {CLEAR:.0} times the gap chance alone typically makes and at \
         least {:.0} points wide. A kind with fewer than {ENOUGH} reviews is not compared.</p>",
        crate::moves::WORTH_SAYING * 100.0
    );

    out.push_str("<div class=\"scroll fits\">\n<table class=\"who\">\n<thead><tr>");
    out.push_str(
        "<th scope=\"col\">Who wrote it</th><th scope=\"col\" class=\"num\">Reviews</th>\
         <th scope=\"col\" class=\"num\">Recommended</th></tr></thead>\n",
    );
    for split in &listed {
        let _ = writeln!(
            out,
            "<tbody><tr class=\"split\"><th scope=\"colgroup\" colspan=\"3\">{}</th></tr>",
            escape(split.label)
        );
        for kind in &split.kinds {
            let _ = write!(
                out,
                "<tr{}><th scope=\"row\">{}</th><td class=\"num\">{}</td>",
                if kind.enough {
                    ""
                } else {
                    " class=\"too-few\""
                },
                escape(kind.label),
                thousands(kind.reviews)
            );
            // A kind carries a share against the rest only where both hold enough reviews, and a
            // gap of no standard errors is never clear, so the sign alone says which way it leans.
            match kind.recommended {
                Some(gap) => {
                    let tone = match (gap.clear, gap.z.is_sign_positive()) {
                        (true, true) => " warmer",
                        (true, false) => " colder",
                        (false, _) => "",
                    };
                    let _ = writeln!(
                        out,
                        "<td class=\"num verdict-share{tone}\">{}</td></tr>",
                        percent(gap.share)
                    );
                }
                _ => {
                    let _ = writeln!(
                        out,
                        "<td class=\"num\">{}</td></tr>",
                        nothing(if kind.enough {
                            "too few others to set it against"
                        } else {
                            "too few to say"
                        })
                    );
                }
            }
        }
        out.push_str("</tbody>\n");
    }
    out.push_str("</table>\n</div>\n");

    out.push_str("<h4>Where they differ</h4>\n");
    let found = crate::who::findings(&app.reading);
    if found.is_empty() {
        out.push_str(
            "<p class=\"note\">No kind of reviewer praises, complains about or recommends \
             anything more or less often than everyone else by more than chance.</p>\n",
        );
        return;
    }
    out.push_str("<ul class=\"differ\">\n");
    for finding in found.iter().take(DIFFERENCES_SHOWN) {
        let _ = writeln!(out, "<li>{}</li>", escape(&finding.sentence));
    }
    out.push_str("</ul>\n");
    if found.len() > DIFFERENCES_SHOWN {
        let _ = writeln!(
            out,
            "<p class=\"note\">The {DIFFERENCES_SHOWN} widest beyond chance of {}.</p>",
            thousands(found.len() as u64)
        );
    }
}

/// What this game's players talk about that no game shares.
///
/// Shown apart from the table above, and without a rate, because these have no readings: a
/// subject was found by reading a hundred reviews chosen to be as unlike each other as the
/// corpus allows, and the model that counts the table has never been trained on it. What can
/// honestly be shown is the subject, a sentence on it, and the reviews it was found in.
fn induced(out: &mut String, app: &AppReport) {
    if app.induced.is_empty() {
        return;
    }
    out.push_str("<h3>What this game's players talk about that others' do not</h3>\n");
    let _ = writeln!(
        out,
        "<p class=\"note\">Found by reading reviews of this game chosen to be as unlike each \
         other as possible, and named only where at least three of them raise the same thing. \
         These rows carry no rate: the model that counts the table above has not been trained \
         on them, so what is shown is the finding and the reviews it rests on.</p>"
    );
    out.push_str("<dl class=\"induced\">\n");
    for found in &app.induced {
        let subject = &found.subject;
        let _ = write!(out, "<div class=\"found\"><dt>{}", escape(&subject.label));
        if let Some(parent) = subject
            .refines
            .as_deref()
            .and_then(|id| SHEET.iter().find(|c| c.id == id))
        {
            let _ = write!(
                out,
                " <span class=\"chip\">a form of {}</span>",
                escape(parent.label)
            );
        }
        let _ = writeln!(out, "</dt>\n<dd><p>{}</p>", escape(&subject.description));
        if !found.reviews.is_empty() {
            let _ = writeln!(
                out,
                "<details><summary>{} of the {} reviews it was found in</summary>",
                found.reviews.len(),
                subject.evidence.len()
            );
            out.push_str("<ol class=\"reviews\">\n");
            for review in &found.reviews {
                // The whole review, with no polarity and no confidence, because the
                // counting model never read it and the renderer shows neither for one it
                // did not.
                let example = Example {
                    review: review.clone(),
                    claim: review.text.clone(),
                    at: (0, 0),
                    polarity: String::new(),
                    confidence: 0.0,
                    also: Vec::new(),
                    from_the_top: false,
                };
                self::review(out, app, &example);
            }
            out.push_str("</ol>\n</details>\n");
        }
        out.push_str("</dd></div>\n");
    }
    out.push_str("</dl>\n");
}

fn top_of_the_pile(out: &mut String, app: &AppReport) {
    if app.top.is_empty() {
        return;
    }
    out.push_str("<h3>The top of the pile</h3>\n");
    let _ = writeln!(
        out,
        "<p class=\"note\">The {} reviews Steam ranks as most helpful, which is roughly what a \
         reader sees before deciding. Every rate above is measured against the whole corpus \
         instead.</p>",
        thousands(app.reading.top_helpful)
    );
    // A native disclosure rather than a scripted one: it folds a long list away without the
    // page needing to work for it, and it still opens when scripting is off.
    // Counted from what is about to be listed rather than from what was measured, since a
    // review whose text has gone missing would otherwise leave the summary claiming one more
    // than the reader can find.
    let _ = writeln!(
        out,
        "<details class=\"pile\">\n<summary>Read all {} of them</summary>",
        thousands(app.top.len() as u64)
    );
    reviews(out, app, &app.top);
    out.push_str("</details>\n");
}

fn reviews(out: &mut String, app: &AppReport, examples: &[Example]) {
    out.push_str("<ol class=\"reviews\">\n");
    for example in examples {
        review(out, app, example);
    }
    out.push_str("</ol>\n");
}

/// The evidence behind a subject, grouped by what it says.
///
/// A reader opening a row wants to know what people praise and what they complain about, and
/// eight claims in a single list make them work that out for themselves. Sorted into praise
/// and complaint, with the counts of each beside the heading, the list answers the question
/// rather than containing the answer. Above each list, the words that side uses and the other
/// does not: nobody's paraphrase, just what was said more on this side than on that one,
/// counted by reviewers.
fn what_they_said(out: &mut String, app: &AppReport, subject: &SubjectCount, examples: &[Example]) {
    if examples.is_empty() {
        return;
    }
    let said = app
        .reading
        .said
        .iter()
        .find(|said| said.subject == subject.id);
    let sides: [(&str, &str, u64, Option<&[crate::said::Term]>); 3] = [
        (
            "praise",
            "What they praise",
            subject.praised + subject.mixed,
            said.map(|said| said.praised.as_slice()),
        ),
        (
            "complaint",
            "What they complain about",
            subject.criticised + subject.mixed,
            said.map(|said| said.criticised.as_slice()),
        ),
        ("neutral", "Said without judging", 0, None),
    ];
    for (polarity, heading, reviews, terms) in sides {
        let shown: Vec<&Example> = examples
            .iter()
            .filter(|example| example.polarity == polarity)
            .collect();
        if shown.is_empty() {
            continue;
        }
        let _ = write!(out, "<h4 class=\"side {polarity}\">{}", escape(heading));
        if reviews > 0 {
            let _ = write!(
                out,
                " <span class=\"count\">{} reviews</span>",
                thousands(reviews)
            );
        }
        out.push_str("</h4>\n");
        if let Some(terms) = terms {
            stands_out(out, terms);
        }
        out.push_str("<ol class=\"reviews\">\n");
        for example in shown {
            review(out, app, example);
        }
        out.push_str("</ol>\n");
    }
}

/// The terms one side of a subject uses far more than the other, with how many reviewers
/// used each. Nothing is written when nothing clears the bar, which on a thin side is the
/// usual and correct outcome.
fn stands_out(out: &mut String, terms: &[crate::said::Term]) {
    if terms.is_empty() {
        return;
    }
    out.push_str("<p class=\"stands-out\"><span class=\"lead\">Words that stand out</span>");
    for term in terms {
        let _ = write!(
            out,
            " <span class=\"term\">{}<span class=\"n\" title=\"reviews using it\">{}</span></span>",
            escape(&term.text),
            thousands(term.reviews)
        );
    }
    out.push_str("</p>\n");
}

fn review(out: &mut String, app: &AppReport, example: &Example) {
    let verdict = if example.review.voted_up {
        ("up", "Recommended")
    } else {
        ("down", "Not recommended")
    };
    out.push_str("<li class=\"review\">\n<div class=\"meta\">");
    let _ = write!(
        out,
        "<span class=\"verdict {}\">{}</span>",
        verdict.0, verdict.1
    );
    if example.review.votes_up > 0 {
        let _ = write!(
            out,
            "<span class=\"votes\">{} found this helpful</span>",
            thousands(u64::from(example.review.votes_up))
        );
    }
    if example.review.playtime_at_review_minutes > 0 {
        let _ = write!(
            out,
            "<span class=\"played\">{} played</span>",
            hours(example.review.playtime_at_review_minutes)
        );
    }
    if !example.review.language.is_empty() {
        let _ = write!(
            out,
            "<span class=\"lang\">{}</span>",
            escape(&example.review.language)
        );
    }
    if example.from_the_top {
        out.push_str("<span class=\"chip top\">top of the pile</span>");
    }
    // A claim that scraped past the threshold and one the model is certain of are not equally
    // good evidence, and a page that shows them identically is inviting the wrong conclusion.
    // A review the model never read carries no polarity and gets no confidence either: an
    // invented "100% sure" on it would be the one lie this line exists to prevent.
    if example.was_read() {
        let _ = write!(
            out,
            "<span class=\"sure\">{} sure</span>",
            percent(f64::from(example.confidence))
        );
    }
    out.push_str("</div>\n");

    // The page is in English and most of the reviews on it are not. Saying so is what lets a
    // screen reader pronounce a Chinese review as Chinese rather than as English, and what
    // lets an Arabic one be laid out the way it was written.
    let tagged = bcp47(&example.review.language).map_or_else(String::new, |tag| {
        let direction = if RIGHT_TO_LEFT.contains(&tag) {
            " dir=\"rtl\""
        } else {
            ""
        };
        format!(" lang=\"{tag}\"{direction}")
    });

    // The claim is what was counted, so the claim is what is quoted. The review it came from
    // follows only where it says something the claim does not, because a reader checking a
    // count should not have to find the one sentence in thirty that earned it.
    let claim = example.claim.trim();
    let _ = write!(
        out,
        "<div class=\"text\"><p class=\"claim\"{tagged}>{}</p></div>",
        escape(claim)
    );

    let whole = example.review.text.trim();
    if whole != claim {
        let long = whole.chars().count() > PREVIEW_CHARS;
        let _ = write!(
            out,
            "<div class=\"text whole{}\"><p{tagged}>{}</p></div>",
            if long { " long" } else { "" },
            escape(whole)
        );
        out.push_str(
            "<button class=\"more\" type=\"button\" data-expands-text>Show the whole \
             review</button>\n",
        );
    }

    out.push_str("<div class=\"filed\">");
    if example.was_read() {
        let _ = write!(
            out,
            "<span class=\"chip {}\">{}</span>",
            escape(&example.polarity),
            escape(&example.polarity)
        );
    }
    for id in &example.also {
        let label = SHEET
            .iter()
            .find(|c| c.id == *id)
            .map_or(id.as_str(), |c| c.label);
        let _ = write!(out, "<span class=\"chip\">{}</span>", escape(label));
    }
    if let Some(url) = example.url(app.app_id()) {
        let _ = write!(
            out,
            "<a class=\"source\" href=\"{}\" rel=\"noopener noreferrer\" target=\"_blank\">\
             On Steam</a>",
            escape(&url)
        );
    }
    out.push_str("</div>\n</li>\n");
}

/// What read the corpus, and how sure it had to be before it would answer.
///
/// The rule is the promise the page is making. Two reports produced by the same weights under
/// different lines are not comparable, and a reader given the numbers without it cannot tell
/// which they have. A reader with a name is named, with its run beside it, because the name
/// is what somebody cites and the run is what the index knows it as.
fn built_from(app: &AppReport) -> String {
    // Plain text: the fact it lands in escapes it, and a tag written here would be read out.
    let reading = &app.reading;
    let who = match (reading.reader.is_empty(), reading.read_with.is_empty()) {
        (true, _) => reading.model.clone(),
        (false, true) => format!("{}, a fine-tune of {}", reading.reader, reading.model),
        (false, false) => format!(
            "{} (run {}), a fine-tune of {}",
            reading.reader, reading.read_with, reading.model
        ),
    };
    let labels = if reading.trained_on.is_empty() {
        String::new()
    } else {
        format!(" trained on label set {},", reading.trained_on)
    };
    // A reader carrying a rule draws a line per subject and per language; the one threshold
    // is all an older reading can say about itself.
    let rule = if reading.read_by_rule.is_empty() {
        format!("answering only above {:.2} confidence", reading.threshold)
    } else {
        format!(
            "answering a claim only where it clears the line drawn for its subject and the one \
             for its language (rule {})",
            reading.read_by_rule
        )
    };
    format!("{who},{labels} {rule}")
}

/// Why a corpus in these languages declines more than usual, which is two different answers.
///
/// A decline far above the usual rate reads as a corpus about something the taxonomy lacks,
/// and that is one of two causes now. The other is that the reader draws its line per language
/// and holds a harder one where the reference set is thinnest, so a corpus weighted towards
/// those declines more for a reason that has nothing to do with the game. The first wants a
/// category and the second wants labels, and a page that names neither leaves a reader to
/// assume the rarer one.
fn why_it_declines(out: &mut String, app: &AppReport, total: u64) {
    let mut say = |share: &[(String, u64)], sentence: &str| {
        if share.is_empty() {
            return;
        }
        let counted: u64 = share.iter().map(|(_, count)| count).sum();
        let named: Vec<String> = share
            .iter()
            .take(LANGUAGES_SHOWN)
            .map(|(name, _)| language_name(name))
            .collect();
        let _ = writeln!(
            out,
            "<p class=\"note\">{}",
            sentence
                .replace("{share}", &percent(share_of(counted, total)))
                .replace("{names}", &escape(&named.join(", ")))
        );
    };

    say(
        &app.reading.unread_languages,
        "{share} of them are in a language the reader declines outright ({names}): the \
         reference set holds too few claims there to keep its accuracy promise, so it says \
         nothing rather than guessing. That is a gap in the labels, not a finding about this \
         game.</p>",
    );
    // Almost always the larger of the two, and the one a reader would otherwise misread.
    say(
        &app.reading.strict_languages,
        "{share} are in a language the reader has to be surer about than English before it \
         will answer at all ({names}). A corpus weighted towards those declines more than \
         usual for that reason rather than for anything about the game, and what it wants is \
         more labels in those languages.</p>",
    );
}

/// What the reader is entitled to conclude, next to the numbers rather than in a footnote.
/// What language the corpus is in, which Steam's own page cannot show a reader at all.
fn languages(out: &mut String, app: &AppReport) {
    if app.reading.languages.is_empty() {
        return;
    }
    let total: u64 = app.reading.languages.iter().map(|(_, n)| n).sum();
    let english = app
        .reading
        .languages
        .iter()
        .find(|(name, _)| name == "english")
        .map_or(0, |(_, count)| *count);

    let not_english = share_of(total - english, total);

    out.push_str("<h3>What language it was said in</h3>\n");
    let _ = writeln!(
        out,
        "<p class=\"note\">{} of these reviews are not in English. Steam shows a reader their \
         own language by default, so most of this argument is one they never see.</p>",
        percent(not_english)
    );

    why_it_declines(out, app, total);

    let widest = app
        .reading
        .languages
        .first()
        .map_or(1, |(_, count)| *count)
        .max(1);
    out.push_str("<ul class=\"languages\">\n");
    for (name, count) in app.reading.languages.iter().take(LANGUAGES_SHOWN) {
        #[expect(
            clippy::cast_precision_loss,
            reason = "review counts are far below 2^53"
        )]
        let fill = *count as f64 / widest as f64;
        // A share as well as a count, the way every other table on the page reads. Six
        // thousand reviews is a different thing in a corpus of thirty thousand and in one of
        // a million, and nothing else on the row says which of those this is.
        let _ = writeln!(
            out,
            "<li><span class=\"lang-name\">{}</span>\
             <span class=\"bar\" style=\"--fill:{fill:.4}\"></span>\
             <span class=\"lang-share\">{}</span><span class=\"lang-count\">{}</span></li>",
            escape(&language_name(name)),
            percent(share_of(*count, total)),
            thousands(*count)
        );
    }
    out.push_str("</ul>\n");

    let tail: u64 = app
        .reading
        .languages
        .iter()
        .skip(LANGUAGES_SHOWN)
        .map(|(_, count)| count)
        .sum();
    let rest = app.reading.languages.len().saturating_sub(LANGUAGES_SHOWN);
    if rest > 0 {
        let _ = writeln!(
            out,
            "<p class=\"note\">and {rest} more languages, {} of the corpus between them.</p>",
            percent(share_of(tail, total))
        );
    }
}

/// A share, where a denominator of nothing is nothing rather than a division by zero.
fn share_of(part: u64, whole: u64) -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "review counts are far below 2^53"
    )]
    if whole > 0 {
        part as f64 / whole as f64
    } else {
        0.0
    }
}

fn trust(out: &mut String, app: &AppReport) {
    out.push_str("<h3>How far to trust this</h3>\n");
    out.push_str("<dl class=\"facts wide\">\n");

    fact(out, "Read by", &built_from(app));
    // Two readings cut by different splitters count different claims from the same reviews,
    // so a page says which cut its claim counts are counts of.
    // Shallow numbers are not deep numbers with less work in them. A review about six things
    // read as one point is about none of them clearly, and a page that did not say which way
    // it was read would invite comparing the two.
    if app.reading.depth != crate::read::Depth::Deep {
        fact(
            out,
            "Depth",
            "shallow: each review read as one point, so anyone who wrote more than a sentence \
             is understated. Not comparable with a deep reading.",
        );
    }
    fact(
        out,
        "Claims read",
        &format!(
            "{} from {} reviews",
            thousands(app.reading.claims),
            thousands(app.reading.reviews)
        ),
    );

    // The single most important number on the page for reading every other one. A model
    // answering a fifth of the claims is not describing the corpus, it is describing the
    // fifth it was sure about, and no rate below can be read without knowing that.
    if let Some(share) = app.reading.unclassified_share() {
        // Against what the model usually declines, because a share on its own cannot say
        // whether this corpus is hard or the taxonomy is short a row. Twice the usual rate
        // is the second, and it is a finding about the game.
        let against = match app.reading.declined_against_usual() {
            Some(ratio) if app.reading.declined_unusually() => format!(
                ". That is {ratio:.1} times what it declines on a game it has never seen: this \
                 game's players talk about something the taxonomy has no row for"
            ),
            // Both rates rather than the ratio between them. "About 76% of what it usually
            // declines" is a true sentence that reads, at a glance, as a share of the claims.
            Some(_) => app
                .reading
                .usual_declined
                .map_or_else(String::new, |usual| {
                    let usual = f64::from(usual);
                    format!(
                        ", {} the {} it usually declines on a game it has never seen",
                        if share > usual { "above" } else { "below" },
                        percent(usual)
                    )
                }),
            None => String::new(),
        };
        fact(
            out,
            "Claims it would not answer",
            &format!(
                "{} ({}), counted as unclassified rather than filed under a best guess{against}",
                thousands(app.reading.unclassified_claims),
                percent(share)
            ),
        );
    }
    if app.reading.silent_reviews > 0 {
        fact(
            out,
            "Reviews it said nothing about",
            &format!(
                "{} ({} of those read), where no point cleared the threshold",
                thousands(app.reading.silent_reviews),
                percent(share_of(app.reading.silent_reviews, app.reading.reviews))
            ),
        );
    }

    // The capture holds more rows than any rate is taken over, and a reader who subtracts
    // the two deserves the difference named rather than left to guess at it.
    let blank = app.crawl.rows_unique.saturating_sub(app.reading.reviews);
    if blank > 0 {
        let why = if app.reading.language.is_some() {
            "another language, or a rating and nothing else"
        } else {
            "a rating and nothing else"
        };
        fact(
            out,
            "Reviews not read",
            &format!("{} ({why}, counted in no rate)", thousands(blank)),
        );
    }
    out.push_str("</dl>\n");

    measured_here(out, app);
}

/// What this game's own labels say about the reading, or why they cannot say anything.
fn measured_here(out: &mut String, app: &AppReport) {
    match &app.agreement {
        crate::report::Measurement::Measured(agreement) => {
            agreement_note(out, agreement);
            if let Some(ceiling) = &app.ceiling {
                ceiling_note(out, ceiling);
            }
        }
        crate::report::Measurement::Unlabelled => unmeasured_here(
            out,
            app,
            "No claims have been labelled for this game, so how often the model is wrong \
             <em>here</em> has not been measured.",
        ),
        crate::report::Measurement::Learned => unmeasured_here(
            out,
            app,
            "This game's labelled claims are in the model's training set, so how often it \
             agrees with them says how well it remembers them, not how it reads, and nothing \
             here is scored against them.",
        ),
        // Not the same thing as nobody having labelled it, and telling a reader it is sends
        // them to do work that is already done.
        crate::report::Measurement::Unscored(why) => {
            let _ = writeln!(
                out,
                "<p class=\"warn\">This game has a reference set, and nothing here is \
                 measured against it: {}. Treat every rate as provisional until it is.</p>",
                escape(why)
            );
        }
    }

    out.push_str(
        "<p class=\"note\">These rates are a census, not a survey: every review Valve serves \
         was counted, so there is no sampling error to report. What they do carry is \
         classifier error, which is what the agreement figure above measures.</p>\n",
    );
}

/// What to tell a reader of a game whose error is not measured here: one nobody has labelled,
/// or one the model learned from. `why` is the sentence that says which.
///
/// Most games a person runs this on will be in exactly this position, and "not measured" on
/// its own is both true and useless: it invites the reader either to distrust everything or to
/// trust everything, and the model does have a measurement, taken on games it had never seen.
/// That figure is not about this corpus and the wording must not pretend otherwise.
fn unmeasured_here(out: &mut String, app: &AppReport, why: &str) {
    let Some(frozen) = app.reading.frozen else {
        let _ = writeln!(
            out,
            "<p class=\"warn\">{why} Treat every rate as provisional.</p>"
        );
        return;
    };
    let _ = writeln!(
        out,
        "<p class=\"warn\">{why} What is measured is how it does on {} games it had never \
         seen, over {} labelled claims: it answers {} of them and names the same subject a \
         separate labeller did {} of the time when it does, declining the rest rather than \
         guessing. Those labellers were themselves language models. Expect this corpus to be \
         somewhere near that and treat every rate as provisional.</p>",
        frozen.games,
        thousands(u64::from(frozen.claims)),
        percent(frozen.coverage),
        percent(frozen.accuracy),
    );
}

/// What a second labeller does to the figure above it.
///
/// Agreement with one labeller cannot say whether a disagreement is the model's mistake or the
/// labeller's, and on a silver standard that is the whole question. Where part of the set has
/// been read a second time, blind, the claims the two reached the same answer for are the ones
/// worth scoring against, and the claims they split on have no single answer to be right about.
fn ceiling_note(out: &mut String, ceiling: &crate::measure::Ceiling) {
    let (Some(between), Some(settled)) =
        (ceiling.between_labellers(), ceiling.against_the_settled())
    else {
        return;
    };
    let range = ceiling.interval().map_or_else(String::new, |(low, high)| {
        format!(", somewhere in [{}, {}]", percent(low), percent(high))
    });
    let split = ceiling.where_they_split().map_or_else(String::new, |rate| {
        format!(
            " On the {} they read differently there is no single answer to be right about, and \
             it lands on one of their two {} of the time.",
            thousands(ceiling.labellers_split),
            percent(rate)
        )
    });
    // The claims nobody could argue about: settled, and neither labeller reached for the
    // contested flag. A model's errors concentrating there would be a different report from
    // one whose errors are all on the hard claims, and a reader deserves to know which.
    let clear = ceiling.on_the_clear().map_or_else(String::new, |rate| {
        format!(
            " Of the settled claims, {} were ones neither labeller called contested, and on \
             those it agrees {} of the time.",
            thousands(ceiling.settled_and_clear),
            percent(rate)
        )
    });
    let _ = writeln!(
        out,
        "<p class=\"note\"><strong>{} of those claims were read a second time</strong>, by a \
         different labeller working blind, and the two reached the same subject on {} of them. \
         On those settled claims the model agrees {}{}, which is the nearest thing to accuracy \
         a set labelled by models can produce: a label two independent readings reached is one \
         worth scoring against.{}{}</p>",
        thousands(ceiling.compared),
        percent(between),
        percent(settled),
        range,
        clear,
        split
    );
}

fn agreement_note(out: &mut String, agreement: &crate::measure::ClaimAgreement) {
    let (Some(rate), Some((low, high))) = (agreement.rate(), agreement.interval()) else {
        return;
    };
    let _ = writeln!(
        out,
        "<p class=\"warn\">Of the {} labelled claims this model was willing to answer, it \
         named the same subject a separate labeller did {} of the time, somewhere in [{}, {}] \
         with 95% confidence. The labeller was itself a language model, so that is \
         <strong>agreement, not accuracy</strong>: two models can be wrong together, most \
         easily on sarcasm and on claims that sit between subjects.</p>",
        thousands(agreement.answered),
        percent(rate),
        percent(low),
        percent(high)
    );

    // The figure above is computed over answered claims only, which is the honest way to score
    // a model that abstains and the dishonest way to describe what it did to a corpus. Both
    // numbers or neither.
    if let Some(declined) = agreement.declined_share() {
        let _ = writeln!(
            out,
            "<p class=\"note\">That figure covers the {} of those claims it answered. It \
             declined the other {}, which are counted as unclassified everywhere on this page \
             rather than being filed under a best guess.</p>",
            percent(1.0 - declined),
            percent(declined)
        );
    }
    // A label names a span of a review, and a splitter that has since learned to cut that
    // review differently leaves the label naming nothing. Those are left out and said, so
    // the labelled count above is a count of what could be compared.
    if agreement.unjoined > 0 {
        let _ = writeln!(
            out,
            "<p class=\"note\">A further {} labelled claims were left out because this \
             build takes their reviews apart differently from the build they were labelled \
             under, so no reading corresponds to them.</p>",
            thousands(agreement.unjoined)
        );
    }

    // One figure for a whole taxonomy hides the shape of the error: the same run finds nine
    // claims in ten of one subject and one in twenty of another.
    let judged: Vec<&crate::measure::SubjectAgreement> = agreement
        .subjects
        .iter()
        .filter(|s| s.labelled >= ENOUGH_TO_JUDGE_A_ROW)
        .collect();
    if judged.is_empty() {
        return;
    }
    let thin = judged
        .iter()
        .filter(|s| s.recall().is_some_and(|recall| recall < THINLY_FOUND))
        .count();
    let _ = writeln!(
        out,
        "<p class=\"note\">Averaged over the subjects rather than over the claims, so a rare \
         one counts as much as a common one, that comes to <strong>{:.2}</strong> on a scale \
         where 1 is perfect agreement. {}</p>",
        agreement.macro_f1().unwrap_or(0.0),
        if thin == 0 {
            format!(
                "Every one of the {} subjects with enough labels to judge is found in at \
                 least a quarter of the claims making it.",
                judged.len()
            )
        } else if thin == 1 {
            format!(
                "One of the {} subjects with enough labels to judge is found in fewer than \
                 a quarter of the claims making it, and its row is marked: read that rate \
                 as a floor.",
                judged.len()
            )
        } else {
            format!(
                "{thin} of the {} subjects with enough labels to judge are found in fewer \
                 than a quarter of the claims making them, and their rows are marked: read \
                 those rates as floors.",
                judged.len()
            )
        }
    );

    // A subject read as one particular other subject is a boundary the taxonomy has not
    // settled, and no amount of training settles it for the taxonomy. Worth naming, because it
    // is the one kind of error a reader of this page can act on.
    let worst = agreement
        .subjects
        .iter()
        .filter(|s| s.labelled >= ENOUGH_TO_JUDGE_A_ROW)
        .filter_map(|s| s.mistaken_for.map(|(other, count)| (s, other, count)))
        .max_by_key(|(_, _, count)| *count);
    if let Some((subject, other, count)) = worst {
        let _ = writeln!(
            out,
            "<p class=\"note\">Where it disagrees most, {} claims the labeller called \
             {} were read as {}. That is a boundary between two subjects rather than a \
             mistake about one of them.</p>",
            thousands(count),
            escape(subject.label),
            escape(other)
        );
    }
}

fn page_footer(out: &mut String, report: &Report) {
    out.push_str("<footer class=\"page\">\n<div class=\"wrap\">\n");
    out.push_str("<h3>What this cannot tell you</h3>\n<ul>\n");
    for limit in FOOTER_LIMITS {
        let _ = writeln!(out, "<li>{limit}</li>");
    }
    out.push_str("</ul>\n");
    let _ = writeln!(
        out,
        "<p class=\"note\">Rendered on {} by SteamGauge, from captures taken on the \
         dates given above. Nothing on this page was sent anywhere to produce it.</p>",
        escape(&crate::time::day(report.generated_unix))
    );
    out.push_str("</div>\n</footer>\n");
}

const FOOTER_LIMITS: [&str; 4] = [
    "Every review Valve will serve is not every review ever written. Reviews from banned \
     accounts, deleted reviews, and reviews the API stops paging through are missing, and \
     nobody outside Valve can measure how many.",
    "A category assignment is a machine reading one review once. It has no idea whether a \
     joke is a joke, and sarcasm is exactly where it is weakest.",
    "Counting how often something is mentioned says nothing about whether the people saying \
     it are right, or whether the ones who never mentioned it disagree.",
    "The top of the pile is Steam's own ordering, which changes over time. A capture is a \
     photograph of it, not a permanent fact.",
];

/// Steam's own names for languages, the tags a browser understands, and what to call them
/// on screen.
///
/// Steam uses names of its own: "schinese", "koreana", "brazilian", "latam". A page that
/// repeats those tells a screen reader nothing and a reader not much more.
const LANGUAGES: [(&str, &str, &str); 31] = [
    ("english", "en", "English"),
    ("schinese", "zh-Hans", "Chinese (simplified)"),
    ("tchinese", "zh-Hant", "Chinese (traditional)"),
    ("japanese", "ja", "Japanese"),
    ("koreana", "ko", "Korean"),
    ("thai", "th", "Thai"),
    ("bulgarian", "bg", "Bulgarian"),
    ("czech", "cs", "Czech"),
    ("danish", "da", "Danish"),
    ("german", "de", "German"),
    ("greek", "el", "Greek"),
    ("spanish", "es", "Spanish"),
    ("latam", "es-419", "Spanish (Latin America)"),
    ("finnish", "fi", "Finnish"),
    ("french", "fr", "French"),
    ("hungarian", "hu", "Hungarian"),
    ("indonesian", "id", "Indonesian"),
    ("italian", "it", "Italian"),
    ("dutch", "nl", "Dutch"),
    ("norwegian", "no", "Norwegian"),
    ("polish", "pl", "Polish"),
    ("portuguese", "pt", "Portuguese"),
    ("brazilian", "pt-BR", "Portuguese (Brazil)"),
    ("romanian", "ro", "Romanian"),
    ("russian", "ru", "Russian"),
    ("swedish", "sv", "Swedish"),
    ("turkish", "tr", "Turkish"),
    ("ukrainian", "uk", "Ukrainian"),
    ("vietnamese", "vi", "Vietnamese"),
    ("arabic", "ar", "Arabic"),
    ("malay", "ms", "Malay"),
];

/// Tags whose script runs the other way, so a review in one is laid out the other way.
///
/// Arabic is the only one Steam offers as a review language, and two reviews of the three
/// million measured here are in it. Two reviews rendered backwards are still two reviews
/// rendered backwards, and the fix is one attribute.
const RIGHT_TO_LEFT: [&str; 1] = ["ar"];

/// The BCP 47 tag for a Steam language name.
///
/// An unknown name gets no tag rather than a guess: an element inheriting the page's English
/// is a smaller error than one claiming to be a language it is not.
fn bcp47(steam: &str) -> Option<&'static str> {
    LANGUAGES
        .iter()
        .find(|(name, _, _)| *name == steam)
        .map(|(_, tag, _)| *tag)
}

/// What to call a Steam language on screen, falling back to whatever Steam called it.
pub(crate) fn language_name(steam: &str) -> String {
    LANGUAGES
        .iter()
        .find(|(name, _, _)| *name == steam)
        .map_or_else(|| steam.to_owned(), |(_, _, display)| (*display).to_owned())
}

fn escape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for character in raw.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(character),
        }
    }
    out
}

/// A cell with no number behind it.
///
/// A dash reads as absence at a glance and as silence to a screen reader, so why the cell is
/// empty is spelled out for anyone who cannot see the column it sits in.
fn nothing(reason: &str) -> String {
    format!(
        "<span aria-hidden=\"true\">\u{2013}</span>\
         <span class=\"read-aloud\">{}</span>",
        escape(reason)
    )
}

pub(crate) fn percent(rate: f64) -> String {
    if rate > 0.0 && rate < 0.001 {
        return "<0.1%".to_owned();
    }
    format!("{:.1}%", rate * 100.0)
}

pub(crate) fn thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

fn hours(minutes: u32) -> String {
    if minutes < 60 {
        return format!("{minutes} min");
    }
    format!("{} h", minutes / 60)
}

const STYLE: &str = include_str!("report.css");
const SCRIPT: &str = include_str!("report.js");

#[cfg(test)]
mod tests {
    use super::*;

    fn a_category(id: &str, label: &str, mentions: u64, top: u64) -> crate::read::SubjectCount {
        crate::read::SubjectCount {
            id: id.to_owned(),
            label: label.to_owned(),
            primary_reviews: mentions / 2,
            mention_reviews: mentions,
            claims: mentions * 2,
            praised: mentions / 3,
            criticised: mentions / 3,
            mixed: mentions / 6,
            top_mention_reviews: top,
            positive_mentions: mentions / 3,
        }
    }

    fn sample_report(text: &str) -> Report {
        let category = a_category;
        let example = Example {
            review: crate::capture::CapturedReview {
                id: "42".to_owned(),
                text: text.to_owned(),
                language: "english".to_owned(),
                author_steamid: "7656119".to_owned(),
                voted_up: false,
                votes_up: 12,
                votes_funny: 0,
                playtime_at_review_minutes: 600,
                created: 1_700_000_000,
            },
            claim: text.to_owned(),
            at: (0, 0),
            polarity: "complaint".to_owned(),
            confidence: 0.87,
            also: vec!["bugs".to_owned(), "performance".to_owned()],
            from_the_top: true,
        };
        Report {
            generated_unix: 1_700_000_000,
            apps: vec![AppReport {
                crawl: crate::report::CrawlFacts {
                    app_id: 7,
                    name: "A Game <& Friends>".to_owned(),
                    review_score_desc: "Mostly Positive".to_owned(),
                    rows_unique: 1_000,
                    valve_total_reviews: 1_000,
                    valve_total_positive: 700,
                    valve_total_negative: 300,
                    coverage: 1.0,
                    snapshot_unix: 1_700_000_000,
                    shards: 1,
                    swept_unix: None,
                    sweeps: 0,
                    rows_swept: 0,
                },
                reading: crate::read::ReadReport {
                    app_id: 7,
                    reviews: 1_000,
                    corpus_reviews: 1_000,
                    language: None,
                    depth: crate::read::Depth::Deep,
                    batch_size: Some(crate::read::DEFAULT_READ_BATCH),
                    claims: 3_000,
                    forward_passes: 3_000,
                    unclassified_claims: 300,
                    silent_reviews: 3,
                    claimless_reviews: 0,
                    positive: 700,
                    top_helpful: 50,
                    model: "test-reader".to_owned(),
                    trained_on: "0123456789abcdef".to_owned(),
                    read_with: "a-reader".to_owned(),
                    reader: "Game Review Reader".to_owned(),
                    read_by_rule: String::new(),
                    usual_declined: Some(0.1),
                    frozen: Some(crate::reader::Frozen {
                        games: 8,
                        claims: 3769,
                        coverage: 0.577,
                        accuracy: 0.749,
                        macro_f1: 0.504,
                    }),
                    context: true,
                    threshold: 0.5,
                    device: "cpu".to_owned(),
                    captured_unix: 1_700_000_000,
                    subjects: vec![
                        category("bugs", "Bugs and crashes", 400, 30),
                        category("performance", "Performance", 100, 2),
                    ],
                    said: vec![said_about_bugs()],
                    languages: vec![("english".to_owned(), 600), ("schinese".to_owned(), 400)],
                    unread_languages: Vec::new(),
                    strict_languages: Vec::new(),
                    months: vec![
                        calendar("2024-01", 400, 320, vec![40, 100]),
                        calendar("2024-02", 600, 380, vec![60, 300]),
                    ],
                    who: Vec::new(),
                    elapsed: std::time::Duration::ZERO,
                },
                examples: vec![("bugs".to_owned(), vec![example])],
                top: Vec::new(),
                agreement: crate::report::Measurement::Unlabelled,
                ceiling: None,
                induced: Vec::new(),
                updates: crate::before_after::Updates::default(),
            }],
        }
    }

    /// What stands out on each side of the bugs row: one praise term, two complaint terms.
    fn said_about_bugs() -> crate::said::SaidAbout {
        let term = |text: &str, reviews| crate::said::Term {
            text: text.to_owned(),
            reviews,
        };
        crate::said::SaidAbout {
            subject: "bugs".to_owned(),
            praising: 200,
            complaining: 100,
            praised: vec![term("patched quickly", 41)],
            criticised: vec![term("save corruption", 37), term("crashes", 29)],
        }
    }

    /// A game measured against labelled claims, agreeing on `agreed` of `answered`.
    fn measured(answered: u64, agreed: u64) -> crate::measure::ClaimAgreement {
        crate::measure::ClaimAgreement {
            app_id: 7,
            matched: answered,
            unjoined: 0,
            answered,
            agreed,
            declined: 0,
            polarity_answered: answered,
            polarity_agreed: agreed,
            clear_answered: answered,
            clear_agreed: agreed,
            contested_answered: 0,
            contested_agreed: 0,
            subjects: Vec::new(),
            beyond_the_first: crate::measure::Beyond::default(),
        }
    }

    /// The same report with a calendar of its own.
    fn with_months(months: Vec<crate::read::Month>) -> Report {
        let mut report = sample_report("ordinary text");
        report.apps[0].reading.months = months;
        report
    }

    fn calendar(
        label: &str,
        reviews: u64,
        positive: u64,
        subjects: Vec<u64>,
    ) -> crate::read::Month {
        crate::read::Month {
            label: label.to_owned(),
            reviews,
            positive,
            subjects,
            praising: Vec::new(),
            complaining: Vec::new(),
        }
    }

    fn month(label: &str, reviews: u64, bugs: u64) -> crate::read::Month {
        calendar(label, reviews, reviews / 2, vec![0, bugs])
    }

    /// 15 January 2024 at midday, halfway through the first month of the sample's calendar.
    const MID_JANUARY: i64 = 1_705_320_000;

    fn compared(before: f64, after: f64, change: bool) -> crate::before_after::Compared {
        crate::before_after::Compared {
            before,
            after,
            z: if after > before { 4.0 } else { -4.0 },
            change,
        }
    }

    /// An update posted at `posted`, with `reviews` either side, the share recommending the game
    /// and complaints and praise of bugs as given.
    fn update_at(
        gid: &str,
        posted: i64,
        reviews: u64,
        recommended: crate::before_after::Compared,
        bugs: (crate::before_after::Compared, crate::before_after::Compared),
    ) -> crate::before_after::Around {
        let enough = reviews >= crate::before_after::ENOUGH;
        let subjects = if enough {
            vec![crate::before_after::Subject {
                subject: "bugs",
                label: "Bugs <and> crashes",
                praise: bugs.0,
                complaint: bugs.1,
            }]
        } else {
            Vec::new()
        };
        crate::before_after::Around {
            update: crate::updates::Update {
                gid: gid.to_owned(),
                title: format!("Patch {gid} <b>"),
                posted,
                link: crate::updates::link(gid),
            },
            before: crate::before_after::Window {
                from: posted - 28 * 86_400,
                to: posted,
                reviews,
            },
            after: crate::before_after::Window {
                from: posted,
                to: posted + 28 * 86_400,
                reviews,
            },
            after_whole: true,
            enough,
            recommended: enough.then_some(recommended),
            changes: usize::from(bugs.0.change) + usize::from(bugs.1.change),
            subjects,
            nearby: 0,
        }
    }

    fn with_updates(asked: Option<i64>, around: Vec<crate::before_after::Around>) -> String {
        let mut report = sample_report("ordinary text");
        report.apps[0].updates = crate::before_after::Updates { asked, around };
        render(&report)
    }

    /// The part of a page from its updates' heading to the table after them.
    fn updates_part(page: &str) -> &str {
        let from = page.find("Around its updates").unwrap();
        let to = page[from..].find("What players talk about").unwrap();
        &page[from..from + to]
    }

    #[test]
    fn an_update_is_marked_where_it_falls_on_the_chart_and_only_there() {
        let steady = compared(0.7, 0.7, false);
        let page = with_updates(
            Some(1),
            vec![
                update_at("1", MID_JANUARY, 50, steady, (steady, steady)),
                update_at("2", MID_JANUARY - 40 * 86_400, 50, steady, (steady, steady)),
            ],
        );
        // Two months over a thousand units, and midday on the 15th is 14.5 of January's 31 days
        // into the first of them.
        assert!(
            page.contains(
                "<line class=\"update\" x1=\"233.87\" y1=\"0\" x2=\"233.87\" y2=\"160\" />"
            ),
            "{}",
            &page[page.find("<line class=\"update\"").unwrap_or(0)..][..120]
        );
        assert_eq!(
            page.matches("<line class=\"update\"").count(),
            1,
            "an update before the first month has nowhere on the chart to go"
        );
        assert_eq!(
            updates_part(&page).matches("<li>").count(),
            2,
            "and is still listed"
        );
    }

    #[test]
    fn a_game_never_asked_about_or_with_no_update_says_which() {
        let never = with_updates(None, Vec::new());
        assert!(updates_part(&never).contains("Steam has not been asked"));
        let none = with_updates(Some(1_700_000_000), Vec::new());
        assert!(
            updates_part(&none).contains("Nothing this game's developer had posted on Steam by 14 November 2023 reads as an update.")
        );
        assert!(!none.contains("class=\"update\""));
    }

    #[test]
    fn the_biggest_updates_are_set_out_with_what_changed_and_the_rest_listed() {
        let steady = compared(0.7, 0.7, false);
        let mut thin = update_at(
            "thin",
            MID_JANUARY - 86_400 * 200,
            10,
            steady,
            (steady, steady),
        );
        thin.after.reviews = 99_999;
        let mut partial = update_at(
            "partial",
            MID_JANUARY + 86_400 * 90,
            300,
            compared(0.8, 0.6, true),
            (compared(0.3, 0.1, true), compared(0.1, 0.2, true)),
        );
        partial.after_whole = false;
        partial.after.to = partial.after.from + 9 * 86_400 + 3_600;
        partial.nearby = 2;
        let mut quiet = update_at("quiet", MID_JANUARY, 400, steady, (steady, steady));
        quiet.nearby = 1;
        let better = update_at(
            "better",
            MID_JANUARY + 86_400 * 40,
            500,
            compared(0.6, 0.7, false),
            (compared(0.1, 0.3, true), compared(0.3, 0.1, true)),
        );
        let page = with_updates(Some(1), vec![thin, quiet, better, partial]);
        let part = updates_part(&page);

        assert!(part.contains("are the 4 updates"));
        assert_eq!(part.matches("<div class=\"update-around\">").count(), 3);
        assert!(!part.contains("<h4><a href=\"https://store.steampowered.com/news/externalpost/steam_community_announcements/thin\""));
        assert!(
            part.contains("Patch quiet &lt;b&gt;"),
            "a title is text, not markup"
        );
        assert!(part.contains("Bugs &lt;and&gt; crashes"));
        assert!(part.contains(
            "400 reviews in the 28 days before, 400 in the 28 days after. One other update was \
             posted within these weeks, and they hold its effect too."
        ));
        assert!(part.contains("Nothing changed beyond chance"));
        assert!(part.contains(
            "300 reviews in the 28 days before, 300 in the 9 days after it that the capture \
             holds. 2 other updates were posted within these weeks, and they hold their effects \
             too."
        ));
        assert!(part.contains(
            "<li class=\"change\"><strong>Recommending the game</strong>: fell from 80.0% to \
             60.0% of reviews</li>"
        ));
        assert!(
            !part.contains("from 60.0% to 70.0%"),
            "a share that did not change is not said"
        );
        assert!(part.contains(
            "<li class=\"change worse\"><strong>Bugs &lt;and&gt; crashes</strong>: praise fell \
             from 30.0% to 10.0% of reviews</li>"
        ));
        assert!(part.contains(
            "<li class=\"change worse\"><strong>Bugs &lt;and&gt; crashes</strong>: complaints \
             rose from 10.0% to 20.0% of reviews</li>"
        ));
        assert!(part.contains(
            "<li class=\"change better\"><strong>Bugs &lt;and&gt; crashes</strong>: praise rose \
             from 10.0% to 30.0% of reviews</li>"
        ));
        assert!(part.contains(
            "<li class=\"change better\"><strong>Bugs &lt;and&gt; crashes</strong>: complaints \
             fell from 30.0% to 10.0% of reviews</li>"
        ));

        let listed: Vec<&str> = part
            .split("<span class=\"said\">")
            .skip(1)
            .map(|rest| &rest[..rest.find("</span>").unwrap()])
            .collect();
        assert_eq!(
            listed,
            [
                "3 changes",
                "2 changes",
                "nothing changed beyond chance",
                "too few reviews either side to compare"
            ],
            "newest first, and the share recommending the game counts as a change"
        );
        assert!(part.contains("rel=\"noopener noreferrer\" target=\"_blank\""));
    }

    #[test]
    fn an_update_with_one_change_says_one() {
        let steady = compared(0.7, 0.7, false);
        let one = update_at(
            "one",
            MID_JANUARY,
            400,
            steady,
            (steady, compared(0.1, 0.2, true)),
        );
        let part_of = with_updates(Some(1), vec![one]);
        assert!(updates_part(&part_of).contains("<span class=\"said\">1 change</span>"));
    }

    #[test]
    fn updates_with_too_few_reviews_anywhere_are_listed_and_none_set_out() {
        let steady = compared(0.7, 0.7, false);
        let page = with_updates(
            Some(1),
            vec![update_at("1", MID_JANUARY, 99, steady, (steady, steady))],
        );
        let part = updates_part(&page);
        assert!(part.contains("None of them has 100 reviews on each side, so none is compared."));
        assert!(!part.contains("update-around"));
    }

    #[test]
    fn every_month_gets_a_bar_however_quiet_it_was() {
        let months = vec![
            month("2024-01", 400, 40),
            month("2024-02", 5, 1),
            month("2024-03", 300, 30),
            month("2024-04", 200, 20),
        ];
        let page = render(&with_months(months));

        assert_eq!(
            page.matches("<rect class=\"bar\"").count(),
            4,
            "a quiet month is a fact about the game and belongs on the chart"
        );
        assert!(page.contains("Jan 2024"), "the first month should be named");
        assert!(page.contains("Apr 2024"), "the last month should be named");
    }

    #[test]
    fn a_month_too_small_to_carry_a_rate_stays_out_of_the_sparkline() {
        // One review in a five-review month is 20% and would set the scale for every month
        // that has something to say. The categories are the same in both cases; only the
        // month sizes differ, and only the second should reach the chart.
        let noisy = vec![
            month("2024-01", 100, 10),
            month("2024-02", 5, 5),
            month("2024-03", 100, 10),
            month("2024-04", 100, 10),
        ];
        let page = render(&with_months(noisy));

        assert!(
            page.contains("peaking at 10.0%"),
            "a five-review month set the scale"
        );
        assert!(page.contains("a month with fewer than 30 reviews carries no rate"));
        // The line stops where the rates stop, so the caption names where it stops and not
        // the last month of a corpus it never reached.
        let quietened = vec![
            month("2024-01", 100, 10),
            month("2024-02", 100, 10),
            month("2024-03", 100, 10),
            month("2024-04", 5, 1),
            month("2024-05", 5, 1),
        ];
        let page = render(&with_months(quietened));
        assert!(
            page.contains("Drawn from Jan 2024 to Mar 2024 on an axis running to May 2024"),
            "the caption claims months the line never reached"
        );
    }

    #[test]
    fn a_review_cannot_break_out_of_the_page_it_is_quoted_in() {
        // Review text is written by strangers and this one is trying. Nothing it contains
        // may reach the browser as markup.
        let hostile = "</p></td></tr></table><script>alert(1)</script><img src=x onerror=1>";
        let page = render(&sample_report(hostile));

        assert!(!page.contains("<script>alert(1)"), "a script tag survived");
        assert!(!page.contains("<img src=x"), "an image tag survived");
        assert!(
            page.contains("&lt;script&gt;alert(1)"),
            "the text itself is missing"
        );
        // The game's own name is equally untrusted, coming from the store.
        assert!(page.contains("A Game &lt;&amp; Friends&gt;"));
    }

    #[test]
    fn the_page_fetches_nothing_and_says_what_it_cannot_tell_you() {
        let page = render(&sample_report("ordinary text"));

        for fetching in ["<script src", "<link ", "<img ", "@import", "url("] {
            assert!(!page.contains(fetching), "the page would fetch: {fetching}");
        }
        assert!(page.contains("What this cannot tell you"));
        assert!(
            page.contains("No claims have been labelled"),
            "a report with no measured agreement must say so"
        );
    }

    #[test]
    fn nothing_offered_to_a_reader_without_scripting_does_nothing() {
        let page = render(&sample_report("ordinary text"));

        let filter = page
            .split_once("<form class=\"filter\"")
            .expect("the category filter is missing")
            .1;
        let opening = filter.split_once('>').expect("an unclosed form tag").0;
        assert!(
            opening.contains(" hidden"),
            "the filter is offered before scripting has said it works: {opening}"
        );
        assert!(
            SCRIPT.contains("form.hidden = false"),
            "nothing ever reveals the filter"
        );
    }

    /// A rate the model is measured to miss most of is not a count, and a reader scanning the
    /// table has no way to tell the two apart unless the page says so.
    #[test]
    fn a_set_read_twice_says_what_the_two_labellers_settled() {
        let ceiling = crate::measure::Ceiling {
            compared: 400,
            labellers_agreed: 360,
            model_agreed_where_they_did: 306,
            labellers_split: 40,
            model_matched_either: 32,
            model_agreed_with_first: 320,
            model_agreed_with_second: 316,
            settled_and_clear: 300,
            model_agreed_on_the_clear: 279,
        };
        let mut out = String::new();
        ceiling_note(&mut out, &ceiling);
        assert!(
            out.contains("400 of those claims were read a second time"),
            "{out}"
        );
        assert!(
            out.contains("90.0%"),
            "the two labellers agreed on 360 of 400: {out}"
        );
        assert!(
            out.contains("85.0%"),
            "the model agreed on 306 of those 360: {out}"
        );
        assert!(
            out.contains("80.0%"),
            "it matched one of two on 32 of 40 split: {out}"
        );
        assert!(out.contains("somewhere in ["), "{out}");
        assert!(
            out.contains("300 were ones neither labeller called contested"),
            "{out}"
        );
        assert!(
            out.contains("93.0%"),
            "the model agreed on 279 of those 300: {out}"
        );

        // Nothing to say where nobody has read the set twice, rather than a row of dashes.
        let mut out = String::new();
        ceiling_note(&mut out, &crate::measure::Ceiling::default());
        assert!(out.is_empty(), "{out}");
    }

    #[test]
    fn a_row_the_model_barely_finds_is_marked_as_one() {
        let scored = |labelled, agreed| crate::measure::SubjectAgreement {
            id: "bugs",
            label: "Bugs and crashes",
            labelled,
            read: agreed,
            agreed,
            seen: labelled * 4,
            mistaken_for: None,
        };

        assert!(
            thinly_measured(&scored(100, 4)).contains("class=\"thin\""),
            "four found in a hundred is a floor, not a count"
        );
        assert!(
            thinly_measured(&scored(100, 90)).is_empty(),
            "a row the model finds should carry no warning"
        );
        assert!(
            thinly_measured(&scored(4, 0)).is_empty(),
            "four labelled claims cannot condemn a row"
        );

        let mut out = String::new();
        how_well_this_row_is_known(&mut out, &scored(100, 4), Some(0.05));
        assert!(out.contains("found 4.0%"), "{out}");
        assert!(out.contains("100 labelled claims"), "{out}");
        assert!(
            out.contains("note warn"),
            "a row this thin should look thin: {out}"
        );
        assert!(
            out.contains("cannot be corrected"),
            "a subject found in 4% of its claims is chance, and a corrected rate from it would \
             be invented: {out}"
        );

        let mut well = String::new();
        how_well_this_row_is_known(&mut well, &scored(100, 80), Some(0.2));
        assert!(
            well.contains("Corrected for those errors"),
            "a subject found four times in five earns a corrected rate: {well}"
        );
        assert!(well.contains("from 100 labels"), "{well}");

        // Twelve labels describe a row; the sensitivity they measure has an interval
        // twenty-five points wide, and a share divided by it would move by more than itself.
        let mut thin = String::new();
        how_well_this_row_is_known(&mut thin, &scored(12, 10), Some(0.2));
        assert!(thin.contains("12 labelled claims"), "{thin}");
        assert!(
            !thin.contains("Corrected for those errors"),
            "a dozen labels cannot correct a rate: {thin}"
        );

        let mut none = String::new();
        how_well_this_row_is_known(&mut none, &scored(0, 0), Some(0.1));
        assert!(none.contains("nothing here is measured"), "{none}");
    }

    /// The filter matches on this and nothing else, so a row without it silently stops
    /// being findable and a panel with it would be filtered away from its own row.
    #[test]
    fn every_row_a_reader_can_filter_carries_the_name_being_matched() {
        let page = render(&sample_report("ordinary text"));

        assert!(page.contains("data-name=\"bugs and crashes\""));
        assert!(page.contains("data-name=\"performance\""));
        assert_eq!(
            page.matches("data-name=").count(),
            2,
            "one game has two categories and no other row should claim a name"
        );
        for panel in page.split("<tr class=\"panel\"").skip(1) {
            let opening = panel.split_once('>').expect("an unclosed row").0;
            assert!(
                !opening.contains("data-name"),
                "a panel named itself: {opening}"
            );
        }
    }

    /// A capture holding more rows than the rates are taken over is normal and invites
    /// exactly one question, so the page answers it rather than leaving the reader to
    /// subtract two numbers and wonder.
    #[test]
    fn the_gap_between_what_was_captured_and_what_was_counted_is_named() {
        let mut report = sample_report("ordinary text");
        report.apps[0].crawl.rows_unique = 1_010;
        report.apps[0].reading.reviews = 1_000;

        let page = render(&report);
        assert!(
            page.contains("1,010 of the 1,000 Valve reports"),
            "the two numbers a reader has to reconcile are not both on the page"
        );
        assert!(
            page.contains("Reviews not read"),
            "the reviews that were never read go unmentioned"
        );
        assert!(
            page.contains("10 (a rating and nothing else"),
            "wrong count for what was captured but not read"
        );
        assert!(
            page.contains("Claims it would not answer"),
            "a page that does not say how much the model declined cannot be read at all"
        );

        report.apps[0].crawl.rows_unique = 1_000;
        let tidy = render(&report);
        assert!(
            !tidy.contains("Reviews not read"),
            "a capture with nothing missing should say nothing"
        );
    }

    /// Folded and filtered are two reasons a row is not on screen, and printing undoes only
    /// the first. Sharing one mechanism would print rows a reader had filtered away.
    #[test]
    fn printing_opens_what_was_folded_and_leaves_what_was_filtered() {
        assert!(
            SCRIPT.contains("classList.toggle('filtered-out'"),
            "the filter must not reach for the attribute that means folded"
        );
        let print = STYLE
            .split_once("@media print")
            .expect("nothing is written for paper")
            .1;
        assert!(
            print.contains("tr.panel[hidden]:not(.filtered-out)"),
            "printing would unfold rows the reader had filtered away"
        );
        assert!(
            STYLE.contains(".filtered-out {"),
            "the filter's class styles nothing"
        );
    }

    /// The finding names a category, and the next question is always which reviews. A link
    /// that lands on a folded row is worse than no link, so the panel has to open itself.
    #[test]
    fn the_finding_leads_to_the_reviews_behind_it() {
        let page = render(&sample_report("ordinary text"));
        let headline = page
            .split_once("<p class=\"headline\">")
            .expect("no finding")
            .1;
        let target = headline
            .split_once("<a href=\"#")
            .expect("the category is not a way to anything")
            .1
            .split_once('"')
            .expect("an unclosed href")
            .0
            .to_owned();

        assert!(
            page.contains(&format!("id=\"{target}\"")),
            "the finding points at {target}, which is not on the page"
        );
        assert!(
            SCRIPT.contains("hashchange"),
            "a second link to a second category would open nothing"
        );
        // 30 of the 50 Steam ranks most helpful raise bugs, against 40.0% of the corpus. The
        // reader is owed the count the claim is built on, not only the share it comes to.
        assert!(
            headline.contains("30 of the 50 reviews Steam ranks most helpful raise it"),
            "the finding hides how few reviews it rests on: {}",
            headline
                .split_once("</p>")
                .map_or(headline, |(head, _)| head)
        );
    }

    /// Every way through the page has to arrive somewhere, and no two things may answer to
    /// the same name. A link into the evidence is worthless if its target moved or doubled.
    #[test]
    fn nothing_on_the_page_points_at_something_that_is_not_there() {
        // Both shapes: the contents, the cross-game matrix and the way back out of a section
        // only exist once there is more than one game, and they are all links.
        for page in [
            render(&sample_report("ordinary text")),
            render(&two_games()),
        ] {
            no_dangling_links(&page);
        }
    }

    /// An induced subject's evidence was never read by the counting model, and the page must
    /// not dress it as though it had been.
    #[test]
    fn an_induced_subject_shows_its_reviews_without_inventing_a_reading() {
        let mut report = sample_report("ordinary text");
        let review = report.apps[0].examples[0].1[0].review.clone();
        report.apps[0].induced = vec![crate::report::InducedEvidence {
            subject: crate::induced::Induced {
                id: "mud-physics".to_owned(),
                label: "Mud physics".to_owned(),
                description: "How trucks behave in mud.".to_owned(),
                refines: Some("gameplay".to_owned()),
                evidence: vec!["42".to_owned(), "43".to_owned(), "44".to_owned()],
            },
            reviews: vec![review],
        }];
        let page = render(&report);

        let section = page
            .split_once("players talk about that others")
            .expect("no induced section")
            .1
            .split_once("<h3>")
            .map_or("", |(before, _)| before);
        assert!(section.contains("Mud physics"), "{section}");
        assert!(section.contains("a form of Gameplay"), "{section}");
        assert!(section.contains("1 of the 3 reviews"), "{section}");
        assert!(
            !section.contains("sure</span>"),
            "a review the model never read must not carry a confidence: {section}"
        );
        assert!(
            !section.contains("class=\"chip neutral\"")
                && !section.contains("class=\"chip praise\""),
            "nor a polarity: {section}"
        );

        let without = render(&sample_report("ordinary text"));
        assert!(
            !without.contains("players talk about that others"),
            "a game with nothing induced gets no empty heading"
        );
    }

    fn two_games() -> Report {
        let mut report = sample_report("ordinary text");
        let mut second = report.apps[0].clone();
        second.crawl.app_id = 9;
        second.crawl.name = "Another Game".to_owned();
        second.reading.app_id = 9;
        report.apps.push(second);
        report
    }

    fn no_dangling_links(page: &str) {
        let mut ids: Vec<&str> = attributes(page, "id=\"");
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(count, ids.len(), "two things answer to the same name");

        let mut targets: Vec<&str> = attributes(page, "aria-controls=\"");
        targets.extend(
            attributes(page, "href=\"#")
                .into_iter()
                .filter(|target| !target.is_empty()),
        );
        targets.extend(attributes(page, "for=\""));
        for target in targets {
            assert!(
                ids.binary_search(&target).is_ok(),
                "the page points at {target}, which is not on it"
            );
        }
    }

    fn attributes<'a>(page: &'a str, opening: &str) -> Vec<&'a str> {
        page.match_indices(opening)
            .filter_map(|(at, _)| page[at + opening.len()..].split_once('"').map(|(v, _)| v))
            .collect()
    }

    /// Every review raising a category recommending the game is 100% whether that is eight
    /// reviews or a thousand, and only one of the two is a warm subject.
    #[test]
    fn a_verdict_share_is_called_warm_only_when_the_count_can_carry_it() {
        let warmth = |mentions: u64, positive: u64| {
            let mut report = sample_report("ordinary text");
            let app = &mut report.apps[0];
            app.reading.reviews = 1_000;
            app.reading.positive = 700;
            app.reading.subjects[1].mention_reviews = mentions;
            app.reading.subjects[1].positive_mentions = positive;
            let row = render(&report)
                .split_once("data-name=\"performance\"")
                .expect("no performance row")
                .1
                .split_once("</tr>")
                .expect("the row never ends")
                .0
                .to_owned();
            assert!(row.contains("100.0%"), "the share itself is missing: {row}");
            (row.contains("warmer"), row.contains("colder"))
        };

        assert_eq!(
            warmth(8, 8),
            (false, false),
            "eight reviews were called a warm subject against a 70% baseline"
        );
        assert_eq!(
            warmth(400, 400),
            (true, false),
            "four hundred reviews were not"
        );
    }

    /// Bars drawn against the busiest month say nothing about size until that month does,
    /// and the dip in the share line is the one thing on the chart a pointer is needed for.
    #[test]
    fn the_chart_says_how_big_the_month_it_is_drawn_against_was() {
        let page = render(&sample_report("ordinary text"));
        assert!(
            page.contains("against a busiest month of 600 in Feb 2024"),
            "the chart has no scale on it"
        );
        // The busiest month is also the angriest here, as it is on most games worth reporting
        // on, and naming it and its size twice in three lines reads as a mistake.
        assert!(
            page.contains("falls furthest in that same month, when 63.3% of them recommended"),
            "the chart's low point can only be reached with a pointer"
        );
        let note = page
            .split_once("Reviews per month,")
            .expect("no chart")
            .1
            .split_once("</p>")
            .expect("the note never ends")
            .0;
        assert!(
            !note.contains("furthest in Feb 2024"),
            "the busiest month is named again as the angriest: {note}"
        );
        assert_eq!(
            note.matches("600").count(),
            1,
            "the size of that month is given twice: {note}"
        );

        // Four reviews and none of them a thumb up is 0% and is not the month a game was
        // hated in; thirty in a hundred is.
        let mut noisy = vec![
            month("2024-01", 100, 10),
            month("2024-02", 4, 1),
            month("2024-03", 100, 10),
        ];
        noisy[0].positive = 30;
        noisy[1].positive = 0;
        assert!(
            render(&with_months(noisy))
                .contains("falls furthest in Jan 2024, when 30.0% of 100 reviews recommended"),
            "a four-review month was called the low point"
        );
    }

    /// Marking a game as the one that talks about a subject most is a claim, and on a row
    /// nobody raises there is nothing to claim: three reviews in a million beat two.
    #[test]
    fn a_subject_nobody_raises_has_no_game_it_belongs_to() {
        let mut report = two_games();
        for (app, mentions) in report.apps.iter_mut().zip([3_u64, 2]) {
            app.reading.reviews = 100_000;
            // Performance is the row that exists only because five reviews in two hundred
            // thousand mention it.
            app.reading.subjects[1].mention_reviews = mentions;
        }
        // Bugs is loud on both games and loudest on one of them, so it keeps its outline.
        report.apps[1].reading.subjects[0].mention_reviews = 300;

        let matrix = |report: &Report| {
            render(report)
                .split_once("<table class=\"matrix\">")
                .expect("no matrix")
                .1
                .split_once("</table>")
                .expect("the matrix never ends")
                .0
                .to_owned()
        };
        let performance = |grid: &str| {
            grid.split_once("<th scope=\"row\">Performance</th>")
                .expect("no performance row")
                .1
                .split_once("</tr>")
                .expect("the row never ends")
                .0
                .to_owned()
        };

        let grid = matrix(&report);
        assert!(
            grid.contains("heat loudest"),
            "a rate worth ranking is not marked"
        );
        assert!(
            !performance(&grid).contains("loudest"),
            "three reviews in a hundred thousand were called a game's subject"
        );

        report.apps[0].reading.subjects[1].mention_reviews = 4_000;
        assert!(
            performance(&matrix(&report)).contains("loudest"),
            "a game that does raise the subject is not marked"
        );

        // Both print 4.0%, so which of them is ahead is a difference the reader cannot see.
        report.apps[1].reading.subjects[1].mention_reviews = 3_999;
        let tied = performance(&matrix(&report));
        assert_eq!(
            tied.matches("4.0%").count(),
            2,
            "the tie is not on the page"
        );
        assert!(
            !tied.contains("loudest"),
            "one review in a hundred thousand was drawn as a finding"
        );
    }

    /// A splitter that no longer cuts any claim a set names puts every game with a set into
    /// this state until it is read again, so it is a state the page spends real time in.
    /// Telling a reader that nobody has labelled the game sends them to do work that is
    /// already done.
    #[test]
    fn a_set_that_cannot_be_scored_is_not_reported_as_no_set_at_all() {
        let why = "none of its 40 labelled claims is a claim this build cuts";
        let mut report = two_games();
        report.apps[0].agreement = crate::report::Measurement::Unscored(why.to_owned());
        let page = render(&report);

        let section = page
            .split_once("id=\"app-7\"")
            .expect("no section for the game")
            .1
            .split_once("</section>")
            .expect("the section never ends")
            .0;
        assert!(
            !section.contains("No claims have been labelled"),
            "a game that has been labelled is reported as never labelled"
        );
        assert!(
            section.contains(why),
            "the page does not say why the set went unscored: {section}"
        );

        let table = page
            .split_once("<table class=\"corpora\">")
            .expect("no corpora table")
            .1;
        assert!(
            table.contains("labelled, not scored"),
            "the dash beside the game is read out as no set at all"
        );
    }

    /// A set of thirty games is a table of thirty rows, and the questions a reader brings to
    /// it are which corpus is biggest and where the classifier is weakest. Both are a sort,
    /// and a sort on the printed text puts 982,291 below 2,000 and an unmeasured game above
    /// every measured one.
    #[test]
    fn the_corpora_can_be_reordered_on_what_they_hold_rather_than_on_the_text_of_it() {
        let mut report = two_games();
        report.apps[1].reading.reviews = 982_291;
        let page = render(&report);
        let table = page
            .split_once("<table class=\"corpora\">")
            .expect("no corpora table")
            .1
            .split_once("</table>")
            .expect("the table never ends")
            .0;

        let head = table.split_once("</thead>").expect("no head").0;
        assert_eq!(
            head.matches("data-sort").count(),
            4,
            "not every column of the corpora table can reorder it: {head}"
        );
        assert!(
            table.contains("data-value=\"982291\""),
            "a review count is left to be sorted as text: {table}"
        );
        // An unmeasured game is not a game measured at zero, and sorting has to keep them
        // apart or the worst-measured game in a set is one nobody measured.
        assert!(
            table.contains("data-value=\"-1.000000\""),
            "a game with no reference set sorts as zero agreement: {table}"
        );
        assert_eq!(
            table.matches("<tr class=\"row\">").count(),
            2,
            "the rows are not the ones the sort moves"
        );
    }

    /// The claim this tool exists to make can always be answered with "that is just that
    /// game" when it is made of one corpus. Made of every corpus in the report, it cannot be,
    /// so the sentence has to be counted over all of them rather than read off the first.
    #[test]
    fn the_bias_claim_across_games_is_counted_over_all_of_them() {
        let mut report = two_games();
        let sentence = |page: &str| {
            page.split_once("<p class=\"headline\">Across these ")
                .expect("no cross-game bias claim")
                .1
                .split_once("</p>")
                .expect("the claim never ends")
                .0
                .to_owned()
        };

        let claim = sentence(&render(&report));
        assert!(
            claim.contains(
                "2 games the top of the pile overstates <strong>bugs and crashes</strong>"
            ) && claim.contains("by <strong>1.5\u{d7}</strong>"),
            "the pooled claim is not the one the pooled counts support: {claim}"
        );
        assert!(
            claim.contains("60 of the 100 reviews at the top of those 2 piles")
                && claim.contains("against 40.0% of all 2,000"),
            "the claim is counted over one game rather than both: {claim}"
        );

        // A subject the second game's most-helpful reviews are full of and its corpus is not.
        // Reading either game alone still names bugs; only pooling moves the claim.
        report.apps[1].reading.subjects[1].top_mention_reviews = 45;
        let moved = sentence(&render(&report));
        assert!(
            moved.contains("<strong>performance</strong>"),
            "a subject only the pooled counts find was not found: {moved}"
        );
    }

    /// A census of a genre is dozens of games and a screen holds about eight, so a claim
    /// carried only by an outline around one cell is a claim most readers never see.
    #[test]
    fn a_row_names_the_game_that_leads_it_rather_than_only_outlining_the_cell() {
        let mut report = two_games();
        report.apps[1].reading.subjects[0].mention_reviews =
            report.apps[0].reading.subjects[0].mention_reviews * 2;

        let row = |page: &str| {
            page.split_once("<table class=\"matrix\">")
                .expect("no matrix")
                .1
                .split_once("<th scope=\"row\">Bugs and crashes</th>")
                .expect("no bugs row")
                .1
                .split_once("</tr>")
                .expect("the row never ends")
                .0
                .to_owned()
        };

        let leading = row(&render(&report));
        let named = leading
            .split_once("class=\"leader\">")
            .expect("the row does not say which game leads it")
            .1
            .split_once("</td>")
            .expect("the summary never ends")
            .0
            .to_owned();
        assert!(
            named.contains("Another Game"),
            "the leading game is not named beside the row: {named}"
        );
        assert!(
            named.contains("href=\"#app-9\""),
            "the named game is not a way of reaching it: {named}"
        );

        // Both games raise it at the same rate, so there is nothing the page can name.
        report.apps[1].reading.subjects[0].mention_reviews =
            report.apps[0].reading.subjects[0].mention_reviews;
        assert!(
            !row(&render(&report)).contains("Another Game"),
            "a tie was reported as a game leading the row"
        );
    }

    /// A section comparing what the games talk about that never compares the games
    /// themselves leaves how big each one is and how well it is measured a visit apart.
    #[test]
    fn the_cross_game_section_compares_the_corpora_and_not_only_the_subjects() {
        let mut report = two_games();
        report.apps[0].agreement =
            crate::report::Measurement::Measured(Box::new(measured(100, 62)));

        let table = render(&report)
            .split_once("<table class=\"corpora\">")
            .expect("no corpora table")
            .1
            .split_once("</table>")
            .expect("the table never ends")
            .0
            .to_owned();
        assert!(table.contains("href=\"#app-7\""), "no way into a game");
        assert!(table.contains("A Game &lt;&amp; Friends&gt;"));
        assert!(table.contains(">1,000<"), "no review count: {table}");
        assert!(table.contains(">70.0%<"), "no recommended share: {table}");
        assert_eq!(
            table.matches("no reference set").count(),
            1,
            "an unmeasured game is given a figure, or a measured one is not: {table}"
        );
    }

    /// A grid of six columns leaves the reader to find the interesting row. The section
    /// says which one it is, and a report of one game has no such row to name.
    #[test]
    fn a_report_of_several_games_says_what_they_disagree_about() {
        let mut report = two_games();
        // Bugs is level across both; performance is the row that separates them.
        report.apps[1].reading.subjects[1].mention_reviews = 20;

        let page = render(&report);
        let finding = page
            .split_once("<h2>Across these games</h2>")
            .expect("no cross-game section")
            .1
            .split_once("</section>")
            .expect("the section never ends")
            .0;
        assert!(
            finding.contains("disagree about most is <strong>performance</strong>"),
            "the wrong row was named: {finding}"
        );
        assert!(finding.contains("A Game &lt;&amp; Friends&gt;"));
        assert!(finding.contains("Another Game"));

        let alone = render(&sample_report("ordinary text"));
        assert!(
            !alone.contains("disagree about most"),
            "one game cannot disagree with itself"
        );
    }

    #[test]
    fn bars_are_drawn_against_the_busiest_month_and_the_share_line_across_them() {
        // 400 and 600 reviews: the busier is the chart's full 160, the other two thirds of it.
        let page = render(&sample_report("It crashes."));
        assert!(
            page.contains("height=\"160.00\""),
            "the busiest month is full height"
        );
        assert!(
            page.contains("height=\"106.67\""),
            "the other is drawn to its scale"
        );
        assert!(page.contains("<polyline class=\"share\""));
    }

    #[test]
    fn a_sparkline_spans_the_whole_width_from_its_first_month_to_its_last() {
        let page = render(&with_months(vec![
            month("2024-01", 400, 40),
            month("2024-02", 400, 80),
            month("2024-03", 400, 120),
        ]));
        assert!(page.contains(" 500.00,"), "the middle month sits halfway");
        assert!(
            page.contains(" 1000.00,"),
            "the last month sits at the right-hand edge"
        );
    }

    #[test]
    fn the_games_disagree_most_by_the_widest_gap_in_points_not_the_largest_ratio() {
        // Bugs at 40% against 20% is twenty points apart; performance at 10% against 3% is
        // seven, though more than three times over.
        let mut report = two_games();
        report.apps[1].reading.subjects[0].mention_reviews = 200;
        report.apps[1].reading.subjects[1].mention_reviews = 30;
        let page = render(&report);
        assert!(page.contains("disagree about most is"));
        assert!(
            !page.contains("disagree about most is <strong>performance</strong>"),
            "the largest ratio was named rather than the widest gap"
        );
    }

    #[test]
    fn agreement_is_pooled_only_when_every_game_was_measured() {
        let willing = "labelled claims this model was willing to answer";
        let mut report = two_games();
        report.apps[0].agreement =
            crate::report::Measurement::Measured(Box::new(measured(100, 80)));
        assert_eq!(
            render(&report).matches(willing).count(),
            1,
            "one measured game of two is that game's figure and no pooled one"
        );
        report.apps[1].agreement =
            crate::report::Measurement::Measured(Box::new(measured(100, 60)));
        assert_eq!(render(&report).matches(willing).count(), 3);

        let mut three = report.clone();
        let mut third = two_games().apps[1].clone();
        third.crawl.app_id = 11;
        third.reading.app_id = 11;
        three.apps.push(third);
        assert_eq!(
            render(&three).matches(willing).count(),
            2,
            "two measured games of three are not pooled"
        );
        three.apps[2].agreement = crate::report::Measurement::Measured(Box::new(measured(100, 70)));
        assert_eq!(render(&three).matches(willing).count(), 4);
    }

    #[test]
    fn a_share_line_needs_two_months_that_have_a_share() {
        let page = render(&with_months(vec![
            month("2024-01", 400, 40),
            month("2024-02", 0, 0),
        ]));
        assert!(
            page.contains("class=\"timeline\""),
            "two months are a chart"
        );
        assert!(
            !page.contains("<polyline class=\"share\""),
            "one point is not a line"
        );
    }

    #[test]
    fn a_cell_with_no_number_says_so_out_loud() {
        let mut out = String::new();
        let unraised = crate::read::SubjectCount {
            id: "performance".to_owned(),
            label: "Performance".to_owned(),
            primary_reviews: 0,
            mention_reviews: 0,
            claims: 0,
            praised: 0,
            criticised: 0,
            mixed: 0,
            top_mention_reviews: 0,
            positive_mentions: 0,
        };
        rate_cell(&mut out, None);
        bias_cell(&mut out, None);
        verdict_cell(&mut out, &unraised, Some(0.8));

        assert!(
            !out.contains('\u{2014}'),
            "an em dash reached the page: {out}"
        );
        assert_eq!(
            out.matches("class=\"read-aloud\"").count(),
            3,
            "a dash was left with nothing to say to a screen reader: {out}"
        );
        assert!(out.contains("no reviews"));
        assert!(out.contains("not measured"));
        assert!(out.contains("none raised it"));
    }

    #[test]
    fn every_category_with_mentions_reaches_the_page_with_its_evidence() {
        let page = render(&sample_report("ordinary text"));

        assert!(page.contains("Bugs and crashes"));
        assert!(page.contains("Performance"));
        // 400 of 1,000 mention bugs, 30 of the top 50 do: 60% against 40%.
        assert!(page.contains("40.0%"), "the corpus rate is missing");
        assert!(
            page.contains("60.0%"),
            "the top-of-the-pile rate is missing"
        );
        assert!(page.contains("1.5\u{d7}"), "the bias factor is missing");
        assert!(
            page.contains("On Steam"),
            "the link back to the source is missing"
        );
    }

    #[test]
    fn a_capture_swept_since_the_reading_says_so_before_any_rate() {
        let mut report = sample_report("ordinary text");
        let app = &mut report.apps[0];
        app.crawl.swept_unix = Some(1_700_500_000);
        app.crawl.sweeps = 1;
        app.crawl.rows_swept = 42;
        app.reading.captured_unix = 1_700_000_000;

        let page = render(&report);
        assert!(
            page.contains("Brought up to date"),
            "the sweep is not a fact on the page"
        );
        assert!(page.contains("42 added or edited since the snapshot"));
        let warning = page
            .find("The capture was brought up to date on")
            .expect("a warning");
        let table = page
            .find("<table class=\"categories\">")
            .expect("the table");
        assert!(
            warning < table,
            "the warning comes after the rates it is about"
        );

        // A reading made after the sweep has nothing to warn about.
        report.apps[0].reading.captured_unix = 1_700_500_000;
        let page = render(&report);
        assert!(!page.contains("The capture was brought up to date on"));
        assert!(page.contains("Brought up to date"));
    }

    #[test]
    fn the_words_a_side_uses_are_shown_with_the_reviewers_behind_them() {
        let page = render(&sample_report("ordinary text"));

        // The complaint side has evidence, so its terms are on the page with their counts.
        assert!(
            page.contains(
                "<span class=\"term\">save corruption<span class=\"n\" title=\"reviews using \
                 it\">37</span></span>"
            ),
            "the term is missing its count"
        );
        // The praise side has no quoted evidence in this report, and a list of words under a
        // heading that is not there would be a finding with nothing to open. The paragraph at
        // the top still quotes it, because the paragraph is about the counts, not the quotes.
        assert!(!page.contains("<span class=\"term\">patched quickly"));
        assert!(page.contains("the praise says \u{201c}patched quickly\u{201d}"));
        assert!(page.contains("Bugs and crashes divides opinion"));
    }

    /// The declarations inside a block, given the text that opens it.
    fn declared_in(marker: &str) -> Vec<&'static str> {
        let start = STYLE
            .find(marker)
            .unwrap_or_else(|| panic!("no {marker} in the stylesheet"));
        let open = start + STYLE[start..].find('{').expect("a block with no brace");
        let mut depth = 0_i32;
        let mut end = open;
        for (offset, character) in STYLE[open..].char_indices() {
            match character {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = open + offset;
                        break;
                    }
                }
                _ => {}
            }
        }
        STYLE[open..end]
            .lines()
            .filter_map(|line| line.trim().strip_prefix("--"))
            .filter_map(|line| line.split(':').next())
            .collect()
    }

    #[test]
    fn no_colour_goes_missing_when_the_page_is_read_in_the_dark() {
        // A token used but not defined for a theme is invisible text, and only in that
        // theme, which is exactly the kind of thing nobody notices until somebody else does.
        let light = declared_in(":root {");
        let media = declared_in("(prefers-color-scheme: dark)");
        let attribute = declared_in(":root[data-theme='dark']");

        // Set on individual elements by the markup rather than by the theme.
        let inline = ["fill", "heat", "offset", "part"];
        for used in STYLE.split("var(--").skip(1) {
            let name = used.split([')', ',', ' ']).next().unwrap_or_default();
            assert!(
                light.contains(&name) || inline.contains(&name),
                "--{name} is used and never defined"
            );
        }
        for name in &light {
            assert_eq!(
                media.contains(name),
                attribute.contains(name),
                "--{name} is themed by one dark rule and not the other"
            );
        }
        assert!(!media.is_empty(), "the dark theme defines nothing at all");
    }

    #[test]
    fn every_character_that_could_close_a_tag_is_escaped() {
        // Review text is arbitrary text written by strangers. A single unescaped angle
        // bracket in a million reviews is a broken page at best.
        assert_eq!(
            escape("<script>alert(\"x\" & 'y')</script>"),
            "&lt;script&gt;alert(&quot;x&quot; &amp; &#39;y&#39;)&lt;/script&gt;"
        );
    }

    /// A column with no definition is a number with no name for what it counts, and the
    /// panel a reader would look in has to hold one for every column the table draws.
    #[test]
    fn every_column_the_table_draws_is_defined_where_a_reader_looks() {
        let page = render(&sample_report("ordinary text"));
        let legend = page
            .split_once("<details class=\"legend\">")
            .expect("no legend")
            .1
            .split_once("</details>")
            .expect("the legend never ends")
            .0;
        for (name, _) in COLUMNS {
            assert!(
                page.contains(&format!(
                    "<button type=\"button\" class=\"sort\" data-sort>{name}"
                )),
                "{name} is defined and never drawn"
            );
            assert!(
                legend.contains(&format!("<dt>{name}</dt>")),
                "{name} is drawn and never defined"
            );
        }
        assert!(
            legend.contains("the 50 reviews Steam ranks"),
            "the definition still carries its placeholder: {legend}"
        );
    }

    /// Six thousand reviews is a different thing in a corpus of thirty thousand and in one
    /// of a million, and a bar says which is bigger but never how much of the whole it is.
    #[test]
    fn every_language_says_how_much_of_the_corpus_it_is() {
        let page = render(&sample_report("ordinary text"));
        let list = page
            .split_once("<ul class=\"languages\">")
            .expect("no languages")
            .1
            .split_once("</ul>")
            .expect("the list never ends")
            .0;
        // 600 English and 400 Chinese of a thousand.
        assert!(list.contains(">60.0%<"), "English has no share: {list}");
        assert!(list.contains(">40.0%<"), "Chinese has no share: {list}");
        assert!(list.contains(">600<") && list.contains(">400<"), "{list}");
    }

    #[test]
    fn steam_language_names_become_tags_a_browser_knows() {
        assert_eq!(bcp47("schinese"), Some("zh-Hans"));
        assert_eq!(bcp47("koreana"), Some("ko"));
        assert_eq!(bcp47("brazilian"), Some("pt-BR"));
        // A language Steam adds after this was written must not be guessed at.
        assert_eq!(bcp47("klingon"), None);
        assert_eq!(language_name("klingon"), "klingon");
        assert_eq!(language_name("latam"), "Spanish (Latin America)");

        // Every tag has to be one, and no two names may claim the same one.
        let mut tags: Vec<&str> = LANGUAGES.iter().map(|(_, tag, _)| *tag).collect();
        tags.sort_unstable();
        let count = tags.len();
        tags.dedup();
        assert_eq!(tags.len(), count, "two languages share a tag");
        for (name, tag, display) in LANGUAGES {
            assert!(!name.is_empty() && !display.is_empty(), "{name} is unnamed");
            assert!(
                tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
                "{tag} is not a tag"
            );
        }
        for tag in RIGHT_TO_LEFT {
            assert!(
                tags.contains(&tag),
                "{tag} runs the other way and is not a language the page knows"
            );
        }
    }

    /// A review written right to left, laid out left to right, is unreadable in a way the
    /// page has all the information to avoid.
    #[test]
    fn a_review_is_laid_out_the_way_it_was_written() {
        let mut report = sample_report("مراجعة عن اللعبة");
        report.apps[0].examples[0].1[0].review.language = "arabic".to_owned();
        assert!(
            render(&report).contains("<p class=\"claim\" lang=\"ar\" dir=\"rtl\">"),
            "an Arabic review is laid out as English"
        );

        let english = render(&sample_report("ordinary text"));
        assert!(
            english.contains("<p class=\"claim\" lang=\"en\">"),
            "an English review has no language on it"
        );
        assert!(
            !english.contains("dir=\"rtl\""),
            "an English review is laid out backwards"
        );
    }

    #[test]
    fn a_rate_too_small_to_round_is_not_shown_as_zero() {
        // 0.0% reads as "nobody said this", which is a different claim from "almost nobody".
        assert_eq!(percent(0.0004), "<0.1%");
        assert_eq!(percent(0.0), "0.0%");
        assert_eq!(percent(0.1234), "12.3%");
        assert_eq!(
            percent(0.001),
            "0.1%",
            "a tenth of a percent rounds to itself"
        );
    }

    #[test]
    fn a_subject_belongs_to_a_game_from_a_tenth_of_a_percent() {
        assert_eq!(belongs_to(&[Some(0.001), Some(0.0)]), Some(0));
        assert_eq!(belongs_to(&[Some(0.0009), Some(0.0)]), None);
    }

    #[test]
    fn the_polarity_column_divides_by_every_review_that_judged_the_subject() {
        let category = crate::read::SubjectCount {
            praised: 2,
            criticised: 1,
            mixed: 1,
            ..a_category("bugs", "Bugs and crashes", 0, 0)
        };
        let mut out = String::new();
        polarity_cell(&mut out, &category);
        assert!(out.contains("50.0% praise"), "{out}");
        assert!(out.contains("25.0% gripe, 25.0% both"), "{out}");
    }

    #[test]
    fn languages_past_the_ones_listed_are_summed_and_none_are_not_mentioned() {
        let page = render(&sample_report("It crashes."));
        assert!(
            !page.contains("more languages"),
            "two languages leave none over"
        );

        let mut report = sample_report("It crashes.");
        report.apps[0].reading.languages = (0..13_u64)
            .map(|rank| (format!("language{rank}"), 100 - rank))
            .collect();
        let page = render(&report);
        assert!(page.contains("and 1 more languages"));
    }

    #[test]
    fn labels_whose_reviews_this_build_cuts_differently_are_counted_out_loud() {
        let mut out = String::new();
        agreement_note(
            &mut out,
            &crate::measure::ClaimAgreement {
                unjoined: 5,
                ..measured(100, 80)
            },
        );
        assert!(out.contains("A further 5 labelled claims"), "{out}");
        let mut quiet = String::new();
        agreement_note(&mut quiet, &measured(100, 80));
        assert!(!quiet.contains("A further"), "{quiet}");
    }

    #[test]
    fn the_whole_review_follows_its_claim_only_where_it_says_more() {
        let mut report = sample_report("It crashes.");
        assert!(!render(&report).contains("class=\"text whole"));
        report.apps[0].examples[0].1[0].review.text = "It crashes. And it is ugly.".to_owned();
        assert!(render(&report).contains("class=\"text whole"));
    }

    #[test]
    fn a_language_the_reader_declines_is_named_rather_than_left_as_a_silence() {
        // Without the sentence the page shows a corpus declined far above the usual rate and
        // gives no reason, and the reason a reader would reach for, that this game is about
        // something the taxonomy lacks, is the wrong one.
        let mut report = sample_report("It crashes.");
        report.apps[0].reading.unread_languages = vec![("indonesian".to_owned(), 250)];
        let page = render(&report);
        assert!(
            page.contains("declines outright"),
            "the page shows the decline and never says it is the labels"
        );
        assert!(
            page.contains("Indonesian"),
            "the page will not say which language it cannot read"
        );

        let quiet = render(&sample_report("It crashes."));
        assert!(
            !quiet.contains("declines outright"),
            "a corpus in languages the reader has lines for is told it has a gap it does not"
        );
    }

    #[test]
    fn a_corpus_in_the_harder_languages_is_told_that_rather_than_left_to_guess() {
        // The two sentences answer the same question, "why is this declining so much", with
        // opposite work: a category against more labels. A page carrying only the first offers
        // the rarer cause for the commoner one, because the silenced languages are usually a
        // rounding error beside the strict ones.
        let mut report = sample_report("It crashes.");
        report.apps[0].reading.strict_languages = vec![("schinese".to_owned(), 400)];
        let page = render(&report);
        assert!(
            page.contains("surer about than English"),
            "a corpus weighted to the languages the reader is least sure in is not told so"
        );
        assert!(
            page.contains("more labels in those languages"),
            "the page names the cause and not the work it asks for"
        );

        let quiet = render(&sample_report("It crashes."));
        assert!(
            !quiet.contains("surer about than English"),
            "an English corpus is told it leans on languages it does not have"
        );
    }

    #[test]
    fn thousands_groups_from_the_right() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_000), "1,000");
        assert_eq!(thousands(1_161_047), "1,161,047");
    }

    #[test]
    fn time_played_is_minutes_under_an_hour_and_whole_hours_from_one() {
        assert_eq!(hours(59), "59 min");
        assert_eq!(hours(60), "1 h");
        assert_eq!(hours(600), "10 h");
    }

    #[test]
    fn a_share_of_nothing_is_nothing() {
        assert!(share_of(3, 0).abs() < f64::EPSILON);
        assert!((share_of(1, 4) - 0.25).abs() < f64::EPSILON);
    }

    /// The part of `page` from `opening` to the next `closing` after it.
    fn between<'a>(page: &'a str, opening: &str, closing: &str) -> &'a str {
        page.split_once(opening)
            .and_then(|(_, rest)| rest.split_once(closing))
            .map_or_else(|| panic!("no {opening} on the page"), |(inside, _)| inside)
    }

    #[test]
    fn the_title_names_the_game_when_there_is_one_and_steam_when_there_are_several() {
        assert!(
            render(&sample_report("ordinary text"))
                .contains("<title>A Game &lt;&amp; Friends&gt; reviews | SteamGauge</title>")
        );
        assert!(render(&two_games()).contains("<title>Steam reviews | SteamGauge</title>"));
    }

    #[test]
    fn several_games_are_listed_first_and_each_section_leads_back_to_the_list() {
        let several = render(&two_games());
        assert!(several.contains("<nav class=\"contents\" id=\"games\""));
        assert_eq!(
            several
                .matches("<a href=\"#games\">Back to the games</a>")
                .count(),
            2
        );

        let one = render(&sample_report("ordinary text"));
        assert!(!one.contains("<nav class=\"contents\""));
        assert!(one.contains("<a href=\"#main\">Back to the top</a>"));
    }

    #[test]
    fn the_matrix_lists_the_subjects_raised_loudest_first_shaded_by_the_root_of_their_rate() {
        let mut report = two_games();
        report.apps[0].reading.subjects[0].mention_reviews = 500;
        report.apps[1].reading.subjects[0].mention_reviews = 80;
        let page = render(&report);
        let matrix = between(&page, "<table class=\"matrix\">", "</table>");
        assert_eq!(
            attributes(matrix, "<tr data-name=\""),
            ["bugs and crashes", "performance"]
        );
        assert!(
            matrix.contains("style=\"--heat:1.000\">50.0%"),
            "half the reviews is full shade"
        );
        assert!(matrix.contains("style=\"--heat:0.400\">8.0%"), "{matrix}");
    }

    #[test]
    fn games_a_tenth_of_a_percent_apart_are_far_enough_apart_to_name_and_level_ones_are_not() {
        assert!(!render(&two_games()).contains("disagree about most"));

        let mut report = two_games();
        report.apps[0].reading.subjects[1].mention_reviews = 1;
        report.apps[1].reading.subjects[1].mention_reviews = 0;
        assert!(render(&report).contains(
            "The subject these games disagree about most is <strong>performance</strong>"
        ));
    }

    #[test]
    fn of_two_subjects_equally_far_apart_the_one_the_sheet_lists_first_is_named() {
        let mut report = two_games();
        // Performance comes before bugs on the sheet, and both are a quarter apart.
        report.apps[0].reading.subjects[0].mention_reviews = 500;
        report.apps[1].reading.subjects[0].mention_reviews = 250;
        report.apps[0].reading.subjects[1].mention_reviews = 250;
        report.apps[1].reading.subjects[1].mention_reviews = 0;
        assert!(render(&report).contains("disagree about most is <strong>performance</strong>"));
    }

    #[test]
    fn what_steam_calls_a_game_is_given_only_where_steam_calls_it_anything() {
        assert!(
            render(&sample_report("ordinary text"))
                .contains("<dt>Steam calls it</dt><dd>Mostly Positive</dd>")
        );
        let mut report = sample_report("ordinary text");
        report.apps[0].crawl.review_score_desc = String::new();
        assert!(!render(&report).contains("Steam calls it"));
    }

    #[test]
    fn the_headline_names_its_subject_as_the_way_to_its_reviews() {
        let page = render(&sample_report("ordinary text"));
        let headline = between(&page, "<p class=\"headline\">", "</p>");
        assert!(
            headline.contains("<a href=\"#panel-7-bugs\"><strong>bugs and crashes</strong></a>"),
            "{headline}"
        );
    }

    #[test]
    fn each_month_has_its_slot_across_the_width_for_its_bar_its_point_and_its_pointer() {
        let months = (1..=4)
            .map(|at| month(&format!("2024-0{at}"), 100, 10))
            .collect();
        let page = render(&with_months(months));
        let slots = ["0.00", "250.00", "500.00", "750.00"];
        assert_eq!(attributes(&page, "<rect class=\"bar\" x=\""), slots);
        assert_eq!(attributes(&page, "<rect class=\"hit\" x=\""), slots);
        let points = between(&page, "<polyline class=\"share\" points=\"", "\"");
        let centres: Vec<&str> = points
            .split(' ')
            .filter_map(|point| point.split_once(',').map(|(x, _)| x))
            .collect();
        assert_eq!(centres, ["125.00", "375.00", "625.00", "875.00"]);
    }

    #[test]
    fn each_row_s_bar_is_its_share_of_the_loudest_row() {
        let page = render(&sample_report("ordinary text"));
        let row = |name: &str| between(&page, &format!("data-name=\"{name}\""), "</tr>").to_owned();
        assert!(row("bugs and crashes").contains("--fill:1.0000"));
        assert!(row("performance").contains("--fill:0.2500"));
    }

    fn scored(id: &'static str, labelled: u64, agreed: u64) -> crate::measure::SubjectAgreement {
        crate::measure::SubjectAgreement {
            id,
            label: "Bugs and crashes",
            labelled,
            read: agreed,
            agreed,
            seen: labelled * 4,
            mistaken_for: None,
        }
    }

    fn measured_on(subjects: Vec<crate::measure::SubjectAgreement>) -> Report {
        let mut report = sample_report("ordinary text");
        let mut agreement = measured(400, 300);
        agreement.subjects = subjects;
        report.apps[0].agreement = crate::report::Measurement::Measured(Box::new(agreement));
        report
    }

    #[test]
    fn a_row_carries_its_own_measurement_and_corrects_the_share_the_model_read() {
        let page = render(&measured_on(vec![scored("bugs", 100, 80)]));
        let bugs = between(&page, "id=\"panel-7-bugs\"", "</tr>");
        assert!(bugs.contains("Corrected for those errors"), "{bugs}");
        assert!(bugs.contains("against the 26.7% the model read"), "{bugs}");

        let page = render(&measured_on(vec![scored("bugs", 100, 4)]));
        let row = |name: &str| between(&page, &format!("data-name=\"{name}\""), "</tr>").to_owned();
        assert!(row("bugs and crashes").contains("class=\"thin\""));
        assert!(!row("performance").contains("class=\"thin\""));

        let mut nothing_read = measured_on(vec![scored("bugs", 100, 80)]);
        nothing_read.apps[0].reading.claims = 0;
        let page = render(&nothing_read);
        assert!(
            !between(&page, "id=\"panel-7-bugs\"", "</tr>").contains("Corrected for those errors"),
            "a reading of no claims has no share to correct"
        );
    }

    #[test]
    fn a_row_is_judged_from_ten_labels_and_thin_only_under_a_quarter() {
        assert!(
            thinly_measured(&scored("bugs", ENOUGH_TO_JUDGE_A_ROW, 1)).contains("class=\"thin\"")
        );

        let mut quarter = String::new();
        how_well_this_row_is_known(&mut quarter, &scored("bugs", 100, 25), Some(0.2));
        assert!(!quarter.contains("note warn"), "{quarter}");
        let mut few = String::new();
        how_well_this_row_is_known(
            &mut few,
            &scored("bugs", ENOUGH_TO_JUDGE_A_ROW - 1, 0),
            None,
        );
        assert!(!few.contains("note warn"), "{few}");
    }

    #[test]
    fn a_subject_s_line_is_drawn_once_three_months_carry_a_rate() {
        let rated = (1..=4)
            .map(|at| month(&format!("2024-0{at}"), 100, 10 * at))
            .collect();
        let page = render(&with_months(rated));
        assert!(
            between(&page, "id=\"panel-7-bugs\"", "</tr>").contains("<figure class=\"spark\">")
        );

        let two = vec![
            month("2024-01", 100, 10),
            month("2024-02", 100, 10),
            month("2024-03", 5, 1),
        ];
        let page = render(&with_months(two));
        assert!(
            !between(&page, "id=\"panel-7-bugs\"", "</tr>").contains("<figure class=\"spark\">")
        );
    }

    #[test]
    fn the_bias_gauge_leaves_the_middle_by_the_log_of_the_factor_either_way_and_stops_at_the_edge()
    {
        let gauge = |factor: f64| {
            let mut out = String::new();
            bias_cell(&mut out, Some(factor));
            out
        };
        assert!(gauge(2.0).contains("gauge over\" style=\"--offset:0.3333\""));
        assert!(gauge(0.5).contains("gauge under\" style=\"--offset:0.3333\""));
        assert!(gauge(64.0).contains("gauge over\" style=\"--offset:1.0000\""));
        assert!(gauge(1.0 / 64.0).contains("gauge under\" style=\"--offset:1.0000\""));
    }

    #[test]
    fn a_share_is_warmer_or_colder_only_once_its_interval_clears_the_baseline() {
        let mut category = a_category("bugs", "Bugs and crashes", 400, 30);
        category.positive_mentions = 300;
        let (low, high) = crate::measure::wilson(300, 400).unwrap();
        let tone = |baseline: f64| {
            let mut out = String::new();
            verdict_cell(&mut out, &category, Some(baseline));
            (out.contains("warmer"), out.contains("colder"))
        };
        assert_eq!(
            tone(low),
            (false, false),
            "touching the baseline is not clearing it"
        );
        assert_eq!(tone(low - 1e-9), (true, false));
        assert_eq!(tone(high), (false, false));
        assert_eq!(tone(high + 1e-9), (false, true));
    }

    #[test]
    fn the_top_of_the_pile_is_listed_in_full() {
        let mut report = sample_report("ordinary text");
        let mut top = report.apps[0].examples[0].1[0].clone();
        top.claim = "Steam put this first".to_owned();
        top.review.text = "Steam put this first".to_owned();
        report.apps[0].top = vec![top];
        let page = render(&report);
        let pile = between(&page, "<h3>The top of the pile</h3>", "</details>");
        assert!(
            pile.contains("<summary>Read all 1 of them</summary>"),
            "{pile}"
        );
        assert!(pile.contains("Steam put this first"), "{pile}");
    }

    #[test]
    fn each_side_of_a_subject_counts_the_reviews_on_it_and_the_neutral_side_counts_none() {
        let mut report = sample_report("ordinary text");
        let example = report.apps[0].examples[0].1[0].clone();
        let side = |polarity: &str| Example {
            polarity: polarity.to_owned(),
            ..example.clone()
        };
        report.apps[0].examples[0].1 = vec![side("praise"), side("complaint"), side("neutral")];
        let page = render(&report);
        let panel = between(&page, "id=\"panel-7-bugs\"", "</tr>");
        // A third of 400 praise, a third complain and a sixth do both.
        assert!(
            panel.contains("What they praise <span class=\"count\">199 reviews</span></h4>"),
            "{panel}"
        );
        assert!(
            panel
                .contains("What they complain about <span class=\"count\">199 reviews</span></h4>")
        );
        assert!(panel.contains("Said without judging</h4>"));
    }

    #[test]
    fn a_quoted_review_says_only_what_it_has_to_say() {
        let report = sample_report("ordinary text");
        let app = &report.apps[0];
        let quoted = |change: &dyn Fn(&mut Example)| {
            let mut example = app.examples[0].1[0].clone();
            change(&mut example);
            let mut out = String::new();
            review(&mut out, app, &example);
            out
        };

        let plain = quoted(&|_| {});
        assert!(plain.contains("<span class=\"votes\">12 found this helpful</span>"));
        assert!(plain.contains("<span class=\"played\">10 h played</span>"));
        assert!(plain.contains("<span class=\"lang\">english</span>"));
        assert!(plain.contains("<span class=\"chip\">Bugs and crashes</span>"));
        assert!(plain.contains("<span class=\"chip\">Performance</span>"));
        let one_other = quoted(&|example| example.also = vec!["bugs".to_owned()]);
        assert!(one_other.contains("<span class=\"chip\">Bugs and crashes</span>"));
        assert!(!one_other.contains("<span class=\"chip\">Performance</span>"));

        let bare = quoted(&|example| {
            example.review.votes_up = 0;
            example.review.playtime_at_review_minutes = 0;
            example.review.language = String::new();
        });
        assert!(!bare.contains("class=\"votes\""));
        assert!(!bare.contains("class=\"played\""));
        assert!(!bare.contains("class=\"lang\""));

        let long = quoted(&|example| {
            example.claim = "The claim.".to_owned();
            example.review.text = format!("The claim. {}", "More. ".repeat(80));
        });
        assert!(long.contains("<div class=\"text whole long\">"));
        let short = quoted(&|example| {
            example.claim = "The claim.".to_owned();
            example.review.text = "The claim. And a little more.".to_owned();
        });
        assert!(short.contains("<div class=\"text whole\">"));
        let at_the_limit = quoted(&|example| {
            example.claim = "The claim.".to_owned();
            example.review.text = format!("The claim.{}", "x".repeat(PREVIEW_CHARS - 10));
        });
        assert!(
            at_the_limit.contains("<div class=\"text whole\">"),
            "a review exactly as long as the preview is shown whole"
        );
    }

    #[test]
    fn the_page_says_who_read_the_corpus_and_by_what_rule() {
        assert!(render(&sample_report("ordinary text")).contains(
            "<dt>Read by</dt><dd>Game Review Reader (run a-reader), a fine-tune of test-reader, \
             trained on label set 0123456789abcdef, answering only above 0.50 confidence</dd>"
        ));
    }

    #[test]
    fn the_languages_say_how_much_is_not_english_and_each_bar_is_against_the_largest() {
        let page = render(&sample_report("ordinary text"));
        assert!(page.contains("40.0% of these reviews are not in English"));
        let languages = between(&page, "<ul class=\"languages\">", "</ul>");
        assert!(languages.contains("English</span><span class=\"bar\" style=\"--fill:1.0000\""));
        assert!(
            languages
                .contains("Chinese (simplified)</span><span class=\"bar\" style=\"--fill:0.6667\"")
        );
    }

    #[test]
    fn only_a_shallow_reading_says_how_it_was_read() {
        assert!(!render(&sample_report("ordinary text")).contains("<dt>Depth</dt>"));
        let mut shallow = sample_report("ordinary text");
        shallow.apps[0].reading.depth = crate::read::Depth::Shallow;
        assert!(render(&shallow).contains("<dt>Depth</dt>"));
    }

    #[test]
    fn what_was_declined_is_set_against_what_is_usually_declined() {
        let page = render(&sample_report("ordinary text"));
        assert!(
            page.contains("below the 10.0% it usually declines on a game it has never seen"),
            "the same share as usual is not called unusual"
        );

        let mut unusual = sample_report("ordinary text");
        unusual.apps[0].reading.unclassified_claims = 900;
        assert!(
            render(&unusual)
                .contains("That is 3.0 times what it declines on a game it has never seen")
        );
    }

    #[test]
    fn reviews_the_model_said_nothing_about_are_counted_only_where_there_are_some() {
        assert!(
            render(&sample_report("ordinary text"))
                .contains("<dt>Reviews it said nothing about</dt><dd>3 (0.3% of those read)")
        );
        let mut silent = sample_report("ordinary text");
        silent.apps[0].reading.silent_reviews = 0;
        assert!(!render(&silent).contains("Reviews it said nothing about"));
    }

    #[test]
    fn the_shape_of_the_error_is_drawn_from_the_subjects_with_enough_labels_to_judge() {
        let mut agreement = measured(400, 300);
        let mistaken =
            |id, labelled, agreed, as_what: &'static str, count| crate::measure::SubjectAgreement {
                mistaken_for: Some((as_what, count)),
                ..scored(id, labelled, agreed)
            };
        agreement.subjects = vec![
            mistaken("bugs", 20, 5, "Story", 3),
            mistaken("performance", 20, 18, "Graphics", 2),
            mistaken("story", ENOUGH_TO_JUDGE_A_ROW - 1, 0, "Audio", 9),
        ];
        let mut out = String::new();
        agreement_note(&mut out, &agreement);
        assert!(
            out.contains(
                "Every one of the 2 subjects with enough labels to judge is found in at least a \
                 quarter of the claims making it."
            ),
            "found in exactly a quarter is not fewer: {out}"
        );
        assert!(
            out.contains(
                "Where it disagrees most, 3 claims the labeller called Bugs and crashes \
                 were read as Story"
            ),
            "{out}"
        );
    }

    /// `reviews` reviews of one kind of reviewer, `positive` recommending, and `complaining`
    /// of them complaining about every subject in `about`.
    fn kind_of_reviewer(
        id: &str,
        reviews: u64,
        positive: u64,
        (about, complaining): (&[&str], u64),
    ) -> crate::who::SegmentCount {
        let per_subject = |of: u64| -> Vec<u64> {
            SHEET
                .iter()
                .map(|category| if about.contains(&category.id) { of } else { 0 })
                .collect()
        };
        crate::who::SegmentCount {
            id: id.to_owned(),
            reviews,
            positive,
            claims: reviews * 2,
            raised: per_subject(complaining),
            praised: vec![0; SHEET.len()],
            criticised: per_subject(complaining),
            mixed: vec![0; SHEET.len()],
            recommending: vec![0; SHEET.len()],
            claims_about: per_subject(complaining),
            months: Vec::new(),
        }
    }

    fn told_apart(counts: Vec<crate::who::SegmentCount>) -> String {
        let mut report = sample_report("ordinary text");
        let mut given: std::collections::HashMap<String, crate::who::SegmentCount> = counts
            .into_iter()
            .map(|count| (count.id.clone(), count))
            .collect();
        report.apps[0].reading.who = crate::who::SEGMENTS
            .iter()
            .map(|segment| {
                given
                    .remove(segment.id)
                    .unwrap_or_else(|| kind_of_reviewer(segment.id, 0, 0, (&[], 0)))
            })
            .collect();
        render(&report)
    }

    #[test]
    fn a_game_read_before_reviewers_were_told_apart_says_so() {
        let page = render(&sample_report("ordinary text"));
        let section = between(&page, "<h3>Who said it</h3>", "<h3>");
        assert!(
            section.contains("not counted when this game was read"),
            "{section}"
        );
        assert!(!section.contains("<table"));
    }

    #[test]
    fn every_kind_of_reviewer_is_listed_and_one_too_small_carries_no_share() {
        let page = told_apart(vec![
            kind_of_reviewer("100-hours-or-more", 400, 360, (&["content"], 120)),
            kind_of_reviewer("10-to-30-hours", 500, 450, (&["content"], 10)),
            kind_of_reviewer("under-2-hours", 300, 120, (&[], 0)),
            kind_of_reviewer("steam-deck", 12, 12, (&[], 0)),
            kind_of_reviewer("elsewhere", 1_188, 918, (&[], 0)),
            kind_of_reviewer("got-it-free", 150, 105, (&[], 0)),
            kind_of_reviewer("paid-for-it", 1_050, 735, (&[], 0)),
        ]);
        let table = between(&page, "<table class=\"who\">", "</table>");
        for split in crate::who::SPLITS {
            assert!(
                table.contains(split.label),
                "{} is not in {table}",
                split.label
            );
        }
        let deck = between(
            table,
            "<tr class=\"too-few\"><th scope=\"row\">Mostly on a Steam Deck",
            "</tr>",
        );
        assert!(
            !deck.contains('%'),
            "twelve reviews are given a share: {deck}"
        );
        assert!(deck.contains("too few to say"), "{deck}");
        let elsewhere = between(table, "Mostly elsewhere</th>", "</tr>");
        assert!(
            elsewhere.contains("too few others to set it against") && !elsewhere.contains('%'),
            "{elsewhere}"
        );
        let newcomers = between(table, "Under 2 hours</th>", "</tr>");
        assert!(
            newcomers.contains("verdict-share colder") && newcomers.contains("40.0%"),
            "{newcomers}"
        );
        let veterans = between(table, "100 hours or more</th>", "</tr>");
        assert!(
            veterans.contains("verdict-share warmer\">90.0%"),
            "{veterans}"
        );
        let free = between(table, "Got it free</th>", "</tr>");
        assert!(
            free.contains("verdict-share\">70.0%"),
            "a share like everyone else's is not marked: {free}"
        );
        assert!(
            between(&page, "100 hours or more</th>", "</tr>").contains("400"),
            "the reviews are counted"
        );

        let differ = between(&page, "<ul class=\"differ\">", "</ul>");
        assert!(
            differ.contains(
                "<li>Reviewers with 100 hours or more played complain about amount of content \
                 in 30.0% of their reviews, against 1.2% of everyone else.</li>"
            ),
            "{differ}"
        );
        assert!(differ.contains("Reviewers with under 2 hours played recommend the game"));
        assert!(!page.contains("widest beyond chance of"));
        let note = between(&page, "<h3>Who said it</h3>", "<div class=\"scroll fits\">");
        assert!(
            note.contains(
                "4 times the gap chance alone typically makes and at least 2 points wide"
            ) && note.contains("fewer than 100 reviews is not compared"),
            "{note}"
        );
    }

    #[test]
    fn as_many_differences_as_are_shown_are_shown_without_a_count_of_the_rest() {
        let ten: Vec<&str> = SHEET
            .iter()
            .skip(6)
            .take(10)
            .map(|category| category.id)
            .collect();
        let page = told_apart(vec![
            kind_of_reviewer("100-hours-or-more", 1_000, 900, (&ten, 500)),
            kind_of_reviewer("10-to-30-hours", 1_000, 900, (&[], 0)),
        ]);
        let differ = between(&page, "<ul class=\"differ\">", "</ul>");
        assert_eq!(differ.matches("<li>").count(), DIFFERENCES_SHOWN);
        assert!(!page.contains("widest beyond chance of"));
    }

    #[test]
    fn a_game_whose_reviewers_all_say_the_same_says_that() {
        let page = told_apart(vec![
            kind_of_reviewer("100-hours-or-more", 400, 360, (&["content"], 20)),
            kind_of_reviewer("10-to-30-hours", 400, 360, (&["content"], 20)),
        ]);
        let section = between(&page, "<h4>Where they differ</h4>", "<h3>");
        assert!(section.contains("No kind of reviewer praises"), "{section}");
        assert!(!section.contains("<ul"));
    }

    #[test]
    fn a_game_with_more_differences_than_a_reader_takes_in_lists_the_widest() {
        let every: Vec<&str> = SHEET.iter().map(|category| category.id).collect();
        let page = told_apart(vec![
            kind_of_reviewer("100-hours-or-more", 1_000, 900, (&every, 500)),
            kind_of_reviewer("10-to-30-hours", 1_000, 900, (&[], 0)),
        ]);
        let differ = between(&page, "<ul class=\"differ\">", "</ul>");
        assert_eq!(differ.matches("<li>").count(), DIFFERENCES_SHOWN);
        // Every subject but the one that says nothing about the game, from each side.
        let found = 2 * (SHEET.len() - 1);
        assert!(
            page.contains(&format!(
                "The {DIFFERENCES_SHOWN} widest beyond chance of {found}."
            )),
            "{differ}"
        );
    }
}
