//! Updating this copy of `SteamGauge` to a newer release, with no signing key anywhere.
//!
//! The release workflow attests build provenance over every file it publishes through
//! Sigstore, keyless, and that attestation is what an update checks: the file the window
//! downloads is installed only when the provenance proves it was built by this repository's
//! release workflow at the tag of a newer release, under that file's name and with the digest
//! it arrived with. What can be done without a window is here; the window decides when, shows
//! how far it has got, and runs what installs it.

pub mod fetch;
pub mod install;
#[cfg(unix)]
pub mod replace;
pub mod verify;

pub use fetch::{
    Failed, Fetched, MOST, Origins, client, download, provenance, provenance_name, sha256_of,
    trusted_root,
};
pub use install::{Install, Owner};
pub use verify::{Arrived, Builder, RELEASE_BUILD, Refusal, verify};

use semver::Version;
use sigstore_verify::trust_root::TrustedRoot;

/// Whether any of `bundles` proves `arrived`, as [`verify`] decides for one. The refusal given
/// is the last bundle's, since the first is the release's own and the rest are GitHub's copies.
///
/// # Errors
///
/// Refuses a release no newer than `running` before reading any bundle, and with
/// [`Refusal::NoProvenance`] when there is none to read.
pub fn verify_any(
    bundles: &[String],
    arrived: &Arrived<'_>,
    running: &Version,
    builder: &Builder,
    root: &TrustedRoot,
) -> Result<(), Refusal> {
    let mut refusal = Refusal::NoProvenance;
    for bundle in bundles {
        match verify(bundle, arrived, running, builder, root) {
            Ok(()) => return Ok(()),
            Err(refused @ Refusal::NotNewer { .. }) => return Err(refused),
            Err(refused) => refusal = refused,
        }
    }
    Err(refusal)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn real() -> String {
        fetch::bundles_in_answer(include_bytes!("provenance-0.1.3.json"))
            .unwrap()
            .remove(0)
    }

    fn check(bundles: &[String], running: &Version) -> Result<(), Refusal> {
        let version = Version::new(0, 1, 3);
        let arrived = Arrived {
            name: "steamgauge-0.1.3-windows-x64-setup.exe",
            sha256: "04949e5832dd9f4ae0488dcbc2777712b5a9a728c7184691365a347863ec520e",
            version: &version,
        };
        verify_any(
            bundles,
            &arrived,
            running,
            &RELEASE_BUILD,
            &verify::embedded_root().unwrap(),
        )
    }

    #[test]
    fn one_bundle_that_proves_the_file_is_enough() {
        let older = Version::new(0, 1, 2);
        assert_eq!(check(&["{}".to_owned(), real()], &older), Ok(()));
        assert_eq!(check(&[real(), "{}".to_owned()], &older), Ok(()));
    }

    #[test]
    fn with_none_that_proves_it_the_last_refusal_is_given() {
        let older = Version::new(0, 1, 2);
        assert_eq!(check(&[], &older), Err(Refusal::NoProvenance));
        let forged = real().replacen("\"sig\":\"MEUCIEaf", "\"sig\":\"MEUCIEag", 1);
        assert_ne!(forged, real());
        assert!(matches!(
            check(&[forged.clone(), "{}".to_owned()], &older),
            Err(Refusal::Unreadable(_))
        ));
        assert!(matches!(
            check(&["{}".to_owned(), forged], &older),
            Err(Refusal::Signature(_))
        ));
    }

    #[test]
    fn a_release_that_is_not_newer_is_refused_whatever_the_bundles_say() {
        let same = Version::new(0, 1, 3);
        assert!(matches!(
            check(&[real()], &same),
            Err(Refusal::NotNewer { .. })
        ));
        assert!(matches!(
            check(&["{}".to_owned(), real()], &same),
            Err(Refusal::NotNewer { .. })
        ));
    }
}
