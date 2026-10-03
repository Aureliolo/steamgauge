//! The one way this crate makes an HTTP client.
//!
//! TLS is rustls with ring as its cryptography. rustls's default, aws-lc, compiles its C with the
//! compiler's warnings off, which leaves the shipped Windows program without the checks
//! `BinSkim` requires; ring's C is compiled with them. reqwest is built without a provider of its own, so
//! ring is installed as the process's provider before any client is made.

use std::sync::Once;

/// A client builder with ring installed as the process's TLS provider.
pub(crate) fn builder() -> reqwest::ClientBuilder {
    static RING: Once = Once::new();
    RING.call_once(|| {
        // Refused only where a provider is already installed, which is then the one in use.
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
    reqwest::Client::builder()
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_client_can_be_built_with_ring_as_the_provider() {
        assert!(super::builder().build().is_ok());
        assert!(rustls::crypto::CryptoProvider::get_default().is_some());
    }
}
