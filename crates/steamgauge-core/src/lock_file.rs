//! The open shared by every file that programs lock.

/// Opens a file that programs lock, made where missing and never truncated. It is opened to
/// write, never to append, because Windows refuses to lock a handle opened to append.
pub(crate) fn open(file: &std::path::Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(file)
}
