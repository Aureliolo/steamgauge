//! The open shared by every file that programs lock, and the lock of a game's folder.

/// Opens a file that programs lock, made where missing and never truncated. It is opened to
/// write, never to append, because Windows refuses to lock a handle opened to append.
pub(crate) fn open(file: &std::path::Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(file)
}

/// A lock on a file, let go when dropped.
///
/// Dropping it lets go even while another handle to the same open file lives on. A program
/// started on macOS or Linux holds a copy of every open handle until it has loaded, and a lock
/// left to the closing of the last handle would outlast this one by that long.
#[derive(Debug)]
pub struct FileLock(std::fs::File);

impl FileLock {
    /// Locks `file` alone, made where missing, or gives `None` while another handle holds a lock
    /// on it, in this program or another.
    ///
    /// # Errors
    ///
    /// Fails where the file cannot be opened or the system cannot lock it.
    pub fn alone(file: &std::path::Path) -> std::io::Result<Option<Self>> {
        let file = open(file)?;
        let taken = file.try_lock();
        Self::taken(file, taken)
    }

    /// Locks `file` alone once no other handle holds a lock on it, asking every quarter second.
    /// The first ask comes before any pause, so a free file costs no wait.
    pub(crate) async fn alone_once_free(file: &std::path::Path) -> std::io::Result<Self> {
        let file = open(file)?;
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(Self(file)),
                Err(std::fs::TryLockError::WouldBlock) => {
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                }
                Err(std::fs::TryLockError::Error(error)) => return Err(error),
            }
        }
    }

    /// Locks `file` shared, made where missing, or gives `None` while another handle holds it
    /// alone, in this program or another.
    ///
    /// # Errors
    ///
    /// Fails where the file cannot be opened or the system cannot lock it.
    pub fn shared(file: &std::path::Path) -> std::io::Result<Option<Self>> {
        let file = open(file)?;
        let taken = file.try_lock_shared();
        Self::taken(file, taken)
    }

    fn taken(
        file: std::fs::File,
        taken: Result<(), std::fs::TryLockError>,
    ) -> std::io::Result<Option<Self>> {
        match taken {
            Ok(()) => Ok(Some(Self(file))),
            Err(std::fs::TryLockError::WouldBlock) => Ok(None),
            Err(std::fs::TryLockError::Error(error)) => Err(error),
        }
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        // Where the release fails, the system still lets go once the last handle closes.
        let _ = self.0.unlock();
    }
}

/// The file a job that writes in a game's folder locks shared meanwhile, its own clean-up
/// included, and a removal the person asks for locks alone. It stays.
pub(crate) const WRITING_LOCK: &str = "writing.lock";

/// The id of the game whose folder `name` is, where `name` is `appid=` and digits.
fn game_id(name: &std::ffi::OsStr) -> Option<&str> {
    let id = name.to_str()?.strip_prefix("appid=")?;
    (!id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit())).then_some(id)
}

/// Holds the lock of the game `path` lies in, shared. Keep it until the last file has its name.
///
/// A path in no game, or in a game whose folder is missing, gives `None` and makes nothing.
///
/// # Errors
///
/// [`crate::Error::Refused`] while the game's files are being removed.
pub(crate) fn writing_in(path: &std::path::Path) -> crate::Result<Option<FileLock>> {
    let Some((game, id)) = path
        .ancestors()
        .find_map(|ancestor| Some((ancestor, game_id(ancestor.file_name()?)?)))
    else {
        return Ok(None);
    };
    if !game.is_dir() {
        return Ok(None);
    }
    let held = FileLock::shared(&game.join(WRITING_LOCK))?.ok_or_else(|| {
        crate::Error::Refused(format!(
            "app {id}: its files are being removed. Try again once that ends."
        ))
    })?;
    Ok(Some(held))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_game_folder_is_appid_and_digits_alone() {
        for (name, id) in [
            ("appid=7", Some("7")),
            ("appid=1245620", Some("1245620")),
            ("appid=", None),
            ("appid=7x", None),
            ("appid=x", None),
            ("snapshot=7", None),
        ] {
            assert_eq!(game_id(std::ffi::OsStr::new(name)), id, "{name}");
        }
    }

    fn refusal(result: crate::Result<impl Sized>) -> String {
        match result {
            Err(crate::Error::Refused(said)) => said,
            Err(other) => panic!("not a refusal: {other}"),
            Ok(_) => panic!("not refused"),
        }
    }

    #[test]
    fn writers_share_a_games_lock_and_a_removal_takes_it_alone() {
        let dir = crate::tempdir::Dir::new();
        std::fs::create_dir_all(dir.path().join("appid=7").join("snapshot=5")).unwrap();
        let record = dir
            .path()
            .join("appid=7")
            .join("snapshot=5")
            .join("readings.parquet");
        let first = writing_in(&record).unwrap();
        let second = writing_in(&record).unwrap();
        assert!(first.is_some() && second.is_some());
        let said = refusal(crate::crawl::lock_for_removal(dir.path(), 7));
        assert!(said.contains("another job is writing"), "{said}");
        drop((first, second));
        let removal = crate::crawl::lock_for_removal(dir.path(), 7).unwrap();
        let said = refusal(writing_in(&dir.path().join("appid=7")));
        assert!(said.contains("being removed"), "{said}");
        drop(removal);
    }

    /// The copy stands for the one a program started at that moment holds until it has loaded.
    #[test]
    fn a_lock_lets_go_when_dropped_while_a_copy_of_its_handle_lives_on() {
        let dir = crate::tempdir::Dir::new();
        let file = dir.path().join("a.lock");
        for take in [FileLock::alone, FileLock::shared] {
            let held = take(&file).unwrap().unwrap();
            let copy = held.0.try_clone().unwrap();
            drop(held);
            assert!(
                FileLock::alone(&file).unwrap().is_some(),
                "the copy kept the lock"
            );
            drop(copy);
        }
    }

    #[tokio::test]
    async fn a_lock_waited_for_lets_go_when_dropped_while_a_copy_of_its_handle_lives_on() {
        let dir = crate::tempdir::Dir::new();
        let file = dir.path().join("a.lock");
        let held = FileLock::alone_once_free(&file).await.unwrap();
        let copy = held.0.try_clone().unwrap();
        drop(held);
        assert!(
            FileLock::alone(&file).unwrap().is_some(),
            "the copy kept the lock"
        );
        drop(copy);
    }

    #[test]
    fn a_path_in_no_game_takes_no_lock() {
        let dir = crate::tempdir::Dir::new();
        for path in ["settings.json", "appid=x/a.json", "appid=9/crawl.json"] {
            assert!(
                writing_in(&dir.path().join(path)).unwrap().is_none(),
                "{path}"
            );
        }
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
