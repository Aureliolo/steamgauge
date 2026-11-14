//! Text for the figures a person reads.

/// A count with a comma between each group of three digits.
#[must_use]
pub fn thousands(count: u64) -> String {
    let digits = count.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// A share as a person reads it.
///
/// From 10% to 90% it reads in whole points. Nearer the ends it keeps a tenth, because an
/// interval around a share holds a tenth of a point only there. Above 0 and under 0.1% it
/// reads "<0.1%", and above 99.9% and under 100% it reads ">99.9%", so only 0 and 1 read as
/// "0%" and "100%". A tenth of 0 is left out.
///
/// The share's shortest decimal text is what gets rounded, half up. Rounding the `f64` or a
/// product of it would round a binary neighbour of the value the writer meant.
#[must_use]
pub fn percent(share: f64) -> String {
    written(share, false)
}

/// Both ends of an interval, each as [`percent`] writes it.
///
/// Where the two would read alike, both are written to a tenth. Where they read alike even
/// then, the interval is too narrow to say anything the rate beside it does not, and the answer
/// is `None`.
#[must_use]
pub fn percent_range(low: f64, high: f64) -> Option<(String, String)> {
    [false, true].into_iter().find_map(|tenths| {
        let (first, second) = (written(low, tenths), written(high, tenths));
        (first != second).then_some((first, second))
    })
}

/// The rule behind [`percent`], with the tenth forced where `tenths` is set.
fn written(share: f64, tenths: bool) -> String {
    if share.is_nan() || share <= 0.0 {
        return "0%".to_owned();
    }
    if share >= 1.0 {
        return "100%".to_owned();
    }
    if share < 0.001 {
        return "<0.1%".to_owned();
    }
    if share > 0.999 {
        return ">99.9%".to_owned();
    }
    let text = share.to_string();
    let fraction = text.split_once('.').map_or("", |(_, fraction)| fraction);
    let digit =
        |place: usize| u32::from(fraction.as_bytes().get(place).map_or(0, |byte| byte - b'0'));
    if !tenths && (0.1..=0.9).contains(&share) {
        let points = digit(0) * 10 + digit(1) + u32::from(digit(2) >= 5);
        return format!("{points}%");
    }
    let in_tenths = digit(0) * 100 + digit(1) * 10 + digit(2) + u32::from(digit(3) >= 5);
    if in_tenths.is_multiple_of(10) {
        format!("{}%", in_tenths / 10)
    } else {
        format!("{}.{}%", in_tenths / 10, in_tenths % 10)
    }
}

#[cfg(test)]
mod tests {
    use super::{percent, percent_range, thousands};

    #[test]
    fn a_count_under_a_thousand_has_no_comma() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
    }

    #[test]
    fn a_thousand_has_one_comma() {
        assert_eq!(thousands(1_000), "1,000");
    }

    #[test]
    fn a_million_has_two_commas() {
        assert_eq!(thousands(1_000_000), "1,000,000");
        assert_eq!(thousands(986_295), "986,295");
        assert_eq!(thousands(25_242_117), "25,242,117");
    }

    #[test]
    fn the_largest_count_groups_from_the_right() {
        assert_eq!(thousands(u64::MAX), "18,446,744,073,709,551,615");
    }

    #[test]
    fn a_share_reads_as_the_table_says_at_every_boundary() {
        let table = [
            (0.0, "0%"),
            (0.0001, "<0.1%"),
            (0.001, "0.1%"),
            (0.0045, "0.5%"),
            (0.0125, "1.3%"),
            (0.09, "9%"),
            (0.0995, "10%"),
            (0.1, "10%"),
            (0.105, "11%"),
            (0.1234, "12%"),
            (0.145, "15%"),
            (0.9, "90%"),
            (0.9005, "90.1%"),
            (0.934, "93.4%"),
            (0.999, "99.9%"),
            (0.9995, ">99.9%"),
            (1.0, "100%"),
        ];
        for (share, text) in table {
            assert_eq!(percent(share), text, "{share}");
        }
    }

    #[test]
    fn a_share_outside_the_range_reads_as_its_nearer_end() {
        assert_eq!(percent(-0.2), "0%");
        assert_eq!(percent(f64::NAN), "0%");
        assert_eq!(percent(1.2), "100%");
        assert_eq!(percent(0.5), "50%");
        assert_eq!(percent(0.3), "30%");
        assert_eq!(percent(0.0999), "10%");
        assert_eq!(percent(0.8999), "90%");
        assert_eq!(percent(0.9989), "99.9%");
        assert_eq!(percent(0.9991), ">99.9%");
        assert_eq!(percent(0.000_99), "<0.1%");
        assert_eq!(percent(0.054), "5.4%");
        assert_eq!(percent(0.9501), "95%");
    }

    #[test]
    fn an_interval_keeps_its_tenths_only_where_its_ends_would_read_alike() {
        assert_eq!(
            percent_range(0.27, 0.33),
            Some(("27%".to_owned(), "33%".to_owned()))
        );
        assert_eq!(
            percent_range(0.201, 0.204),
            Some(("20.1%".to_owned(), "20.4%".to_owned()))
        );
        assert_eq!(percent_range(0.199_752, 0.200_248), None);
        assert_eq!(percent_range(0.0001, 0.0002), None);
    }
}
