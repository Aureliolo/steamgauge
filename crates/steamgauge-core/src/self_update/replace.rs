//! Putting a verified release in place of the running copy, on the systems where that is done
//! by moving files: an app bundle on macOS, the portable archive's folder on Linux.
//!
//! Everything is unpacked first into a folder on the same file system as what it replaces, so
//! the replacing itself is a rename, which never leaves a file half written. The archive is
//! read from the handle that was hashed, never opened again by its path. Its entries' extended
//! attributes are not applied, so nothing unpacked carries a quarantine flag.

use std::{
    fs,
    io::{self, Read},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};

/// A folder holding what was unpacked, removed with everything in it when dropped.
#[derive(Debug)]
pub struct Staging {
    root: PathBuf,
}

impl Staging {
    /// A new, empty folder inside `beside`.
    fn new(beside: &Path) -> io::Result<Self> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        let root = beside.join(format!(".steamgauge-update-{}-{nanos}", std::process::id()));
        fs::create_dir(&root)?;
        Ok(Self { root })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.root
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Unpacks the gzipped tar `archive` into a new folder inside `beside`.
fn unpack(archive: impl Read, beside: &Path) -> io::Result<Staging> {
    let staging = Staging::new(beside)?;
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(archive));
    tar.set_unpack_xattrs(false);
    tar.set_preserve_permissions(false);
    tar.unpack(staging.path())?;
    Ok(staging)
}

fn invalid(what: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what)
}

/// The names directly inside `dir`, sorted.
fn names(dir: &Path) -> io::Result<Vec<String>> {
    let mut names = fs::read_dir(dir)?
        .map(|entry| Ok(entry?.file_name().to_string_lossy().into_owned()))
        .collect::<io::Result<Vec<_>>>()?;
    names.sort();
    Ok(names)
}

/// A folder on the same file system as `target`, where a rename into `target` can be made:
/// the system's temporary folder where it is on that file system, else the folder holding
/// `target`.
fn same_volume(target: &Path) -> io::Result<PathBuf> {
    let device = fs::symlink_metadata(target)?.dev();
    [Some(std::env::temp_dir()), target.parent().map(Path::to_path_buf)]
        .into_iter()
        .flatten()
        .find(|dir| fs::metadata(dir).is_ok_and(|found| found.dev() == device))
        .ok_or_else(|| {
            io::Error::other(format!(
                "no folder on the same disk as {} to unpack the update into",
                target.display()
            ))
        })
}

/// The new app, unpacked and ready to take the place of `bundle`.
#[derive(Debug)]
pub struct StagedApp {
    staging: Staging,
    app: PathBuf,
}

impl StagedApp {
    /// The new bundle, until [`swap`] puts it in place; the previous one is here after.
    #[must_use]
    pub fn app(&self) -> &Path {
        &self.app
    }

    /// Where the previous bundle is moved where the two cannot be exchanged in one step.
    #[must_use]
    pub fn backup(&self) -> PathBuf {
        self.staging.path().join("previous.app")
    }
}

/// Unpacks `archive`, which holds one app bundle named [`super::install::APP`] and nothing
/// else, next to `bundle`, and makes it openable by every account as an installed app is.
///
/// # Errors
///
/// Fails on an archive holding anything else, or a bundle without its program, and on the
/// failures of unpacking.
pub fn stage_app(archive: impl Read, bundle: &Path) -> io::Result<StagedApp> {
    let staging = unpack(archive, &same_volume(bundle)?)?;
    let found = names(staging.path())?;
    if found != [super::install::APP] {
        return Err(invalid(format!(
            "the archive holds {found:?} rather than {} alone",
            super::install::APP
        )));
    }
    let app = staging.path().join(super::install::APP);
    if !app.join("Contents").join("MacOS").join("steamgauge").is_file() {
        return Err(invalid(format!(
            "{} in the archive has no program",
            super::install::APP
        )));
    }
    fs::set_permissions(&app, fs::Permissions::from_mode(0o755))?;
    Ok(StagedApp { staging, app })
}

/// Puts the staged app in place of `bundle`. The two are exchanged in one step where the file
/// system can, so `bundle` is never missing; elsewhere `bundle` is moved aside, the new one
/// moved in, and the old one put back if that fails. Either way the previous app is left in the
/// staging folder, which is removed with it.
///
/// # Errors
///
/// A permission error means this account cannot write beside `bundle`, and
/// [`super::install::privileged_swap`] is what does it then. Anything else leaves `bundle` as
/// it was.
pub fn swap(staged: &StagedApp, bundle: &Path) -> io::Result<()> {
    match exchange(staged.app(), bundle) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => return Err(error),
        Err(_) => replace(staged.app(), bundle, &staged.backup())?,
    }
    touch(bundle);
    Ok(())
}

