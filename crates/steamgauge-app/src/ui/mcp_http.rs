//! The MCP server over HTTP, for clients that connect no other way: the same tools as the local
//! socket, answered on 127.0.0.1 while the app is open, and only once Settings switches it on.
//!
//! A port on 127.0.0.1 is open to every program on the computer, a web page among them, where
//! the socket is open to this user alone. So every request has to carry a token kept in the
//! user's own folder, a request a browser sent is refused by its `Origin`, and one addressed to
//! any other host than this one is refused by its `Host`, which is how a web page that renamed
//! itself to 127.0.0.1 would reach it.

use std::{
    convert::Infallible,
    io,
    net::Ipv4Addr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use bytes::Bytes;
use http_body_util::{BodyExt as _, Full, combinators::BoxBody};
use hyper::{Request, Response, StatusCode, body::Incoming, header, service::service_fn};
use hyper_util::rt::TokioIo;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;

use super::{mcp::Server, settings::Settings, text};

/// Where the token is kept, beside the app's other local data.
fn token_file(local: &Path) -> PathBuf {
    local.join("mcp-http-token")
}

fn local_data(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_local_data_dir().map_err(text)
}

fn draw_token() -> io::Result<String> {
    let mut drawn = [0_u8; 32];
    getrandom::fill(&mut drawn).map_err(io::Error::other)?;
    Ok(super::mcp::hex(&drawn))
}

fn write_token(local: &Path, token: &str) -> io::Result<()> {
    std::fs::create_dir_all(local)?;
    let file = token_file(local);
    let fresh = file.with_extension("new");
    std::fs::write(&fresh, token)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fresh, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(fresh, file)
}

/// The token a request has to carry, drawn the first time it is asked for.
fn token(local: &Path) -> io::Result<String> {
    if let Ok(kept) = std::fs::read_to_string(token_file(local)) {
        let kept = kept.trim();
        if kept.len() == 64 {
            return Ok(kept.to_owned());
        }
    }
    let token = draw_token()?;
    write_token(local, &token)?;
    Ok(token)
}

/// What the server is doing.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Answering {
    Off,
    On { port: u16, token: String },
    Failed { port: u16, why: String },
}

#[derive(Debug, Default)]
pub struct Http {
    state: Mutex<Option<(Answering, CancellationToken)>>,
}

/// How a client reaches the server, for Settings to show.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Reach {
    /// The address, where the server answers.
    pub address: Option<String>,
    pub token: Option<String>,
    /// Why it is not answering though switched on.
    pub problem: Option<String>,
}

pub fn reach(app: &AppHandle) -> Reach {
    let state = app.state::<Http>();
    let state = state
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match state.as_ref().map(|(answering, _)| answering) {
        Some(Answering::On { port, token }) => Reach {
            address: Some(address(*port)),
            token: Some(token.clone()),
            problem: None,
        },
        Some(Answering::Failed { port, why }) => Reach {
            address: None,
            token: None,
            problem: Some(format!("Nothing answers on port {port}: {why}")),
        },
        Some(Answering::Off) | None => Reach {
            address: None,
            token: None,
            problem: None,
        },
    }
}

fn address(port: u16) -> String {
    format!("http://127.0.0.1:{port}/mcp")
}

/// Starts, stops or moves the server to match Settings. Called when the app opens and whenever
/// Settings or the token changes.
pub fn follow(app: &AppHandle) {
    let settings = Settings::load(app);
    let wanted = settings.answer_over_http.then_some(settings.http_port);
    let token = match (wanted, local_data(app)) {
        (Some(_), Ok(local)) => token(&local).map_err(text),
        (Some(_), Err(why)) => Err(why),
        (None, _) => Ok(String::new()),
    };
    let http = app.state::<Http>();
    let mut state = http
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let now = match (wanted, &token) {
        (None, _) => Answering::Off,
        (Some(port), Ok(token)) => Answering::On {
            port,
            token: token.clone(),
        },
        (Some(port), Err(why)) => Answering::Failed {
            port,
            why: why.clone(),
        },
    };
    if state.as_ref().is_some_and(|(was, _)| *was == now) {
        return;
    }
    if let Some((_, stop)) = state.take() {
        stop.cancel();
    }
    let stop = CancellationToken::new();
    if let Answering::On { port, token } = &now {
        let listening =
            std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, *port)).and_then(|listener| {
                listener.set_nonblocking(true)?;
                Ok(listener)
            });
        match listening {
            Ok(listener) => {
                tauri::async_runtime::spawn(serve(
                    app.clone(),
                    listener,
                    *port,
                    token.clone(),
                    stop.clone(),
                ));
            }
            Err(error) => {
                let why = if error.kind() == io::ErrorKind::AddrInUse {
                    "another program has it; choose another port".to_owned()
                } else {
                    error.to_string()
                };
                *state = Some((Answering::Failed { port: *port, why }, stop));
                return;
            }
        }
    }
    *state = Some((now, stop));
}

