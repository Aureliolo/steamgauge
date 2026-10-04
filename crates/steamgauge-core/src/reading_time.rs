//! How long reading has taken on this machine's processor, so a choice between sizes can say
//! what each costs here.
//!
//! No published figure can say how fast somebody else's processor is, and "about eight times
//! as long" is not a time anybody can plan around. Every reading on the processor is timed and
//! kept, per size and per language counted, since a reading of English alone reads only the
//! English reviews and takes a share of the time the same capture takes read whole.
//!
//! Kept in the library directory as `reading-times.json`.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::Result;
use crate::reader::Size;

/// The file the times are kept in, in the library directory.
pub const FILE: &str = "reading-times.json";

/// Every reading timed on this machine's processor, summed per size and language.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReadingTimes {
    taken: Vec<Taken>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Taken {
    reader: String,
    /// `None` for a reading of every language.
    language: Option<String>,
    seconds: f64,
    /// Reviews in the captures read, whatever the language: what is known of a game before it
    /// has been read, and so what an estimate has to start from.
    reviews: u64,
}

impl ReadingTimes {
    /// The times kept in `dir`, or none where there is no file or it cannot be read: an
    /// estimate is a courtesy, and a reading must never wait on one.
    #[must_use]
    pub fn load(dir: &Path) -> Self {
        std::fs::read(dir.join(FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Keeps the times in `dir`.
    ///
    /// # Errors
    ///
    /// Fails if the file cannot be written.
    pub fn save(&self, dir: &Path) -> Result<()> {
        std::fs::write(dir.join(FILE), serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    /// Adds one reading: `seconds` spent reading a capture of `reviews` reviews with `size`.
    pub fn note(&mut self, size: &Size, language: Option<&str>, seconds: f64, reviews: u64) {
        if reviews == 0 || !seconds.is_finite() || seconds <= 0.0 {
            return;
        }
        if let Some(taken) = self
            .taken
            .iter_mut()
            .find(|taken| taken.reader == size.name && taken.language.as_deref() == language)
        {
            taken.seconds += seconds;
            taken.reviews += reviews;
        } else {
            self.taken.push(Taken {
                reader: size.name.to_owned(),
                language: language.map(str::to_owned),
                seconds,
                reviews,
            });
        }
    }

    /// About how long `size` would take here over a capture of `reviews` reviews.
    ///
    /// From its own readings where it has any. Otherwise from another size's readings of the
    /// same language, scaled by what the two took on one processor over the same claims, since
    /// a processor twice as fast is twice as fast at both. `None` where nothing has been read
    /// here in that language: the ratio alone says how much longer, never how long.
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        reason = "review counts are far below 2^53"
    )]
    pub fn seconds(&self, size: &Size, language: Option<&str>, reviews: u64) -> Option<f64> {
        let per_review = |taken: &Taken| taken.seconds / taken.reviews as f64;
        if let Some(taken) = self.find(size.name, language) {
            return Some(per_review(taken) * reviews as f64);
        }
        crate::reader::SIZES.iter().find_map(|other| {
            let taken = self.find(other.name, language)?;
            let scale = size.processor_seconds / other.processor_seconds;
            Some(per_review(taken) * scale * reviews as f64)
        })
    }

    fn find(&self, reader: &str, language: Option<&str>) -> Option<&Taken> {
        self.taken
            .iter()
            .find(|taken| taken.reader == reader && taken.language.as_deref() == language)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size(name: &str) -> &'static Size {
        Size::named(name).unwrap()
    }

    #[test]
    fn a_size_read_here_is_timed_from_its_own_readings() {
        let mut times = ReadingTimes::default();
        times.note(size("small"), Some("english"), 60.0, 10_000);
        times.note(size("small"), Some("english"), 90.0, 5_000);

        assert_eq!(
            times.seconds(size("small"), Some("english"), 15_000),
            Some(150.0)
        );
    }

    #[test]
    fn a_reading_of_another_size_or_language_is_kept_apart() {
        let mut times = ReadingTimes::default();
        times.note(size("small"), Some("english"), 60.0, 10_000);
        times.note(size("small"), None, 90.0, 5_000);
        times.note(size("standard"), Some("english"), 300.0, 10_000);

        assert_eq!(
            times.seconds(size("small"), Some("english"), 10_000),
            Some(60.0)
        );
        assert_eq!(times.seconds(size("small"), None, 5_000), Some(90.0));
        assert_eq!(
            times.seconds(size("standard"), Some("english"), 10_000),
            Some(300.0)
        );
    }

    #[test]
    fn a_size_not_yet_read_here_is_scaled_from_one_that_was() {
        let mut times = ReadingTimes::default();
        times.note(size("small"), None, 36.0, 1_000);

        let standard = times.seconds(size("standard"), None, 1_000).unwrap();
        assert!((standard - 284.9).abs() < 1e-9, "{standard}");
    }

    #[test]
    fn nothing_read_in_a_language_says_nothing_about_how_long_it_takes() {
        let mut times = ReadingTimes::default();
        times.note(size("small"), None, 60.0, 10_000);

        assert_eq!(times.seconds(size("small"), Some("english"), 10_000), None);
        assert_eq!(
            ReadingTimes::default().seconds(size("standard"), None, 10_000),
            None
        );
    }

    #[test]
    fn a_reading_that_took_no_time_or_read_nothing_is_not_kept() {
        let mut times = ReadingTimes::default();
        times.note(size("small"), None, 0.0, 10_000);
        times.note(size("small"), None, 60.0, 0);
        times.note(size("small"), None, f64::NAN, 10_000);

        assert_eq!(times, ReadingTimes::default());
    }

    #[test]
    fn the_times_survive_being_kept() {
        let dir = crate::tempdir::Dir::new();
        let mut times = ReadingTimes::default();
        times.note(size("standard"), Some("english"), 90.0, 3_000);
        times.save(dir.path()).unwrap();

        assert_eq!(ReadingTimes::load(dir.path()), times);
        assert_eq!(
            ReadingTimes::load(&dir.path().join("elsewhere")),
            ReadingTimes::default()
        );
    }
}