/// Swaps two paths in one step: `renamex_np` with `RENAME_SWAP` on macOS, `renameat2` with
/// `RENAME_EXCHANGE` on Linux.
fn exchange(one: &Path, other: &Path) -> io::Result<()> {
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        one,
        rustix::fs::CWD,
        other,
        rustix::fs::RenameFlags::EXCHANGE,
    )
    .map_err(io::Error::from)
}

/// Moves `target` to `backup` and `staged` to `target`, moving `backup` back if the second
/// move fails, so `target` ends as the new one or as it was.
fn replace(staged: &Path, target: &Path, backup: &Path) -> io::Result<()> {
    fs::rename(target, backup)?;
    if let Err(error) = fs::rename(staged, target) {
        let _ = fs::rename(backup, target);
        return Err(error);
    }
    Ok(())
}

/// Marks the bundle changed, which is how Launch Services and the Finder notice its new
/// version. Nothing depends on it succeeding.
fn touch(bundle: &Path) {
    if let Ok(folder) = fs::File::open(bundle) {
        let _ = folder.set_modified(std::time::SystemTime::now());
    }
}

/// Replaces the files in `dir` with those of the Linux archive `archive`, whose files sit in
/// the one folder `top`. Each file is renamed over its old self, the program last, so a copy
/// stopped half way still starts. A file of the old copy the new one no longer has stays.
///
/// # Errors
///
/// Fails on an archive with anything other than `top` holding plain files, and on the failures
/// of unpacking and renaming.
pub fn replace_files(archive: impl Read, top: &str, dir: &Path) -> io::Result<()> {
    let staging = unpack(archive, dir)?;
    let found = names(staging.path())?;
    if found != [top] {
        return Err(invalid(format!(
            "the archive holds {found:?} rather than {top} alone"
        )));
    }
    let unpacked = staging.path().join(top);
    let mut files = names(&unpacked)?;
    for name in &files {
        if !fs::symlink_metadata(unpacked.join(name))?.is_file() {
            return Err(invalid(format!("{name} in the archive is not a file")));
        }
    }
    if !files.iter().any(|name| name == "steamgauge") {
        return Err(invalid(format!("{top} in the archive has no program")));
    }
    files.sort_by_key(|name| name == "steamgauge");
    for name in &files {
        fs::rename(unpacked.join(name), dir.join(name))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tempdir::Dir;

    /// A gzipped tar of `entries`, each a path and its contents, and a mode.
    fn archive(entries: &[(&str, &[u8], u32)]) -> Vec<u8> {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::fast(),
        ));
        for (path, contents, mode) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(contents.len() as u64);
            header.set_mode(*mode);
            header.set_entry_type(tar::EntryType::Regular);
            builder.append_data(&mut header, path, *contents).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    fn app_archive(program: &[u8]) -> Vec<u8> {
        archive(&[
            ("SteamGauge.app/Contents/Info.plist", b"<plist/>", 0o644),
            ("SteamGauge.app/Contents/MacOS/steamgauge", program, 0o755),
        ])
    }

    fn installed_app(dir: &Path, program: &[u8]) -> PathBuf {
        let bundle = dir.join("SteamGauge.app");
        fs::create_dir_all(bundle.join("Contents").join("MacOS")).unwrap();
        fs::write(bundle.join("Contents").join("MacOS").join("steamgauge"), program).unwrap();
        fs::write(bundle.join("Contents").join("old-only"), b"old").unwrap();
        bundle
    }

    fn program_of(bundle: &Path) -> Vec<u8> {
        fs::read(bundle.join("Contents").join("MacOS").join("steamgauge")).unwrap()
    }

    #[test]
    fn the_new_app_takes_the_old_one_s_place_whole() {
        let dir = Dir::new();
        let bundle = installed_app(dir.path(), b"old");
        let staged = stage_app(app_archive(b"new").as_slice(), &bundle).unwrap();
        swap(&staged, &bundle).unwrap();
        assert_eq!(program_of(&bundle), b"new");
        assert!(!bundle.join("Contents").join("old-only").exists(), "nothing of the old app stays");
        let mode = fs::metadata(&bundle).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o755, "every account can open it");
        let program_mode = fs::metadata(bundle.join("Contents").join("MacOS").join("steamgauge"))
            .unwrap()
            .permissions()
            .mode();
        assert_ne!(program_mode & 0o111, 0, "the program can still be run");
        let staging = staged.staging.path().to_path_buf();
        drop(staged);
        assert!(!staging.exists(), "the previous app goes with the staging folder");
        assert_eq!(names(dir.path()).unwrap(), ["SteamGauge.app"]);
    }

    #[test]
    fn where_the_two_cannot_be_exchanged_the_old_one_is_moved_aside_and_the_new_one_in() {
        let dir = Dir::new();
        let bundle = installed_app(dir.path(), b"old");
        let staged = stage_app(app_archive(b"new").as_slice(), &bundle).unwrap();
        replace(staged.app(), &bundle, &staged.backup()).unwrap();
        assert_eq!(program_of(&bundle), b"new");
        assert_eq!(program_of(&staged.backup()), b"old");
    }

    #[test]
    fn a_move_that_fails_puts_the_old_app_back() {
        let dir = Dir::new();
        let bundle = installed_app(dir.path(), b"old");
        let missing = dir.path().join("never-staged.app");
        let backup = dir.path().join("previous.app");
        assert!(replace(&missing, &bundle, &backup).is_err());
        assert_eq!(program_of(&bundle), b"old");
        assert!(!backup.exists());
    }

    #[test]
    fn an_archive_that_is_not_the_app_alone_is_refused_and_leaves_nothing() {
        let dir = Dir::new();
        let bundle = installed_app(dir.path(), b"old");
        for refused in [
            archive(&[
                ("SteamGauge.app/Contents/MacOS/steamgauge", b"new", 0o755),
                ("Other.app/Contents/MacOS/other", b"x", 0o755),
            ]),
            archive(&[("Other.app/Contents/MacOS/steamgauge", b"new", 0o755)]),
            archive(&[("SteamGauge.app/Contents/Info.plist", b"<plist/>", 0o644)]),
            b"not an archive".to_vec(),
        ] {
            assert!(stage_app(refused.as_slice(), &bundle).is_err());
        }
        assert_eq!(program_of(&bundle), b"old");
        let left: Vec<String> = names(dir.path()).unwrap();
        assert_eq!(left, ["SteamGauge.app"], "no staging folder is left beside the app");
    }

    #[test]
    fn a_missing_bundle_has_no_volume_to_stage_on() {
        let dir = Dir::new();
        assert!(stage_app(app_archive(b"new").as_slice(), &dir.path().join("Gone.app")).is_err());
    }

    fn linux_archive(top: &str, program: &[u8]) -> Vec<u8> {
        archive(&[
            (&format!("{top}/steamgauge"), program, 0o755),
            (&format!("{top}/README.md"), b"new readme", 0o644),
            (&format!("{top}/LICENSE"), b"licence", 0o644),
        ])
    }

    #[test]
    fn the_archive_s_files_replace_the_old_ones_in_their_folder() {
        let dir = Dir::new();
        fs::write(dir.path().join("steamgauge"), b"old").unwrap();
        fs::write(dir.path().join("README.md"), b"old readme").unwrap();
        fs::write(dir.path().join("notes.txt"), b"mine").unwrap();
        replace_files(
            linux_archive("steamgauge-0.2.0-x86_64-linux-gnu", b"new").as_slice(),
            "steamgauge-0.2.0-x86_64-linux-gnu",
            dir.path(),
        )
        .unwrap();
        assert_eq!(fs::read(dir.path().join("steamgauge")).unwrap(), b"new");
        assert_eq!(fs::read(dir.path().join("README.md")).unwrap(), b"new readme");
        assert_eq!(fs::read(dir.path().join("LICENSE")).unwrap(), b"licence");
        assert_eq!(fs::read(dir.path().join("notes.txt")).unwrap(), b"mine");
        let mode = fs::metadata(dir.path().join("steamgauge")).unwrap().permissions().mode();
        assert_ne!(mode & 0o111, 0, "the program can still be run");
        assert_eq!(
            names(dir.path()).unwrap(),
            ["LICENSE", "README.md", "notes.txt", "steamgauge"],
            "the staging folder is gone"
        );
    }

    #[test]
    fn an_archive_for_another_folder_or_without_the_program_replaces_nothing() {
        let dir = Dir::new();
        fs::write(dir.path().join("steamgauge"), b"old").unwrap();
        for (refused, top) in [
            (linux_archive("steamgauge-0.1.0-x86_64-linux-gnu", b"new"), "steamgauge-0.2.0-x86_64-linux-gnu"),
            (
                archive(&[("top/README.md", b"readme", 0o644)]),
                "top",
            ),
            (
                archive(&[("top/steamgauge", b"new", 0o755), ("top/lib/inner", b"x", 0o644)]),
                "top",
            ),
        ] {
            assert!(replace_files(refused.as_slice(), top, dir.path()).is_err());
            assert_eq!(fs::read(dir.path().join("steamgauge")).unwrap(), b"old");
        }
        assert_eq!(names(dir.path()).unwrap(), ["steamgauge"]);
    }

    #[test]
    fn the_program_is_the_last_file_replaced() {
        let dir = Dir::new();
        // A folder full of something where the program goes, which no file can be renamed over.
        fs::create_dir_all(dir.path().join("steamgauge").join("in-the-way")).unwrap();
        let refused = replace_files(
            archive(&[
                ("top/steamgauge", b"new", 0o755),
                ("top/x-after-the-program.txt", b"new", 0o644),
            ])
            .as_slice(),
            "top",
            dir.path(),
        );
        assert!(refused.is_err());
        assert_eq!(
            fs::read(dir.path().join("x-after-the-program.txt")).unwrap(),
            b"new",
            "every other file was in place before the program was tried"
        );
    }
}
