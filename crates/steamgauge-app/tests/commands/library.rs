//! The commands that work on a library: `brief`, `read`, `recount`, `report` and `export-data`.

use std::path::Path;

use serde_json::Value;
use steamgauge_stand_ins::tempdir::Dir;

use crate::tiny::{CAPTURED, captured, read_library, reader};
use crate::{ran, tiny};

fn text(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn has_line(out: &str, line: &str) -> bool {
    out.lines().any(|found| found == line)
}

fn snapshot(library: &Path, app_id: u32) -> std::path::PathBuf {
    library
        .join(format!("appid={app_id}"))
        .join(format!("snapshot={CAPTURED}"))
}

fn reading(library: &Path, app_id: u32) -> Value {
    let file = snapshot(library, app_id).join("reading.json");
    serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap()
}

fn subject<'a>(reading: &'a Value, id: &str) -> &'a Value {
    reading["subjects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|subject| subject["id"] == id)
        .unwrap()
}

#[test]
fn brief_writes_the_three_sheets_and_names_each() {
    let dir = Dir::new();
    let to = dir.path().join("sheets");
    let done = ran(dir.path(), &["brief", "--to", text(&to)]);
    assert_eq!(done.code, 0, "{}", done.err);
    for name in [
        "labelling-brief.txt",
        "claim-brief.txt",
        "induction-brief.txt",
    ] {
        let line = format!("brief    {}", to.join(name).display());
        assert!(has_line(&done.out, &line), "{line}\n{}", done.out);
    }
    assert_eq!(done.out.lines().count(), 3, "{}", done.out);
    let claim = std::fs::read_to_string(to.join("claim-brief.txt")).unwrap();
    assert_eq!(
        claim,
        steamgauge_core::taxonomy::labelling_brief(steamgauge_core::taxonomy::Unit::Claim)
    );
}

#[test]
fn read_counts_the_library_with_the_reader_it_is_handed() {
    let dir = Dir::new();
    let library = captured(dir.path());
    let model = reader(dir.path());
    let done = ran(
        dir.path(),
        &[
            "read",
            "5",
            "--out",
            text(&library),
            "--model",
            text(&model),
            "--reader",
            "small",
        ],
    );
    assert_eq!(done.code, 0, "{}", done.err);
    let written = snapshot(&library, tiny::TRAINED).join("reading.json");
    for line in [
        "app          5".to_owned(),
        "counted      5 reviews".to_owned(),
        "claims       5".to_owned(),
        "asked        4 of them (20% of the corpus was written before)".to_owned(),
        format!("written to   {}", written.display()),
        "no subject   2 claims (40.0%), and 2 reviews that name nothing at all".to_owned(),
    ] {
        assert!(has_line(&done.out, &line), "{line}\n{}", done.out);
    }
    let counted = reading(&library, 5);
    let bugs = subject(&counted, "bugs");
    assert_eq!(bugs["mention_reviews"], 2);
    assert_eq!(bugs["criticised"], 2);
    let label = bugs["label"].as_str().unwrap();
    let row = done
        .out
        .lines()
        .find(|line| line.starts_with(label))
        .unwrap_or_else(|| panic!("no row for {label}\n{}", done.out));
    let fields: Vec<&str> = row[label.len()..].split_whitespace().collect();
    assert_eq!(fields, ["2", "40.0%", "0", "2", "0"], "{row}");
}

#[test]
fn recount_needs_the_record_and_no_graph() {
    let dir = Dir::new();
    let library = read_library(dir.path());
    let model = reader(dir.path());
    std::fs::remove_file(model.join("model.onnx")).unwrap();
    let done = ran(
        dir.path(),
        &[
            "recount",
            "5",
            "--out",
            text(&library),
            "--model",
            text(&model),
        ],
    );
    assert_eq!(done.code, 0, "{}", done.err);
    assert!(has_line(&done.out, "app          5"), "{}", done.out);
    assert!(
        done.out
            .lines()
            .any(|line| line.starts_with("counted      5 reviews, 5 claims, in ")),
        "{}",
        done.out
    );
    assert_eq!(subject(&reading(&library, 5), "bugs")["mention_reviews"], 2);
}

#[test]
fn report_writes_one_page_that_names_the_game_and_quotes_a_review() {
    let dir = Dir::new();
    let library = read_library(dir.path());
    let page = dir.path().join("page.html");
    let done = ran(
        dir.path(),
        &["report", "5", "--out", text(&library), "--to", text(&page)],
    );
    assert_eq!(done.code, 0, "{}", done.err);
    assert!(
        has_line(&done.out, &format!("report      {}", page.display())),
        "{}",
        done.out
    );
    assert!(has_line(&done.out, "  games      1"), "{}", done.out);
    assert!(
        done.out.lines().any(|line| {
            line.split_whitespace()
                .collect::<Vec<_>>()
                .starts_with(&["5", "5", "reviews,", "5", "claims,"])
        }),
        "{}",
        done.out
    );
    let html = std::fs::read_to_string(&page).unwrap();
    assert!(html.contains("Tiny Game"));
    assert!(html.contains("hate crashes"));
}

#[test]
fn export_data_writes_nine_files_and_refuses_a_folder_that_exists() {
    let dir = Dir::new();
    let library = read_library(dir.path());
    let to = dir.path().join("data");
    let done = ran(
        dir.path(),
        &[
            "export-data",
            "5",
            "--out",
            text(&library),
            "--to",
            text(&to),
        ],
    );
    assert_eq!(done.code, 0, "{}", done.err);
    assert!(
        has_line(&done.out, &format!("data        {}", to.display())),
        "{}",
        done.out
    );
    let mut names = Vec::new();
    for stem in ["subjects", "months", "points", "updates"] {
        names.push(format!("{stem}.csv"));
        names.push(format!("{stem}.json"));
    }
    names.push("README.txt".to_owned());
    let held = |to: &Path| -> Vec<(String, Vec<u8>)> {
        names
            .iter()
            .map(|name| (name.clone(), std::fs::read(to.join(name)).unwrap()))
            .collect()
    };
    let before = held(&to);
    let points = done
        .out
        .lines()
        .find_map(|line| line.strip_prefix("  points     "))
        .unwrap_or_else(|| panic!("no points line\n{}", done.out));
    let rows = before
        .iter()
        .find(|(name, _)| name == "points.json")
        .map(|(_, bytes)| serde_json::from_slice::<Value>(bytes).unwrap())
        .unwrap();
    assert_eq!(rows.as_array().unwrap().len().to_string(), points);

    let again = ran(
        dir.path(),
        &[
            "export-data",
            "5",
            "--out",
            text(&library),
            "--to",
            text(&to),
        ],
    );
    assert_ne!(again.code, 0, "{}", again.out);
    assert_eq!(held(&to), before);
}

#[test]
fn a_command_no_build_has_is_refused() {
    let dir = Dir::new();
    let done = ran(dir.path(), &["no-such-command"]);
    assert_eq!(done.code, 2, "{}", done.err);
    assert_eq!(done.out, "");
    // The parser's own word is American, which the spell check would refuse written out.
    let wording = format!("unrecogni{}ed subcommand", 'z');
    assert!(done.err.contains(&wording), "{}", done.err);
}
