# FolkEngine — Specification Addendum

## The Credential & Selective-Disclosure Layer (Gordian Envelope)

Status: draft for review. **Part 4** — the first *layer built on* the substrate,
not substrate itself. Parts 0–3 (`SPEC-canonical-encoding.md`,
`SPEC-cas-and-sigsuite.md`, `SPEC-rebuild-and-rotation.md`) are the BLAKE3 core;
this part adds credential-style selective disclosure on top of it.

> **Decision recorded.** FolkEngine adopts **Gordian Envelope as-is** (Blockchain
> Commons; IETF draft-mcnally-envelope) as its credential and selective-disclosure
> format. "As-is" means the standard format unchanged: the **SHA-256 digest tree**,
> the **Gordian dCBOR** profile, and the reference Rust implementation
> (`bc-envelope`). We accept a confined SHA-256 "island" inside an otherwise-BLAKE3
> system in exchange for a mature, standards-track spec with a working
> implementation, rather than reimplementing elision over BLAKE3 and forking from
> the ecosystem.

Depends on: `ContentStore` / CAS, `ContentHash` (Part 1); the canonical encoding
profile (Part 0); `Entry<E>`, `entry-payload`, `signing_digest` (Parts 0, 2);
DIDs and FROST identity (ARCHITECTURE §4.4).

---

## Part 4 — Selective Disclosure via Gordian Envelope

### 4.1 What "as-is" commits us to

A Gordian Envelope is a recursive smart-document of semantic triples
(subject-predicate-object) over deterministic CBOR, with a built-in Merkle-like
digest tree. Its defining capability is **elision**: any subtree can be replaced
by its digest, leaving the tree's root digest — and therefore any signature over
that root — unchanged. This gives **holder-driven selective disclosure**: an issuer
signs a complete credential once; a holder later strips it to only the fields a
given verifier needs, and the original signature still verifies. Elision is
reversible (progressive trust), and the format pushes encryption, compression,
decorrelation, and inclusion proofs into extension specs.

Adopting it as-is means three concrete commitments:

1. **Digest tree is SHA-256.** This is not configurable without leaving the
   standard. It is the one place FolkEngine's BLAKE3 discipline does not reach;
   §4.3 confines it.
2. **Encoding is Gordian dCBOR.** This **resolves `SPEC-canonical-encoding §0.8`**:
   the canonical encoding profile is **dCBOR (RFC 8949 §4.2.1 / Gordian profile)**,
   not DAG-CBOR. One profile governs both ledger entries and envelopes. Running two
   CBOR profiles is forbidden — it reintroduces the "two ways to encode the same
   data" hazard determinism exists to prevent.
3. **Implementation is `bc-envelope`** (plus `bc-components` for digests and
   signatures). We link it; we do not re-derive it.

### 4.2 The two-hash-world boundary — the load-bearing insight

FolkEngine now has two digests in play, and the integration is clean *because they
answer different questions*:

| Digest | Hash | Computed over | Answers |
|---|---|---|---|
| **CAS address** | BLAKE3 | the envelope's serialized dCBOR bytes | *where this exact blob lives in the DAG* |
| **Envelope root** | SHA-256 | the envelope's internal digest tree | *the seal that survives elision* |

The substrate never computes or trusts the SHA-256 root; it only stores and
retrieves envelope bytes, addressing them by BLAKE3 like any other blob. The
envelope module (§4.3) is the only code that touches SHA-256.

The subtle, essential consequence — derived, not assumed — is what makes the two
worlds coexist:

> **Elision changes the CAS address but not the envelope root.** Eliding a subtree
> rewrites the serialized bytes (the subtree becomes its 32-byte digest), so the
> **BLAKE3 CAS address changes**. But elision is *defined* to leave every
> ancestor's SHA-256 digest unchanged, so the **SHA-256 root is invariant** and the
> issuer's signature over it still verifies.

Therefore:

- The **SHA-256 root identifies a credential** across *all* its disclosures — it is
  the stable cross-disclosure identifier.
- The **BLAKE3 CAS address identifies one particular disclosure** — the full
  envelope and each elided projection are *distinct CAS blobs sharing one root*.

This is the bridge. A credential issued once (root `R`, CAS address `A_full`) and
disclosed two ways yields two CAS entries `A_full`, `A_subset` — different BLAKE3
addresses, same SHA-256 root `R`, same valid signature. "Private during, auditable
after" composes with this on a different axis: that pattern is *temporal* (reveal
later); elision is *selective* (reveal a subset, now); progressive trust combines
them (reveal more of the same-rooted credential over time).

