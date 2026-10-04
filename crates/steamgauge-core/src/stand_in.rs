//! A server on 127.0.0.1 standing in for Steam, GitHub or the Hugging Face Hub in a test, so
//! the code that talks to them is tested on what it asks and what it does with the answer,
//! and no test reaches the network.
//!
//! Plain HTTP/1.1, one request per connection, every answer closing its connection: the least
//! a client has to agree with to be served, which every client here does.

use std::{
    fmt::Write as _,
    io::{BufRead, BufReader, Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

/// One request as the stand-in received it.
#[derive(Debug, Clone)]
pub struct Asked {
    pub method: String,
    /// The path and the query string, as the request line gave them.
    pub target: String,
    pub headers: Vec<(String, String)>,
}

impl Asked {
    pub fn path(&self) -> &str {
        self.target.split('?').next().unwrap_or_default()
    }

    /// A query parameter, as it was sent.
    pub fn param(&self, name: &str) -> Option<&str> {
        self.target
            .split_once('?')?
            .1
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value)
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// What the stand-in answers one request with.
#[derive(Debug, Clone)]
pub struct Answer {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    hang_up: bool,
}

impl Answer {
    pub fn status(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: Vec::new(),
            hang_up: false,
        }
    }

    /// The connection closed with nothing said, which a client sees as a failed request.
    pub fn hang_up() -> Self {
        Self {
            hang_up: true,
            ..Self::status(0)
        }
    }

    pub fn body(body: impl Into<Vec<u8>>) -> Self {
        Self {
            body: body.into(),
            ..Self::status(200)
        }
    }

    pub fn json(value: &serde_json::Value) -> Self {
        Self::body(value.to_string()).header("Content-Type", "application/json")
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }
}

type Handler = dyn Fn(&Asked) -> Answer + Send + Sync;

/// The stand-in, serving until it is dropped.
#[derive(Debug)]
pub struct Server {
    address: SocketAddr,
    asked: Arc<Mutex<Vec<Asked>>>,
    stopping: Arc<AtomicBool>,
}

impl Server {
    /// Answers every request with whatever `handler` makes of it, each connection on a thread of
    /// its own so that requests made at once are answered at once.
    pub fn new(handler: impl Fn(&Asked) -> Answer + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let asked = Arc::new(Mutex::new(Vec::new()));
        let stopping = Arc::new(AtomicBool::new(false));
        let handler: Arc<Handler> = Arc::new(handler);
        {
            let asked = Arc::clone(&asked);
            let stopping = Arc::clone(&stopping);
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    if stopping.load(Ordering::SeqCst) {
                        break;
                    }
                    let Ok(stream) = stream else { continue };
                    let handler = Arc::clone(&handler);
                    let asked = Arc::clone(&asked);
                    std::thread::spawn(move || serve(stream, &*handler, &asked));
                }
            });
        }
        Self {
            address,
            asked,
            stopping,
        }
    }

    /// Where to send a client, as `http://127.0.0.1:port` with no slash after it.
    pub fn origin(&self) -> String {
        format!("http://{}", self.address)
    }

    /// Every request received so far, in the order each arrived.
    pub fn asked(&self) -> Vec<Asked> {
        self.asked.lock().unwrap().clone()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // The accepting thread is blocked on the listener, and a connection is what wakes it to
        // find it should stop.
        self.stopping.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.address);
    }
}

fn serve(stream: TcpStream, handler: &Handler, asked: &Mutex<Vec<Asked>>) {
    let Some(request) = read(&stream) else {
        return;
    };
    let answer = handler(&request);
    asked.lock().unwrap().push(request);
    if answer.hang_up {
        return;
    }
    let mut stream = stream;
    let mut head = format!(
        "HTTP/1.1 {} Stand-in\r\nContent-Length: {}\r\nConnection: close\r\n",
        answer.status,
        answer.body.len()
    );
    for (name, value) in &answer.headers {
        let _ = write!(head, "{name}: {value}\r\n");
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&answer.body);
    let _ = stream.flush();
}

fn read(stream: &TcpStream) -> Option<Asked> {
    let mut reader = BufReader::new(stream);
    let mut start = String::new();
    reader.read_line(&mut start).ok()?;
    let mut parts = start.split_whitespace();
    let method = parts.next()?.to_owned();
    let target = parts.next()?.to_owned();
    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_owned(), value.trim().to_owned()));
        }
    }
    let length = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.parse().ok())
        .unwrap_or(0);
    // Read whole before answering, so that the answer never races a client still sending.
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    Some(Asked {
        method,
        target,
        headers,
    })
}