/// Draws a new token, so whoever held the old one is shut out, and answers with it from now on.
pub fn new_token(app: &AppHandle) -> Result<(), String> {
    let local = local_data(app)?;
    write_token(&local, &draw_token().map_err(text)?).map_err(text)?;
    follow(app);
    Ok(())
}

/// Whether a request carries the token, compared in time that does not depend on how much of it
/// was right.
fn carries(request: &Request<Incoming>, token: &str) -> bool {
    let Some(given) = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
    else {
        return false;
    };
    same(given.as_bytes(), token.as_bytes())
}

fn same(given: &[u8], token: &[u8]) -> bool {
    given.len() == token.len()
        && given
            .iter()
            .zip(token)
            .fold(0_u8, |differs, (a, b)| differs | (a ^ b))
            == 0
}

fn refused() -> Response<BoxBody<Bytes, Infallible>> {
    let mut response = Response::new(
        Full::new(Bytes::from_static(
            b"SteamGauge answers only a request that carries its token, which Settings shows",
        ))
        .boxed(),
    );
    *response.status_mut() = StatusCode::UNAUTHORIZED;
    response.headers_mut().insert(
        header::WWW_AUTHENTICATE,
        header::HeaderValue::from_static("Bearer"),
    );
    response
}

async fn serve(
    app: AppHandle,
    listener: std::net::TcpListener,
    port: u16,
    token: String,
    stop: CancellationToken,
) {
    let Ok(listener) = tokio::net::TcpListener::from_std(listener) else {
        return;
    };
    let config = StreamableHttpServerConfig::default()
        .with_allowed_hosts([format!("127.0.0.1:{port}"), format!("localhost:{port}")])
        .enforce_origin_validation()
        .with_cancellation_token(stop.child_token());
    let service = Arc::new(StreamableHttpService::new(
        move || Ok(Server::new(app.clone())),
        Arc::new(LocalSessionManager::default()),
        config,
    ));
    let token: Arc<str> = token.into();
    loop {
        let stream = tokio::select! {
            () = stop.cancelled() => return,
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => stream,
                Err(_) => continue,
            },
        };
        let service = Arc::clone(&service);
        let token = Arc::clone(&token);
        let stop = stop.clone();
        tauri::async_runtime::spawn(async move {
            let answer = service_fn(move |request: Request<Incoming>| {
                let service = Arc::clone(&service);
                let token = Arc::clone(&token);
                async move {
                    Ok::<_, Infallible>(if carries(&request, &token) {
                        service.handle(request).await
                    } else {
                        refused()
                    })
                }
            });
            let connection = hyper::server::conn::http1::Builder::new()
                .serve_connection(TokioIo::new(stream), answer);
            tokio::select! {
                () = stop.cancelled() => {}
                _ = connection => {}
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_whole_token_is_the_token() {
        assert!(same(b"abcd", b"abcd"));
        assert!(!same(b"abce", b"abcd"));
        assert!(!same(b"abc", b"abcd"));
        assert!(!same(b"", b"abcd"));
    }

    #[test]
    fn a_token_is_drawn_once_and_kept() {
        let folder = std::env::temp_dir().join(format!("steamgauge-token-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let first = token(&folder).unwrap();
        assert_eq!(first.len(), 64);
        assert_eq!(token(&folder).unwrap(), first);
        write_token(&folder, &draw_token().unwrap()).unwrap();
        assert_ne!(token(&folder).unwrap(), first);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn the_address_names_the_loopback_and_the_port() {
        assert_eq!(address(47_800), "http://127.0.0.1:47800/mcp");
    }
}