> **Conformance TODO.** Add a test vector to the corpus (built via `bc-envelope`):
> a signed envelope, its elided projection, asserting `blake3(full) != blake3(sub)`,
> `envelope_root(full) == envelope_root(sub)`, and that the `verifiedBy` signature
> verifies against both. This is the §3.6-style "invariant as a test, not prose"
> for the two-hash boundary.

### 4.3 Confinement — the purity boundary for SHA-256

SHA-256 and `bc-components` live **only** in a dedicated `envelope` module. The
rest of `folkcore` treats an envelope as an **opaque dCBOR blob**: the CAS stores
its bytes, `get_verified` rehashes them with BLAKE3, the verified fold links and
references it by `ContentHash` — and none of that path ever imports a SHA-256
digest or an envelope type. This is the domain-kernel purity rule applied to a
foreign hash function: a format/crypto dependency that the core does not need MUST
NOT leak into the core's public API.

```rust
/// The ONLY module that links bc-envelope / bc-components and touches SHA-256.
/// Everything it returns to the rest of folkcore is either opaque bytes or a
/// BLAKE3 ContentHash — never a SHA-256 digest or an Envelope type.
pub mod envelope {
    /// An issued credential as opaque, CAS-addressable bytes. Its SHA-256 root
    /// and signature are internal to the envelope; the substrate sees only bytes.
    pub fn to_cas_bytes(env: &Envelope) -> Vec<u8> { /* dCBOR-serialize */ }

    /// Verify the issuer signature over the (wrapped) envelope root, and that
    /// any elision is well-formed. Returns the FolkEngine-facing facts only:
    /// the issuer DID and the stable cross-disclosure root, NOT SHA-256 in the
    /// public type if it can be avoided (newtype it as `CredentialRoot`).
    pub fn verify_issued(bytes: &[u8]) -> Result<Verified, EnvelopeError> { /* … */ }

    pub struct Verified {
        pub issuer: Did,                 // mapped to FolkEngine identity (§4.5)
        pub root:   CredentialRoot,      // opaque stable id; wraps the SHA-256 root
    }
}
```

The CAS, the rotation registry, and `fold_verified` are unchanged: an envelope is
just a payload whose bytes happen to be a Gordian Envelope.

### 4.4 Carrying credentials in the ledger — two nested signatures, never confused

A credential envelope is carried as the body of a ledger entry via a new
`domain-entry` variant (Part 0 §0.4). The two signatures it then bears live at
different layers and MUST NOT be conflated:

- **Ledger signature** (FolkEngine): FROST-Ed25519 over the entry preimage, digest
  via BLAKE3 `signing_digest` under `Domain::LedgerEntry`. Answers *"this is an
  authorized append to the group's log."*
- **Envelope signature** (Gordian): the issuer's signature over the wrapped
  envelope's SHA-256 root, attached as a `'verifiedBy'` assertion. Answers *"this
  credential's contents were attested by this issuer."*

```cddl
; New domain-entry variant (extends the §0.4 payload union). The body is OPAQUE
; envelope bytes — the substrate does not parse them; only the §4.3 module does.
domain-entry //= [ "credential", credential-envelope ]
credential-envelope = bytes   ; a Gordian Envelope, dCBOR-encoded, issuer-signed
```

The domain-separation discipline already in Part 2 keeps these apart at the ledger
layer: a credential payload signs under `Domain::LedgerEntry`, structurally unable
to be replayed as a `GroupRotation`. The envelope signature is outside that domain
entirely — different hash, different verify path, different key — so there is no
surface on which the two could be substituted for one another.

### 4.5 Identity boundary — no second identity system

The Gordian ecosystem uses XIDs (32-byte key-derived identifiers resolving to XID
documents). FolkEngine identity is **did:key / did:web only** (ARCHITECTURE §4.4),
and that stays canonical. XIDs MUST NOT enter the identity layer as a parallel
scheme — same "second source of truth" hygiene applied to OpenFGA-vs-UCAN and
NATS-vs-ledger. Where an envelope names an issuer or subject, the §4.3 module maps
that identity to a FolkEngine DID at the boundary and surfaces only the DID. If
XID interop is ever wanted, it is a deliberate, separately-registered decision —
not an accident of adopting envelopes.

### 4.6 Disclosure discipline — salting is mandatory for low-entropy fields

Elision relies on deterministic hashing, which means a low-entropy elided field
can be brute-forced: hash the candidate values, match the digest, recover the
"hidden" value. Gordian's countermeasure is salting (a random object added to a
subject/predicate/object), and for credentials it is **not optional**:

> Any elidable field whose value space is small enough to enumerate (booleans,
> small enums, dates, names, ID numbers, monetary amounts) MUST be salted before
> issuance. The issuer salts at issuance; the holder relies on it at elision.

