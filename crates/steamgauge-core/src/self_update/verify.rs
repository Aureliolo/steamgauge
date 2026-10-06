//! Whether a downloaded file is one this repository's release workflow built, for the release
//! the app set out to install.
//!
//! The release workflow attests SLSA build provenance over every file it publishes with
//! Sigstore, keyless: GitHub's OIDC token names the workflow run, Fulcio certifies that name for
//! ten minutes, and Rekor logs the signature in public. Nothing here holds a key. What is
//! checked is who the certificate was issued to, at which tag, from which repository, and that
//! the signed statement names this very file with the digest it arrived with.

use semver::Version;
use serde::Deserialize;
use sigstore_verify::{
    VerificationPolicy, Verifier,
    trust_root::TrustedRoot,
    types::{Bundle, Sha256Hash, SignatureContent},
};

/// Who may have built a file that is installed. Every field is compared exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Builder {
    /// The repository, as `https://github.com/<owner>/<name>`.
    pub repository: &'static str,
    /// The workflow that signs, as a path in that repository. It is the reusable workflow
    /// that builds and attests, which the certificate names rather than the one that called it.
    pub workflow: &'static str,
    pub issuer: &'static str,
    /// GitHub's number for the repository, which survives a rename and is not reused by a
    /// repository of the same name made after this one was deleted.
    pub repository_id: &'static str,
    pub owner_id: &'static str,
    /// A self-hosted runner is a machine its owner controls; GitHub's are not.
    pub runner: &'static str,
    pub predicate_type: &'static str,
}

/// This repository's release workflow, as Fulcio certifies it.
pub const RELEASE_BUILD: Builder = Builder {
    repository: env!("CARGO_PKG_REPOSITORY"),
    workflow: ".github/workflows/release-build.yml",
    issuer: "https://token.actions.githubusercontent.com",
    repository_id: "1361019156",
    owner_id: "19254254",
    runner: "github-hosted",
    predicate_type: "https://slsa.dev/provenance/v1",
};

impl Builder {
    /// The certificate's subject for a build at the tag of `version`.
    #[must_use]
    pub fn identity(&self, version: &Version) -> String {
        format!(
            "{}/{}@refs/tags/v{version}",
            self.repository, self.workflow
        )
    }
}

/// The file about to be installed, as it arrived.
#[derive(Debug, Clone, Copy)]
pub struct Arrived<'a> {
    /// The release asset's name, which the provenance names it by.
    pub name: &'a str,
    /// Its SHA-256 in lower-case hex, taken as it was written.
    pub sha256: &'a str,
    /// The release it was downloaded from.
    pub version: &'a Version,
}

/// Why a file is not installed. Each reads as the end of "it was not installed because".
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    #[error("{offered} is not newer than {running}, which this computer has")]
    NotNewer { offered: String, running: String },

    #[error("no build provenance could be fetched for it")]
    NoProvenance,

    #[error("its build provenance cannot be read: {0}")]
    Unreadable(String),

    #[error("its signature does not verify: {0}")]
    Signature(String),

    #[error("the signing certificate's {claim} is {actual}, not {expected}")]
    Claim {
        claim: &'static str,
        expected: String,
        actual: String,
    },

    #[error("the signed statement is {0}, not build provenance")]
    NotProvenance(String),

    #[error("the build provenance does not name {name} with the digest it arrived with")]
    NotNamed { name: String },
}

/// The in-toto statement inside the signed envelope, as far as it is read here.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Statement {
    predicate_type: String,
    #[serde(default)]
    subject: Vec<Subject>,
}

#[derive(Deserialize)]
struct Subject {
    #[serde(default)]
    name: String,
    #[serde(default)]
    digest: std::collections::BTreeMap<String, String>,
}

