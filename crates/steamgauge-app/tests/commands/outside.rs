//! The commands that reach outside the program: Steam, the release page, an MCP client's
//! settings and a client's pipe. Each is started against a stand-in, so no test reaches the
//! network or writes a person's own settings.
//!
//! The first four are built in debug only: a release build holds no read of the stand-in's
//! environment variable and would ask Steam and GitHub themselves.

use std::path::Path;

use serde_json::{Value, json};
use steamgauge_stand_ins::tempdir::Dir;

use crate::{ran_with, started};

#[cfg(debug_assertions)]
use steamgauge_stand_ins::{
    stand_in::{Answer, Asked, Server},
    steam::{edits, valve},
};

#[cfg(debug_assertions)]
use crate::tiny::{CAPTURED, as_steam_serves, captured};

fn text(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn has_line(out: &str, line: &str) -> bool {
    out.lines().any(|found| found == line)
}

#[cfg(debug_assertions)]
fn has_fields(out: &str, fields: &[&str]) -> bool {
    out.lines()
        .any(|line| line.split_whitespace().eq(fields.iter().copied()))
}

/// The ids of every review a capture holds, in id order.
#[cfg(debug_assertions)]
fn ids_in(snapshot: &Path) -> Vec<String> {
    let mut ids = Vec::new();
    steamgauge_core::capture::for_each_row(snapshot, |row, _| {
        ids.push(row.recommendationid);
        Ok(())
    })
    .unwrap();
    ids.sort();
    ids
}

#[cfg(debug_assertions)]
#[test]
fn crawl_downloads_what_the_stand_in_serves() {
    let dir = Dir::new();
    let steam = Server::new(valve(5, 100, as_steam_serves(5), None, 0));
    let library = dir.path().join("library");
    let done = ran_with(
        dir.path(),
        &["crawl", "5", "--out", text(&library), "--pace-ms", "0"],
        &[("STEAMGAUGE_STAND_IN", &steam.origin())],
    );
    assert_eq!(done.code, 0, "{}", done.err);
    for line in [
        "app 5  (Very Positive)",
        "  unique       5",
        "  valve total  5  (5 up / 0 down)",
    ] {
        assert!(has_line(&done.out, line), "{line}\n{}", done.out);
    }
    // The stand-in answers the request for the developer's posts with a page of reviews, so the
    // crawl gives up on them at once and asks nothing twice.
    assert!(
        done.out
            .lines()
            .any(|line| line.starts_with("  updates      not asked again")),
        "{}",
        done.out
    );
    let snapshot = steamgauge_core::embed::latest_snapshot(&library, 5).unwrap();
    assert_eq!(ids_in(&snapshot), ["501", "502", "503", "504", "505"]);
    assert!(
        steam
            .asked()
            .iter()
            .any(|request| request.path() == "/api/appdetails"),
        "{:?}",
        steam.asked()
    );
}

#[cfg(debug_assertions)]
const POSTS: &str = "/ISteamNews/GetNewsForApp/v2/";

#[cfg(debug_assertions)]
#[test]
fn sweep_fetches_the_review_written_since() {
    let dir = Dir::new();
    let library = captured(dir.path());
    let mut written = as_steam_serves(5).remove(0);
    written["recommendationid"] = json!("506");
    written["timestamp_created"] = json!(CAPTURED + 50);
    written["timestamp_updated"] = json!(CAPTURED + 50);
    let reviews = edits(5, CAPTURED, vec![vec![written]], 6);
    let steam = Server::new(move |asked: &Asked| {
        if asked.path() == POSTS {
            Answer::json(&json!({"appnews": {"appid": 5, "count": 1, "newsitems": [{
                "gid": "9001",
                "title": "Patch 1.1",
                "date": CAPTURED + 10,
                "feedname": steamgauge_core::updates::FEED,
                "appid": 5,
            }]}}))
        } else {
            reviews(asked)
        }
    });
    let done = ran_with(
        dir.path(),
        &["sweep", "5", "--out", text(&library), "--pace-ms", "0"],
        &[("STEAMGAUGE_STAND_IN", &steam.origin())],
    );
    assert_eq!(done.code, 0, "{}", done.err);
    assert!(has_line(&done.out, "app 5"), "{}", done.out);
    assert!(
        done.out
            .lines()
            .any(|line| line.starts_with("  fetched")
                && line.contains("1 reviews, 1 new and 0 edited")),
        "{}",
        done.out
    );
    assert!(
        done.out
            .lines()
            .any(|line| line.starts_with("  held         6 of Valve's 6,")),
        "{}",
        done.out
    );
    assert_eq!(
        done.out.lines().last(),
        Some("The readings describe the capture as it was. Read it again to count what arrived."),
        "{}",
        done.out
    );
    assert!(
        has_line(
            &done.out,
            "  updates      1 of the developer's 1 posts on Steam"
        ),
        "{}",
        done.out
    );
    let snapshot = steamgauge_core::embed::latest_snapshot(&library, 5).unwrap();
    assert!(ids_in(&snapshot).contains(&"506".to_owned()));
    let kept = steamgauge_core::updates::kept(&library, 5).unwrap();
    assert_eq!(kept.posts.len(), 1);
    assert_eq!(kept.posts[0].title, "Patch 1.1");
    let asked = steam.asked();
    assert_eq!(
        asked
            .iter()
            .filter(|request| request.path() == POSTS)
            .count(),
        1,
        "{asked:?}"
    );
}

#[cfg(debug_assertions)]
fn store_answering(categories: Value) -> Server {
    Server::new(move |_| {
        Answer::json(&json!({"5": {"success": true, "data": {"categories": categories}}}))
    })
}

#[cfg(debug_assertions)]
#[test]
fn store_facts_keeps_the_stores_answer_beside_the_game() {
    let dir = Dir::new();
    let library = captured(dir.path());
    let reference = dir.path().join("reference");
    let facts = library.join("appid=5").join(steamgauge_core::facts::FILE);
    std::fs::remove_file(&facts).unwrap();
    let steam = store_answering(json!([{"id": 54, "description": "VR Only"}]));
    let asked_of = |stand_in: &Server| {
        ran_with(
            dir.path(),
            &[
                "store-facts",
                "5",
                "--out",
                text(&library),
                "--reference",
                text(&reference),
            ],
            &[("STEAMGAUGE_STAND_IN", &stand_in.origin())],
        )
    };
    let done = asked_of(&steam);
    assert_eq!(done.code, 0, "{}", done.err);
    assert!(
        has_fields(&done.out, &["5", "played", "only", "in", "a", "headset"]),
        "{}",
        done.out
    );
    let kept = steamgauge_core::facts::Facts::load(&library.join("appid=5")).unwrap();
    assert!(kept.headset_only);

    std::fs::remove_file(&facts).unwrap();
    let silent = Server::new(|_| Answer::json(&json!({})));
    let done = asked_of(&silent);
    assert_ne!(done.code, 0, "{}", done.out);
    assert!(
        done.err.contains("the store gave no answer for 5"),
        "{}",
        done.err
    );
    assert!(!facts.exists());
}

#[cfg(debug_assertions)]
#[test]
fn update_says_so_when_this_is_the_newest_release() {
    let version = env!("CARGO_PKG_VERSION");
    let location = format!(
        "{}/tag/v{version}",
        steamgauge_core::newer_version::RELEASES
    );
    let github = Server::new(move |_| Answer::status(302).header("Location", &location));
    let dir = Dir::new();
    let done = ran_with(
        dir.path(),
        &["update"],
        &[("STEAMGAUGE_STAND_IN", &github.origin())],
    );
    assert_eq!(done.code, 0, "{}", done.err);
    assert_eq!(
        done.out,
        format!("SteamGauge {version} is the newest release\n")
    );
    let asked = github.asked();
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert_eq!(
        (asked[0].method.as_str(), asked[0].path()),
        ("HEAD", "/latest")
    );
}

/// A folder standing for Codex's, which `CODEX_HOME` moves away from the person's own.
fn codex_folder(dir: &Dir) -> std::path::PathBuf {
    let folder = dir.path().join("codex-home");
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

fn servers_of(file: &Path) -> toml_edit::DocumentMut {
    std::fs::read_to_string(file).unwrap().parse().unwrap()
}

#[test]
fn connect_adds_this_copy_to_the_clients_own_file() {
    let dir = Dir::new();
    let folder = codex_folder(&dir);
    let home = [("CODEX_HOME", text(&folder))];
    let file = folder.join("config.toml");

    let done = ran_with(dir.path(), &["connect", "codex"], &home);
    assert_eq!(done.code, 0, "{}", done.err);
    assert!(
        has_line(&done.out, &format!("added       to {}", file.display())),
        "{}",
        done.out
    );
    let document = servers_of(&file);
    let servers = document["mcp_servers"].as_table().unwrap();
    assert_eq!(servers.len(), 1);
    let (_, entry) = servers.iter().next().unwrap();
    let args: Vec<&str> = entry["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|arg| arg.as_str().unwrap())
        .collect();
    assert_eq!(args, ["mcp"]);

    let again = ran_with(dir.path(), &["connect", "codex"], &home);
    assert_eq!(again.code, 0, "{}", again.err);
    assert!(
        has_line(
            &again.out,
            &format!("unchanged   {} starts this copy already", file.display())
        ),
        "{}",
        again.out
    );

    let refused = ran_with(dir.path(), &["connect", "nosuch"], &home);
    assert_ne!(refused.code, 0, "{}", refused.out);
    assert_eq!(
        std::fs::read_dir(&folder).unwrap().count(),
        1,
        "nothing else is written to the folder"
    );
}

#[test]
fn clients_says_which_client_starts_this_copy() {
    let dir = Dir::new();
    let folder = codex_folder(&dir);
    let home = [("CODEX_HOME", text(&folder))];
    let codex_line = |out: &str| {
        out.lines()
            .find(|line| line.split_whitespace().next() == Some("codex"))
            .unwrap_or_else(|| panic!("no line for codex in:\n{out}"))
            .to_owned()
    };

    let before = ran_with(dir.path(), &["clients"], &home);
    assert_eq!(before.code, 0, "{}", before.err);
    let line = codex_line(&before.out);
    assert!(line.ends_with("not added"), "{line}");

    let added = ran_with(dir.path(), &["connect", "codex"], &home);
    assert_eq!(added.code, 0, "{}", added.err);
    let after = ran_with(dir.path(), &["clients"], &home);
    assert_eq!(after.code, 0, "{}", after.err);
    let line = codex_line(&after.out);
    assert!(line.ends_with("starts this copy"), "{line}");

    let last = after.out.lines().last().unwrap().trim();
    let command = last
        .strip_suffix(" mcp")
        .and_then(|rest| rest.strip_prefix('"'))
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or_else(|| panic!("the last line is not the program in quotes, then mcp: {last}"));
    assert_eq!(
        Path::new(command),
        Path::new(env!("CARGO_BIN_EXE_steamgauge"))
    );
}

#[test]
fn disconnect_takes_the_entry_out_again() {
    let dir = Dir::new();
    let folder = codex_folder(&dir);
    let home = [("CODEX_HOME", text(&folder))];
    let file = folder.join("config.toml");
    std::fs::write(&file, "[mcp_servers.other]\ncommand = \"other\"\n").unwrap();

    let added = ran_with(dir.path(), &["connect", "codex"], &home);
    assert_eq!(added.code, 0, "{}", added.err);
    let done = ran_with(dir.path(), &["disconnect", "codex"], &home);
    assert_eq!(done.code, 0, "{}", done.err);
    assert!(
        has_line(&done.out, &format!("removed     from {}", file.display())),
        "{}",
        done.out
    );
    let document = servers_of(&file);
    let servers = document["mcp_servers"].as_table().unwrap();
    assert_eq!(servers.len(), 1);
    assert_eq!(servers["other"]["command"].as_str(), Some("other"));

    let again = ran_with(dir.path(), &["disconnect", "codex"], &home);
    assert_eq!(again.code, 0, "{}", again.err);
    assert!(
        has_line(
            &again.out,
            &format!("unchanged   {} has no entry for SteamGauge", file.display())
        ),
        "{}",
        again.out
    );
}

#[test]
fn mcp_lists_the_apps_tools_while_no_window_is_open() {
    use std::io::{BufRead as _, BufReader, Write as _};
    use std::time::{Duration, Instant};

    let dir = Dir::new();
    let mut child = started(dir.path(), &["mcp"], &[]);
    let mut input = child.stdin.take().unwrap();
    for line in [
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {"name": "outside-test", "version": "0"},
        }}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    ] {
        writeln!(input, "{line}").unwrap();
    }
    input.flush().unwrap();

    let (lines, heard) = std::sync::mpsc::channel();
    let stdout = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if lines.send(line).is_err() {
                break;
            }
        }
    });
    let stderr = crate::drained(child.stderr.take().unwrap());

    let begun = Instant::now();
    let mut printed = Vec::new();
    let listing = loop {
        let left = crate::LIMIT.saturating_sub(begun.elapsed());
        let line = heard.recv_timeout(left).unwrap_or_else(|_| {
            let _ = child.kill();
            panic!("no reply to tools/list, after {printed:?}");
        });
        let message: Value = serde_json::from_str(&line)
            .unwrap_or_else(|error| panic!("`{line}` on the client's pipe is no JSON: {error}"));
        printed.push(line);
        if message["id"] == 2 {
            break message;
        }
    };
    drop(input);
    while begun.elapsed() < crate::LIMIT && child.try_wait().unwrap().is_none() {
        std::thread::sleep(Duration::from_millis(10));
    }
    let status = child.try_wait().unwrap().unwrap_or_else(|| {
        let _ = child.kill();
        panic!("the program kept running after its input closed");
    });
    reader.join().unwrap();
    for line in heard.try_iter() {
        assert!(
            serde_json::from_str::<Value>(&line).is_ok(),
            "`{line}` on the client's pipe is no JSON"
        );
    }
    assert_eq!(status.code(), Some(0), "{}", stderr.join().unwrap());

    let named: Vec<&str> = listing["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    let wanted: Vec<String> = steamgauge_app::reference::tools()
        .into_iter()
        .map(|tool| tool.name)
        .collect();
    assert_ne!(named.len(), 0, "the app offers tools");
    assert_eq!(named, wanted);
}
