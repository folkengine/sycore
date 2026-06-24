//! The §4.2 two-hash conformance vector, as a pedagogically-ordered suite.
//!
//! These tests are adapted directly from the `assert!`s in the real
//! `examples/credential_envelope.rs`. They ramp from constructing the
//! credential (top) to proving the full elision invariant (bottom). Make them
//! pass in order.

use bc_components::{PrivateKeyBase, SigningPrivateKey, SigningPublicKey};
use bc_envelope::prelude::*;
use credential_envelope_kata::{build_credential, cas_address, elide_concert, issue};

/// Register the standard CBOR tags once so envelopes format/serialize correctly.
/// (Idempotent — safe to call from every test.)
fn setup() {
    bc_envelope::register_tags();
}

/// A fresh issuer keypair: an Ed25519 signer and its matching verifier.
fn issuer_keys() -> (SigningPrivateKey, SigningPublicKey) {
    let base = PrivateKeyBase::new();
    let signer = base.ed25519_signing_private_key();
    let verifier = signer.public_key().expect("derive public key");
    (signer, verifier)
}

// ── Stage A · Construction ──────────────────────────────────────────────────

/// The credential carries exactly the three attestation assertions.
#[test]
fn credential_has_three_assertions() {
    setup();
    let credential = build_credential("M001", "C01", "RSO", "Cello");
    assert_eq!(
        credential.assertions().len(),
        3,
        "expected assignedTo, orchestra, and instrument assertions"
    );
}

/// Salting is randomized, so two issuances of the *same* facts must differ.
/// If you forgot to salt (or used a fixed salt), the digests would be equal —
/// and an elided low-entropy field could be brute-forced. This is the §4.6
/// privacy property, expressed as a test.
#[test]
fn low_entropy_fields_are_salted() {
    setup();
    let a = build_credential("M001", "C01", "RSO", "Cello");
    let b = build_credential("M001", "C01", "RSO", "Cello");
    assert_ne!(
        a.digest(),
        b.digest(),
        "identical facts must produce different envelopes — proof that salt was applied"
    );
}

// ── Stage B · Issue & address ───────────────────────────────────────────────

/// The issuer's signature verifies on the full disclosure.
#[test]
fn signature_verifies_on_full_disclosure() {
    setup();
    let (signer, verifier) = issuer_keys();
    let credential = build_credential("M001", "C01", "RSO", "Cello");
    let signed = issue(&credential, &signer);
    assert!(
        signed.verify_signature_from(&verifier).is_ok(),
        "the issued credential must carry a verifiable signature"
    );
}

// ── Stage C · Elision & the two-hash invariant ──────────────────────────────

/// Eliding the concert rewrites the serialized bytes, so the BLAKE3 CAS address
/// changes — the full and elided disclosures are *distinct blobs*.
#[test]
fn elision_changes_the_cas_address() {
    setup();
    let (signer, _) = issuer_keys();
    let credential = build_credential("M001", "C01", "RSO", "Cello");
    let signed = issue(&credential, &signer);
    let elided = elide_concert(&signed, &credential);
    assert_ne!(
        cas_address(&signed),
        cas_address(&elided),
        "BLAKE3 CAS address must change when the serialized bytes change"
    );
}

/// ...but the SHA-256 envelope root is invariant under elision. This is the
/// load-bearing insight: the root is the stable cross-disclosure identifier.
#[test]
fn elision_preserves_the_envelope_root() {
    setup();
    let (signer, _) = issuer_keys();
    let credential = build_credential("M001", "C01", "RSO", "Cello");
    let signed = issue(&credential, &signer);
    let elided = elide_concert(&signed, &credential);
    assert_eq!(
        signed.digest(),
        elided.digest(),
        "SHA-256 envelope root must be invariant under elision"
    );
}

/// Because the root is unchanged, the issuer signature still verifies on the
/// elided disclosure — the holder revealed less without breaking the seal.
#[test]
fn signature_survives_elision() {
    setup();
    let (signer, verifier) = issuer_keys();
    let credential = build_credential("M001", "C01", "RSO", "Cello");
    let signed = issue(&credential, &signer);
    let elided = elide_concert(&signed, &credential);
    assert!(
        elided.verify_signature_from(&verifier).is_ok(),
        "the signature must still verify after elision"
    );
}

/// The elided disclosure actually hides the concert: the full form shows it,
/// the elided form shows `ELIDED` and never leaks the id.
#[test]
fn elided_disclosure_hides_the_concert() {
    setup();
    let (signer, _) = issuer_keys();
    let credential = build_credential("M001", "C01", "RSO", "Cello");
    let signed = issue(&credential, &signer);
    let elided = elide_concert(&signed, &credential);

    assert!(
        signed.format().contains("assignedTo"),
        "the full disclosure should reveal the concert"
    );
    let elided_fmt = elided.format();
    assert!(
        !elided_fmt.contains("C01"),
        "the elided disclosure must not leak the concert id"
    );
    assert!(
        elided_fmt.contains("ELIDED"),
        "the removed assertion should appear as ELIDED"
    );
}