/// Whether `bundle`, a Sigstore bundle as JSON, proves that `arrived` was built by `builder` at
/// the tag of its release, and that this release is newer than `running`.
///
/// Sigstore checks that the certificate chains to Fulcio, carries a log's timestamp, was in
/// the transparency log when it was used, and signed this envelope, and that the envelope's
/// statement has the file's digest among its subjects. Everything Sigstore leaves to the caller
/// is checked here: the identity, the issuer and the certificate's claims about the build, the
/// statement's type, and that the subject carrying the digest is this file's name.
///
/// # Errors
///
/// Returns the first [`Refusal`] that applies.
pub fn verify(
    bundle: &str,
    arrived: &Arrived<'_>,
    running: &Version,
    builder: &Builder,
    root: &TrustedRoot,
) -> Result<(), Refusal> {
    if arrived.version.cmp_precedence(running).is_le() {
        return Err(Refusal::NotNewer {
            offered: arrived.version.to_string(),
            running: running.to_string(),
        });
    }
    let bundle = Bundle::from_json(bundle).map_err(|error| Refusal::Unreadable(error.to_string()))?;
    let digest = Sha256Hash::from_hex(arrived.sha256)
        .map_err(|error| Refusal::Unreadable(error.to_string()))?;
    let verifier = Verifier::new(root).map_err(|error| Refusal::Signature(error.to_string()))?;
    let tag = format!("refs/tags/v{}", arrived.version);
    let policy = VerificationPolicy::new(builder.identity(arrived.version), builder.issuer);
    let result = verifier
        .verify(digest, &bundle, &policy)
        .map_err(|error| match error {
            sigstore_verify::Error::IdentityMismatch { expected, actual } => Refusal::Claim {
                claim: "identity",
                expected: expected.to_string(),
                actual: actual.map_or_else(|| "missing".to_owned(), |actual| actual.to_string()),
            },
            sigstore_verify::Error::IssuerMismatch { expected, actual } => Refusal::Claim {
                claim: "issuer",
                expected,
                actual: actual.unwrap_or_else(|| "missing".to_owned()),
            },
            other => Refusal::Signature(other.to_string()),
        })?;
    // The policy never relaxes any of these; a result that skipped one is refused all the same.
    if !(result.certificate_verified() && result.sct_verified() && result.tlog_verified()) {
        return Err(Refusal::Signature(
            "the certificate, its timestamp or the transparency log went unchecked".to_owned(),
        ));
    }
    let claims = &result
        .certificate()
        .ok_or_else(|| Refusal::Signature("the bundle carries no certificate".to_owned()))?
        .ci_claims;
    for (claim, expected, actual) in [
        (
            "repository identifier",
            builder.repository_id,
            &claims.source_repository_identifier,
        ),
        (
            "owner identifier",
            builder.owner_id,
            &claims.source_repository_owner_identifier,
        ),
        ("source ref", tag.as_str(), &claims.source_repository_ref),
        (
            "runner environment",
            builder.runner,
            &claims.runner_environment,
        ),
    ] {
        if actual.as_deref() != Some(expected) {
            return Err(Refusal::Claim {
                claim,
                expected: expected.to_owned(),
                actual: actual.clone().unwrap_or_else(|| "missing".to_owned()),
            });
        }
    }
    let SignatureContent::DsseEnvelope(envelope) = &bundle.content else {
        return Err(Refusal::NotProvenance("a bare signature".to_owned()));
    };
    let statement: Statement = serde_json::from_slice(envelope.payload.as_bytes())
        .map_err(|error| Refusal::Unreadable(error.to_string()))?;
    if statement.predicate_type != builder.predicate_type {
        return Err(Refusal::NotProvenance(statement.predicate_type));
    }
    let named = statement.subject.iter().any(|subject| {
        subject.name == arrived.name
            && subject
                .digest
                .get("sha256")
                .is_some_and(|digest| digest.eq_ignore_ascii_case(arrived.sha256))
    });
    if !named {
        return Err(Refusal::NotNamed {
            name: arrived.name.to_owned(),
        });
    }
    Ok(())
}

