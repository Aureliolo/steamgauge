//! Models loaded on first use and let go once nothing has used them for a while.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, TryLockError},
    time::{Duration, Instant},
};

use super::text;

/// How long a model stays loaded after its last use: long enough that a queue of games, or a
/// person searching one after another, loads it once, and short enough that the gigabytes it
/// holds on the card, which Windows charges to the system's memory as well, come back soon after
/// the work ends.
const IDLE: Duration = Duration::from_secs(120);

/// How often the app looks for a model gone idle.
pub(super) const LOOK_EVERY: Duration = Duration::from_secs(30);

/// One model, loaded when first asked for and kept until it has gone unused for [`IDLE`].
pub(super) struct Kept<T>(Arc<Mutex<Slot<T>>>);

struct Slot<T> {
    /// The model and the path it was loaded from: a reader of another size is another model.
    model: Option<(PathBuf, T)>,
    used: Instant,
}

impl<T> Default for Kept<T> {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(Slot {
            model: None,
            used: Instant::now(),
        })))
    }
}

impl<T> Clone for Kept<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T> Kept<T> {
    /// The model's slot. It waits for whoever is using it.
    pub(super) fn lock(&self) -> Result<Held<'_, T>, String> {
        self.0.lock().map(Held).map_err(text)
    }

    /// The model's slot, or none while somebody else is using it.
    pub(super) fn try_lock(&self) -> Result<Option<Held<'_, T>>, String> {
        match self.0.try_lock() {
            Ok(slot) => Ok(Some(Held(slot))),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Poisoned(poisoned)) => Err(text(poisoned)),
        }
    }

    /// Drops the model if nothing has used it for [`IDLE`]. One in use is never idle, however
    /// long it has been held.
    pub(super) fn let_go_if_idle(&self) {
        if let Ok(mut slot) = self.0.try_lock()
            && slot.used.elapsed() >= IDLE
        {
            slot.model = None;
        }
    }

    /// Drops the model now, waiting for whoever is using it, so its files can be removed.
    pub(super) fn let_go(&self) {
        if let Ok(mut slot) = self.0.lock() {
            slot.model = None;
        }
    }
}

/// A kept model in use. Its idle time starts when this is dropped.
pub(super) struct Held<'a, T>(MutexGuard<'a, Slot<T>>);

impl<T> Drop for Held<'_, T> {
    fn drop(&mut self) {
        self.0.used = Instant::now();
    }
}

impl<T> Held<'_, T> {
    /// The model loaded from `from`. It is loaded now unless it already is.
    pub(super) fn get(
        &mut self,
        from: &Path,
        load: impl FnOnce(&Path) -> steamgauge_core::Result<T>,
    ) -> Result<&mut T, String> {
        if self
            .0
            .model
            .as_ref()
            .is_none_or(|(loaded, _)| loaded != from)
        {
            // The one before goes first, so the two never hold the card at once.
            self.0.model = None;
            self.0.model = Some((from.to_owned(), load(from).map_err(text)?));
        }
        self.0
            .model
            .as_mut()
            .map(|(_, model)| model)
            .ok_or_else(|| "the model did not load".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_model_is_loaded_once_while_it_is_used() {
        let kept: Kept<u32> = Kept::default();
        let mut loads = 0;
        for _ in 0..3 {
            let mut held = kept.lock().unwrap();
            let model = held
                .get(Path::new("a"), |_| {
                    loads += 1;
                    Ok(7)
                })
                .unwrap();
            assert_eq!(*model, 7);
        }
        assert_eq!(loads, 1);
    }

    #[test]
    fn a_model_from_another_path_replaces_the_one_kept() {
        let kept: Kept<&str> = Kept::default();
        let mut held = kept.lock().unwrap();
        assert_eq!(
            *held.get(Path::new("small"), |_| Ok("small")).unwrap(),
            "small"
        );
        assert_eq!(
            *held.get(Path::new("standard"), |_| Ok("standard")).unwrap(),
            "standard"
        );
    }

    #[test]
    fn a_model_in_use_is_not_let_go_and_a_recent_one_is_kept() {
        let kept: Kept<u32> = Kept::default();
        let mut held = kept.lock().unwrap();
        held.get(Path::new("a"), |_| Ok(1)).unwrap();
        kept.let_go_if_idle();
        assert!(held.0.model.is_some());
        drop(held);
        kept.let_go_if_idle();
        assert!(kept.lock().unwrap().0.model.is_some());
    }

    #[test]
    fn a_model_unused_for_the_idle_time_is_let_go() {
        let kept: Kept<u32> = Kept::default();
        kept.lock().unwrap().get(Path::new("a"), |_| Ok(1)).unwrap();
        kept.0.lock().unwrap().used = Instant::now().checked_sub(IDLE).unwrap();
        kept.let_go_if_idle();
        assert!(kept.0.lock().unwrap().model.is_none());
    }

    #[test]
    fn somebody_else_holding_it_is_told_rather_than_waited_for() {
        let kept: Kept<u32> = Kept::default();
        let _held = kept.lock().unwrap();
        assert!(kept.try_lock().unwrap().is_none());
    }
}
