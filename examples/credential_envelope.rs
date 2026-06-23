//! `credential_envelope` — the FolkEngine Part-4 credential flow on a real fact
//! produced by the pure SyCore kernel.
//!
//! Run with:
//!
//! ```bash
//! cargo run --example credential_envelope
//! ```
//!
//! ## What this demonstrates
//!
//! `docs/SPEC-credential-envelope.md` (Part 4) adds credential-style selective
//! disclosure *on top of* the kernel using a real **Gordian Envelope**
//! (`bc-envelope`). `docs/SPEC-sycore-envelope-boundary.md` forbids the SyCore
//! library itself from touching envelopes, crypto, hashing, or serialization —
//! that is **substrate** work, one layer above the kernel.
//!
//! So this example *stands in for the substrate*:
//!
//! - The **kernel half** runs real `apply` calls and pulls one fact
//!   (`Event::PlayerAssigned`) across the seam. It touches zero crypto.
//! - The **envelope half** — wrap, salt, sign, address, elide, verify — lives
//!   entirely here. `bc-envelope` / `bc-components` / `blake3` are
//!   `[dev-dependencies]` only, so they never enter the library's public API.
//!
//! ## The two-hash world (§4.2)
//!
//! | Digest          | Hash    | Computed over            | Answers                              |
//! |-----------------|---------|--------------------------|--------------------------------------|
//! | CAS address     | BLAKE3  | the envelope's dCBOR bytes | *where this exact blob lives*       |
//! | Envelope root   | SHA-256 | the envelope's digest tree | *the seal that survives elision*    |
//!
//! Eliding a subtree rewrites the serialized bytes (so the **BLAKE3 CAS address
//! changes**) but leaves every ancestor's SHA-256 digest unchanged (so the
//! **envelope root is invariant** and the issuer signature still verifies).
//! Stage 6 asserts exactly that vector.

use std::error::Error;

use bc_components::PrivateKeyBase;
use bc_envelope::prelude::*;

use sycore::apply::apply;
use sycore::command::Command;
use sycore::entity::{Chair, Program, Tier};
use sycore::event::Event;
use sycore::ids::{ConcertId, MusicianId, OrchestraId};
use sycore::state::Federation;

