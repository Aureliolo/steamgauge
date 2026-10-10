//! Claude Code, or any other program that speaks the Model Context Protocol, steering
//! `SteamGauge`.
//!
//! The open app owns the library, the work board and the window, so it is the server: it listens
//! on a local socket only this user can reach, and every tool there is something the window can
//! do. `steamgauge mcp` is what a client starts. It joins its standard input and output to that
//! socket, and where the app is not running it opens it without its window first.

use std::{
    io,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use interprocess::local_socket::{Name, tokio::Stream, traits::tokio::Stream as _};
use tokio::io::AsyncWriteExt;

use crate::update::IDENTIFIER;

/// The argument the app is opened with when a client asked for it and nobody else did: it runs
/// with its window hidden, and closes once the last client has gone and its work is done.
pub const WITHOUT_WINDOW: &str = "--without-window";

/// How long `steamgauge mcp` waits for an app it opened to start listening.
const START_WAIT: Duration = Duration::from_secs(60);

/// The app's local data folder, the one Tauri names from the same identifier.
pub fn local_data() -> Result<PathBuf> {
    Ok(dirs::data_local_dir()
        .context("this system names no folder for the app's local data")?
        .join(IDENTIFIER))
}

/// Where the open app writes the name of its socket. The folder is the user's own, so the name
/// is as private as the folder: a program another user runs can neither read it to reach the
/// app nor take it first to stand in for the app.
pub fn rendezvous(local: &Path) -> PathBuf {
    local.join("mcp-socket")
}

/// The name a socket is reached by, from what the rendezvous holds: a pipe's name on Windows, a
/// file's path elsewhere.
pub fn socket_name(written: &str) -> io::Result<Name<'static>> {
    #[cfg(windows)]
    {
        use interprocess::local_socket::{GenericNamespaced, ToNsName};
        written.to_owned().to_ns_name::<GenericNamespaced>()
    }
    #[cfg(not(windows))]
    {
        use interprocess::local_socket::{GenericFilePath, ToFsName};
        PathBuf::from(written).to_fs_name::<GenericFilePath>()
    }
}

async fn connect(local: &Path) -> io::Result<Stream> {
    let written = std::fs::read_to_string(rendezvous(local))?;
    Stream::connect(socket_name(written.trim())?).await
}

/// Opens the app without its window, apart from whatever started this command, so a client that
/// closes takes neither the app nor the work it was asked to do down with it.
fn open_app() -> Result<()> {
    let mut command = std::process::Command::new(std::env::current_exe()?);
    command
        .arg(WITHOUT_WINDOW)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        let apart = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
        // A job that may not be left refuses the breakaway; the app then stays in it, and lives
        // as long as the job does.
        if command
            .creation_flags(apart | CREATE_BREAKAWAY_FROM_JOB)
            .spawn()
            .is_ok()
        {
            return Ok(());
        }
        command.creation_flags(apart);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command.spawn().context("could not open SteamGauge")?;
    Ok(())
}

async fn reach_app(local: &Path) -> Result<Stream> {
    if let Ok(stream) = connect(local).await {
        return Ok(stream);
    }
    open_app()?;
    let started = Instant::now();
    loop {
        tokio::time::sleep(Duration::from_millis(200)).await;
        match connect(local).await {
            Ok(stream) => return Ok(stream),
            Err(error) if started.elapsed() >= START_WAIT => {
                return Err(error).context(format!(
                    "SteamGauge did not start listening within {} seconds",
                    START_WAIT.as_secs()
                ));
            }
            Err(_) => {}
        }
    }
}

/// `steamgauge mcp`: the client's messages to the app and the app's answers back, until either
/// side closes.
pub async fn bridge() -> Result<()> {
    let local = local_data()?;
    let stream = reach_app(&local).await?;
    let (mut from_app, mut to_app) = tokio::io::split(stream);
    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let asking = async {
        tokio::io::copy(&mut stdin, &mut to_app).await?;
        to_app.shutdown().await
    };
    let answering = async {
        tokio::io::copy(&mut from_app, &mut stdout).await?;
        stdout.flush().await
    };
    tokio::pin!(asking, answering);
    tokio::select! {
        asked = &mut asking => {
            asked.context("could not pass the client's messages on")?;
            // The client has said all it will; what the app still owes it is let through.
            tokio::time::timeout(Duration::from_secs(10), answering).await.ok();
            Ok(())
        }
        answered = &mut answering => {
            match answered {
                Ok(()) => eprintln!("SteamGauge closed"),
                Err(error) => eprintln!("could not pass SteamGauge's answers on: {error}"),
            }
            // Standard input is read on a thread of its own that only a message or the client
            // closing wakes, and returning would wait for it.
            std::process::exit(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rendezvous_is_in_the_apps_own_folder() {
        let local = Path::new("local").join(IDENTIFIER);
        assert_eq!(rendezvous(&local), local.join("mcp-socket"));
    }

    #[test]
    fn a_written_name_reaches_a_socket() {
        #[cfg(windows)]
        assert!(socket_name("steamgauge-mcp-0123456789abcdef").is_ok());
        #[cfg(not(windows))]
        assert!(socket_name("/tmp/steamgauge-mcp/socket").is_ok());
    }
}
