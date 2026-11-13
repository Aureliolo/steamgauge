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

#[cfg(test)]
mod tests {
    use super::thousands;

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
}
