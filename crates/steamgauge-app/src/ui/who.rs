//! Who wrote a read game's reviews: the kinds of reviewer, where one differs from everyone else,
//! and one kind beside another with every subject's figures for both.

use serde::Serialize;
use steamgauge_core::who;
use tauri::AppHandle;

use super::{MonthOut, corrected_share, library_dir, measurement, read_report, text};

/// Who wrote a game's reviews, as its page opens.
#[derive(Debug, Clone, Serialize)]
pub(super) struct Overview {
    /// By the fact that makes them; empty where the reading was counted before reviewers were
    /// told apart.
    kinds: Vec<who::Kinds>,
    /// Clearest first.
    findings: Vec<who::Finding>,
}

pub(super) fn overview(report: &steamgauge_core::read::ReadReport) -> Overview {
    Overview {
        kinds: who::kinds(report),
        findings: who::findings(report),
    }
}

/// One subject for both sides, with the share of points each would have with the reader's
/// measured errors taken out, where this game has labels to measure them by.
#[derive(Debug, Clone, Serialize)]
struct SubjectBeside {
    #[serde(flatten)]
    beside: who::Beside,
    these_corrected: Option<f64>,
    others_corrected: Option<f64>,
}

/// One kind of reviewer beside another, ready for the window.
#[derive(Debug, Clone, Serialize)]
pub(super) struct Beside {
    split: &'static str,
    these: who::Head,
    others: who::Head,
    recommended: who::Gap,
    subjects: Vec<SubjectBeside>,
    /// The first side's months, as the timeline draws them.
    months: Vec<MonthOut>,
}

/// A month of one kind of reviewer, with a share only where the month holds enough reviews to
/// carry one, by the same rule as the whole game's months.
fn month_out(month: &who::SegmentMonth) -> MonthOut {
    let enough = month.reviews >= steamgauge_core::read::Month::ENOUGH_FOR_A_RATE;
    MonthOut {
        label: month.label.clone(),
        name: steamgauge_core::time::month_name(&month.label),
        reviews: month.reviews,
        positive: enough
            .then(|| super::share_of(month.positive, month.reviews))
            .flatten(),
    }
}

/// One kind of reviewer beside `others`, or beside everyone else where none is named.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri hands a command its arguments by value"
)]
pub(super) fn who_wrote(
    app: AppHandle,
    app_id: u32,
    these: String,
    others: Option<String>,
) -> Result<Beside, String> {
    let dir = library_dir(&app);
    let snapshot = steamgauge_core::embed::latest_snapshot(&dir, app_id).map_err(text)?;
    let found = read_report(&snapshot)?;
    let compared = who::compare(&found, &these, others.as_deref()).map_err(text)?;
    let (_, agreement) = measurement(&dir, app_id);
    let subjects = compared
        .subjects
        .into_iter()
        .map(|beside| SubjectBeside {
            these_corrected: corrected_share(
                agreement.as_ref(),
                beside.id,
                beside.these.claims,
                compared.these.claims,
            ),
            others_corrected: corrected_share(
                agreement.as_ref(),
                beside.id,
                beside.others.claims,
                compared.others.claims,
            ),
            beside,
        })
        .collect();
    Ok(Beside {
        split: compared.split,
        months: compared.months.iter().map(month_out).collect(),
        these: compared.these,
        others: compared.others,
        recommended: compared.recommended,
        subjects,
    })
}

/// The ids of every review written by one kind of reviewer, for narrowing the claims behind a
/// figure to the reviews it was counted from.
pub(super) fn written_by(
    snapshot: &std::path::Path,
    kind: &str,
) -> Result<std::collections::HashSet<String>, String> {
    let segment =
        who::segment(kind).ok_or_else(|| format!("no kind of reviewer is called {kind}"))?;
    let ids = steamgauge_core::capture::rows_kept(snapshot, |row, _| {
        segment.holds(&row.reviewer).then_some(row.recommendationid)
    })
    .map_err(text)?;
    Ok(ids.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::month_out;

    #[test]
    fn a_month_of_one_kind_carries_a_share_only_where_it_holds_enough_reviews() {
        let month = |reviews| steamgauge_core::who::SegmentMonth {
            label: "2024-02".to_owned(),
            reviews,
            positive: reviews / 2,
        };
        let enough = steamgauge_core::read::Month::ENOUGH_FOR_A_RATE;
        assert_eq!(month_out(&month(enough - 1)).positive, None);
        let drawn = month_out(&month(enough));
        assert_eq!(drawn.positive, Some(0.5));
        assert_eq!(drawn.reviews, enough);
        assert_eq!(drawn.label, "2024-02");
        assert_eq!(drawn.name, steamgauge_core::time::month_name("2024-02"));
    }
}
