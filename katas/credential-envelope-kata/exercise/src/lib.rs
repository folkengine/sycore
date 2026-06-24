//! # Credential Envelope kata — selective disclosure & the two-hash boundary
//!
//! You are standing in for the **substrate**: a fact has already been produced
//! by a pure domain kernel (here, just four plain strings — "musician M001
//! plays Cello for orchestra RSO, assigned to concert C01"), and your job is to
//! turn it into a verifiable credential that supports *holder-driven selective
//! disclosure*.
//!
//! The tool is a real [Gordian Envelope](https://www.blockchaincommons.com):
//! a tree of subject–predicate–object triples with a built-in digest tree. Its
//! defining trick is **elision** — replacing any subtree with its digest leaves
//! every ancestor digest (and any signature over the root) unchanged. So an
//! issuer signs a credential once; a holder later strips it to just the fields
//! a verifier needs, and the original signature still verifies.
//!
//! ## The two hashes (this is the whole point)
//!
//! | Digest        | Hash    | Computed over              | Answers                          |
//! |---------------|---------|----------------------------|----------------------------------|
//! | CAS address   | BLAKE3  | the envelope's dCBOR bytes | *where this exact blob lives*    |
//! | Envelope root | SHA-256 | the envelope's digest tree | *the seal that survives elision* |
//!
//! Eliding a subtree **rewrites the serialized bytes** (so the BLAKE3 CAS
//! address changes) but **leaves the SHA-256 root invariant** (so the issuer
//! signature still verifies). One credential therefore has *one seal* but *many
//! addresses* — the full disclosure and each elided projection are distinct CAS
//! blobs sharing one root. That is the boundary you will reconstruct and prove.
//!
//! ## Your task
//!
//! Implement the four functions below (each is `todo!()`). Run `cargo test` and
//! make the suite in `tests/two_hash.rs` pass, in order — the tests ramp from
//! constructing the credential to asserting the full invariant.

use bc_components::SigningPrivateKey;
use bc_envelope::prelude::*;

/// Build the **salted, unsigned** credential envelope for an assignment
/// attestation.
///
/// Structure it as a Gordian Envelope:
/// - **subject:** the `musician` id
/// - **assertion:** `"assignedTo"` → `concert`
/// - **assertion:** `"orchestra"` → `orchestra`
/// - **assertion:** `"instrument"` → `instrument`
///
/// Every object here is *low-entropy* — a short id or a constrained vocabulary
/// whose whole value space could be enumerated. Such fields MUST be **salted**
/// at issuance: without salt, a verifier could later brute-force an *elided*
/// field by hashing candidate values until one matches the leftover digest.
/// Salt makes the privacy property structural instead of hoped-for.
pub fn build_credential(
    musician: &str,
    concert: &str,
    orchestra: &str,
    instrument: &str,
) -> Envelope {
    todo!("build a Gordian Envelope with the musician as subject and three salted assertions")
}

/// Issue the credential by **wrapping it, then signing** with the issuer's key.
///
/// Order matters: wrapping first means the signature covers the *entire*
/// credential as one wrapped-root digest, so it keeps verifying after a holder
/// later elides an inner field. (Signing the bare subject instead would only
/// attest the subject, not the assertions.)
pub fn issue(credential: &Envelope, issuer: &SigningPrivateKey) -> Envelope {
    todo!("wrap the credential and add the issuer's signature")
}

/// The **CAS address**: a BLAKE3 hash over the envelope's dCBOR serialization.
///
/// This answers *where this exact blob lives*. It is computed over the
/// serialized bytes, so it changes whenever those bytes change — including
/// after elision.
pub fn cas_address(envelope: &Envelope) -> [u8; 32] {
    todo!("BLAKE3 over the envelope's tagged-CBOR bytes")
}

/// Holder-side **elision**: return a disclosure of `signed` with the
/// `"assignedTo"` (concert) assertion removed — proving the rest of the
/// credential without revealing *which* concert.
///
/// Elision works by digest: you locate the assertion to remove (its digest is
/// identical wherever that subtree appears, so take it from the original
/// unsigned `credential`) and tell the signed envelope to elide that target.
pub fn elide_concert(signed: &Envelope, credential: &Envelope) -> Envelope {
    todo!("find the assignedTo assertion and elide it from the signed envelope")
}
