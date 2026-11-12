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
pub(crate) fn writing_in(path: &std::path::Path) -> crate::Result<Option<std::fs::File>> {
    let Some((game, id)) = path
        .ancestors()
        .find_map(|ancestor| Some((ancestor, game_id(ancestor.file_name()?)?)))
    else {
        return Ok(None);
    };
    if !game.is_dir() {
        return Ok(None);
    }
    let file = open(&game.join(WRITING_LOCK))?;
    match file.try_lock_shared() {
        Ok(()) => Ok(Some(file)),
        Err(std::fs::TryLockError::WouldBlock) => Err(crate::Error::Refused(format!(
            "app {id}: its files are being removed. Try again once that ends."
        ))),
        Err(std::fs::TryLockError::Error(error)) => Err(error.into()),
    }
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
