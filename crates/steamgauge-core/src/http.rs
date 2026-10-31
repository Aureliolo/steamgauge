//! The one way this crate makes an HTTP client.
//!
//! TLS is rustls with ring as its cryptography. rustls's default, aws-lc, compiles its C with the
//! compiler's warnings off, which leaves the shipped Windows program without the checks
//! `BinSkim` requires, but ring's C is compiled with them. reqwest is built without a provider of
//! its own, so ring is installed as the process's provider before any client is made.

use std::sync::Once;
use std::time::Duration;

/// The pauses before each further try of a request that got no answer at all. A connection to a
/// host that answers everyone else is now and then refused, dropped or left without a reply, and
/// the person whose download it was would otherwise lose the whole operation to it.
const AGAIN_AFTER: [Duration; 2] = [Duration::from_secs(1), Duration::from_secs(3)];

/// A client builder with ring installed as the process's TLS provider.
pub(crate) fn builder() -> reqwest::ClientBuilder {
    static RING: Once = Once::new();
    RING.call_once(|| {
        // Refused only where a provider is already installed, which is then the one in use.
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
    reqwest::Client::builder()
}

/// Sends the request `request` makes, and makes and sends it again after each of
/// [`AGAIN_AFTER`]'s pauses for as long as no answer comes back. Any answer, whatever its status,
/// is returned as it came. Only for a request that is safe to repeat, as every GET is.
pub(crate) async fn send(
    request: impl Fn() -> reqwest::RequestBuilder,
) -> reqwest::Result<reqwest::Response> {
    let mut pauses = AGAIN_AFTER.iter();
    loop {
        match request().send().await {
            // reqwest's request kind is every failure between sending and an answer's head: a
            // connection refused, timed out or dropped, a TLS handshake that failed. A request
            // that could not be made, or a redirect refused, is another kind and final.
            Err(failed) if failed.is_request() => match pauses.next() {
                Some(pause) => tokio::time::sleep(*pause).await,
                None => return Err(failed),
            },
            sent => return sent,
        }
    }
}

/// What `failed` says, followed by each cause under it. reqwest's own message stops at "error
/// sending request for url (...)", leaving out whether the connection timed out, was refused or
/// failed its TLS handshake, which is the part a person can act on.
pub(crate) fn explained(failed: &reqwest::Error) -> String {
    let mut text = failed.to_string();
    let mut cause = std::error::Error::source(failed);
    while let Some(under) = cause {
        let said = under.to_string();
        if !text.contains(&said) {
            text.push_str(": ");
            text.push_str(&said);
        }
        cause = under.source();
    }
    text
}

#[cfg(test)]
mod tests {
    use crate::stand_in;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn a_client_can_be_built_with_ring_as_the_provider() {
        assert!(super::builder().build().is_ok());
        assert!(rustls::crypto::CryptoProvider::get_default().is_some());
    }

    /// A stand-in that hangs up on the first `hang_ups` requests and answers the rest with
    /// `answer`.
    fn hanging_up(hang_ups: usize, answer: stand_in::Answer) -> stand_in::Server {
        let asked = Arc::new(AtomicUsize::new(0));
        stand_in::Server::new(move |_| {
            if asked.fetch_add(1, Ordering::SeqCst) < hang_ups {
                stand_in::Answer::hang_up()
            } else {
                answer.clone()
            }
        })
    }

    #[tokio::test]
    async fn a_request_nobody_answered_is_sent_again_until_it_is_answered() {
        let server = hanging_up(2, stand_in::Answer::body("here"));
        let http = super::builder().build().unwrap();
        let url = format!("{}/file", server.origin());
        let response = super::send(|| http.get(&url)).await.unwrap();
        assert_eq!(response.text().await.unwrap(), "here");
        assert_eq!(server.asked().len(), 3);
    }

    #[tokio::test]
    async fn a_request_nobody_answers_is_given_up_after_its_last_pause() {
        let server = hanging_up(usize::MAX, stand_in::Answer::status(200));
        let http = super::builder().build().unwrap();
        let url = format!("{}/file", server.origin());
        let failed = super::send(|| http.get(&url)).await.unwrap_err();
        assert!(failed.is_request(), "{failed}");
        assert_eq!(server.asked().len(), super::AGAIN_AFTER.len() + 1);
    }

    #[tokio::test]
    async fn a_redirect_refused_is_not_sent_again() {
        let server =
            stand_in::Server::new(|_| stand_in::Answer::status(302).header("Location", "/file"));
        let http = super::builder()
            .redirect(reqwest::redirect::Policy::limited(2))
            .build()
            .unwrap();
        let url = format!("{}/file", server.origin());
        let failed = super::send(|| http.get(&url)).await.unwrap_err();
        assert!(failed.is_redirect(), "{failed}");
        assert_eq!(
            server.asked().len(),
            3,
            "the request and the two redirects followed"
        );
    }

    #[tokio::test]
    async fn an_answer_is_returned_as_it_came_whatever_its_status() {
        let server = stand_in::Server::new(|_| stand_in::Answer::status(503));
        let http = super::builder().build().unwrap();
        let url = format!("{}/file", server.origin());
        let response = super::send(|| http.get(&url)).await.unwrap();
        assert_eq!(response.status(), 503);
        assert_eq!(server.asked().len(), 1);
    }

    #[tokio::test]
    async fn a_failure_is_explained_down_to_its_cause() {
        // Nothing listens on a port the system handed out and took back.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let http = super::builder().build().unwrap();
        let failed = http
            .get(format!("http://127.0.0.1:{port}/file"))
            .send()
            .await
            .unwrap_err();
        assert!(
            failed.is_request(),
            "a refused connection is sent again: {failed}"
        );
        let explained = super::explained(&failed);
        assert!(explained.starts_with(&failed.to_string()), "{explained}");
        let cause = std::error::Error::source(&failed).unwrap().to_string();
        assert!(explained.contains(&cause), "{explained}");
        assert!(explained.len() > failed.to_string().len(), "{explained}");
    }
}
