//! # Credential Envelope kata — reference solution
//!
//! The real implementation, extracted from `examples/credential_envelope.rs`.
//! See `SOLUTION.md` for the walkthrough.

use bc_components::SigningPrivateKey;
use bc_envelope::prelude::*;

/// Build the salted, unsigned credential envelope (§4.6: salt low-entropy
/// fields at issuance).
pub fn build_credential(
    musician: &str,
    concert: &str,
    orchestra: &str,
    instrument: &str,
) -> Envelope {
    // `add_assertion_salted(predicate, object, salted)` with `salted = true`
    // attaches a random salt to each assertion so the object cannot be
    // brute-forced from its digest after elision.
    Envelope::new(musician)
        .add_assertion_salted("assignedTo", concert, true)
        .add_assertion_salted("orchestra", orchestra, true)
        .add_assertion_salted("instrument", instrument, true)
}

/// Wrap, then sign: the `verifiedBy` signature covers the single wrapped-root
/// digest, so it survives later elision of any inner assertion.
pub fn issue(credential: &Envelope, issuer: &SigningPrivateKey) -> Envelope {
    credential.wrap().add_signature(issuer)
}

/// CAS address: BLAKE3 over the envelope's dCBOR serialization.
pub fn cas_address(envelope: &Envelope) -> [u8; 32] {
    let bytes = envelope.tagged_cbor().to_cbor_data();
    *blake3::hash(&bytes).as_bytes()
}

/// Remove the `assignedTo` assertion. The target digest is taken from the
/// original unsigned credential; that same digest identifies the subtree inside
/// the signed tree, so `elide_removing_target` can reach it through the wrap.
pub fn elide_concert(signed: &Envelope, credential: &Envelope) -> Envelope {
    let concert_assertion = credential
        .assertion_with_predicate("assignedTo")
        .expect("credential has an assignedTo assertion");
    signed.elide_removing_target(&concert_assertion)
}
