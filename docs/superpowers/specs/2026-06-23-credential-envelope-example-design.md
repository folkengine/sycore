# Design — `credential_envelope` example

Date: 2026-06-23
Status: approved (brainstorming) → ready for implementation plan

## Purpose

Provide a runnable example that takes a fact produced by the **pure SyCore
kernel** and demonstrates the full FolkEngine Part-4 credential flow on top of
it — issue → salt → sign → elide → verify — using a *real* Gordian Envelope.

The example asserts the §4.2 two-hash conformance vector and applies the §4.6
salting rule, both drawn from `docs/SPEC-credential-envelope.md` and reconciled
against `docs/SPEC-sycore-envelope-boundary.md`.

Run with:

```bash
cargo run --example credential_envelope
```

## Architectural framing — why this lives in an example, not the library

`SPEC-sycore-envelope-boundary.md` forbids the SyCore library from touching
envelopes, crypto, hashing, or serialization (obligations 6–8). The envelope
layer is **substrate** work, one layer above the kernel.

This example therefore **stands in for the substrate**:

- The **kernel half** (`apply` → `Event::PlayerAssigned`) touches zero crypto.
- The **envelope half** lives entirely in the example file.
- `bc-envelope` / `bc-components` / `blake3` are added to `[dev-dependencies]`
  only, so they never enter the library's dependency graph and the public API
  stays pure.

The example demonstrates the **seam**: the kernel produces the *fact*; the
example (as substrate) wraps, signs, addresses, and discloses it.

## The credential

Following §4.9 / Q4's top-named candidate — an **assignment attestation**:
*"musician M001 (Cello) is assigned to concert C01."*

Structured as a Gordian Envelope:

- **subject:** `MusicianId "M001"`
- **salted assertion:** `"assignedTo" → "C01"` (concert id — low-entropy, enumerable)
- **salted assertion:** `"orchestra" → "RSO"` (orchestra id — low-entropy)
- **salted assertion:** `"instrument" → "Cello"` (constrained vocabulary — low-entropy)

All three assertion objects are low-entropy per the §4.6 / Q2 inventory, so each
is salted at issuance.

## Flow — six stages, each printed

1. **Kernel.** Build a `Federation` via real `apply` calls (`RegisterMusician`,
   `FoundOrchestra`, `AddToRoster`, `ProgramConcert`, `AssignPlayer`), then pull
   the emitted `Event::PlayerAssigned { concert, musician }` out of the returned
   `Transition`. The kernel does nothing envelope-related — this is the seam.

2. **Seam → envelope.** Map that `Event` (plus the roster instrument) into the
   envelope above, salting each low-entropy assertion
   (`add_assertion_salted` / `add_salt`). A comment marks this as the substrate's
   §4.5 identity-boundary job: kernel string-ids flow in; no XIDs are introduced.

3. **Issue (sign).** `credential.wrap().add_signature(&issuer_keys)`, with an
   issuer keypair from `bc-components`. Wrap-then-sign means the `verifiedBy`
   signature covers the single wrapped-root digest.

4. **Address (full disclosure).** Compute and print both hashes:
   - envelope root = `signed.digest()` (SHA-256, the cross-disclosure id)
   - CAS address = `blake3(signed.to_cbor_data())` (BLAKE3, where this blob lives)

5. **Holder elides.** `signed.elide_removing_set([...])` to strip the
   `assignedTo` (concert) assertion — proving "M001 plays Cello for RSO" without
   revealing *which* concert. Print the elided notation; recompute both hashes.

6. **Verifier asserts the §4.2 vector** (real `assert!`s — a violated invariant
   should abort the example):
   - `blake3(full) != blake3(elided)` — different CAS addresses
   - `full.digest() == elided.digest()` — same envelope root
   - `full.verify_signature_from(&issuer_pub)` **and**
     `elided.verify_signature_from(&issuer_pub)` — signature survives elision
   - comment: the elided concert id cannot be brute-forced because it was salted
     (§4.6) — the privacy property is structural, not hoped-for.

## Order is load-bearing

`salt → sign → elide`. The issuer salts low-entropy fields *before* signing
(§4.6); the holder elides *after*. Eliding an unsalted low-entropy field would
let a verifier recover it from its digest.

## Error handling

Example code, not library code. `main` returns `Result<(), Box<dyn Error>>` and
uses `?` throughout — idiomatic for examples and consistent with the CLAUDE.md
no-`unwrap`/`expect` rule (which targets *library* code). The conformance-vector
`assert!`s are intentional: a broken invariant *should* panic.

## Dependencies

Add to `[dev-dependencies]` only:

- `bc-envelope` (0.43.x)
- `bc-components` (0.31.x)
- `blake3`

Library `[dependencies]` are untouched; the public API stays pure.

## Risks / decision points

- **`deny.toml` allow-list.** `bc-envelope` is `BSD-2-Clause-Patent` and pulls a
  `bc-*` / `dcbor` stack. CI runs `cargo-deny`; the new licenses and crates must
  be added to the allow-list or CI fails. Implementation includes updating
  `deny.toml`.
- **MSRV / edition.** Crate is `rust-version = 1.88`, `edition 2024`. Confirm
  `bc-envelope` 0.43 builds under that during implementation; if it requires a
  newer toolchain, that is a decision point (dev-only MSRV note, or pin a
  compatible version).

## Testing

The example *is* an executable assertion of the §4.2 vector. A parallel
`#[test]` in `tests/` is **deferred** (matches the boundary spec's Q1 stance:
defer the canonical-encoding test until the substrate dCBOR profile exists, and
avoid pulling the dev-deps into the test graph for now).

## Out of scope

- Any change to the SyCore library's public API or core modules.
- XID / XID-document interop (§4.5 deferral).
- zk `ProofEnvelope` leaf (§4.8 deferral).
- Revocation / TTL / capability logic (§4.7 — substrate concern).
