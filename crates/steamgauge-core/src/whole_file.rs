//! A file is written beside its place under a name for its program, flushed, then renamed. A stop
//! leaves the file before or after.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

/// One write at a time in a program, so two writers of one record never share the file beside.
static WRITING: Mutex<()> = Mutex::new(());

/// How long a stopped program's file beside is kept before a later write removes it. A record's
/// bytes are in memory, so no live write takes anything like this long.
const LEFT_FOR: Duration = Duration::from_hours(1);

/// Where `path`'s bytes are written before they are renamed into place: `<path>.<process id>.partial`.
/// No two live programs share a process id, so none shares the file.
#[must_use]
pub fn beside(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(format!(".{}.partial", std::process::id()));
    PathBuf::from(name)
}

/// Writes `bytes` whole, flushed to the disk, and renames them over `path`, so a stop at any point
/// leaves the file as it was or as it is meant to be.
///
/// # Errors
///
/// Fails where the file beside `path` cannot be written or renamed. `path` is then as it was.
/// Fails while the files of the game `path` lies in are being removed.
pub fn write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let _writing = crate::lock_file::writing_in(path).map_err(std::io::Error::other)?;
    let _one_at_a_time = WRITING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let partial = beside(path);
    let mut file = std::fs::File::create(&partial)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    if let Err(error) = std::fs::rename(&partial, path) {
        let _ = std::fs::remove_file(&partial);
        return Err(error);
    }
    remove_left_beside(path);
    Ok(())
}

/// Removes each `<name>.<digits>.partial` of the record that was last written over an hour ago,
/// which a stopped program left and no later program names.
fn remove_left_beside(path: &Path) {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(folder_of(path)) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        if !is_beside_of(file_name, name) {
            continue;
        }
        let old = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .is_ok_and(|modified| now.duration_since(modified).is_ok_and(left_long_enough));
        if old {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Whether a file beside a record, unwritten for `age`, was left by a stopped program.
fn left_long_enough(age: Duration) -> bool {
    age > LEFT_FOR
}

/// The folder `path` sits in. A bare name's parent is empty, which no folder listing takes.
fn folder_of(path: &Path) -> &Path {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    }
}

fn is_beside_of(file_name: &str, name: &str) -> bool {
    file_name
        .strip_prefix(name)
        .and_then(|rest| rest.strip_prefix('.'))
        .and_then(|rest| rest.strip_suffix(".partial"))
        .is_some_and(|id| !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_write_that_cannot_finish_leaves_the_file_before() {
        let dir = crate::tempdir::Dir::new();
        let path = dir.path().join("record.json");
        std::fs::write(&path, b"old").unwrap();
        std::fs::create_dir(beside(&path)).unwrap();
        assert!(write(&path, b"new").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"old");
    }

    #[test]
    fn a_record_in_a_game_is_not_written_while_its_files_are_removed() {
        let dir = crate::tempdir::Dir::new();
        let snapshot = dir.path().join("appid=7").join("snapshot=5");
        std::fs::create_dir_all(&snapshot).unwrap();
        let path = snapshot.join("crawl.json");
        std::fs::write(&path, b"old").unwrap();
        let removal = crate::crawl::lock_for_removal(dir.path(), 7).unwrap();
        let error = write(&path, b"new").unwrap_err();
        assert!(error.to_string().contains("being removed"), "{error}");
        assert_eq!(std::fs::read(&path).unwrap(), b"old");
        assert_eq!(names(&snapshot), ["crawl.json"]);
        drop(removal);
        write(&path, b"new").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
    }

    #[test]
    fn a_write_puts_the_bytes_in_place_and_leaves_nothing_beside() {
        let dir = crate::tempdir::Dir::new();
        let path = dir.path().join("record.json");
        std::fs::write(&path, b"old").unwrap();
        write(&path, b"new").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        assert_eq!(names(dir.path()), ["record.json"]);
    }

    #[test]
    fn a_file_at_the_name_every_program_once_shared_stops_no_write() {
        let dir = crate::tempdir::Dir::new();
        let path = dir.path().join("record.json");
        std::fs::write(&path, b"old").unwrap();
        std::fs::create_dir(dir.path().join("record.json.partial")).unwrap();
        write(&path, b"new").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
    }

    #[test]
    fn a_write_removes_what_stopped_programs_left_beside_its_record() {
        let dir = crate::tempdir::Dir::new();
        let path = dir.path().join("record.json");
        let age = |name: &str, hours: u64| {
            let file = dir.path().join(name);
            std::fs::write(&file, b"cut").unwrap();
            let modified = SystemTime::now() - Duration::from_secs(hours * 60 * 60);
            std::fs::File::options()
                .write(true)
                .open(&file)
                .unwrap()
                .set_modified(modified)
                .unwrap();
        };
        age("record.json.1.partial", 2);
        age("record.json.partial", 2);
        age("other.json.3.partial", 2);
        age("record.json..partial", 2);
        age("record.json.copy.partial", 2);
        age("record.json.2.partial", 0);
        write(&path, b"new").unwrap();
        assert_eq!(
            names(dir.path()),
            [
                "other.json.3.partial",
                "record.json",
                "record.json..partial",
                "record.json.2.partial",
                "record.json.copy.partial",
                "record.json.partial"
            ]
        );
    }

    #[test]
    fn a_file_beside_is_taken_as_left_only_past_the_hour() {
        assert!(!left_long_enough(LEFT_FOR));
        assert!(left_long_enough(LEFT_FOR + Duration::from_nanos(1)));
    }

    #[test]
    fn a_bare_name_is_in_the_working_folder() {
        assert_eq!(folder_of(Path::new("record.json")), Path::new("."));
        assert_eq!(
            folder_of(Path::new("library/record.json")),
            Path::new("library")
        );
    }

    #[test]
    fn a_write_waits_while_another_write_of_the_program_is_in_place() {
        let dir = crate::tempdir::Dir::new();
        let path = dir.path().join("record.json");
        std::fs::write(&path, b"old").unwrap();
        let held = WRITING.lock().unwrap();
        let writer = {
            let path = path.clone();
            std::thread::spawn(move || write(&path, b"new"))
        };
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(std::fs::read(&path).unwrap(), b"old");
        drop(held);
        writer.join().unwrap().unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
    }
}