/// The trusted root as it ships inside this build, for when the current one cannot be fetched.
///
/// # Errors
///
/// Fails only if the embedded root does not parse, which its own crate's tests rule out.
pub fn embedded_root() -> Result<TrustedRoot, Refusal> {
    TrustedRoot::from_json(sigstore_verify::trust_root::SIGSTORE_PRODUCTION_TRUSTED_ROOT)
        .map_err(|error| Refusal::Unreadable(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real answer GitHub's attestations API gave for the v0.1.3 Windows setup program,
    /// with `?predicate_type=provenance`, kept as it came.
    const ANSWER: &str = include_str!("provenance-0.1.3.json");
    const SETUP: &str = "steamgauge-0.1.3-windows-x64-setup.exe";
    const SETUP_SHA256: &str = "04949e5832dd9f4ae0488dcbc2777712b5a9a728c7184691365a347863ec520e";
    const ZIP: &str = "steamgauge-0.1.3-x86_64-pc-windows-msvc.zip";
    const ZIP_SHA256: &str = "14a01c3f1353f9608086f480fed9df321d5c09e3bae99988de71bb193e3ffbf9";

    fn bundle() -> String {
        super::super::fetch::bundles_in_answer(ANSWER.as_bytes())
            .unwrap()
            .remove(0)
    }

    fn release() -> Version {
        Version::new(0, 1, 3)
    }

    fn older() -> Version {
        Version::new(0, 1, 2)
    }

    fn check(arrived: &Arrived<'_>, running: &Version, builder: &Builder) -> Result<(), Refusal> {
        verify(&bundle(), arrived, running, builder, &embedded_root().unwrap())
    }

    fn setup(version: &Version) -> Arrived<'_> {
        Arrived {
            name: SETUP,
            sha256: SETUP_SHA256,
            version,
        }
    }

    #[test]
    fn the_real_setup_program_of_0_1_3_verifies_offline_against_the_embedded_root() {
        let release = release();
        assert_eq!(check(&setup(&release), &older(), &RELEASE_BUILD), Ok(()));
    }

    #[test]
    fn every_file_the_provenance_names_verifies_under_its_own_name() {
        let release = release();
        let zip = Arrived {
            name: ZIP,
            sha256: ZIP_SHA256,
            version: &release,
        };
        assert_eq!(check(&zip, &older(), &RELEASE_BUILD), Ok(()));
    }

    #[test]
    fn a_release_that_is_not_newer_than_the_running_one_is_refused_before_anything_is_read() {
        let release = release();
        for running in [release.clone(), Version::new(0, 1, 4), Version::new(1, 0, 0)] {
            assert_eq!(
                verify("not a bundle", &setup(&release), &running, &RELEASE_BUILD, &embedded_root().unwrap()),
                Err(Refusal::NotNewer {
                    offered: "0.1.3".to_owned(),
                    running: running.to_string(),
                })
            );
        }
        assert!(
            check(&setup(&release), &Version::parse("0.1.3-rc.1").unwrap(), &RELEASE_BUILD).is_ok(),
            "a release is newer than its own pre-release"
        );
    }

    #[test]
    fn the_same_file_claimed_for_another_tag_is_refused_by_its_certificate() {
        let other = Version::new(0, 1, 4);
        let refused = check(&setup(&other), &older(), &RELEASE_BUILD).unwrap_err();
        assert!(
            matches!(&refused, Refusal::Claim { claim: "identity", actual, .. } if actual.ends_with("@refs/tags/v0.1.3")),
            "{refused}"
        );
    }

    #[test]
    fn a_build_by_another_workflow_is_refused() {
        let release = release();
        let other = Builder {
            workflow: ".github/workflows/release.yml",
            ..RELEASE_BUILD
        };
        assert!(matches!(
            check(&setup(&release), &older(), &other),
            Err(Refusal::Claim { claim: "identity", .. })
        ));
    }

    #[test]
    fn a_build_in_another_repository_is_refused() {
        let release = release();
        let other = Builder {
            repository: "https://github.com/someone/steamgauge",
            ..RELEASE_BUILD
        };
        assert!(matches!(
            check(&setup(&release), &older(), &other),
            Err(Refusal::Claim { claim: "identity", .. })
        ));
    }

    #[test]
    fn a_certificate_from_another_issuer_is_refused() {
        let release = release();
        let other = Builder {
            issuer: "https://accounts.google.com",
            ..RELEASE_BUILD
        };
        assert!(matches!(
            check(&setup(&release), &older(), &other),
            Err(Refusal::Claim { claim: "issuer", .. })
        ));
    }

    #[test]
    fn each_claim_about_the_build_is_held_to_its_pin() {
        let release = release();
        for (claim, builder) in [
            (
                "repository identifier",
                Builder {
                    repository_id: "1361019157",
                    ..RELEASE_BUILD
                },
            ),
            (
                "owner identifier",
                Builder {
                    owner_id: "19254255",
                    ..RELEASE_BUILD
                },
            ),
            (
                "runner environment",
                Builder {
                    runner: "self-hosted",
                    ..RELEASE_BUILD
                },
            ),
        ] {
            let refused = check(&setup(&release), &older(), &builder).unwrap_err();
            assert!(
                matches!(&refused, Refusal::Claim { claim: found, .. } if *found == claim),
                "{claim}: {refused}"
            );
        }
    }

    #[test]
    fn a_statement_of_another_kind_is_refused() {
        let release = release();
        let other = Builder {
            predicate_type: "https://spdx.dev/Document/v2.3",
            ..RELEASE_BUILD
        };
        assert_eq!(
            check(&setup(&release), &older(), &other),
            Err(Refusal::NotProvenance(
                "https://slsa.dev/provenance/v1".to_owned()
            ))
        );
    }

    #[test]
    fn a_digest_the_provenance_does_not_name_is_refused() {
        let release = release();
        let changed = Arrived {
            sha256: "04949e5832dd9f4ae0488dcbc2777712b5a9a728c7184691365a347863ec520f",
            ..setup(&release)
        };
        assert!(matches!(
            check(&changed, &older(), &RELEASE_BUILD),
            Err(Refusal::Signature(_))
        ));
    }

    #[test]
    fn a_signed_file_under_another_name_is_refused() {
        let release = release();
        for name in [ZIP, "steamgauge-0.1.3-windows-x64-setup.exe.exe", ""] {
            let renamed = Arrived {
                name,
                ..setup(&release)
            };
            assert_eq!(
                check(&renamed, &older(), &RELEASE_BUILD),
                Err(Refusal::NotNamed {
                    name: name.to_owned()
                })
            );
        }
    }

    #[test]
    fn a_bundle_changed_by_a_single_byte_is_refused() {
        let release = release();
        let tampered = bundle().replacen("\"sig\":\"MEUCIEaf", "\"sig\":\"MEUCIEag", 1);
        assert_ne!(tampered, bundle());
        assert!(matches!(
            verify(&tampered, &setup(&release), &older(), &RELEASE_BUILD, &embedded_root().unwrap()),
            Err(Refusal::Signature(_))
        ));
    }

    #[test]
    fn a_digest_that_is_not_hex_is_refused() {
        let release = release();
        let odd = Arrived {
            sha256: "not a digest",
            ..setup(&release)
        };
        assert!(matches!(
            check(&odd, &older(), &RELEASE_BUILD),
            Err(Refusal::Unreadable(_))
        ));
    }

    #[test]
    fn the_identity_names_the_reusable_workflow_at_the_release_tag() {
        assert_eq!(
            RELEASE_BUILD.identity(&release()),
            "https://github.com/Aureliolo/steamgauge/.github/workflows/release-build.yml@refs/tags/v0.1.3"
        );
    }
}
