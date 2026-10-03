//! A local page that writes what it is given to a file, so a night's work is never only in a
//! browser.
//!
//! The adjudication page keeps answers in `localStorage`, which survives a closed tab and does
//! not survive a cleared cache, a private window, or a second machine. For a thousand questions
//! of somebody's own judgement that is the wrong place for the only copy, and asking them to
//! remember to press Export is asking them to be the backup.
//!
//! So the page is served from here instead of opened as a file. Every answer is posted as it is
//! made and lands on disk before the next question is drawn; the page reads the file back when
//! it opens, so the disk is the copy that matters and the browser is the cache. Nothing is
//! exported and nothing is lost.
//!
//! It is deliberately the smallest server that can do that: it binds to the loopback address,
//! answers three routes, and knows nothing about the page it is serving. There is no network
//! here to speak to and no second user to serve.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};

use crate::Result;

/// Bodies above this are refused rather than read. A thousand answers is well under a megabyte
/// and nothing else is expected to arrive.
const MOST_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug)]
pub struct Adjudication {
    page: String,
    answers: PathBuf,
}

impl Adjudication {
    #[must_use]
    pub fn new(page: String, answers: PathBuf) -> Self {
        Self { page, answers }
    }

    /// Serves until the process is stopped, printing the address to open.
    ///
    /// # Errors
    ///
    /// Fails if the port cannot be bound.
    pub fn serve(&self, port: u16, say: &dyn Fn(&str)) -> Result<()> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))?;
        let bound = listener.local_addr()?.port();
        say(&format!("adjudicating at http://127.0.0.1:{bound}/"));
        say(&format!(
            "answers are written to {} as they are made; stop with ctrl-c",
            self.answers.display()
        ));

        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                continue;
            };
            // One bad request is one bad request, never the end of the sitting: the page is
            // the only client and a browser will retry.
            if let Err(problem) = self.answer(&mut stream) {
                say(&format!("a request failed: {problem}"));
            }
        }
        Ok(())
    }

    fn answer(&self, stream: &mut TcpStream) -> Result<()> {
        let Some((method, path, body)) = read_request(stream)? else {
            return Ok(());
        };
        match (method.as_str(), path.as_str()) {
            ("GET", "/") => reply(
                stream,
                200,
                "text/html; charset=utf-8",
                self.page.as_bytes(),
            ),
            ("GET", "/answers") => {
                let held = std::fs::read(&self.answers).unwrap_or_else(|_| b"[]".to_vec());
                reply(stream, 200, "application/json; charset=utf-8", &held)
            }
            ("POST", "/answers") => {
                self.keep(&body)?;
                reply(
                    stream,
                    200,
                    "application/json; charset=utf-8",
                    b"{\"saved\":true}",
                )
            }
            _ => reply(
                stream,
                404,
                "text/plain; charset=utf-8",
                b"no such thing here",
            ),
        }
    }

    /// Writes the answers beside the target and renames, so a crash mid-write cannot leave a
    /// half-written file where the whole adjudication used to be.
    ///
    /// Merged into what is already held rather than replacing it. The page posts everything it
    /// knows, and it knows only what is in this browser's storage, which is keyed by the sheet
    /// and the size of the draw. Redraw the page and that key changes, so a fresh session would
    /// post an empty list over a finished adjudication and the one artefact here that cannot be
    /// recomputed would be gone. An answer is addressed by its claim, so the two merge cleanly
    /// and a re-answer of the same claim wins.
    fn keep(&self, body: &[u8]) -> Result<()> {
        let incoming: Vec<serde_json::Value> = serde_json::from_slice(body)?;
        let body = &serde_json::to_vec_pretty(&self.merged(incoming))?;
        if let Some(parent) = self
            .answers
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)?;
        }
        let beside = self.answers.with_extension("json.part");
        std::fs::write(&beside, body)?;
        std::fs::rename(&beside, &self.answers)?;
        Ok(())
    }

    /// What is already on disk, with the incoming answers laid over it by claim.
    fn merged(&self, incoming: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
        let key = |row: &serde_json::Value| {
            format!(
                "{}#{}#{}",
                row.get("app_id").unwrap_or(&serde_json::Value::Null),
                row.get("review_id").unwrap_or(&serde_json::Value::Null),
                row.get("index").unwrap_or(&serde_json::Value::Null),
            )
        };
        let held: Vec<serde_json::Value> = std::fs::read(&self.answers)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();

        let answering: std::collections::HashSet<String> = incoming.iter().map(key).collect();
        let mut merged: Vec<serde_json::Value> = held
            .into_iter()
            .filter(|row| !answering.contains(&key(row)))
            .collect();
        merged.extend(incoming);
        merged
    }
}

