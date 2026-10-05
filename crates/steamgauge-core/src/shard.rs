//! Splitting a corpus into date windows small enough to crawl independently.
//!
//! A single cursor walk over a million-review corpus is ten thousand sequential pages that
//! cannot be resumed, parallelised, or trusted to hold together. Valve honours
//! `start_date`/`end_date`, so the corpus is split by *count* rather than by time: windows
//! are halved until each holds few enough reviews. Splitting by equal time spans would not
//! work, because reviews cluster hard around release and sales.

use crate::{Result, api::SteamClient};

/// Steam reviews launched in late 2013, so nothing can predate this. Windows below the
/// first real review collapse to nothing on the first probe and cost one request.
pub const CORPUS_EPOCH: i64 = 1_356_998_400; // 2013-01-01

/// Splitting stops here regardless of count. A window this small holding more than the
/// target means genuinely dense history, not a planning failure.
const MIN_WINDOW_SECS: i64 = 86_400;

pub const DEFAULT_SHARD_TARGET: u64 = 40_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shard {
    pub start_date: i64,
    pub end_date: i64,
    /// Valve's reported count for this window at planning time, used to order work and to
    /// spot windows that came back short.
    pub expected: u64,
}

/// Probes Valve for counts, halving any window holding more than `target` reviews.
///
/// # Errors
///
/// Propagates transport and throttling failures from the count probes.
pub async fn plan(
    client: &SteamClient,
    app_id: u32,
    from: i64,
    to: i64,
    target: u64,
) -> Result<Vec<Shard>> {
    plan_by(
        |window| client.count(app_id, Some(window)),
        from,
        to,
        target,
    )
    .await
}

/// [`plan`] over any count of the reviews in a window, which is what lets it be tested
/// without Valve.
async fn plan_by<F, C>(mut count: C, from: i64, to: i64, target: u64) -> Result<Vec<Shard>>
where
    C: FnMut((i64, i64)) -> F,
    F: Future<Output = Result<u64>>,
{
    let mut shards = Vec::new();
    let mut pending = vec![(from, to)];

    while let Some((start, end)) = pending.pop() {
        if start > end {
            continue;
        }
        let expected = count((start, end)).await?;

        // An empty window yields no shard at all, so sparse history costs one probe rather
        // than a crawl that fetches nothing.
        if expected == 0 {
            continue;
        }
        if expected <= target || end - start <= MIN_WINDOW_SECS {
            shards.push(Shard {
                start_date: start,
                end_date: end,
                expected,
            });
            continue;
        }

        let mid = start + (end - start) / 2;
        pending.push((start, mid));
        pending.push((mid + 1, end));
    }

    shards.sort_by_key(|s| s.start_date);
    Ok(shards)
}

/// Total reviews the plan expects to retrieve.
#[must_use]
pub fn expected_total(shards: &[Shard]) -> u64 {
    shards.iter().map(|s| s.expected).sum()
}

/// Windows must tile the range without overlapping, or reviews would be counted twice.
#[must_use]
pub fn is_disjoint(shards: &[Shard]) -> bool {
    shards
        .windows(2)
        .all(|pair| pair[0].end_date < pair[1].start_date)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shard(start: i64, end: i64, expected: u64) -> Shard {
        Shard {
            start_date: start,
            end_date: end,
            expected,
        }
    }

    #[test]
    fn adjacent_windows_do_not_overlap() {
        assert!(is_disjoint(&[shard(0, 10, 1), shard(11, 20, 1)]));
        assert!(!is_disjoint(&[shard(0, 10, 1), shard(10, 20, 1)]));
    }

    #[test]
    fn splitting_at_mid_leaves_no_gap_and_no_overlap() {
        let (start, end) = (1_000_i64, 2_000_i64);
        let mid = start + (end - start) / 2;
        assert!(is_disjoint(&[shard(start, mid, 1), shard(mid + 1, end, 1)]));
        // Every second in the original range still belongs to exactly one half.
        assert_eq!((mid - start + 1) + (end - (mid + 1) + 1), end - start + 1);
    }

    const DAY: i64 = MIN_WINDOW_SECS;

    /// Plans over reviews posted at `times`, counting them the way Valve would, and says which
    /// windows were asked about.
    fn planned(times: &[i64], from: i64, to: i64, target: u64) -> (Vec<Shard>, Vec<(i64, i64)>) {
        let mut asked = Vec::new();
        let shards = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(plan_by(
                |(start, end)| {
                    asked.push((start, end));
                    let held = times.iter().filter(|&&t| start <= t && t <= end).count();
                    std::future::ready(Ok(held as u64))
                },
                from,
                to,
                target,
            ))
            .unwrap();
        (shards, asked)
    }

    #[test]
    fn a_window_over_the_target_is_halved_until_each_half_holds_few_enough() {
        // Two reviews a day for eight days, and a target of one: halving goes all the way down
        // to single days, which are kept over the target because no smaller window is tried.
        let times: Vec<i64> = (0..8).flat_map(|day| [day * DAY, day * DAY + 5]).collect();
        let (shards, _) = planned(&times, 0, 8 * DAY - 1, 1);
        let days: Vec<Shard> = (0..8)
            .map(|day| shard(day * DAY, (day + 1) * DAY - 1, 2))
            .collect();
        assert_eq!(shards, days);
    }

    #[test]
    fn a_window_holding_exactly_the_target_is_kept_whole() {
        let times = [0, DAY, 2 * DAY, 3 * DAY];
        let (shards, asked) = planned(&times, 0, 4 * DAY, 4);
        assert_eq!(shards, [shard(0, 4 * DAY, 4)]);
        assert_eq!(asked, [(0, 4 * DAY)]);
    }

    #[test]
    fn a_window_of_a_day_is_kept_however_many_it_holds() {
        let (shards, asked) = planned(&[0, 1, 2], 0, DAY, 1);
        assert_eq!(shards, [shard(0, DAY, 3)]);
        assert_eq!(asked.len(), 1);
    }

    #[test]
    fn a_window_just_over_a_day_is_still_halved() {
        let (shards, _) = planned(&[0, DAY + 1], 0, DAY + 1, 1);
        assert_eq!(
            shards,
            [shard(0, DAY / 2, 1), shard(DAY / 2 + 1, DAY + 1, 1)]
        );
    }

    #[test]
    fn an_empty_window_costs_one_probe_and_yields_no_shard() {
        // Everything in the second half: the first half is asked about once and dropped.
        let (shards, asked) = planned(&[5 * DAY, 6 * DAY], 0, 8 * DAY - 1, 1);
        assert_eq!(
            shards,
            [
                shard(4 * DAY, 6 * DAY - 1, 1),
                shard(6 * DAY, 8 * DAY - 1, 1)
            ]
        );
        assert!(asked.contains(&(0, 4 * DAY - 1)));
        assert!(
            !asked
                .iter()
                .any(|&(start, _)| (1..4 * DAY).contains(&start))
        );
    }

    #[test]
    fn a_range_of_one_second_is_a_window_and_a_backward_range_is_none() {
        let (shards, _) = planned(&[7], 7, 7, 10);
        assert_eq!(shards, [shard(7, 7, 1)]);
        let (shards, asked) = planned(&[7], 8, 7, 10);
        assert_eq!(shards, [] as [Shard; 0]);
        assert!(
            asked.is_empty(),
            "nothing lies in a backward range to ask about"
        );
    }

    #[test]
    fn expected_total_sums_the_plan() {
        assert_eq!(
            expected_total(&[shard(0, 1, 40_000), shard(2, 3, 12_345)]),
            52_345
        );
        assert_eq!(expected_total(&[]), 0);
    }
}