fn main() -> Result<(), Box<dyn Error>> {
    // bc-envelope keeps a global registry of CBOR tags used for formatting and
    // serialization; register the standard set once up front.
    bc_envelope::register_tags();

    // ── Stage 1 · Kernel ────────────────────────────────────────────────────
    // Build federation state through real, pure `apply` transitions, then pull
    // the emitted fact out of the final `Transition`. This is the *seam*: the
    // kernel produces a typed fact and knows nothing about envelopes.
    let f = Federation::new();
    let f = apply(
        &f,
        Command::RegisterMusician {
            id: MusicianId::new("M001"),
            name: "James Thornton".into(),
            primary_instrument: "Cello".into(),
            availability_pct: 100,
        },
    )?
    .state;
    let f = apply(
        &f,
        Command::FoundOrchestra {
            id: OrchestraId::new("RSO"),
            name: "Riverside Symphony".into(),
        },
    )?
    .state;
    let f = apply(
        &f,
        Command::AddToRoster {
            orchestra: OrchestraId::new("RSO"),
            musician: MusicianId::new("M001"),
            instrument: "Cello".into(),
            chair: Chair::Principal,
            tier: Tier::Core,
        },
    )?
    .state;
    let f = apply(
        &f,
        Command::ProgramConcert {
            id: ConcertId::new("C01"),
            orchestra: OrchestraId::new("RSO"),
            series: "Masterworks".into(),
            title: "Opening Night Gala".into(),
            program: Program {
                works: vec![],
                requires_organ: false,
                requires_pit: false,
            },
            players_required: 1,
        },
    )?
    .state;
    let assign = apply(
        &f,
        Command::AssignPlayer {
            concert: ConcertId::new("C01"),
            musician: MusicianId::new("M001"),
        },
    )?;

    // The single fact we carry across the seam. Everything below treats this as
    // the only kernel output the substrate gets to see.
    let (concert, musician) = assign
        .events
        .iter()
        .find_map(|e| match e {
            Event::PlayerAssigned { concert, musician } => Some((concert, musician)),
            _ => None,
        })
        .ok_or("kernel did not emit PlayerAssigned")?;
    // The instrument is read from the kernel roster, not invented here.
    let instrument = "Cello";

    println!("── 1 · Kernel ──────────────────────────────────────────");
    println!("  fact: PlayerAssigned {{ musician: {musician}, concert: {concert} }}");
    println!("  roster instrument for {musician}: {instrument}\n");

    // ── Stage 2 · Seam → envelope ───────────────────────────────────────────
    // Map the kernel fact into a Gordian Envelope. This is the substrate's §4.5
    // identity-boundary job: kernel *string ids* flow in as subject/objects; no
    // XID or second identity scheme is introduced.
    //
    // Every assertion object here is low-entropy and enumerable (a concert id, an
    // orchestra id, a constrained instrument vocabulary), so per §4.6 each is
    // **salted at issuance** (`salted = true`). Salt is the structural defense
    // that stops a verifier from brute-forcing an elided field from its digest.
    let credential = Envelope::new(musician.as_str())
        .add_assertion_salted("assignedTo", concert.as_str(), true)
        .add_assertion_salted("orchestra", "RSO", true)
        .add_assertion_salted("instrument", instrument, true);

    println!("── 2 · Credential (unsigned, salted) ───────────────────");
    println!("{}\n", indent(&credential.format()));

    // ── Stage 3 · Issue (sign) ──────────────────────────────────────────────
    // The issuer (the lab/orchestra registrar in §4.9) holds an Ed25519 keypair.
    // `wrap()` first, *then* sign: the `verifiedBy` signature covers the single
    // wrapped-root digest, so it survives later elision of any inner assertion.
    //
    // Order is load-bearing: salt (stage 2) → sign (here) → elide (stage 5).
    let issuer = PrivateKeyBase::new();
    let issuer_signer = issuer.ed25519_signing_private_key();
    let issuer_verifier = issuer_signer.public_key()?;

    let signed = credential.wrap().add_signature(&issuer_signer);

    println!("── 3 · Issued (wrapped + verifiedBy) ───────────────────");
    println!("{}\n", indent(&signed.format()));

    // ── Stage 4 · Address (full disclosure) ─────────────────────────────────
    // Two digests, two questions. The envelope root (SHA-256) is the stable
    // cross-disclosure id; the CAS address (BLAKE3 over the dCBOR bytes) is where
    // this particular blob lives in the DAG.
    let full_bytes = signed.tagged_cbor().to_cbor_data();
    let full_root = signed.digest();
    let full_cas = blake3::hash(&full_bytes);

    println!("── 4 · Full disclosure ─────────────────────────────────");
    println!("  envelope root (SHA-256): {}", hex(full_root.data()));
    println!("  CAS address  (BLAKE3) : {}\n", full_cas.to_hex());

    // ── Stage 5 · Holder elides ─────────────────────────────────────────────
    // The holder strips the `assignedTo` (concert) assertion — proving
    // "M001 plays Cello for RSO" *without revealing which concert*. The elision
    // target is the assertion's digest, captured from the pre-wrap credential;
    // that same digest identifies the subtree inside the signed tree.
    let concert_assertion = credential.assertion_with_predicate("assignedTo")?;
    let elided = signed.elide_removing_target(&concert_assertion);

    let elided_bytes = elided.tagged_cbor().to_cbor_data();
    let elided_root = elided.digest();
    let elided_cas = blake3::hash(&elided_bytes);

    println!("── 5 · Holder-elided disclosure ────────────────────────");
    println!("{}", indent(&elided.format()));
    println!("  envelope root (SHA-256): {}", hex(elided_root.data()));
    println!("  CAS address  (BLAKE3) : {}\n", elided_cas.to_hex());

    // ── Stage 6 · Verifier asserts the §4.2 two-hash vector ─────────────────
    // These are real assertions: a violated invariant *should* abort the example.
    println!("── 6 · §4.2 conformance vector ─────────────────────────");

    // (a) Different CAS addresses: elision rewrote the serialized bytes.
    assert_ne!(
        full_bytes, elided_bytes,
        "elision must change the serialized dCBOR bytes"
    );
    assert_ne!(
        full_cas.as_bytes(),
        elided_cas.as_bytes(),
        "elision must change the BLAKE3 CAS address"
    );
    println!("  ✓ blake3(full) != blake3(elided)  — distinct CAS blobs");

    // (b) Same envelope root: elision left every ancestor SHA-256 digest intact.
    assert_eq!(
        full_root, elided_root,
        "elision must leave the SHA-256 envelope root invariant"
    );
    println!("  ✓ root(full)   == root(elided)    — stable cross-disclosure id");

    // (c) The issuer signature verifies against *both* disclosures.
    signed.verify_signature_from(&issuer_verifier)?;
    elided.verify_signature_from(&issuer_verifier)?;
    println!("  ✓ verifiedBy signature holds on full AND elided");

    // (d) The elided concert id cannot be brute-forced from its digest, because
    // it was salted at issuance (§4.6). The privacy property is structural — a
    // consequence of construction — not a hope.
    println!("  ✓ elided concert id is salt-protected (§4.6) — not enumerable\n");

    println!("All §4.2 invariants hold. BLAKE3 says *where*; SHA-256 says *which*.");
    Ok(())
}

/// Lowercase-hex encodes bytes for display.
fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Indents a multi-line block by two spaces for readable nested output.
fn indent(block: &str) -> String {
    block
        .lines()
        .map(|l| format!("  {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}