/// The method, the path and the body of one request, or `None` when the connection said nothing.
fn read_request(stream: &mut TcpStream) -> Result<Option<(String, String, Vec<u8>)>> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut start = String::new();
    if reader.read_line(&mut start)? == 0 {
        return Ok(None);
    }
    let mut parts = start.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let path = parts.next().unwrap_or("/").to_owned();

    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some(said) = line.trim().strip_prefix("Content-Length:") {
            length = said.trim().parse().unwrap_or(0);
        }
    }
    if length > MOST_BYTES {
        return Ok(Some((method, path, Vec::new())));
    }

    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    // The query string is not used by any route and would only ever be a way to reach one by
    // accident.
    let path = path.split('?').next().unwrap_or("/").to_owned();
    Ok(Some((method, path, body)))
}

fn reply(stream: &mut TcpStream, status: u16, kind: &str, body: &[u8]) -> Result<()> {
    let said = match status {
        200 => "OK",
        404 => "Not Found",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {said}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    stream.flush()?;
    Ok(())
}

/// Reads answers already on disk, for a caller that wants to say how many there are.
#[must_use]
pub fn answers_held(path: &Path) -> usize {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Vec<serde_json::Value>>(&bytes).ok())
        .map_or(0, |rows| rows.len())
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::{Ipv4Addr, TcpListener, TcpStream};

    use super::Adjudication;

    #[test]
    fn a_posted_answer_lands_on_disk_whole_or_not_at_all() {
        let dir = crate::tempdir::Dir::new();
        let target = dir.path().join("gold-answers.json");
        let page = Adjudication::new("<p>hello</p>".to_owned(), target.clone());

        page.keep(br#"[{"subject":"verdict"}]"#)
            .expect("valid answers are written");
        assert_eq!(super::answers_held(&target), 1);

        // Nothing that is not JSON reaches the file, so a half-sent body cannot replace an
        // adjudication with a fragment of one.
        assert!(page.keep(b"{not json").is_err());
        assert_eq!(super::answers_held(&target), 1);
        assert!(!target.with_extension("json.part").exists());
    }

    #[test]
    fn a_fresh_session_cannot_post_away_a_finished_adjudication() {
        // The page posts what this browser holds, and its storage is keyed by the sheet and the
        // size of the draw. Redrawing changes that key, so the next session opens empty; if the
        // post replaced the file, an afternoon of answers would be gone and nothing could
        // recompute them.
        let dir = crate::tempdir::Dir::new();
        let target = dir.path().join("gold-answers.json");
        let page = Adjudication::new("<p>hello</p>".to_owned(), target.clone());

        page.keep(
            br#"[{"app_id":1,"review_id":"r1","index":0,"subject":"verdict"},
                       {"app_id":1,"review_id":"r2","index":0,"subject":"bugs"}]"#,
        )
        .expect("valid answers are written");
        assert_eq!(super::answers_held(&target), 2);

        // A new draw of the same set: empty storage, one fresh answer, and the two earlier ones
        // must survive it.
        page.keep(br#"[{"app_id":1,"review_id":"r3","index":0,"subject":"story"}]"#)
            .expect("a later session merges");
        assert_eq!(super::answers_held(&target), 3);

        // Answering the same claim again is a correction and replaces it rather than doubling it.
        page.keep(br#"[{"app_id":1,"review_id":"r1","index":0,"subject":"graphics"}]"#)
            .expect("a re-answer merges");
        assert_eq!(super::answers_held(&target), 3);
        let held: Vec<serde_json::Value> =
            serde_json::from_slice(&std::fs::read(&target).expect("the file")).expect("json");
        let again = held
            .iter()
            .find(|row| row["review_id"] == "r1")
            .expect("the re-answered claim");
        assert_eq!(again["subject"], "graphics");
    }

    #[test]
    fn answers_held_is_zero_when_there_is_no_file_yet() {
        let dir = crate::tempdir::Dir::new();
        assert_eq!(super::answers_held(&dir.path().join("nothing.json")), 0);
    }

    /// Sends `request` to the page as a browser would and returns all it said back, and the
    /// error answering it ended in, if it did.
    fn ask(page: &Adjudication, request: Vec<u8>) -> (String, Option<String>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let browser = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(address).unwrap();
            stream.write_all(&request).unwrap();
            stream.shutdown(std::net::Shutdown::Write).unwrap();
            let mut said = Vec::new();
            let _ = stream.read_to_end(&mut said);
            String::from_utf8_lossy(&said).into_owned()
        });
        let (mut stream, _) = listener.accept().unwrap();
        let failed = page
            .answer(&mut stream)
            .err()
            .map(|error| error.to_string());
        drop(stream);
        (browser.join().unwrap(), failed)
    }

    fn post(body: &[u8]) -> Vec<u8> {
        let mut request = format!(
            "POST /answers HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\n\
             Content-Length: {}\r\n\r\n",
            body.len()
        )
        .into_bytes();
        request.extend_from_slice(body);
        request
    }

    fn get(path: &str) -> Vec<u8> {
        format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").into_bytes()
    }

    fn page_in(dir: &crate::tempdir::Dir) -> (Adjudication, std::path::PathBuf) {
        let target = dir.path().join("gold-answers.json");
        (
            Adjudication::new("<p>the page</p>".to_owned(), target.clone()),
            target,
        )
    }

    #[test]
    fn the_page_is_served_at_the_root_and_nothing_is_served_anywhere_else() {
        let dir = crate::tempdir::Dir::new();
        let (page, _) = page_in(&dir);
        let (said, _) = ask(&page, get("/"));
        assert!(said.starts_with("HTTP/1.1 200 OK\r\n"), "{said}");
        assert!(said.contains("Content-Type: text/html"), "{said}");
        assert!(said.ends_with("\r\n\r\n<p>the page</p>"), "{said}");

        let (said, _) = ask(&page, get("/elsewhere"));
        assert!(said.starts_with("HTTP/1.1 404 Not Found\r\n"), "{said}");
        let (said, _) = ask(&page, b"DELETE /answers HTTP/1.1\r\n\r\n".to_vec());
        assert!(said.starts_with("HTTP/1.1 404 Not Found\r\n"), "{said}");
    }

    #[test]
    fn answers_posted_are_on_disk_and_read_back_by_the_page() {
        let dir = crate::tempdir::Dir::new();
        let (page, target) = page_in(&dir);
        let (said, _) = ask(&page, get("/answers"));
        assert!(
            said.ends_with("\r\n\r\n[]"),
            "nothing held reads as none: {said}"
        );

        let (said, failed) = ask(&page, post(br#"[{"app_id":1,"review_id":"r1","index":0}]"#));
        assert_eq!(failed, None);
        assert!(said.starts_with("HTTP/1.1 200 OK\r\n"), "{said}");
        assert!(said.ends_with("{\"saved\":true}"), "{said}");
        assert_eq!(super::answers_held(&target), 1);

        // The query string reaches no route of its own.
        let (said, _) = ask(&page, get("/answers?fresh=1"));
        assert!(said.contains("\"review_id\": \"r1\""), "{said}");
    }

    #[test]
    fn a_request_written_with_bare_line_feeds_is_read_as_well() {
        let dir = crate::tempdir::Dir::new();
        let (page, target) = page_in(&dir);
        let body = br#"[{"app_id":1,"review_id":"r1","index":0}]"#;
        let mut request =
            format!("POST /answers HTTP/1.1\nContent-Length: {}\n\n", body.len()).into_bytes();
        request.extend_from_slice(body);
        let (said, failed) = ask(&page, request);
        assert_eq!(failed, None);
        assert!(said.starts_with("HTTP/1.1 200 OK"), "{said}");
        assert_eq!(super::answers_held(&target), 1);
    }

    #[test]
    fn a_connection_that_says_nothing_is_not_answered() {
        let dir = crate::tempdir::Dir::new();
        let (page, _) = page_in(&dir);
        assert_eq!(ask(&page, Vec::new()), (String::new(), None));
    }

    #[test]
    fn a_body_up_to_the_cap_is_read_whole_and_one_over_it_is_not_read() {
        let dir = crate::tempdir::Dir::new();
        let (page, target) = page_in(&dir);
        page.keep(br#"[{"app_id":1,"review_id":"r1","index":0}]"#)
            .unwrap();

        // An empty list padded out to exactly the cap with the whitespace JSON allows.
        let mut whole = vec![b' '; super::MOST_BYTES];
        whole[0] = b'[';
        whole[super::MOST_BYTES - 1] = b']';
        let (said, failed) = ask(&page, post(&whole));
        assert_eq!(failed, None);
        assert!(said.starts_with("HTTP/1.1 200 OK"), "{said}");
        assert_eq!(super::answers_held(&target), 1, "merged, not replaced");

        // Said to be one byte over, which is refused before a byte of it is read.
        let over = format!(
            "POST /answers HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            super::MOST_BYTES + 1
        );
        let (said, failed) = ask(&page, over.into_bytes());
        assert!(failed.is_some());
        assert_eq!(said, "");
        assert_eq!(super::answers_held(&target), 1);
    }

    #[test]
    fn answers_are_kept_in_a_directory_that_did_not_exist_yet() {
        let dir = crate::tempdir::Dir::new();
        let target = dir.path().join("gold").join("answers.json");
        let page = Adjudication::new(String::new(), target.clone());
        page.keep(b"[{}]").unwrap();
        assert_eq!(super::answers_held(&target), 1);
    }

    #[test]
    fn serving_says_where_to_open_the_page_and_answers_there() {
        let dir = crate::tempdir::Dir::new();
        let (page, _) = page_in(&dir);
        let (said, heard) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let say = move |line: &str| {
                let _ = said.send(line.to_owned());
            };
            let _ = page.serve(0, &say);
        });
        let opening = heard
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("the address is said");
        let address = opening
            .strip_prefix("adjudicating at http://")
            .and_then(|rest| rest.strip_suffix('/'))
            .unwrap_or_else(|| panic!("{opening}"));
        let mut stream = TcpStream::connect(address).unwrap();
        stream.write_all(&get("/")).unwrap();
        let mut answer = String::new();
        stream.read_to_string(&mut answer).unwrap();
        assert!(answer.ends_with("<p>the page</p>"), "{answer}");
    }
}