This is the disclosure-layer analog of the encoding-layer rejecting-decoder rule:
a privacy property must be structural (salted by construction), not a hope.

### 4.7 Revocation — at the FolkEngine layer, not the envelope

A signed envelope cannot be unsigned, so credential revocation does not happen
*to* the envelope. It happens through the mechanisms already in the substrate:

- **Revoke the share.** Access to present or read a credential is a capability;
  revoking the UCAN / sharing contract (within one TTL window, axiom 4) revokes
  the disclosure, even though the envelope bytes remain valid.
- **Supersede in the ledger.** A superseding entry (§1.6) can record that a
  credential is withdrawn; derived views fold that in.
- **Optional issuer revocation list.** An issuer MAY publish a revocation-status
  envelope that verifiers check; this is an ecosystem pattern, not a substrate
  requirement.

The envelope's own validity (signature-over-root) is *attestation that the issuer
said this*, permanently; whether a verifier should still *act* on it is a
FolkEngine authorization question.

### 4.8 Relationship to the zk `ProofEnvelope` — complementary, not competing

This decision **narrows, not cuts**, the deferred zk spike. The two disclosure
modes cover different cases and compose:

- **Elision (now, this part):** reveal a *subset of signed fields*; prove the
  revealed fields were part of the signed whole. Cannot prove anything about a
  field kept hidden.
- **zk `ProofEnvelope` (deferred):** prove a *predicate over hidden fields*
  (`age ≥ 18`) without revealing them. Cannot cheaply do arbitrary structured
  redaction.

Because an envelope is recursive, a future zk proof can be carried as an envelope
*leaf* — an elided, salted credential whose remaining disclosed element is a zk
proof about the elided contents. So the zk spike stays on the roadmap for the
predicate-proof case; elision takes over the redaction case immediately. Map onto
the standing framing: replay (FolkEngine) / hold-back-with-membership-proof
(elision) / prove-without-seeing (zk) — three disclosure axes, now two of them
shipping.

### 4.9 Application fit

- **Medical records (primary fit).** A lab issues a result as an envelope, signed
  over its root (lab = issuer). The patient (holder) elides to disclose only the
  relevant fields to a new provider; the lab's signature survives. This unifies
  ingestion contracts (one-time delivery, recipient ownership), referenced
  ownership, and selective disclosure in one artifact.
- **Credential proving ground.** Mirrors Blockchain Commons' own educational
  credential use case — a natural first exercise of the layer.
- **Scoped document shares / mailing-list segments.** A document or segment issued
  as an envelope, disclosed per-recipient by elision rather than by re-encrypting
  per-view.

### 4.10 Implement-now-vs-defer & open edges

Implement now:

- Mark `SPEC-canonical-encoding §0.8` **resolved → dCBOR (Gordian profile)**;
  update the ARCHITECTURE §11 wire-format row accordingly.
- Stand up the `envelope` module (§4.3) linking `bc-envelope`; expose only
  bytes + DID + opaque `CredentialRoot`.
- Add the `"credential"` `domain-entry` variant (§4.4) and the two-hash conformance
  vector (§4.2 TODO).
- Write the salting rule (§4.6) into the issuance path.

Defer:

- The full XID/XID-document interop story (§4.5) — only if cross-ecosystem identity
  is ever required.
- The zk `ProofEnvelope` leaf (§4.8) — predicate proofs remain a separate spike.
- Issuer revocation-list envelopes (§4.7) — substrate revocation covers v1.

Register in ARCHITECTURE §12:

- **Open edge — credential revocation semantics.** Confirm the share-revocation
  vs. issuer-revocation split (§4.7) against the medical-records break-glass and
  referenced-ownership flows.
- **Open edge — root as cross-disclosure correlator.** The SHA-256 root is, by
  design, stable across disclosures (§4.2) — which is exactly what a verifier
  colluding across contexts could correlate on. Decide where unsalted root
  exposure is acceptable and where a per-context wrapping is required. (This is the
  noncorrelation question at the *credential* granularity rather than the field
  granularity of §4.6.)

---

The thread, continued: Parts 0–3 made *one* hash function — BLAKE3 — carry
identity, integrity, and authorization across the substrate. Part 4 admits a
second, SHA-256, but only inside a sealed module and only because the standard it
buys is worth more than hash-function monoculture. The two never compete: BLAKE3
says *where a disclosure lives*, SHA-256 says *which credential it is a disclosure
of*, and elision is the hinge that lets one credential have many addresses and one
seal.

*Part 4 — first layer above the substrate. Resolves `SPEC-canonical-encoding §0.8`
→ dCBOR. Depends on Parts 0–2.*
