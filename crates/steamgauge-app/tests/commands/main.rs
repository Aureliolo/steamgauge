//! Each command started as the program, on a tiny library built by code, and held to the files
//! it writes and the lines it prints. An in-process test cannot read a line back from a pipe,
//! and cannot see the parser, the dispatch or the exit code.
//!
//! A call of `ran`, `ran_with` or `started` names its command as the first literal of the
//! argument slice, so a test over the list of tested commands can read these files for it.

mod answers;
mod draws;
mod library;
mod outside;
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

/// The commands a test in this folder starts.
const TESTED: &[&str] = &[
    "brief",
    "ceiling",
    "clients",
    "compare-labels",
    "connect",
    "crawl",
    "declined",
    "disconnect",
    "export-data",
    "export-pool",
    "export-review-facts",
    "export-training",
    "gold",
    "ingest-claims",
    "ingest-gold",
    "ingest-induced",
    "ingest-revisit",
    "mcp",
    "measure-claims",
    "rare-claims",
    "read",
    "recount",
    "report",
    "revisit",
    "sample-claims",
    "second-opinion",
    "store-facts",
    "sweep",
    "update",
];

/// The commands no test starts, each with why.
const UNTESTED: &[(&str, &str)] = &[
    (
        "embed",
        "its encoder is a pinned download checked by hash, and the core tests `embed_corpus` on a tiny graph",
    ),
    (
        "distinct",
        "it reads the vectors only `embed` writes, and the core tests `diverse::handout`",
    ),
];

/// The first literal of the argument slice of every `ran(`, `ran_with(` and `started(` call in
/// `source`, which has no whitespace.
fn started_commands(source: &str) -> Vec<&str> {
    let mut found = Vec::new();
    for call in ["ran(", "ran_with(", "started("] {
        for (at, _) in source.match_indices(call) {
            let rest = &source[at + call.len()..];
            let Some(slice) = rest.find(",&[\"") else {
                continue;
            };
            let name = &rest[slice + 4..];
            if let Some(end) = name.find('"') {
                found.push(&name[..end]);
            }
        }
    }
    found
}

#[test]
fn every_command_has_a_test_or_a_reason() {
    let names = std::thread::Builder::new()
        .stack_size(steamgauge_app::CLI_STACK)
        .spawn(|| {
            steamgauge_app::reference::command()
                .get_subcommands()
                .map(|command| command.get_name().to_owned())
                .collect::<std::collections::BTreeSet<_>>()
        })
        .unwrap()
        .join()
        .unwrap();
    let untested: Vec<&str> = UNTESTED.iter().map(|(name, _)| *name).collect();
    assert_eq!(untested, ["embed", "distinct"]);
    assert!(
        UNTESTED.iter().all(|(_, why)| !why.is_empty()),
        "an untested command needs its reason"
    );
    let held: std::collections::BTreeSet<String> = TESTED
        .iter()
        .chain(&untested)
        .map(|name| (*name).to_owned())
        .collect();
    assert_eq!(held.len(), TESTED.len() + untested.len(), "in both lists");
    assert_eq!(
        names, held,
        "the parser's commands against TESTED and UNTESTED"
    );

    let source: String = [
        include_str!("library.rs"),
        include_str!("draws.rs"),
        include_str!("answers.rs"),
        include_str!("outside.rs"),
    ]
    .concat()
    .split_whitespace()
    .collect();
    let started = started_commands(&source);
    for name in TESTED {
        assert!(
            started.contains(name),
            "`{name}` is in TESTED and no test starts it"
        );
    }
}
