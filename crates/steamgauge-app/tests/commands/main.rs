//! Each command started as the program, on a tiny library built by code, and held to the files
//! it writes and the lines it prints. An in-process test cannot read a line back from a pipe,
//! and cannot see the parser, the dispatch or the exit code.
//!
//! A call of `ran`, `ran_with` or `started` names its command as the first literal of the
//! argument slice, so a test over the list of tested commands can read these files for it.

mod draws;
mod library;
mod tiny;

#[cfg(windows)]
use std::os::windows::process::CommandExt as _;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// How long a started program may run before the test fails.
const LIMIT: Duration = Duration::from_secs(60);

/// What a finished program left: its exit code and both streams.
#[derive(Debug)]
struct Ran {
    code: i32,
    out: String,
    err: String,
}

/// Starts the program in `folder` with all three streams piped. The seam and data variables a
/// developer's shell may hold are removed, so a test never reaches a stand-in or a library it
/// did not build.
fn started(folder: &Path, args: &[&str], environment: &[(&str, &str)]) -> Child {
    let mut command = Command::new(env!("CARGO_BIN_EXE_steamgauge"));
    command
        .current_dir(folder)
        .args(args)
        .env_remove("STEAMGAUGE_STAND_IN")
        .env_remove("STEAMGAUGE_DATA")
        .envs(environment.iter().copied())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // CREATE_NO_WINDOW: a console program started from a test opens no window.
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);
    command.spawn().unwrap()
}

/// Reads a stream to its end on a thread of its own, so a full pipe never stalls the program.
fn drained<R: std::io::Read + Send + 'static>(mut stream: R) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stream.read_to_end(&mut bytes);
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

/// `started`, its input closed, ended and failed if it still runs after a minute.
fn ran_with(folder: &Path, args: &[&str], environment: &[(&str, &str)]) -> Ran {
    let mut child = started(folder, args, environment);
    drop(child.stdin.take());
    let out = drained(child.stdout.take().unwrap());
    let err = drained(child.stderr.take().unwrap());
    let begun = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if begun.elapsed() > LIMIT {
            let _ = child.kill();
            let _ = child.wait();
            panic!("`steamgauge {args:?}` ran for more than {LIMIT:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Ran {
        code: status.code().unwrap_or(-1),
        out: out.join().unwrap(),
        err: err.join().unwrap(),
    }
}

/// The program started as a person starts it, with `args` as its command line. The program with
/// no argument opens the window, so an empty list is refused.
fn ran(folder: &Path, args: &[&str]) -> Ran {
    assert!(!args.is_empty(), "no argument would open the window");
    ran_with(folder, args, &[])
}
